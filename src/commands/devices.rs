use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crate::input::{connect, connect_stream, ConnectorEvent};
use crate::transport::{self, DeviceEvent, TransportAddr};

use super::cli_ui::CliUi;

const BASE_PORT: u16 = 9753;
const PORT_SCAN_RANGE: u16 = 10;
const SCAN_TIMEOUT: Duration = Duration::from_secs(5);

struct ProbeApp {
    port: u16,
    app: String,
    version: String,
    build_mode: String,
}

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog devices");
    ui.section("Scanning");
    let spinner = ui.spinner("Scanning devices");

    let mut rx = transport::start_discovery(BASE_PORT);
    let started_at = Instant::now();
    let deadline = Instant::now() + SCAN_TIMEOUT;
    let mut devices = HashMap::new();
    let mut app_results = HashMap::new();
    let mut found_app = false;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining.min(Duration::from_millis(250)), rx.recv()).await {
            Ok(Some(DeviceEvent::Added(device))) | Ok(Some(DeviceEvent::Updated(device))) => {
                let apps = probe_apps(&device).await;
                found_app |= !apps.is_empty();
                app_results.insert(device.id.clone(), apps);
                devices.insert(device.id.clone(), device);
            }
            Ok(Some(DeviceEvent::Removed(id))) => {
                devices.remove(&id);
                app_results.remove(&id);
            }
            Ok(None) => break,
            Err(_) => {}
        }
        if should_stop_scan(started_at.elapsed(), found_app) {
            break;
        }
    }
    spinner.finish();

    if devices.is_empty() {
        ui.empty("no devices found");
        return Ok(());
    }

    let mut devices = devices.into_values().collect::<Vec<_>>();
    devices.sort_by(|a, b| a.name.cmp(&b.name));

    for device in devices {
        println!();
        println!("  {}", device.name);
        println!("    ◇ {}", connection_label(&device));
        let apps = app_results.remove(&device.id).unwrap_or_default();
        if apps.is_empty() {
            println!("{}", no_app_line());
        } else {
            for app in apps {
                println!(
                    "{}",
                    app_line(app.port, &app.app, &app.version, &app.build_mode)
                );
            }
        }
    }

    Ok(())
}

async fn probe_apps(device: &transport::device_monitor::Device) -> Vec<ProbeApp> {
    let mut apps = Vec::new();
    for offset in 0..PORT_SCAN_RANGE {
        let port = BASE_PORT + offset;
        if let Some(app) = probe_one(device, port).await {
            apps.push(app);
        }
    }
    apps
}

async fn probe_one(device: &transport::device_monitor::Device, port: u16) -> Option<ProbeApp> {
    let plan = transport::resolve_transport_addr(device, port).ok()?;
    let result = match plan {
        TransportAddr::Localhost { port } => {
            let url = format!("ws://localhost:{}", port);
            connect(&url).await.ok()
        }
        TransportAddr::AdbForward { serial, port } => {
            let local_port = transport::adb::setup_forward(&serial, port).await?;
            let url = format!("ws://localhost:{}", local_port);
            let result = connect(&url).await.ok();
            transport::adb::remove_forward(&serial, local_port).await;
            result
        }
        TransportAddr::Usbmuxd { device_id, port } => {
            let tunnel = transport::usbmuxd::connect_device(device_id, port)
                .await
                .ok()?;
            let url = format!("ws://localhost:{}", port);
            connect_stream(tunnel, &url).await.ok()
        }
    };

    let (mut rx, _handle) = result?;
    while let Some(event) = rx.recv().await {
        if let ConnectorEvent::Connected(info) = event {
            return Some(ProbeApp {
                port,
                app: info.app,
                version: info.app_version,
                build_mode: info.build_mode,
            });
        }
    }
    None
}

fn connection_label(device: &transport::device_monitor::Device) -> &'static str {
    match device.kind {
        transport::device_monitor::DeviceKind::Android => "Android / adb",
        transport::device_monitor::DeviceKind::IosUsb { .. } => "iOS USB",
        transport::device_monitor::DeviceKind::Local => "Local",
    }
}

pub(crate) fn app_line(port: u16, app: &str, version: &str, build_mode: &str) -> String {
    format!("    ✓ {}  {}  {}  {}", port, app, version, build_mode)
}

pub(crate) fn no_app_line() -> &'static str {
    "    - no flog_dart app found"
}

pub(crate) fn should_stop_scan(elapsed: Duration, found_app: bool) -> bool {
    found_app && elapsed >= Duration::from_secs(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_line_contains_port_app_version_and_mode() {
        let line = app_line(9753, "com.example", "1.2.0", "debug");
        assert_eq!(line, "    ✓ 9753  com.example  1.2.0  debug");
    }

    #[test]
    fn no_app_line_is_stable() {
        assert_eq!(no_app_line(), "    - no flog_dart app found");
    }

    #[test]
    fn scan_can_stop_after_one_second_once_app_found() {
        assert!(should_stop_scan(Duration::from_millis(1_000), true));
    }

    #[test]
    fn scan_does_not_stop_early_without_app() {
        assert!(!should_stop_scan(Duration::from_millis(1_000), false));
    }
}
