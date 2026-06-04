use chrono::Utc;
use serde::Serialize;

pub const AI_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Serialize)]
pub struct AiEnvelope<T: Serialize> {
    pub ok: bool,
    pub meta: AiMeta,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Serialize)]
pub struct AiMeta {
    pub flog_version: &'static str,
    pub schema_version: u16,
    pub command: String,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub error: AiError,
    pub diagnostics: Vec<DiagnosticNote>,
}

#[derive(Debug, Serialize)]
pub struct AiError {
    pub code: AiErrorCode,
    pub message: String,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorCode {
    NoDeviceFound,
    NoFlogAppFound,
    MultipleAppsFound,
    HandshakeTimeout,
    AppBusy,
    ReplayIncomplete,
    RecordNotFound,
    AdbForwardFailed,
    UsbmuxdConnectFailed,
    FlutterNotFound,
    FlutterDevicesFailed,
    FlutterScreenshotFailed,
    ScreenshotUnsupported,
    ProtocolMismatch,
    PermissionOrAuthorizationRequired,
    InternalError,
}

#[derive(Debug, Serialize, Default)]
pub struct DiagnosticNote {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Serialize, Default)]
pub struct SnapshotPayload {
    pub app: Option<AiApp>,
    pub collection: CollectionMeta,
    pub summary: Summary,
    pub notable: Vec<serde_json::Value>,
    pub logs: Vec<serde_json::Value>,
    pub network: Vec<serde_json::Value>,
    pub screenshot: Option<serde_json::Value>,
    pub diagnostics: Vec<DiagnosticNote>,
}

#[derive(Debug, Serialize, Default)]
pub struct AiApp {
    pub id: String,
    pub name: String,
    pub package: String,
    pub version: String,
    pub device: String,
    pub device_id: String,
    pub os: String,
    pub build_mode: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Default)]
pub struct CollectionMeta {
    pub ports_scanned: Vec<u16>,
    pub wait_ms: u64,
    pub settle_ms: u64,
    pub complete: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct Summary {
    pub logs: usize,
    pub errors: usize,
    pub warnings: usize,
    pub network: usize,
    pub failed_requests: usize,
    pub active_sse: usize,
    pub websockets: usize,
}

impl AiError {
    pub fn new(code: AiErrorCode, message: impl Into<String>, next_actions: Vec<String>) -> Self {
        Self {
            code,
            message: message.into(),
            next_actions,
        }
    }
}

impl<T: Serialize> AiEnvelope<T> {
    pub fn new(command: &str, ok: bool, payload: T) -> Self {
        Self {
            ok,
            meta: AiMeta {
                flog_version: env!("CARGO_PKG_VERSION"),
                schema_version: AI_SCHEMA_VERSION,
                command: command.to_string(),
                generated_at: Utc::now().to_rfc3339(),
            },
            payload,
        }
    }
}

impl AiEnvelope<ErrorPayload> {
    pub fn error(command: &str, error: AiError) -> Self {
        Self::new(
            command,
            false,
            ErrorPayload {
                error,
                diagnostics: Vec::new(),
            },
        )
    }
}

impl AiEnvelope<SnapshotPayload> {
    pub fn snapshot(payload: SnapshotPayload) -> Self {
        Self::new("snapshot", true, payload)
    }
}

impl SnapshotPayload {
    #[cfg(test)]
    pub fn empty_for_tests() -> Self {
        Self::default()
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
