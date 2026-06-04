use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use crate::input::{connect, ConnectorEvent};

use super::cli_ui::CliUi;

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog doctor");

    ui.section("flog");
    ui.ok("Version", &format!("v{}", env!("CARGO_PKG_VERSION")));
    match std::env::current_exe() {
        Ok(path) => ui.ok("Binary", &path.display().to_string()),
        Err(e) => ui.warn("Binary", &e.to_string()),
    }

    ui.section("Network");
    let spinner = ui.spinner("Checking GitHub release");
    let release = crate::commands::update::latest_release().await;
    spinner.finish();
    match release {
        Ok(release) => ui.ok("GitHub", &format!("reachable {}", release.tag_name)),
        Err(e) => ui.warn("GitHub", &format!("unreachable: {}", e)),
    }

    ui.section("Tools");
    if command_in_path("adb") {
        ui.ok("adb", "found");
    } else {
        ui.warn("adb", "not found");
    }

    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/var/run/usbmuxd").exists() {
            ui.ok("usbmuxd", "available");
        } else {
            ui.warn("usbmuxd", "not found");
        }
    }

    ui.section("Ports");
    for port in default_ports() {
        match classify_port(port).await {
            PortStatus::Free => ui.ok(&port.to_string(), "free"),
            PortStatus::FlogApp(app) => ui.ok(&port.to_string(), &PortStatus::FlogApp(app).label()),
            PortStatus::OpenNonFlog => ui.warn(&port.to_string(), &PortStatus::OpenNonFlog.label()),
        }
    }

    Ok(())
}

pub(crate) fn default_ports() -> Vec<u16> {
    (9753..=9762).collect()
}

#[cfg(test)]
pub(crate) fn path_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}

fn command_in_path(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|path| {
        let candidate = path.join(command);
        if candidate.exists() {
            return true;
        }
        #[cfg(windows)]
        {
            path.join(format!("{}.exe", command)).exists()
        }
        #[cfg(not(windows))]
        {
            false
        }
    })
}

fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(120)).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PortStatus {
    Free,
    FlogApp(String),
    OpenNonFlog,
}

impl PortStatus {
    pub(crate) fn label(&self) -> String {
        match self {
            PortStatus::Free => "free".to_string(),
            PortStatus::FlogApp(app) => format!("flog_dart {}", app),
            PortStatus::OpenNonFlog => "open, not flog_dart".to_string(),
        }
    }
}

async fn classify_port(port: u16) -> PortStatus {
    if !port_open(port) {
        return PortStatus::Free;
    }
    let url = format!("ws://127.0.0.1:{}", port);
    let Ok((mut rx, _handle)) = connect(&url).await else {
        return PortStatus::OpenNonFlog;
    };
    while let Some(event) = rx.recv().await {
        if let ConnectorEvent::Connected(info) = event {
            return PortStatus::FlogApp(info.app);
        }
    }
    PortStatus::OpenNonFlog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ports_are_9753_through_9762() {
        assert_eq!(default_ports(), (9753..=9762).collect::<Vec<_>>());
    }

    #[test]
    fn command_exists_detects_missing_absolute_path() {
        assert!(!path_exists("/definitely/not/a/flog/tool"));
    }

    #[test]
    fn port_status_formats_flog_app() {
        assert_eq!(
            PortStatus::FlogApp("com.example".into()).label(),
            "flog_dart com.example"
        );
    }

    #[test]
    fn port_status_formats_non_flog_open_port() {
        assert_eq!(PortStatus::OpenNonFlog.label(), "open, not flog_dart");
    }
}
