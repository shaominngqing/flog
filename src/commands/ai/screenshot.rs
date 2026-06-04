use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::process::Command;

use crate::commands::ai::output::{AiError, AiErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotPayload {
    pub screenshot: ScreenshotResult,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotResult {
    pub ok: bool,
    pub path: Option<String>,
    pub source: String,
    pub captured_at: String,
    pub warning: Option<String>,
    pub error: Option<AiError>,
}

pub fn flutter_screenshot_command(device_id: &str, out: &str) -> ShellCommand {
    ShellCommand {
        program: "flutter".to_string(),
        args: vec![
            "screenshot".to_string(),
            "-d".to_string(),
            device_id.to_string(),
            "-o".to_string(),
            out.to_string(),
        ],
    }
}

pub fn default_screenshot_path(device_id: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let safe_device = device_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    std::env::temp_dir()
        .join("flog-ai")
        .join("screenshots")
        .join(format!("{millis}-{safe_device}.png"))
}

pub async fn capture_with_flutter(device_id: &str, out: Option<PathBuf>) -> ScreenshotResult {
    let path = out.unwrap_or_else(|| default_screenshot_path(device_id));
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return failure(
                AiErrorCode::InternalError,
                format!("Could not create screenshot directory: {e}"),
            );
        }
    }
    let path_str = path.to_string_lossy().to_string();
    let cmd = flutter_screenshot_command(device_id, &path_str);
    let output = Command::new(&cmd.program).args(&cmd.args).output().await;
    match output {
        Ok(output) if output.status.success() && path.exists() => ScreenshotResult {
            ok: true,
            path: Some(path_str),
            source: "flutter_screenshot".to_string(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            warning: Some(
                "Device screenshot may include content outside the Flutter app.".to_string(),
            ),
            error: None,
        },
        Ok(output) => failure_with_output(device_id, &output.stdout, &output.stderr),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => failure(
            AiErrorCode::FlutterNotFound,
            "The flutter command was not found in PATH.".to_string(),
        ),
        Err(e) => failure(
            AiErrorCode::FlutterScreenshotFailed,
            format!("Flutter screenshot failed: {e}"),
        ),
    }
}

fn failure_with_output(device_id: &str, stdout: &[u8], stderr: &[u8]) -> ScreenshotResult {
    let text = command_output_text(stdout, stderr);
    let code = if text.to_ascii_lowercase().contains("not supported") {
        AiErrorCode::ScreenshotUnsupported
    } else {
        AiErrorCode::FlutterScreenshotFailed
    };
    let suffix = if text.is_empty() {
        String::new()
    } else {
        format!(" Output: {text}")
    };
    failure(
        code,
        format!("Flutter screenshot failed for device {device_id}.{suffix}"),
    )
}

fn command_output_text(stdout: &[u8], stderr: &[u8]) -> String {
    let mut chunks = Vec::new();
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    if !stdout.is_empty() {
        chunks.push(stdout);
    }
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !stderr.is_empty() {
        chunks.push(stderr);
    }
    chunks.join(" ")
}

fn failure(code: AiErrorCode, message: String) -> ScreenshotResult {
    ScreenshotResult {
        ok: false,
        path: None,
        source: "flutter_screenshot".to_string(),
        captured_at: chrono::Utc::now().to_rfc3339(),
        warning: None,
        error: Some(AiError::new(
            code,
            message,
            vec![
                "Run `flutter devices`".to_string(),
                "Run `flog ai doctor --format json`".to_string(),
            ],
        )),
    }
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
