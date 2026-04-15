//! Device discovery via `flutter devices --machine`.

use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct FlutterDevice {
    pub name: String,
    pub id: String,
    pub platform: String,
    pub emulator: bool,
}

/// How to connect to a device.
pub enum ConnectionMethod {
    /// Direct localhost — iOS simulator, macOS
    Localhost,
    /// adb forward — Android devices
    AdbForward { serial: String },
    /// usbmuxd — iOS real device
    Usbmuxd { udid: String },
}

impl FlutterDevice {
    pub fn connection_method(&self) -> ConnectionMethod {
        if self.platform.starts_with("android") {
            ConnectionMethod::AdbForward { serial: self.id.clone() }
        } else if self.platform == "ios" && !self.emulator {
            ConnectionMethod::Usbmuxd { udid: self.id.clone() }
        } else {
            ConnectionMethod::Localhost
        }
    }
}

pub struct DeviceMonitor {
    known_devices: Vec<FlutterDevice>,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        Self { known_devices: Vec::new() }
    }

    /// Scan for devices. Returns (new_devices, removed_devices).
    pub async fn scan(&mut self) -> (Vec<FlutterDevice>, Vec<FlutterDevice>) {
        let devices = Self::query_flutter_devices().await;

        let new: Vec<FlutterDevice> = devices
            .iter()
            .filter(|d| !self.known_devices.contains(d))
            .cloned()
            .collect();

        let removed: Vec<FlutterDevice> = self
            .known_devices
            .iter()
            .filter(|d| !devices.contains(d))
            .cloned()
            .collect();

        self.known_devices = devices;
        (new, removed)
    }

    pub fn devices(&self) -> &[FlutterDevice] {
        &self.known_devices
    }

    async fn query_flutter_devices() -> Vec<FlutterDevice> {
        let output = match Command::new("flutter")
            .args(["devices", "--machine"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .await
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(),
        };

        let json_str = match String::from_utf8(output) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let arr: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        arr.iter()
            .filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let id = v.get("id")?.as_str()?.to_string();
                let platform = v.get("targetPlatform")?.as_str()?.to_string();
                let emulator = v.get("emulator")?.as_bool().unwrap_or(false);
                // Skip non-mobile/desktop platforms
                if platform == "darwin" || platform.starts_with("web-") || platform == "linux" || platform == "windows" {
                    return None;
                }
                Some(FlutterDevice { name, id, platform, emulator })
            })
            .collect()
    }
}
