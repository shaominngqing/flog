# CLI Maintenance Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `flog update`, `flog uninstall`, `flog doctor`, and `flog devices` with polished terminal output while preserving the current default TUI startup.

**Architecture:** Keep clap parsing in `src/cli.rs`, route subcommands from `src/main.rs`, and place command behavior in a new `src/commands/` directory. Shared terminal presentation lives in `src/commands/cli_ui.rs`; network/update, uninstall, doctor, and device probing each get focused modules.

**Tech Stack:** Rust 2021, clap, tokio, reqwest, serde_json, existing `input` and `transport` modules, std filesystem/process APIs.

---

## File Structure

- Modify `src/cli.rs` to add a `Command` enum and tests for bare subcommands.
- Modify `src/main.rs` to dispatch commands before entering raw-mode TUI.
- Create `src/commands/mod.rs` as the command dispatcher.
- Create `src/commands/cli_ui.rs` for symbols, colors, spinner, progress bar, and aligned status rows.
- Create `src/commands/update.rs` for latest-release lookup, asset naming, download, extraction, verification, and replacement.
- Create `src/commands/uninstall.rs` for deletion planning, confirmation, and removal.
- Create `src/commands/doctor.rs` for version/path/network/tool/port checks.
- Create `src/commands/devices.rs` for short device/app probing and output formatting.
- Update `README.md` and `README_EN.md` command examples after code is working.

## Task 1: Clap Subcommands and Main Dispatch

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Test: `src/cli.rs`

- [ ] **Step 1: Write failing CLI parsing tests**

Add tests in `src/cli.rs`:

```rust
#[test]
fn cli_update_subcommand() {
    let cli = Cli::parse_from(["flog", "update"]);
    assert_eq!(cli.command, Some(Command::Update));
}

#[test]
fn cli_uninstall_subcommand() {
    let cli = Cli::parse_from(["flog", "uninstall"]);
    assert_eq!(cli.command, Some(Command::Uninstall));
}

#[test]
fn cli_doctor_subcommand() {
    let cli = Cli::parse_from(["flog", "doctor"]);
    assert_eq!(cli.command, Some(Command::Doctor));
}

#[test]
fn cli_devices_subcommand() {
    let cli = Cli::parse_from(["flog", "devices"]);
    assert_eq!(cli.command, Some(Command::Devices));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test cli_ -- --nocapture
```

Expected: compile failure because `Command` and `Cli::command` do not exist.

- [ ] **Step 3: Add clap command enum**

In `src/cli.rs`, import `Subcommand`, add `Command`, and add an optional field:

```rust
use clap::{Parser, Subcommand};

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Update,
    Uninstall,
    Doctor,
    Devices,
}

#[derive(Parser, Debug)]
#[command(name = "flog", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Server port for flog_dart connections
    #[arg(long, default_value = "9753")]
    pub port: u16,

    /// Initial minimum log level (v/d/i/w/e)
    #[arg(long, value_parser = parse_level)]
    pub level: Option<crate::domain::LogLevel>,

    /// Initial tag filter
    #[arg(long)]
    pub tag: Option<String>,
}
```

- [ ] **Step 4: Add command module and dispatch stub**

In `src/main.rs`, add `mod commands;` near the other modules.

After `let cli = Cli::parse();`, add:

```rust
    if let Some(command) = cli.command {
        return commands::run(command).await;
    }
```

Create `src/commands/mod.rs`:

```rust
//! Non-TUI maintenance commands.

use std::io;

use crate::cli::Command;

mod cli_ui;
mod devices;
mod doctor;
mod uninstall;
mod update;

pub async fn run(command: Command) -> io::Result<()> {
    match command {
        Command::Update => update::run().await,
        Command::Uninstall => uninstall::run().await,
        Command::Doctor => doctor::run().await,
        Command::Devices => devices::run().await,
    }
}
```

Create temporary stub modules returning `Ok(())` so the project compiles.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test cli_ -- --nocapture
```

Expected: parsing tests pass.

## Task 2: Shared CLI UI Components

**Files:**
- Create/Modify: `src/commands/cli_ui.rs`
- Test: `src/commands/cli_ui.rs`

- [ ] **Step 1: Write tests for plain output decisions**

In `src/commands/cli_ui.rs`, add tests for pure helpers:

```rust
#[test]
fn progress_bar_renders_expected_width() {
    assert_eq!(progress_bar(50, 100, 10, false), "━━━━━░░░░░");
}

#[test]
fn progress_bar_handles_zero_total() {
    assert_eq!(progress_bar(10, 0, 6, false), "░░░░░░");
}

#[test]
fn status_line_aligns_label() {
    assert_eq!(status_line_plain("✓", "Version", "v0.5.2"), "    ✓ Version  v0.5.2");
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test commands::cli_ui -- --nocapture
```

Expected: compile failure because helpers do not exist.

- [ ] **Step 3: Implement minimal UI helper**

Implement:

```rust
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub struct CliUi {
    color: bool,
}

impl CliUi {
    pub fn new() -> Self {
        Self {
            color: colors_enabled(),
        }
    }

    pub fn title(&self, text: &str) {
        println!("{}", self.paint("◆", MAUVE, true).to_string() + " " + text);
        println!();
    }

    pub fn section(&self, text: &str) {
        println!("  {} {}", self.paint("▸", BLUE, true), text);
    }

    pub fn ok(&self, label: &str, value: &str) {
        println!("{}", status_line(&self.paint("✓", GREEN, true), label, value));
    }

    pub fn warn(&self, label: &str, value: &str) {
        println!("{}", status_line(&self.paint("!", YELLOW, true), label, value));
    }

    pub fn fail(&self, label: &str, value: &str) {
        println!("{}", status_line(&self.paint("✕", RED, true), label, value));
    }

    pub fn empty(&self, value: &str) {
        println!("    {} {}", self.paint("-", DIM, false), value);
    }

    pub fn progress(&self, current: u64, total: u64) {
        let bar = progress_bar(current, total, 28, self.color);
        let pct = if total > 0 { current.saturating_mul(100) / total } else { 0 };
        print!("\r    {}  {}/{}MB  {:>3}%", bar, mb(current), mb(total), pct);
        let _ = io::stdout().flush();
    }

    pub fn finish_progress(&self) {
        println!();
    }

    fn paint(&self, s: &str, color: &str, bold: bool) -> String {
        if !self.color {
            return s.to_string();
        }
        let weight = if bold { "1;" } else { "" };
        format!("\x1b[{}{}m{}\x1b[0m", weight, color, s)
    }
}

const MAUVE: &str = "38;5;183";
const BLUE: &str = "38;5;117";
const GREEN: &str = "38;5;120";
const YELLOW: &str = "38;5;229";
const RED: &str = "38;5;203";
const DIM: &str = "2";

fn colors_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::env::var_os("CI").is_none()
}

pub(crate) fn progress_bar(current: u64, total: u64, width: usize, color: bool) -> String {
    let filled = if total > 0 {
        ((current.min(total) as f64 / total as f64) * width as f64).round() as usize
    } else {
        0
    };
    let plain = format!("{}{}", "━".repeat(filled), "░".repeat(width.saturating_sub(filled)));
    if !color {
        return plain;
    }
    format!(
        "\x1b[38;5;39m{}\x1b[2m{}\x1b[0m",
        "━".repeat(filled),
        "━".repeat(width.saturating_sub(filled))
    )
}

pub(crate) fn status_line_plain(mark: &str, label: &str, value: &str) -> String {
    format!("    {} {:<8} {}", mark, label, value)
}

fn status_line(mark: &str, label: &str, value: &str) -> String {
    status_line_plain(mark, label, value)
}

fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test commands::cli_ui -- --nocapture
```

Expected: tests pass.

## Task 3: Update Command

**Files:**
- Modify: `src/commands/update.rs`
- Test: `src/commands/update.rs`

- [ ] **Step 1: Write pure helper tests**

Add tests:

```rust
#[test]
fn asset_name_for_macos_aarch64() {
    assert_eq!(asset_name("macos", "aarch64"), "flog-macos-aarch64.tar.gz");
}

#[test]
fn asset_name_for_windows_x86_64() {
    assert_eq!(asset_name("windows", "x86_64"), "flog-windows-x86_64.zip");
}

#[test]
fn normalize_latest_tag_strips_v() {
    assert_eq!(version_from_tag("v0.5.3"), "0.5.3");
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test commands::update -- --nocapture
```

Expected: compile failure because helpers do not exist.

- [ ] **Step 3: Implement update flow**

Implement these helpers and `run()`:

```rust
use std::io;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

use super::cli_ui::CliUi;

const REPO: &str = "shaominngqing/flog";

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog update");
    ui.section("Checking release");

    let current = env!("CARGO_PKG_VERSION");
    ui.ok("Current", &format!("v{}", current));

    let release = latest_release().await.map_err(to_io)?;
    let latest = version_from_tag(&release.tag_name);
    ui.ok("Latest", &format!("v{}", latest));

    if latest == current {
        ui.empty("Already up to date");
        return Ok(());
    }

    let platform = platform().map_err(to_io)?;
    let asset = asset_name(&platform.os, &platform.arch);
    let Some(url) = release.asset_url(&asset) else {
        ui.fail("Asset", &format!("missing {}", asset));
        return Ok(());
    };

    ui.section("Downloading");
    let tmp_dir = std::env::temp_dir().join(format!("flog-update-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)?;
    let archive = tmp_dir.join(&asset);
    download(&url, &archive, &ui).await.map_err(to_io)?;

    ui.section("Installing");
    let extracted = extract_archive(&archive, &tmp_dir).map_err(to_io)?;
    verify_binary(&extracted).map_err(to_io)?;
    replace_current_exe(&extracted)?;
    ui.ok("Updated", &format!("v{}", latest));
    let _ = std::fs::remove_dir_all(tmp_dir);
    Ok(())
}
```

Define `Release`, `ReleaseAsset`, `latest_release`, `download`, `extract_archive`, `verify_binary`, and `replace_current_exe` in the same module. For extraction, start with platform commands:

- Unix `.tar.gz`: `tar xzf <archive> -C <tmp_dir>`
- Windows `.zip`: PowerShell `Expand-Archive -Force`

If these commands fail on supported CI targets, add small archive crates in a follow-up task.

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test commands::update -- --nocapture
```

Expected: pure helper tests pass.

## Task 4: Uninstall Command

**Files:**
- Modify: `src/commands/uninstall.rs`
- Test: `src/commands/uninstall.rs`

- [ ] **Step 1: Write deletion-plan test**

Add:

```rust
#[test]
fn uninstall_plan_contains_binary_and_config_dir() {
    let plan = uninstall_plan(
        PathBuf::from("/tmp/flog"),
        Some(PathBuf::from("/tmp/config")),
    );
    assert_eq!(plan.binary, PathBuf::from("/tmp/flog"));
    assert_eq!(plan.config_dir, Some(PathBuf::from("/tmp/config/flog")));
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test commands::uninstall -- --nocapture
```

Expected: compile failure because `uninstall_plan` does not exist.

- [ ] **Step 3: Implement uninstall**

Implement:

```rust
use std::io::{self, Write};
use std::path::PathBuf;

use super::cli_ui::CliUi;

pub struct UninstallPlan {
    pub binary: PathBuf,
    pub config_dir: Option<PathBuf>,
}

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog uninstall");

    let exe = std::env::current_exe()?;
    let plan = uninstall_plan(exe, dirs::config_dir());

    ui.section("Remove");
    ui.ok("Binary", &plan.binary.display().to_string());
    if let Some(config_dir) = &plan.config_dir {
        ui.ok("Data", &config_dir.display().to_string());
    }

    print!("\n  Continue? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
        ui.empty("Cancelled");
        return Ok(());
    }

    if let Some(config_dir) = &plan.config_dir {
        if config_dir.exists() {
            std::fs::remove_dir_all(config_dir)?;
        }
    }
    std::fs::remove_file(&plan.binary)?;
    ui.ok("Removed", "flog");
    Ok(())
}

pub fn uninstall_plan(binary: PathBuf, config_base: Option<PathBuf>) -> UninstallPlan {
    UninstallPlan {
        binary,
        config_dir: config_base.map(|p| p.join("flog")),
    }
}
```

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test commands::uninstall -- --nocapture
```

Expected: tests pass.

## Task 5: Doctor Command

**Files:**
- Modify: `src/commands/doctor.rs`
- Test: `src/commands/doctor.rs`

- [ ] **Step 1: Write pure tests**

Add:

```rust
#[test]
fn default_ports_are_9753_through_9762() {
    assert_eq!(default_ports(), (9753..=9762).collect::<Vec<_>>());
}

#[test]
fn command_exists_detects_missing_absolute_path() {
    assert!(!path_exists("/definitely/not/a/flog/tool"));
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test commands::doctor -- --nocapture
```

Expected: compile failure because helpers do not exist.

- [ ] **Step 3: Implement doctor**

Implement `run()`:

```rust
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

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
    match crate::commands::update::latest_release().await {
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
        if port_open(port) {
            ui.warn(&port.to_string(), "open");
        } else {
            ui.ok(&port.to_string(), "free");
        }
    }

    Ok(())
}
```

Add `default_ports`, `command_in_path`, `path_exists`, and `port_open`. Keep port checks simple TCP checks; `devices` owns protocol handshakes.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test commands::doctor -- --nocapture
```

Expected: tests pass.

## Task 6: Devices Command

**Files:**
- Modify: `src/commands/devices.rs`
- Test: `src/commands/devices.rs`

- [ ] **Step 1: Write formatting tests**

Add:

```rust
#[test]
fn app_line_contains_port_app_version_and_mode() {
    let line = app_line(9753, "com.example", "1.2.0", "debug");
    assert_eq!(line, "    ✓ 9753  com.example  1.2.0  debug");
}

#[test]
fn no_app_line_is_stable() {
    assert_eq!(no_app_line(), "    - no flog_dart app found");
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo test commands::devices -- --nocapture
```

Expected: compile failure because helpers do not exist.

- [ ] **Step 3: Implement devices scan**

Implement:

```rust
use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use crate::input::{connect, connect_stream, ConnectorEvent};
use crate::transport::{self, DeviceEvent, TransportAddr};

use super::cli_ui::CliUi;

const BASE_PORT: u16 = 9753;
const PORT_SCAN_RANGE: u16 = 10;
const SCAN_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog devices");
    ui.section("Scanning");

    let mut rx = transport::start_discovery(BASE_PORT);
    let deadline = Instant::now() + SCAN_TIMEOUT;
    let mut devices = HashMap::new();

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining.min(Duration::from_millis(250)), rx.recv()).await {
            Ok(Some(DeviceEvent::Added(device))) | Ok(Some(DeviceEvent::Updated(device))) => {
                devices.insert(device.id.clone(), device);
            }
            Ok(Some(DeviceEvent::Removed(id))) => {
                devices.remove(&id);
            }
            Ok(None) => break,
            Err(_) => {}
        }
    }

    if devices.is_empty() {
        ui.empty("no devices found");
        return Ok(());
    }

    for device in devices.values() {
        println!();
        println!("  {}", device.name);
        println!("    ◇ {}", connection_label(device));
        let apps = probe_apps(device).await;
        if apps.is_empty() {
            println!("{}", no_app_line());
        } else {
            for app in apps {
                println!("{}", app_line(app.port, &app.app, &app.version, &app.build_mode));
            }
        }
    }

    Ok(())
}
```

Define `ProbeApp`, `probe_apps`, `probe_one`, and `connection_label`.

`probe_one` should resolve the transport exactly once and then drop the
connection after `ConnectorEvent::Connected`:

```rust
async fn probe_one(device: &transport::Device, port: u16) -> Option<ProbeApp> {
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
            let tunnel = transport::usbmuxd::connect_device(device_id, port).await.ok()?;
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
```

`probe_apps` should call `probe_one` for `9753..9762`, collect successes,
and return an empty vector when no flog_dart app responds.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test commands::devices -- --nocapture
```

Expected: tests pass.

## Task 7: Docs and Full Verification

**Files:**
- Modify: `README.md`
- Modify: `README_EN.md`

- [ ] **Step 1: Add command examples to README files**

Add a short section near installation:

```markdown
## Maintenance Commands

```bash
flog update      # update flog from the latest GitHub Release
flog uninstall   # remove flog and its local config
flog doctor      # check update/network/device prerequisites
flog devices     # list discovered devices and flog_dart apps
```
```

- [ ] **Step 2: Run formatting and tests**

Run:

```bash
cargo fmt -- --check
cargo test --all
```

Expected: both pass.

- [ ] **Step 3: Manual smoke checks**

Run:

```bash
cargo run -- doctor
cargo run -- devices
```

Expected: both commands print polished CLI output and exit without entering the TUI.

- [ ] **Step 4: Review diff**

Run:

```bash
git diff -- src/cli.rs src/main.rs src/commands README.md README_EN.md Cargo.toml
```

Expected: no unrelated changes; command code stays outside TUI modules.

## Self-Review

- Spec coverage: all four approved bare commands are covered. Output polish is centralized in `cli_ui.rs`. No flags or parameter matrix are included.
- Architecture rules: command modules do not require `domain`, `parser`, `input`, `transport`, or `app` to depend on ratatui/crossterm. `main.rs` only dispatches before entering TUI raw mode.
- Device coverage: the plan covers Local, Android adb-forward, and iOS usbmuxd one-shot probes without entering the TUI reconnect loop.
