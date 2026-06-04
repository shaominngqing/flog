# AI CLI and Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `flog ai` headless inspection commands and a `flog-inspect` skill so AI agents can read recent app diagnostics and optional screenshots without TUI copy/paste.

**Architecture:** Add a read-only command layer under `src/commands/ai/` that reuses existing `transport`, `input`, `run::dispatch`, and `App` state. Keep AI JSON structs and redaction helpers local to the command layer, with pure notable detection in `src/domain/diagnostics.rs` so it stays UI-agnostic and testable. Add the skill as a thin wrapper that invokes the CLI JSON contract.

**Tech Stack:** Rust 2021, clap derive, serde/serde_json, tokio, existing flog WebSocket protocol, Flutter CLI for screenshots, Codex skill markdown.

---

## File Structure

- Modify `src/cli.rs`: add `Command::Ai(AiCommand)` and nested clap args.
- Modify `src/commands/mod.rs`: route `Command::Ai`.
- Create `src/commands/ai/mod.rs`: module entry and top-level dispatcher.
- Create `src/commands/ai/args.rs`: duration parsing and shared AI options.
- Create `src/commands/ai/output.rs`: stable JSON envelope, app metadata, collection metadata, error helpers.
- Create `src/commands/ai/redact.rs`: sensitive-key redaction and byte-safe truncation previews.
- Create `src/domain/diagnostics.rs`: pure notable diagnostics over logs and network entries.
- Modify `src/domain/mod.rs`: expose `diagnostics`.
- Create `src/commands/ai/snapshot.rs`: convert `App` state into AI snapshot JSON records.
- Create `src/commands/ai/session.rs`: headless device/app discovery, connection, collection, settle waits.
- Create `src/commands/ai/screenshot.rs`: Flutter screenshot command selection, platform fallbacks, output path handling.
- Create `src/commands/ai/watch.rs`: bounded NDJSON event stream.
- Create `src/commands/ai/get.rs`: detail lookup by stable record id.
- Modify or create tests beside production modules using the existing sibling pattern.
- Create `skills/flog-inspect/SKILL.md`: concise workflow for agents using the CLI.
- Create `skills/flog-inspect/agents/openai.yaml`: UI metadata for Codex skill discovery.

---

### Task 1: Clap Surface and Duration Parsing

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/commands/mod.rs`
- Create: `src/commands/ai/mod.rs`
- Create: `src/commands/ai/args.rs`
- Test: `src/cli.rs`
- Test: `src/commands/ai/args_tests.rs`

- [ ] **Step 1: Add failing CLI parse tests**

Add tests in `src/cli.rs`:

```rust
#[test]
fn cli_ai_snapshot_defaults_parse() {
    let cli = Cli::parse_from(["flog", "ai", "snapshot"]);
    assert!(matches!(cli.command, Some(Command::Ai(AiCommand::Snapshot(_)))));
}

#[test]
fn cli_ai_snapshot_screenshot_parse() {
    let cli = Cli::parse_from(["flog", "ai", "snapshot", "--screenshot"]);
    let Some(Command::Ai(AiCommand::Snapshot(args))) = cli.command else {
        panic!("expected ai snapshot command");
    };
    assert!(args.screenshot);
}

#[test]
fn cli_ai_get_parse() {
    let cli = Cli::parse_from(["flog", "ai", "get", "net#42", "--body"]);
    let Some(Command::Ai(AiCommand::Get(args))) = cli.command else {
        panic!("expected ai get command");
    };
    assert_eq!(args.id, "net#42");
    assert!(args.body);
}

#[test]
fn cli_ai_watch_duration_parse() {
    let cli = Cli::parse_from(["flog", "ai", "watch", "--duration", "30s"]);
    let Some(Command::Ai(AiCommand::Watch(args))) = cli.command else {
        panic!("expected ai watch command");
    };
    assert_eq!(args.duration.as_millis(), 30_000);
}
```

- [ ] **Step 2: Add failing duration parser tests**

Create `src/commands/ai/args_tests.rs`:

```rust
use super::*;
use std::time::Duration;

#[test]
fn parse_duration_accepts_ms_seconds_and_minutes() {
    assert_eq!(parse_duration("750ms").unwrap(), Duration::from_millis(750));
    assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
    assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
}

#[test]
fn parse_duration_rejects_empty_unitless_and_unknown_units() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("500").is_err());
    assert!(parse_duration("1h").is_err());
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cargo test cli_ai_ parse_duration_ -- --nocapture
```

Expected: tests fail because `AiCommand` and `parse_duration` do not exist.

- [ ] **Step 4: Implement clap types and parser**

In `src/cli.rs`, replace the `Command` enum with this shape and import `Duration`:

```rust
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Update,
    Uninstall,
    Doctor,
    Devices,
    Ai(AiCommand),
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub enum AiCommand {
    Snapshot(AiSnapshotArgs),
    Watch(AiWatchArgs),
    Get(AiGetArgs),
    Doctor(AiDoctorArgs),
    Screenshot(AiScreenshotArgs),
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFormat {
    Json,
    Ndjson,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiSelectArgs {
    #[arg(long)]
    pub device: Option<String>,
    #[arg(long)]
    pub app: Option<String>,
    #[arg(long, default_value = "9753")]
    pub port: u16,
    #[arg(long, value_parser = crate::commands::ai::args::parse_duration, default_value = "5s")]
    pub wait: Duration,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiSnapshotArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, default_value = "300")]
    pub last: usize,
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
    #[arg(long, value_parser = crate::commands::ai::args::parse_duration, default_value = "750ms")]
    pub settle: Duration,
    #[arg(long)]
    pub errors: bool,
    #[arg(long)]
    pub network: bool,
    #[arg(long)]
    pub sse: bool,
    #[arg(long)]
    pub ws: bool,
    #[arg(long)]
    pub include_headers: bool,
    #[arg(long)]
    pub include_body: bool,
    #[arg(long)]
    pub no_redact: bool,
    #[arg(long)]
    pub screenshot: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiWatchArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, value_parser = crate::commands::ai::args::parse_duration, default_value = "30s")]
    pub duration: Duration,
    #[arg(long, value_enum, default_value_t = AiFormat::Ndjson)]
    pub format: AiFormat,
    #[arg(long)]
    pub errors: bool,
    #[arg(long)]
    pub network: bool,
    #[arg(long)]
    pub since: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiGetArgs {
    pub id: String,
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long)]
    pub chunks: bool,
    #[arg(long)]
    pub body: bool,
    #[arg(long)]
    pub stacktrace: bool,
    #[arg(long)]
    pub no_redact: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiDoctorArgs {
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct AiScreenshotArgs {
    #[command(flatten)]
    pub select: AiSelectArgs,
    #[arg(long, value_enum, default_value_t = AiFormat::Json)]
    pub format: AiFormat,
    #[arg(long)]
    pub out: Option<String>,
}
```

In `src/commands/ai/args.rs`:

```rust
use std::time::Duration;

pub fn parse_duration(input: &str) -> Result<Duration, String> {
    let Some((number, unit)) = split_duration(input) else {
        return Err("duration must use ms, s, or m suffix".to_string());
    };
    let value = number
        .parse::<u64>()
        .map_err(|_| format!("invalid duration value '{number}'"))?;
    match unit {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => Ok(Duration::from_secs(value)),
        "m" => Ok(Duration::from_secs(value * 60)),
        _ => Err(format!("invalid duration unit '{unit}', use ms/s/m")),
    }
}

fn split_duration(input: &str) -> Option<(&str, &str)> {
    if input.is_empty() {
        return None;
    }
    let unit_start = input
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(input.len());
    if unit_start == 0 || unit_start == input.len() {
        return None;
    }
    Some(input.split_at(unit_start))
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
```

In `src/commands/ai/mod.rs`:

```rust
pub mod args;

use std::io;

use crate::cli::AiCommand;

pub async fn run(command: AiCommand) -> io::Result<()> {
    match command {
        AiCommand::Snapshot(_) => Ok(()),
        AiCommand::Watch(_) => Ok(()),
        AiCommand::Get(_) => Ok(()),
        AiCommand::Doctor(_) => Ok(()),
        AiCommand::Screenshot(_) => Ok(()),
    }
}
```

In `src/commands/mod.rs`:

```rust
pub(crate) mod ai;
```

and add the match arm:

```rust
Command::Ai(command) => ai::run(command).await,
```

- [ ] **Step 5: Run tests to verify pass**

Run:

```bash
cargo test cli_ai_ parse_duration_ -- --nocapture
```

Expected: all new tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs src/commands/mod.rs src/commands/ai/mod.rs src/commands/ai/args.rs src/commands/ai/args_tests.rs
git commit -m "feat: add ai command surface"
```

---

### Task 2: JSON Envelope, Errors, Redaction, and Truncation

**Files:**
- Create: `src/commands/ai/output.rs`
- Create: `src/commands/ai/output_tests.rs`
- Create: `src/commands/ai/redact.rs`
- Create: `src/commands/ai/redact_tests.rs`
- Modify: `src/commands/ai/mod.rs`

- [ ] **Step 1: Add failing output tests**

Create `src/commands/ai/output_tests.rs`:

```rust
use super::*;

#[test]
fn error_envelope_serializes_code_message_and_next_actions() {
    let json = serde_json::to_value(AiEnvelope::error(
        "snapshot",
        AiError::new(
            AiErrorCode::NoFlogAppFound,
            "No flog_dart app responded on ports 9753-9762 within 5s.",
            vec!["Run `flog ai doctor --format json`".to_string()],
        ),
    ))
    .unwrap();

    assert_eq!(json["ok"], false);
    assert_eq!(json["meta"]["schema_version"], 1);
    assert_eq!(json["error"]["code"], "no_flog_app_found");
    assert_eq!(
        json["error"]["next_actions"][0],
        "Run `flog ai doctor --format json`"
    );
}

#[test]
fn success_envelope_omits_error() {
    let payload = SnapshotPayload::empty_for_tests();
    let json = serde_json::to_value(AiEnvelope::snapshot(payload)).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json.get("error").is_none());
}
```

- [ ] **Step 2: Add failing redaction tests**

Create `src/commands/ai/redact_tests.rs`:

```rust
use super::*;

#[test]
fn redact_headers_hides_sensitive_keys_case_insensitively() {
    let value = serde_json::json!({
        "Authorization": "Bearer abc",
        "content-type": "application/json",
        "X-Api-Key": "secret"
    });

    let redacted = redact_json_value(&value);

    assert_eq!(redacted["Authorization"], "[redacted]");
    assert_eq!(redacted["content-type"], "application/json");
    assert_eq!(redacted["X-Api-Key"], "[redacted]");
}

#[test]
fn redact_body_hides_nested_secret_keys() {
    let value = serde_json::json!({
        "user": {"token": "abc"},
        "items": [{"password": "pw"}],
        "ok": true
    });

    let redacted = redact_json_value(&value);

    assert_eq!(redacted["user"]["token"], "[redacted]");
    assert_eq!(redacted["items"][0]["password"], "[redacted]");
    assert_eq!(redacted["ok"], true);
}

#[test]
fn preview_text_truncates_and_reports_original_bytes() {
    let preview = preview_text("abcdef", 4);
    assert_eq!(preview.preview, "abcd");
    assert!(preview.truncated);
    assert_eq!(preview.original_bytes, 6);
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cargo test ai::output ai::redact -- --nocapture
```

Expected: tests fail because modules and types are missing.

- [ ] **Step 4: Implement output structs**

In `src/commands/ai/mod.rs`, add:

```rust
mod output;
mod redact;
```

In `src/commands/ai/output.rs`:

```rust
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
```

- [ ] **Step 5: Implement redaction helpers**

In `src/commands/ai/redact.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextPreview {
    pub present: bool,
    pub preview: String,
    pub truncated: bool,
    pub original_bytes: usize,
    pub redacted: bool,
}

pub fn redact_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), serde_json::Value::String("[redacted]".to_string()));
                } else {
                    out.insert(key.clone(), redact_json_value(value));
                }
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(redact_json_value).collect())
        }
        _ => value.clone(),
    }
}

pub fn preview_text(input: &str, max_chars: usize) -> TextPreview {
    let original_bytes = input.len();
    let preview: String = input.chars().take(max_chars).collect();
    let truncated = preview.len() < input.len();
    TextPreview {
        present: true,
        preview,
        truncated,
        original_bytes,
        redacted: false,
    }
}

pub fn redact_text_patterns(input: &str) -> String {
    static BEARER_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]+").unwrap()
    });
    BEARER_RE.replace_all(input, "Bearer [redacted]").to_string()
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    matches!(key.as_str(), "authorization" | "cookie" | "set-cookie" | "x-api-key")
        || key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("api_key")
        || key.contains("apikey")
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
```

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test ai::output ai::redact -- --nocapture
```

Expected: all new tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/output.rs src/commands/ai/output_tests.rs src/commands/ai/redact.rs src/commands/ai/redact_tests.rs
git commit -m "feat: add ai json output helpers"
```

---

### Task 3: Pure Notable Diagnostics

**Files:**
- Create: `src/domain/diagnostics.rs`
- Create: `src/domain/diagnostics_tests.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Add failing diagnostics tests**

Create `src/domain/diagnostics_tests.rs`:

```rust
use super::*;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use crate::domain::{LogEntry, LogLevel};

#[test]
fn diagnostics_include_error_logs() {
    let logs = vec![LogEntry::new(LogLevel::Error, "Repo", "failed".to_string())];
    let items = collect_notable(&logs, &[]);
    assert_eq!(items[0].kind, "error_log");
    assert_eq!(items[0].severity, DiagnosticSeverity::Error);
    assert_eq!(items[0].evidence, vec!["log#0"]);
}

#[test]
fn diagnostics_include_failed_http_status() {
    let mut entry = NetworkEntry::new_http(42, "GET".to_string(), "/x".to_string(), String::new());
    entry.status = NetworkStatus::Completed;
    entry.http_status = Some(500);

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "http_error_status");
    assert_eq!(items[0].evidence, vec!["net#42"]);
}

#[test]
fn diagnostics_include_completed_empty_sse_merge() {
    let mut entry = NetworkEntry::new_sse(7, "POST".to_string(), "/sse".to_string(), String::new());
    entry.status = NetworkStatus::Completed;
    entry.sse_chunks.push(crate::domain::network::SseChunk {
        data: "{\"choices\":[{\"delta\":{\"content\":\"\"}}]}".to_string(),
    });

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "completed_empty_sse_merge");
    assert_eq!(items[0].severity, DiagnosticSeverity::Warning);
}

#[test]
fn diagnostics_include_abnormal_ws_close() {
    let mut entry = NetworkEntry::new_ws(9, "wss://x".to_string(), String::new());
    entry.protocol = Protocol::Ws;
    entry.ws_close_code = Some(1006);

    let items = collect_notable(&[], &[entry]);

    assert_eq!(items[0].kind, "websocket_abnormal_close");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test diagnostics_ -- --nocapture
```

Expected: tests fail because `diagnostics` module is missing.

- [ ] **Step 3: Implement diagnostics**

In `src/domain/mod.rs`, add:

```rust
pub mod diagnostics;
```

In `src/domain/diagnostics.rs`:

```rust
//! Pure AI-oriented diagnostics over logs and network entries.
//!
//! This module has no UI dependencies. It produces stable evidence ids that
//! command-layer JSON can serialize for agents.

use serde::Serialize;

use crate::domain::entry::{LogEntry, LogLevel};
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use crate::domain::sse_merge;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NotableDiagnostic {
    pub id: String,
    pub severity: DiagnosticSeverity,
    pub kind: String,
    pub message: String,
    pub evidence: Vec<String>,
    pub next_actions: Vec<String>,
}

pub fn collect_notable(logs: &[LogEntry], network: &[NetworkEntry]) -> Vec<NotableDiagnostic> {
    let mut out = Vec::new();
    for (idx, log) in logs.iter().enumerate() {
        if matches!(log.level, LogLevel::Error) {
            out.push(item(
                format!("diag#log-error-{idx}"),
                DiagnosticSeverity::Error,
                "error_log",
                format!("Error log from tag {}", log.tag),
                vec![format!("log#{idx}")],
            ));
        } else if matches!(log.level, LogLevel::Warning) {
            out.push(item(
                format!("diag#log-warning-{idx}"),
                DiagnosticSeverity::Warning,
                "warning_log",
                format!("Warning log from tag {}", log.tag),
                vec![format!("log#{idx}")],
            ));
        }
    }

    for entry in network {
        if matches!(entry.status, NetworkStatus::Failed) {
            out.push(item(
                format!("diag#net-failed-{}", entry.id),
                DiagnosticSeverity::Error,
                "network_failed",
                format!("Network request failed: {}", entry.url),
                vec![format!("net#{}", entry.id)],
            ));
        }
        if matches!(entry.status, NetworkStatus::Orphan) {
            out.push(item(
                format!("diag#net-orphan-{}", entry.id),
                DiagnosticSeverity::Warning,
                "orphan_response",
                "Response arrived without a matching request".to_string(),
                vec![format!("net#{}", entry.id)],
            ));
        }
        if let Some(status) = entry.http_status {
            if status >= 400 {
                out.push(item(
                    format!("diag#http-status-{}", entry.id),
                    DiagnosticSeverity::Error,
                    "http_error_status",
                    format!("HTTP request returned status {status}"),
                    vec![format!("net#{}", entry.id)],
                ));
            }
        }
        if entry.protocol == Protocol::Sse {
            append_sse_diagnostics(&mut out, entry);
        }
        if entry.protocol == Protocol::Ws {
            append_ws_diagnostics(&mut out, entry);
        }
    }
    out
}

fn append_sse_diagnostics(out: &mut Vec<NotableDiagnostic>, entry: &NetworkEntry) {
    if entry.status == NetworkStatus::Active && !entry.sse_chunks.is_empty() {
        out.push(item(
            format!("diag#sse-active-{}", entry.id),
            DiagnosticSeverity::Info,
            "active_sse_with_chunks",
            "SSE stream has chunks but did not finish during collection".to_string(),
            vec![format!("net#{}", entry.id)],
        ));
    }

    if entry.status == NetworkStatus::Completed && !entry.sse_chunks.is_empty() {
        let path = sse_merge::auto_detect_field(&entry.sse_chunks);
        if let Some(path) = path {
            let merged = sse_merge::merge_field(&entry.sse_chunks, &path);
            if merged.trim().is_empty() {
                out.push(item(
                    format!("diag#sse-empty-{}", entry.id),
                    DiagnosticSeverity::Warning,
                    "completed_empty_sse_merge",
                    "SSE completed but the auto-detected merged text is empty".to_string(),
                    vec![format!("net#{}", entry.id)],
                ));
            }
        }
    }
}

fn append_ws_diagnostics(out: &mut Vec<NotableDiagnostic>, entry: &NetworkEntry) {
    if let Some(code) = entry.ws_close_code {
        if code != 1000 && code != 1001 {
            out.push(item(
                format!("diag#ws-close-{}", entry.id),
                DiagnosticSeverity::Warning,
                "websocket_abnormal_close",
                format!("WebSocket closed with abnormal code {code}"),
                vec![format!("net#{}", entry.id)],
            ));
        }
    }
}

fn item(
    id: String,
    severity: DiagnosticSeverity,
    kind: &str,
    message: String,
    evidence: Vec<String>,
) -> NotableDiagnostic {
    NotableDiagnostic {
        id,
        severity,
        kind: kind.to_string(),
        message,
        evidence,
        next_actions: Vec::new(),
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test diagnostics_ -- --nocapture
```

Expected: all diagnostics tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/domain/mod.rs src/domain/diagnostics.rs src/domain/diagnostics_tests.rs
git commit -m "feat: add ai notable diagnostics"
```

---

### Task 4: Snapshot Serialization From App State

**Files:**
- Create: `src/commands/ai/snapshot.rs`
- Create: `src/commands/ai/snapshot_tests.rs`
- Modify: `src/commands/ai/mod.rs`
- Modify: `src/commands/ai/output.rs`

- [ ] **Step 1: Add failing snapshot tests**

Create `src/commands/ai/snapshot_tests.rs`:

```rust
use super::*;
use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus};
use crate::domain::{LogEntry, LogLevel};

#[test]
fn build_snapshot_counts_logs_errors_and_network() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Error, "Repo", "failed".to_string()));
    let mut net = NetworkEntry::new_http(42, "GET".to_string(), "/x".to_string(), String::new());
    net.status = NetworkStatus::Failed;
    app.network_store.push_entry(net);

    let snapshot = build_snapshot(&app, SnapshotBuildOptions::for_tests());

    assert_eq!(snapshot.summary.logs, 1);
    assert_eq!(snapshot.summary.errors, 1);
    assert_eq!(snapshot.summary.network, 1);
    assert_eq!(snapshot.summary.failed_requests, 1);
    assert_eq!(snapshot.logs[0]["id"], "log#0");
    assert_eq!(snapshot.network[0]["id"], "net#42");
    assert_eq!(snapshot.notable[0]["kind"], "error_log");
}

#[test]
fn build_snapshot_respects_last_limit() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Info, "A", "one".to_string()));
    app.add_entry(LogEntry::new(LogLevel::Info, "A", "two".to_string()));

    let snapshot = build_snapshot(
        &app,
        SnapshotBuildOptions {
            last: 1,
            ..SnapshotBuildOptions::for_tests()
        },
    );

    assert_eq!(snapshot.logs.len(), 1);
    assert_eq!(snapshot.logs[0]["message"], "two");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test ai::snapshot -- --nocapture
```

Expected: tests fail because `snapshot` module is missing.

- [ ] **Step 3: Implement snapshot builder**

In `src/commands/ai/mod.rs`, add:

```rust
mod snapshot;
```

In `src/commands/ai/snapshot.rs`:

```rust
use crate::app::App;
use crate::domain::diagnostics::collect_notable;
use crate::domain::network::{NetworkStatus, Protocol};

use super::output::{CollectionMeta, SnapshotPayload, Summary};
use super::redact::preview_text;

#[derive(Debug, Clone)]
pub struct SnapshotBuildOptions {
    pub last: usize,
    pub include_headers: bool,
    pub include_body: bool,
    pub redact: bool,
    pub ports_scanned: Vec<u16>,
    pub wait_ms: u64,
    pub settle_ms: u64,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn build_snapshot(app: &App, options: SnapshotBuildOptions) -> SnapshotPayload {
    let logs_all = app.store.iter().cloned().collect::<Vec<_>>();
    let network_all = app.network_store.iter().cloned().collect::<Vec<_>>();
    let logs_slice = tail(&logs_all, options.last);

    let notable = collect_notable(&logs_all, &network_all)
        .into_iter()
        .map(|item| serde_json::to_value(item).unwrap_or(serde_json::Value::Null))
        .collect();

    SnapshotPayload {
        app: None,
        collection: CollectionMeta {
            ports_scanned: options.ports_scanned,
            wait_ms: options.wait_ms,
            settle_ms: options.settle_ms,
            complete: options.complete,
            warnings: options.warnings,
        },
        summary: summarize(&logs_all, &network_all),
        notable,
        logs: logs_slice
            .iter()
            .enumerate()
            .map(|(offset, log)| {
                let absolute = logs_all.len().saturating_sub(logs_slice.len()) + offset;
                serde_json::json!({
                    "id": format!("log#{absolute}"),
                    "timestamp": log.timestamp,
                    "level": log.level.as_str(),
                    "tag": log.tag,
                    "message": log.message,
                    "stacktrace": log.stacktrace.as_ref().map(|s| preview_text(s, 800)),
                    "repeat_count": log.repeat_count,
                })
            })
            .collect(),
        network: network_all
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "id": format!("net#{}", entry.id),
                    "protocol": protocol_label(entry.protocol),
                    "method": entry.method,
                    "url": entry.url,
                    "status": entry.http_status,
                    "network_status": status_label(entry.status),
                    "duration_ms": entry.duration,
                    "request": {
                        "headers": headers_value(entry.request_headers.as_deref(), options.include_headers),
                        "body": body_value(entry.request_body.as_deref(), options.include_body),
                    },
                    "response": {
                        "headers": headers_value(entry.response_headers.as_deref(), options.include_headers),
                        "body": body_value(entry.response_body.as_deref(), options.include_body),
                    },
                    "sse": if entry.protocol == Protocol::Sse {
                        serde_json::json!({"chunks": entry.sse_chunks.len()})
                    } else {
                        serde_json::Value::Null
                    },
                })
            })
            .collect(),
        screenshot: None,
        diagnostics: Vec::new(),
    }
}

fn summarize(
    logs: &[crate::domain::LogEntry],
    network: &[crate::domain::network::NetworkEntry],
) -> Summary {
    Summary {
        logs: logs.len(),
        errors: logs
            .iter()
            .filter(|log| matches!(log.level, crate::domain::LogLevel::Error))
            .count(),
        warnings: logs
            .iter()
            .filter(|log| matches!(log.level, crate::domain::LogLevel::Warning))
            .count(),
        network: network.len(),
        failed_requests: network
            .iter()
            .filter(|entry| {
                matches!(entry.status, NetworkStatus::Failed)
                    || entry.http_status.is_some_and(|status| status >= 400)
            })
            .count(),
        active_sse: network
            .iter()
            .filter(|entry| entry.protocol == Protocol::Sse && entry.status == NetworkStatus::Active)
            .count(),
        websockets: network.iter().filter(|entry| entry.protocol == Protocol::Ws).count(),
    }
}

fn tail<T>(items: &[T], limit: usize) -> &[T] {
    let start = items.len().saturating_sub(limit);
    &items[start..]
}

fn protocol_label(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Http => "http",
        Protocol::Sse => "sse",
        Protocol::Ws => "ws",
    }
}

fn status_label(status: NetworkStatus) -> &'static str {
    match status {
        NetworkStatus::Pending => "pending",
        NetworkStatus::Connecting => "connecting",
        NetworkStatus::Active => "active",
        NetworkStatus::Completed => "completed",
        NetworkStatus::Failed => "failed",
        NetworkStatus::Orphan => "orphan",
    }
}

fn headers_value(headers: Option<&str>, include: bool) -> serde_json::Value {
    if include {
        headers
            .and_then(|h| serde_json::from_str(h).ok())
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::String("redacted".to_string())
    }
}

fn body_value(body: Option<&str>, include: bool) -> serde_json::Value {
    match (body, include) {
        (Some(body), true) => serde_json::to_value(preview_text(body, 1200)).unwrap(),
        (Some(_), false) => serde_json::json!({"present": true}),
        (None, _) => serde_json::json!({"present": false}),
    }
}

impl SnapshotBuildOptions {
    #[cfg(test)]
    pub fn for_tests() -> Self {
        Self {
            last: 300,
            include_headers: false,
            include_body: false,
            redact: true,
            ports_scanned: vec![9753],
            wait_ms: 5000,
            settle_ms: 750,
            complete: true,
            warnings: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run snapshot tests**

Run:

```bash
cargo test ai::snapshot -- --nocapture
```

Expected: snapshot tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/snapshot.rs src/commands/ai/snapshot_tests.rs
git commit -m "feat: build ai snapshot payloads"
```

---

### Task 5: Headless Snapshot Session

**Files:**
- Create: `src/commands/ai/session.rs`
- Create: `src/commands/ai/session_tests.rs`
- Modify: `src/commands/ai/mod.rs`
- Modify: `src/commands/ai/output.rs`
- Modify: `src/run/dispatch.rs`

- [ ] **Step 1: Expose dispatch for command layer**

Change `src/run/dispatch.rs`:

```rust
pub(crate) fn dispatch_client_message(app: &mut App, msg: ClientMessage) {
```

This is already `pub(crate)` in the current source; if it is not, make it
`pub(crate)` so `commands::ai::session` can reuse the same ingest logic.

- [ ] **Step 2: Add failing session selection tests**

Create `src/commands/ai/session_tests.rs`:

```rust
use super::*;

#[test]
fn select_single_app_accepts_one_candidate() {
    let candidates = vec![AiAppCandidate::for_tests("app-a", "Device")];
    let selected = select_candidate(&candidates, None, None).unwrap();
    assert_eq!(selected.app_id, "app-a");
}

#[test]
fn select_multiple_apps_requires_selector() {
    let candidates = vec![
        AiAppCandidate::for_tests("app-a", "Device A"),
        AiAppCandidate::for_tests("app-b", "Device B"),
    ];
    let err = select_candidate(&candidates, None, None).unwrap_err();
    assert!(matches!(err.code, super::output::AiErrorCode::MultipleAppsFound));
}

#[test]
fn select_app_matches_name_package_or_id() {
    let candidates = vec![AiAppCandidate {
        app_id: "local:9753".to_string(),
        app_name: "Demo".to_string(),
        package_name: "com.example.demo".to_string(),
        device_id: "dev-1".to_string(),
        device_name: "Device".to_string(),
        port: 9753,
    }];
    assert!(select_candidate(&candidates, Some("Demo"), None).is_ok());
    assert!(select_candidate(&candidates, Some("com.example.demo"), None).is_ok());
    assert!(select_candidate(&candidates, Some("local:9753"), None).is_ok());
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cargo test ai::session -- --nocapture
```

Expected: tests fail because session module is missing.

- [ ] **Step 4: Implement candidate selection and temporary no-app collection**

In `src/commands/ai/mod.rs`, add:

```rust
mod session;
```

In `src/commands/ai/session.rs`:

```rust
use std::time::{Duration, Instant};

use crate::app::App;
use crate::commands::ai::output::{AiError, AiErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAppCandidate {
    pub app_id: String,
    pub app_name: String,
    pub package_name: String,
    pub device_id: String,
    pub device_name: String,
    pub port: u16,
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
    _base_port: u16,
    _deadline: Instant,
) -> Result<Vec<AiAppCandidate>, AiError> {
    Err(AiError::new(
        AiErrorCode::NoFlogAppFound,
        "No flog_dart app responded on scanned ports.",
        vec![
            "Run `flog ai doctor --format json`".to_string(),
            "Check that Flog.init() is called before runApp()".to_string(),
        ],
    ))
}

async fn collect_app_frames(
    _candidate: &AiAppCandidate,
    _deadline: Instant,
    _settle: Duration,
) -> Result<App, AiError> {
    Ok(App::new())
}

impl AiAppCandidate {
    #[cfg(test)]
    pub fn for_tests(app_id: &str, device_name: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            app_name: app_id.to_string(),
            package_name: format!("com.example.{app_id}"),
            device_id: device_name.to_string(),
            device_name: device_name.to_string(),
            port: 9753,
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
```

- [ ] **Step 5: Implement transport probing**

In `discover_candidates`, implement the same pattern as `src/commands/devices.rs`:

```rust
let mut rx = crate::transport::start_discovery(_base_port);
let mut devices = Vec::new();
while Instant::now() < _deadline {
    let remaining = _deadline.saturating_duration_since(Instant::now());
    match tokio::time::timeout(remaining.min(Duration::from_millis(250)), rx.recv()).await {
        Ok(Some(crate::transport::DeviceEvent::Added(device)))
        | Ok(Some(crate::transport::DeviceEvent::Updated(device))) => devices.push(device),
        Ok(Some(crate::transport::DeviceEvent::Removed(_))) => {}
        Ok(None) => break,
        Err(_) => {}
    }
}
```

For each device and port, call a helper `probe_candidate(&device, port)` that mirrors
`commands::devices::probe_one`: resolve `TransportAddr`, call `connect` or
`connect_stream`, read `ConnectorEvent::Connected(info)`, remove adb forwards, and
return `AiAppCandidate`.

- [ ] **Step 6: Implement frame collection**

Implement `collect_app_frames` to connect to the selected candidate, create `App::new()`,
send `ServerMessage::Subscribe`, dispatch incoming `ConnectorEvent::Message(msg)` through
`run::dispatch::dispatch_client_message`, and exit when no frame arrives for `settle` or
the deadline expires.

Use this loop shape:

```rust
let mut app = App::new();
let mut last_frame = Instant::now();
handle.send(crate::input::ServerMessage::Subscribe {});
loop {
    let now = Instant::now();
    if now >= _deadline || now.duration_since(last_frame) >= _settle {
        break;
    }
    let remaining = _deadline.saturating_duration_since(now);
    let tick = remaining.min(_settle.saturating_sub(now.duration_since(last_frame)));
    match tokio::time::timeout(tick, event_rx.recv()).await {
        Ok(Some(crate::input::ConnectorEvent::Message(msg))) => {
            crate::run::dispatch::dispatch_client_message(&mut app, msg);
            last_frame = Instant::now();
        }
        Ok(Some(crate::input::ConnectorEvent::Disconnected { .. })) | Ok(None) => break,
        Ok(Some(crate::input::ConnectorEvent::Connected(_))) => {}
        Err(_) => break,
    }
}
```

- [ ] **Step 7: Wire snapshot command to JSON output**

In `src/commands/ai/mod.rs`, dispatch snapshot:

```rust
crate::cli::AiCommand::Snapshot(args) => {
    let result = session::collect_snapshot_session(
        args.select.port,
        args.select.wait,
        args.settle,
        args.select.app.as_deref(),
        args.select.device.as_deref(),
    )
    .await;
    match result {
        Ok(session) => {
            let payload = snapshot::build_snapshot(
                &session.app,
                snapshot::SnapshotBuildOptions {
                    last: args.last,
                    include_headers: args.include_headers,
                    include_body: args.include_body,
                    redact: !args.no_redact,
                    ports_scanned: session.ports_scanned,
                    wait_ms: args.select.wait.as_millis() as u64,
                    settle_ms: args.settle.as_millis() as u64,
                    complete: session.complete,
                    warnings: session.warnings,
                },
            );
            print_json(&output::AiEnvelope::snapshot(payload))
        }
        Err(error) => print_json(&output::AiEnvelope::error("snapshot", error)),
    }
}
```

Add:

```rust
fn print_json<T: serde::Serialize>(value: &T) -> std::io::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
```

- [ ] **Step 8: Run tests and manual no-app smoke**

Run:

```bash
cargo test ai::session ai::snapshot -- --nocapture
cargo run -- ai snapshot --wait 1s --format json
```

Expected: tests pass. The manual smoke returns JSON with `ok=false` and
`no_flog_app_found` when no app is running.

- [ ] **Step 9: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/session.rs src/commands/ai/session_tests.rs src/commands/ai/snapshot.rs src/run/dispatch.rs
git commit -m "feat: collect ai snapshots headlessly"
```

---

### Task 6: `get` and `watch`

**Files:**
- Create: `src/commands/ai/get.rs`
- Create: `src/commands/ai/get_tests.rs`
- Create: `src/commands/ai/watch.rs`
- Modify: `src/commands/ai/mod.rs`

- [ ] **Step 1: Add failing record id tests**

Create `src/commands/ai/get_tests.rs`:

```rust
use super::*;

#[test]
fn parse_record_id_accepts_log_net_and_chunk() {
    assert_eq!(parse_record_id("log#12").unwrap(), RecordId::Log(12));
    assert_eq!(parse_record_id("net#42").unwrap(), RecordId::Net(42));
    assert_eq!(parse_record_id("chunk#42.13").unwrap(), RecordId::Chunk { net_id: 42, chunk: 13 });
}

#[test]
fn parse_record_id_rejects_unknown_shape() {
    assert!(parse_record_id("request#1").is_err());
    assert!(parse_record_id("chunk#x.y").is_err());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test ai::get -- --nocapture
```

Expected: tests fail because `get` module is missing.

- [ ] **Step 3: Implement record id parsing and lookup**

In `src/commands/ai/mod.rs`, add:

```rust
mod get;
mod watch;
```

In `src/commands/ai/get.rs`:

```rust
use crate::app::App;
use crate::commands::ai::output::{AiError, AiErrorCode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordId {
    Log(usize),
    Net(u64),
    Chunk { net_id: u64, chunk: usize },
}

pub fn parse_record_id(input: &str) -> Result<RecordId, AiError> {
    if let Some(rest) = input.strip_prefix("log#") {
        return rest
            .parse()
            .map(RecordId::Log)
            .map_err(|_| record_not_found(input));
    }
    if let Some(rest) = input.strip_prefix("net#") {
        return rest
            .parse()
            .map(RecordId::Net)
            .map_err(|_| record_not_found(input));
    }
    if let Some(rest) = input.strip_prefix("chunk#") {
        let Some((net, chunk)) = rest.split_once('.') else {
            return Err(record_not_found(input));
        };
        return Ok(RecordId::Chunk {
            net_id: net.parse().map_err(|_| record_not_found(input))?,
            chunk: chunk.parse().map_err(|_| record_not_found(input))?,
        });
    }
    Err(record_not_found(input))
}

pub fn lookup_record(app: &App, id: &RecordId) -> Result<serde_json::Value, AiError> {
    match id {
        RecordId::Log(index) => app
            .store
            .get(*index)
            .map(|log| serde_json::json!({
                "id": format!("log#{index}"),
                "timestamp": log.timestamp,
                "level": log.level.as_str(),
                "tag": log.tag,
                "message": log.message,
                "stacktrace": log.stacktrace,
            }))
            .ok_or_else(|| record_not_found(&format!("log#{index}"))),
        RecordId::Net(net_id) => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .map(|entry| serde_json::to_value(entry).unwrap_or(serde_json::Value::Null))
            .ok_or_else(|| record_not_found(&format!("net#{net_id}"))),
        RecordId::Chunk { net_id, chunk } => app
            .network_store
            .iter()
            .find(|entry| entry.id == *net_id)
            .and_then(|entry| entry.sse_chunks.get(*chunk))
            .map(|chunk_value| serde_json::json!({
                "id": format!("chunk#{net_id}.{chunk}"),
                "data": chunk_value.data,
            }))
            .ok_or_else(|| record_not_found(&format!("chunk#{net_id}.{chunk}"))),
    }
}

fn record_not_found(id: &str) -> AiError {
    AiError::new(
        AiErrorCode::RecordNotFound,
        format!("Record {id} was not found in the replay buffer."),
        vec!["Run `flog ai snapshot --format json` to refresh ids.".to_string()],
    )
}

#[cfg(test)]
#[path = "get_tests.rs"]
mod tests;
```

- [ ] **Step 4: Wire `get` command**

In `src/commands/ai/mod.rs`, handle `AiCommand::Get(args)` by collecting a
session with `args.select`, parsing `args.id`, calling `lookup_record`, and
printing a success envelope with this payload:

```rust
#[derive(serde::Serialize)]
struct GetPayload {
    record: serde_json::Value,
}
```

- [ ] **Step 5: Implement bounded watch**

In `src/commands/ai/watch.rs`, implement a minimal bounded stream that connects
through `session`, dispatches incoming frames into an `App`, and prints one
NDJSON line per received `ConnectorEvent::Message`:

```rust
pub async fn run_watch(args: crate::cli::AiWatchArgs) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + args.duration;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    Ok(())
}
```

Then use the same connection helper used by snapshot collection. Each message
line must include:

```json
{"type":"message","received_at":"...","message":{...}}
```

The v1 stream serializes raw `ClientMessage`; summary transforms stay outside
this plan.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test ai::get -- --nocapture
cargo run -- ai watch --duration 1s --format ndjson
```

Expected: tests pass. Manual watch exits after one second.

- [ ] **Step 7: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/get.rs src/commands/ai/get_tests.rs src/commands/ai/watch.rs
git commit -m "feat: add ai get and watch"
```

---

### Task 7: Screenshot Command

**Files:**
- Create: `src/commands/ai/screenshot.rs`
- Create: `src/commands/ai/screenshot_tests.rs`
- Modify: `src/commands/ai/mod.rs`
- Modify: `src/commands/ai/snapshot.rs`

- [ ] **Step 1: Add failing screenshot command tests**

Create `src/commands/ai/screenshot_tests.rs`:

```rust
use super::*;

#[test]
fn flutter_screenshot_command_uses_device_and_output() {
    let command = flutter_screenshot_command("emulator-5554", "/tmp/out.png");
    assert_eq!(command.program, "flutter");
    assert_eq!(
        command.args,
        vec!["screenshot", "-d", "emulator-5554", "-o", "/tmp/out.png"]
    );
}

#[test]
fn default_screenshot_path_is_png_under_temp_dir() {
    let path = default_screenshot_path("emulator-5554");
    assert!(path.to_string_lossy().contains("flog-ai"));
    assert!(path.to_string_lossy().ends_with(".png"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test ai::screenshot -- --nocapture
```

Expected: tests fail because screenshot module is missing.

- [ ] **Step 3: Implement screenshot helpers**

In `src/commands/ai/mod.rs`, add:

```rust
mod screenshot;
```

In `src/commands/ai/screenshot.rs`:

```rust
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

pub async fn capture_with_flutter(
    device_id: &str,
    out: Option<PathBuf>,
) -> ScreenshotResult {
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
    let status = Command::new(&cmd.program).args(&cmd.args).status().await;
    match status {
        Ok(status) if status.success() && path.exists() => ScreenshotResult {
            ok: true,
            path: Some(path_str),
            source: "flutter_screenshot".to_string(),
            captured_at: chrono::Utc::now().to_rfc3339(),
            warning: Some(
                "Device screenshot may include content outside the Flutter app.".to_string(),
            ),
            error: None,
        },
        Ok(_) => failure(
            AiErrorCode::FlutterScreenshotFailed,
            format!("Flutter screenshot failed for device {device_id}."),
        ),
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
```

- [ ] **Step 4: Wire `ai screenshot`**

In `src/commands/ai/mod.rs`, handle `AiCommand::Screenshot(args)` by selecting
a device id. For v1, use `args.select.device` when provided; otherwise return
`AiErrorCode::NoDeviceFound` with next action `Run flutter devices`.

Print:

```rust
let result = screenshot::capture_with_flutter(&device_id, args.out.map(Into::into)).await;
print_json(&output::AiEnvelope::new(
    "screenshot",
    result.ok,
    screenshot::ScreenshotPayload { screenshot: result },
))
```

- [ ] **Step 5: Wire `snapshot --screenshot`**

After building snapshot payload in `AiCommand::Snapshot`, if `args.screenshot`
is true and session has a `candidate.device_id`, call `capture_with_flutter`
and set `payload.screenshot = Some(serde_json::to_value(result).unwrap())`.
If capture fails, keep the snapshot envelope `ok=true`.

- [ ] **Step 6: Run tests and smoke command**

Run:

```bash
cargo test ai::screenshot -- --nocapture
cargo run -- ai screenshot --device definitely-missing-device --format json
```

Expected: tests pass. Smoke command returns JSON with `screenshot.ok=false`.

- [ ] **Step 7: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/screenshot.rs src/commands/ai/screenshot_tests.rs src/commands/ai/snapshot.rs
git commit -m "feat: add ai screenshot support"
```

---

### Task 8: AI Doctor

**Files:**
- Create: `src/commands/ai/doctor.rs`
- Create: `src/commands/ai/doctor_tests.rs`
- Modify: `src/commands/ai/mod.rs`

- [ ] **Step 1: Add failing doctor tests**

Create `src/commands/ai/doctor_tests.rs`:

```rust
use super::*;

#[test]
fn port_range_defaults_to_ten_ports() {
    assert_eq!(port_range(9753), vec![9753, 9754, 9755, 9756, 9757, 9758, 9759, 9760, 9761, 9762]);
}

#[test]
fn command_status_maps_not_found() {
    let status = command_status_from_error(std::io::ErrorKind::NotFound);
    assert_eq!(status, CheckStatus::Missing);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test ai::doctor -- --nocapture
```

Expected: tests fail because doctor module is missing.

- [ ] **Step 3: Implement doctor checks**

In `src/commands/ai/mod.rs`, add:

```rust
mod doctor;
```

In `src/commands/ai/doctor.rs`:

```rust
use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Missing,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct DoctorPayload {
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

pub fn port_range(base: u16) -> Vec<u16> {
    (base..base + 10).collect()
}

pub fn command_status_from_error(kind: std::io::ErrorKind) -> CheckStatus {
    if kind == std::io::ErrorKind::NotFound {
        CheckStatus::Missing
    } else {
        CheckStatus::Failed
    }
}

pub async fn run_doctor() -> DoctorPayload {
    let mut checks = Vec::new();
    checks.push(command_check("flutter", &["--version"]).await);
    checks.push(command_check("adb", &["version"]).await);
    DoctorPayload { checks }
}

async fn command_check(program: &str, args: &[&str]) -> DoctorCheck {
    match Command::new(program).args(args).status().await {
        Ok(status) if status.success() => DoctorCheck {
            name: program.to_string(),
            status: CheckStatus::Ok,
            message: format!("{program} is available"),
        },
        Ok(status) => DoctorCheck {
            name: program.to_string(),
            status: CheckStatus::Failed,
            message: format!("{program} exited with status {status}"),
        },
        Err(e) => DoctorCheck {
            name: program.to_string(),
            status: command_status_from_error(e.kind()),
            message: e.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
```

- [ ] **Step 4: Wire `ai doctor`**

In `src/commands/ai/mod.rs`, handle `AiCommand::Doctor(_)`:

```rust
let payload = doctor::run_doctor().await;
print_json(&output::AiEnvelope::new("doctor", true, payload))
```

- [ ] **Step 5: Run tests and command**

Run:

```bash
cargo test ai::doctor -- --nocapture
cargo run -- ai doctor --format json
```

Expected: tests pass. Command prints checks for `flutter` and `adb`.

- [ ] **Step 6: Commit**

```bash
git add src/commands/ai/mod.rs src/commands/ai/doctor.rs src/commands/ai/doctor_tests.rs
git commit -m "feat: add ai doctor"
```

---

### Task 9: `flog-inspect` Skill

**Files:**
- Create: `skills/flog-inspect/SKILL.md`
- Create: `skills/flog-inspect/agents/openai.yaml`

- [ ] **Step 1: Create skill directory**

Run:

```bash
mkdir -p skills/flog-inspect/agents
```

- [ ] **Step 2: Create `SKILL.md`**

Write `skills/flog-inspect/SKILL.md`:

```markdown
---
name: flog-inspect
description: Use when a user asks an agent to inspect Flutter app logs, network traffic, SSE/WebSocket streams, current page state, screenshots, or debugging context through flog instead of copying from the TUI.
---

# flog Inspect

Use the `flog ai` CLI. Do not reimplement the flog wire protocol.

## Workflow

1. Decide if the request is visual. If the user mentions page, screen, UI, loading, button, layout, or current state, include `--screenshot`.
2. Run `flog ai snapshot --format json --last 300`. Add `--screenshot` for visual requests.
3. If `ok=false`, explain `error.code`, `message`, and `next_actions`.
4. If `ok=true`, inspect `summary`, `notable`, `logs`, `network`, and `screenshot`.
5. Use `flog ai get <id>` only for the smallest extra detail needed.
6. Do not use `--no-redact` unless the user explicitly approves exposing secrets.
7. Cite stable ids such as `log#188`, `net#42`, and `chunk#42.13`.
8. Separate visual observations from log/network conclusions.

## Commands

```bash
flog ai snapshot --format json --last 300
flog ai snapshot --format json --last 300 --screenshot
flog ai get net#42 --body
flog ai watch --duration 30s --format ndjson
flog ai doctor --format json
```
```

- [ ] **Step 3: Create `openai.yaml`**

Write `skills/flog-inspect/agents/openai.yaml`:

```yaml
display_name: flog Inspect
short_description: Inspect Flutter flog logs, network traffic, and screenshots.
default_prompt: Use flog to inspect the current Flutter app state and explain what is going wrong with cited log or network ids.
```

- [ ] **Step 4: Validate skill shape**

Run:

```bash
test -f skills/flog-inspect/SKILL.md
test -f skills/flog-inspect/agents/openai.yaml
rg -n "flog ai snapshot|flog ai get|--no-redact" skills/flog-inspect/SKILL.md
```

Expected: all commands exit 0 and `rg` prints matching skill lines.

- [ ] **Step 5: Commit**

```bash
git add skills/flog-inspect/SKILL.md skills/flog-inspect/agents/openai.yaml
git commit -m "feat: add flog inspect skill"
```

---

### Task 10: Final Verification and Documentation

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Add README examples**

Add a compact section to both README files:

```markdown
### AI inspection

`flog ai` provides a headless JSON interface for AI agents:

```bash
flog ai snapshot --format json --last 300
flog ai snapshot --format json --screenshot
flog ai get net#42 --body
flog ai doctor --format json
```

The output is read-only, redacted by default, and uses stable ids such as
`log#12` and `net#42` so an agent can cite evidence without copying from the
TUI.
```

- [ ] **Step 2: Run full Rust verification**

Run:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Expected: all commands pass.

- [ ] **Step 3: Run manual JSON smoke commands**

Run:

```bash
cargo run -- ai snapshot --wait 1s --format json
cargo run -- ai doctor --format json
cargo run -- ai watch --duration 1s --format ndjson
```

Expected:

- Snapshot prints a valid JSON envelope. Without a running app, `ok=false` and an actionable error.
- Doctor prints a valid JSON envelope with checks.
- Watch exits after one second.

- [ ] **Step 4: Commit**

```bash
git add README.md README_EN.md
git commit -m "docs: document ai inspection commands"
```

---

## Self-Review

Spec coverage:

- Headless CLI commands: Tasks 1, 5, 6, 7, 8.
- Stable JSON schema: Tasks 2 and 4.
- Redaction/truncation: Task 2, used by Task 4.
- Notable diagnostics: Task 3, used by Task 4.
- Screenshot through Flutter CLI: Task 7.
- Skill wrapper: Task 9.
- Failure envelopes and error codes: Tasks 2, 5, 7, 8.
- Tests and documentation: Tasks 1-10.

Implementation guardrails:

- No new code imports ratatui outside `ui/`.
- `domain/diagnostics.rs` depends only on domain types and serde.
- AI command modules stay under `src/commands/ai/` and do not become dependencies of lower layers.
- Write/control actions remain out of scope.
