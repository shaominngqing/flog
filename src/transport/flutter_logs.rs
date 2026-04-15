//! Read Flutter app logs via `flutter logs` command.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// A running `flutter logs` process that streams log lines.
pub struct FlutterLogs {
    child: Child,
    reader: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

impl FlutterLogs {
    /// Start `flutter logs` for a specific device.
    /// If device_id is None, uses the default device.
    pub async fn start(device_id: Option<&str>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut cmd = Command::new("flutter");
        cmd.arg("logs").arg("--clear");
        if let Some(id) = device_id {
            cmd.args(["-d", id]);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("No stdout")?;
        let reader = BufReader::new(stdout).lines();

        Ok(Self { child, reader })
    }

    /// Read the next log line. Returns None when the process exits.
    pub async fn next_line(&mut self) -> Option<String> {
        self.reader.next_line().await.ok().flatten()
    }

    /// Stop the flutter logs process.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for FlutterLogs {
    fn drop(&mut self) {
        self.stop();
    }
}
