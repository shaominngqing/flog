use std::time::{Duration, Instant};

use crate::app::App;
use crate::commands::ai::output::{AiError, AiErrorCode};
use crate::input::{connect, connect_stream, ConnectorEvent, ConnectorHandle};
use crate::transport::{self, DeviceEvent, TransportAddr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAppCandidate {
    pub app_id: String,
    pub app_name: String,
    pub app_version: String,
    pub os: String,
    pub package_name: String,
    pub build_mode: String,
    pub device_id: String,
    pub device_name: String,
    pub port: u16,
    pub transport: TransportAddr,
}

pub struct CollectedSession {
    pub app: App,
    pub candidate: AiAppCandidate,
    pub ports_scanned: Vec<u16>,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn select_candidate(
    candidates: &[AiAppCandidate],
    app_selector: Option<&str>,
    device_selector: Option<&str>,
) -> Result<AiAppCandidate, AiError> {
    let matches = candidates
        .iter()
        .filter(|candidate| {
            let app_matches = app_selector.is_none_or(|selector| {
                candidate.app_id == selector
                    || candidate.app_name == selector
                    || candidate.package_name == selector
            });
            let device_matches =
                device_selector.is_none_or(|selector| candidate.device_id == selector);
            app_matches && device_matches
        })
        .cloned()
        .collect::<Vec<_>>();

    match matches.len() {
        1 => Ok(matches[0].clone()),
        0 => Err(AiError::new(
            AiErrorCode::NoFlogAppFound,
            "No matching flog_dart app responded.",
            vec!["Run `flog ai doctor --format json`".to_string()],
        )),
        _ => Err(AiError::new(
            AiErrorCode::MultipleAppsFound,
            "Multiple flog_dart apps responded; select one with --app or --device.",
            vec!["Run `flog ai snapshot --app <name>`".to_string()],
        )),
    }
}

pub async fn collect_snapshot_session(
    base_port: u16,
    wait: Duration,
    settle: Duration,
    app_selector: Option<&str>,
    device_selector: Option<&str>,
) -> Result<CollectedSession, AiError> {
    let ports_scanned = (base_port..base_port + 10).collect::<Vec<_>>();
    let deadline = Instant::now() + wait;
    let candidates = discover_candidates(base_port, deadline).await?;
    let candidate = select_candidate(&candidates, app_selector, device_selector)?;
    let app = collect_app_frames(&candidate, deadline, settle).await?;
    Ok(CollectedSession {
        app,
        candidate,
        ports_scanned,
        complete: Instant::now() <= deadline,
        warnings: Vec::new(),
    })
}

async fn discover_candidates(
    base_port: u16,
    deadline: Instant,
) -> Result<Vec<AiAppCandidate>, AiError> {
    let mut rx = transport::start_discovery(base_port);
    let mut devices = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining.min(Duration::from_millis(250)), rx.recv()).await {
            Ok(Some(DeviceEvent::Added(device))) | Ok(Some(DeviceEvent::Updated(device))) => {
                devices.push(device);
            }
            Ok(Some(DeviceEvent::Removed(_))) => {}
            Ok(None) => break,
            Err(_) => {}
        }
    }

    let mut candidates = Vec::new();
    for device in &devices {
        for port in base_port..base_port + 10 {
            if Instant::now() >= deadline {
                break;
            }
            if let Some(candidate) = probe_candidate(device, port, deadline).await {
                candidates.push(candidate);
            }
        }
    }

    if candidates.is_empty() {
        Err(AiError::new(
            AiErrorCode::NoFlogAppFound,
            "No flog_dart app responded on scanned ports.",
            vec![
                "Run `flog ai doctor --format json`".to_string(),
                "Check that Flog.init() is called before runApp()".to_string(),
            ],
        ))
    } else {
        Ok(candidates)
    }
}

async fn collect_app_frames(
    candidate: &AiAppCandidate,
    deadline: Instant,
    settle: Duration,
) -> Result<App, AiError> {
    let mut connection = open_transport(&candidate.transport)
        .await
        .map_err(|e| {
            AiError::new(
                AiErrorCode::NoFlogAppFound,
                format!("Could not reconnect to selected flog_dart app: {e}"),
                vec!["Run `flog ai snapshot --format json` again.".to_string()],
            )
        })?;
    let mut app = App::new();
    let mut last_frame = Instant::now();
    connection.handle.send_subscribe();

    loop {
        let now = Instant::now();
        if now >= deadline || now.duration_since(last_frame) >= settle {
            break;
        }
        let remaining = deadline.saturating_duration_since(now);
        let settle_remaining = settle.saturating_sub(now.duration_since(last_frame));
        let tick = remaining.min(settle_remaining);
        match tokio::time::timeout(tick, connection.rx.recv()).await {
            Ok(Some(ConnectorEvent::Message(msg))) => {
                crate::run::dispatch_client_message(&mut app, msg);
                last_frame = Instant::now();
            }
            Ok(Some(ConnectorEvent::Disconnected { .. })) | Ok(None) => break,
            Ok(Some(ConnectorEvent::Connected(_))) => {}
            Err(_) => break,
        }
    }

    connection.cleanup().await;
    Ok(app)
}

async fn probe_candidate(
    device: &transport::device_monitor::Device,
    port: u16,
    deadline: Instant,
) -> Option<AiAppCandidate> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    let plan = transport::resolve_transport_addr(device, port).ok()?;
    let mut connection = tokio::time::timeout(remaining, open_transport(&plan))
        .await
        .ok()?
        .ok()?;
    while let Some(event) = connection.rx.recv().await {
        if let ConnectorEvent::Connected(info) = event {
            let candidate = AiAppCandidate {
                app_id: format!("{}:{}", device.id, port),
                app_name: info.app,
                app_version: info.app_version,
                os: info.os,
                package_name: info.package_name,
                build_mode: info.build_mode,
                device_id: device.id.clone(),
                device_name: device.name.clone(),
                port,
                transport: plan,
            };
            connection.cleanup().await;
            return Some(candidate);
        }
    }
    connection.cleanup().await;
    None
}

struct ActiveConnection {
    rx: tokio::sync::mpsc::UnboundedReceiver<ConnectorEvent>,
    handle: ConnectorHandle,
    adb_forward: Option<(String, u16)>,
}

impl ActiveConnection {
    async fn cleanup(&mut self) {
        if let Some((serial, local_port)) = self.adb_forward.take() {
            transport::adb::remove_forward(&serial, local_port).await;
        }
    }
}

async fn open_transport(
    plan: &TransportAddr,
) -> Result<ActiveConnection, Box<dyn std::error::Error + Send + Sync>> {
    match plan {
        TransportAddr::Localhost { port } => {
            let url = format!("ws://localhost:{port}");
            let (rx, handle) = connect(&url).await?;
            Ok(ActiveConnection {
                rx,
                handle,
                adb_forward: None,
            })
        }
        TransportAddr::AdbForward { serial, port } => {
            let local_port = transport::adb::setup_forward(serial, *port)
                .await
                .ok_or_else(|| std::io::Error::other("adb forward failed"))?;
            let url = format!("ws://localhost:{local_port}");
            let (rx, handle) = connect(&url).await?;
            Ok(ActiveConnection {
                rx,
                handle,
                adb_forward: Some((serial.clone(), local_port)),
            })
        }
        TransportAddr::Usbmuxd { device_id, port } => {
            let tunnel = transport::usbmuxd::connect_device(*device_id, *port)
                .await
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let url = format!("ws://localhost:{port}");
            let (rx, handle) = connect_stream(tunnel, &url).await?;
            Ok(ActiveConnection {
                rx,
                handle,
                adb_forward: None,
            })
        }
    }
}

impl AiAppCandidate {
    #[cfg(test)]
    pub fn for_tests(app_id: &str, device_name: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            app_name: app_id.to_string(),
            app_version: "1.0.0".to_string(),
            os: "test".to_string(),
            package_name: format!("com.example.{app_id}"),
            build_mode: "debug".to_string(),
            device_id: device_name.to_string(),
            device_name: device_name.to_string(),
            port: 9753,
            transport: TransportAddr::Localhost { port: 9753 },
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
