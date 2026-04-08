//! ADB logcat input source.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use super::SourceEvent;

#[derive(Clone)]
pub struct AdbDevice {
    pub serial: String,
    pub model: String,
}

/// List connected ADB devices.
pub async fn list_adb_devices() -> Vec<AdbDevice> {
    let output = match Command::new("adb").args(["devices", "-l"]).output().await {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    for line in text.lines().skip(1) {
        // Format: SERIAL  device usb:... product:... model:MODEL ...
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            let serial = parts[0].to_string();
            let model = parts.iter()
                .find(|p| p.starts_with("model:"))
                .map(|p| p[6..].replace('_', " "))
                .unwrap_or_else(|| serial.clone());
            devices.push(AdbDevice { serial, model });
        }
    }

    devices
}

pub struct AdbSource {
    child: Child,
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    device_name: String,
}

impl AdbSource {
    /// Create and start an ADB logcat source.
    pub async fn new(device_serial: Option<&str>) -> std::io::Result<Self> {
        let device_name = Self::query_device_name(device_serial).await;

        let mut args: Vec<&str> = Vec::new();
        if let Some(serial) = device_serial {
            args.extend_from_slice(&["-s", serial]);
        }
        // Capture flutter + related tags. Don't use -s (silence all) which drops too much.
        // Instead use logcat with threadtime format for richer info, filter in parser.
        args.extend_from_slice(&["logcat", "-v", "brief",
            "-s",
            "flutter:V",           // print() / debugPrint()
            "FlutterJNI:W",        // Flutter engine
            "Flutter:V",           // Flutter framework (capital F)
            "DartVM:W",            // Dart VM messages
            "System.out:I",        // Some Dart output goes here
        ]);

        let mut child = Command::new("adb")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = child.stdout.take().expect("failed to capture adb stdout");
        let reader = BufReader::new(stdout).lines();

        Ok(Self { child, reader, device_name })
    }

    async fn query_device_name(serial: Option<&str>) -> String {
        let mut args: Vec<&str> = Vec::new();
        if let Some(s) = serial {
            args.extend_from_slice(&["-s", s]);
        }
        args.extend_from_slice(&["shell", "getprop", "ro.product.model"]);

        match Command::new("adb").args(&args).output().await {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => "Android".to_string(),
        }
    }

    /// Kill the ADB process on drop.
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
    }
}

impl AdbSource {
    pub async fn next_event(&mut self) -> Option<SourceEvent> {
        loop {
            match self.reader.next_line().await {
                Ok(Some(line)) => return Some(SourceEvent::RawLine(line)),
                Ok(None) => return None,
                Err(_) => continue, // skip non-UTF8 lines
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.device_name
    }
}
