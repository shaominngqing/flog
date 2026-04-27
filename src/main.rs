//! flog — Flutter Log Viewer.

mod app;
mod cli;
mod domain;
mod event;
pub mod input;
pub mod parser;
mod replay;
mod session;
mod transport;
mod ui;

use std::io;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::Mutex;

use app::App;
use cli::Cli;
use input::{connect, connect_stream, ClientMessage, ConnectorEvent};

/// Pattern: `[LEVEL][Tag] message` — used to parse raw log text.
/// Optionally preceded by `[epoch_ms]` which is ignored (timestamp comes from the message field).
/// Applied against the first line only; stack frames on subsequent lines are extracted separately.
static RAW_LOG_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^(?:\[\d{10,13}\])?\[(\w+)\]\[([^\]]+)\]\s?(.*)$").unwrap()
});

/// Detects Dart stack frame lines: `#N ...`.
static STACK_FRAME_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^#\d+\s").unwrap());

// ── Per-connection reconnect backoff (TRANS-008) ────────────────────────
//
// The connector task retries a failed WS connect with exponential backoff:
// each failure doubles the delay, capped at the maximum. A successful
// connection resets the delay back to the initial value.
//
// Values: 2s → 4s → 8s → 16s → 30s (cap). 2s is short enough that a flaky
// handshake recovers quickly; 30s is long enough that a dead device doesn't
// burn log noise or CPU.
/// Initial delay before the first retry after a failed connection.
const RECONNECT_INITIAL_DELAY_SECS: u64 = 2;
/// Cap on the exponential backoff so the delay never grows unbounded.
const RECONNECT_MAX_DELAY_SECS: u64 = 30;
/// Multiplier applied to the delay after each failure.
const RECONNECT_BACKOFF_FACTOR: u64 = 2;

/// Split a message body into (leading_text, Option<stacktrace>).
///
/// The stacktrace begins at the first line matching `#\d+ ` and continues to the end.
/// Both halves are returned with trailing newlines trimmed.
fn split_stacktrace(body: &str) -> (String, Option<String>) {
    let mut split_at: Option<usize> = None;
    let mut cursor = 0usize;
    for line in body.split_inclusive('\n') {
        let line_no_nl = line.strip_suffix('\n').unwrap_or(line);
        if STACK_FRAME_RE.is_match(line_no_nl) {
            split_at = Some(cursor);
            break;
        }
        cursor += line.len();
    }
    match split_at {
        Some(idx) => {
            let head = body[..idx].trim_end_matches(['\n', ' ']).to_string();
            let stack = body[idx..].trim_end_matches('\n').to_string();
            let stack_opt = if stack.is_empty() { None } else { Some(stack) };
            (head, stack_opt)
        }
        None => (body.trim_end_matches('\n').to_string(), None),
    }
}

/// Convert epoch milliseconds to HH:MM:SS.mmm.
fn format_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let millis = ms % 1000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60,
        millis
    )
}

/// Dispatch a client message to the app.
fn dispatch_client_message(app: &mut App, msg: ClientMessage) {
    match msg {
        ClientMessage::Hello { .. } => {
            // Hello is handled at connection time, nothing more to do here
        }
        ClientMessage::Log {
            level,
            tag,
            message,
            error,
            stack_trace,
            timestamp,
        } => {
            // Match `[LEVEL][Tag] ...` against the first line only; remaining lines may
            // carry error text and/or a Dart stack trace (`#N ...` + asynchronous suspension).
            let (first_line, rest) = match message.split_once('\n') {
                Some((head, tail)) => (head, Some(tail)),
                None => (message.as_str(), None),
            };

            let entry = if let (Some(level), Some(tag)) = (level, tag) {
                // Structured log from FlogLogger — level/tag provided explicitly.
                let log_level =
                    domain::LogLevel::from_str(&level).unwrap_or(domain::LogLevel::Info);
                let mut e = domain::LogEntry::new(log_level, tag, message);
                e.error = error;
                e.stacktrace = stack_trace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            } else if let Some(caps) = RAW_LOG_RE.captures(first_line) {
                // Raw text matching [LEVEL][Tag] format (e.g. AuraLogger via debugPrint).
                let level_str = caps.get(1).unwrap().as_str();
                let tag_str = caps.get(2).unwrap().as_str();
                let msg_str = caps.get(3).unwrap().as_str();
                let log_level =
                    domain::LogLevel::from_str(level_str).unwrap_or(domain::LogLevel::Debug);

                let (extra_body, stacktrace) = match rest {
                    Some(r) => split_stacktrace(r),
                    None => (String::new(), None),
                };

                // Treat non-stack text after the first line as continuation of the message.
                let full_msg = if extra_body.is_empty() {
                    msg_str.to_string()
                } else {
                    format!("{msg_str}\n{extra_body}")
                };

                let mut e = domain::LogEntry::new(log_level, tag_str, full_msg);
                e.stacktrace = stacktrace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            } else {
                // Unstructured raw text (e.g. Flutter framework output via debugPrint).
                // Still split off `#N ...` stack frames so the list view can collapse them.
                let (body, stacktrace) = split_stacktrace(&message);
                let mut e = domain::LogEntry::new(domain::LogLevel::Debug, "debugPrint", body);
                e.stacktrace = stacktrace;
                if let Some(ts) = timestamp {
                    e.timestamp = format_ts(ts);
                }
                e
            };
            app.add_entry(entry);
        }
        ClientMessage::Net { msg } => {
            app.network_store.process_message(msg);
            app.network.invalidate_filter();
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let app = Arc::new(Mutex::new(App::new()));
    {
        let mut a = app.lock().await;
        a.server_port = cli.port;
        session::load_session(&mut a);
        if let Some(level) = cli.level {
            a.filter.min_level = level;
        }
        if let Some(ref tag) = cli.tag {
            a.filter.parse_tag_filter(tag);
        }
        a.invalidate_filter();
        a.source_name = format!("Scanning... (port {})", cli.port);
    }

    // Start event-driven device discovery
    let mut device_rx = transport::start_discovery(cli.port);

    // Channel for UI to request switching to a specific app
    let (switch_app_tx, mut switch_app_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let mut a = app.lock().await;
        a.connect_device_tx = Some(switch_app_tx.clone());
    }

    // Track active connection tasks: key = "device_id:port"
    let active_tasks: Arc<Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Track active adb forwards for cleanup on task abort: key = "device_id:port"
    let adb_forwards: Arc<Mutex<std::collections::HashMap<String, (String, u16)>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Number of ports to scan per device
    const PORT_SCAN_RANGE: u16 = 10;

    // Task 1: Device discovery → spawn per-device port-scanning connection tasks
    let app_for_discovery = Arc::clone(&app);
    let active_tasks_c = Arc::clone(&active_tasks);
    let adb_forwards_c = Arc::clone(&adb_forwards);
    let base_port = cli.port;
    tokio::spawn(async move {
        // TRANS-015 (A-class ack): the device discovery channel is expected
        // to outlive the app — `start_discovery` spawns three infinite
        // loops that never drop their sender. If `device_rx.recv()`
        // returns None (i.e. all senders were dropped), something inside
        // transport layer has crashed hard; we intentionally exit the
        // task and rely on reconnect/reopen via session restart. A richer
        // "restart discovery" strategy is deferred to Phase 3.5.
        while let Some(event) = device_rx.recv().await {
            match event {
                transport::DeviceEvent::Added(device) => {
                    // Sync to App for UI
                    {
                        let mut a = app_for_discovery.lock().await;
                        a.discovered_devices
                            .entry(device.id.clone())
                            .or_insert_with(|| device.clone());
                    }

                    // Spawn a connection task for each port in range
                    for port_offset in 0..PORT_SCAN_RANGE {
                        let target_port = base_port + port_offset;
                        let task_key = format!("{}:{}", device.id, target_port);

                        // Skip if we already have a task for this device:port
                        {
                            let tasks = active_tasks_c.lock().await;
                            if tasks.contains_key(&task_key) {
                                continue;
                            }
                        }

                        let device = device.clone();
                        let app_c = Arc::clone(&app_for_discovery);
                        let adb_fwd = Arc::clone(&adb_forwards_c);
                        let task_key_c = task_key.clone();

                        let task = tokio::spawn(async move {
                            let mut retry_delay_secs: u64 = RECONNECT_INITIAL_DELAY_SECS;
                            loop {
                                // Track adb forward for cleanup
                                let mut adb_forward_info: Option<(String, u16)> = None;

                                // TRANS-009: `resolve_transport_addr` turns the
                                // device kind into a structured plan; the
                                // match below then runs the platform-specific
                                // shell-out side effects symmetrically.
                                let plan =
                                    match transport::resolve_transport_addr(&device, target_port) {
                                        Ok(plan) => plan,
                                        Err(e) => {
                                            // No variants today; future-proof.
                                            tokio::time::sleep(Duration::from_secs(
                                                retry_delay_secs,
                                            ))
                                            .await;
                                            retry_delay_secs = (retry_delay_secs
                                                * RECONNECT_BACKOFF_FACTOR)
                                                .min(RECONNECT_MAX_DELAY_SECS);
                                            eprintln!("transport resolve failed: {e}");
                                            continue;
                                        }
                                    };

                                let ws_result = match plan {
                                    transport::TransportAddr::Localhost { port } => {
                                        let url = format!("ws://localhost:{}", port);
                                        connect(&url).await.map_err(|e| e.to_string())
                                    }
                                    transport::TransportAddr::AdbForward { serial, port } => {
                                        match transport::adb::setup_forward(&serial, port).await {
                                            Some(local_port) => {
                                                adb_forward_info =
                                                    Some((serial.clone(), local_port));
                                                adb_fwd.lock().await.insert(
                                                    task_key_c.clone(),
                                                    (serial.clone(), local_port),
                                                );
                                                let url = format!("ws://localhost:{}", local_port);
                                                connect(&url).await.map_err(|e| e.to_string())
                                            }
                                            None => Err("adb forward failed".to_string()),
                                        }
                                    }
                                    transport::TransportAddr::Usbmuxd {
                                        device_id: uid,
                                        port,
                                    } => {
                                        match transport::usbmuxd::connect_device(uid, port).await {
                                            Ok(tunnel) => {
                                                let url = format!("ws://localhost:{}", port);
                                                connect_stream(tunnel, &url)
                                                    .await
                                                    .map_err(|e| e.to_string())
                                            }
                                            Err(e) => Err(e.to_string()),
                                        }
                                    }
                                };

                                if let Ok((mut event_rx, handle)) = ws_result {
                                    // Reset backoff on successful connection.
                                    retry_delay_secs = RECONNECT_INITIAL_DELAY_SECS;
                                    while let Some(evt) = event_rx.recv().await {
                                        let mut a = app_c.lock().await;
                                        match evt {
                                            ConnectorEvent::Connected(info) => {
                                                let device_name = a
                                                    .discovered_devices
                                                    .get(&device.id)
                                                    .map(|d| d.name.clone())
                                                    .unwrap_or_else(|| device.name.clone());
                                                let app_info = app::ConnectedApp {
                                                    id: task_key_c.clone(),
                                                    device_id: device.id.clone(),
                                                    port: target_port,
                                                    device_name: device_name.clone(),
                                                    app_name: info.app.clone(),
                                                    app_version: info.app_version.clone(),
                                                    os: info.os.clone(),
                                                    package_name: info.package_name.clone(),
                                                    build_mode: info.build_mode.clone(),
                                                    handle: handle.clone(),
                                                };
                                                a.add_connected_app(app_info);
                                                a.show_status(format!(
                                                    "Connected: {} ({})",
                                                    info.app, device_name
                                                ));
                                                let json = a.mock_rules.to_json_string();
                                                handle.send_mock_sync(json);
                                            }
                                            ConnectorEvent::Disconnected => {
                                                if let Some((ref serial, local_port)) =
                                                    adb_forward_info
                                                {
                                                    transport::adb::remove_forward(
                                                        serial, local_port,
                                                    )
                                                    .await;
                                                    adb_fwd.lock().await.remove(&task_key_c);
                                                }
                                                a.remove_connected_app(&task_key_c);
                                                a.show_status(format!(
                                                    "Disconnected: {}",
                                                    device.name
                                                ));
                                                break;
                                            }
                                            ConnectorEvent::Message(msg) => {
                                                // TRANS-010 (A-class ack): inactive-app
                                                // messages are intentionally dropped here.
                                                // Each flog_dart instance buffers its own
                                                // log/network entries via FlogStore; when
                                                // the user switches active_app_id we
                                                // subscribe() which replays the buffer.
                                                if a.active_app_id.as_deref()
                                                    == Some(task_key_c.as_str())
                                                {
                                                    dispatch_client_message(&mut a, msg);
                                                }
                                            }
                                        }
                                    }
                                }

                                // Clean up adb forward on failure
                                if let Some((ref serial, local_port)) = adb_forward_info {
                                    transport::adb::remove_forward(serial, local_port).await;
                                    adb_fwd.lock().await.remove(&task_key_c);
                                }

                                // Retry with exponential backoff
                                // (2s → 4s → 8s → 16s → 30s cap). TRANS-008.
                                //
                                // TRANS-011 (A-class ack): the retry loop is
                                // intentionally unlogged per-attempt — the
                                // reader/writer task exit-cause eprintln!s
                                // from TRANS-006 already tell the user the
                                // connection dropped, and spamming status
                                // bar toasts on every 2s–30s poll cycle
                                // would drown out real events. Observability
                                // upgrade (retry_count on ConnectedApp) is
                                // deferred to Phase 3.5.
                                tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
                                retry_delay_secs = (retry_delay_secs * RECONNECT_BACKOFF_FACTOR)
                                    .min(RECONNECT_MAX_DELAY_SECS);
                            }
                        });

                        active_tasks_c.lock().await.insert(task_key, task);
                    }
                }
                transport::DeviceEvent::Removed(id) => {
                    // Cancel all connection tasks for this device (all ports)
                    let mut tasks = active_tasks_c.lock().await;
                    let keys_to_remove: Vec<String> = tasks
                        .keys()
                        .filter(|k| k.starts_with(&format!("{}:", id)))
                        .cloned()
                        .collect();
                    for key in &keys_to_remove {
                        if let Some(task) = tasks.remove(key) {
                            task.abort();
                        }
                    }
                    drop(tasks);

                    // Clean up any adb forwards orphaned by aborted tasks
                    {
                        let mut fwds = adb_forwards_c.lock().await;
                        for key in &keys_to_remove {
                            if let Some((serial, local_port)) = fwds.remove(key) {
                                transport::adb::remove_forward(&serial, local_port).await;
                            }
                        }
                    }

                    // Clean up app state — remove device and all its connected apps
                    {
                        let mut a = app_for_discovery.lock().await;
                        a.discovered_devices.remove(&id);
                        // Remove all connected apps for this device
                        let app_ids: Vec<String> = a
                            .connected_apps
                            .iter()
                            .filter(|app| app.device_id == id)
                            .map(|app| app.id.clone())
                            .collect();
                        for app_id in app_ids {
                            a.remove_connected_app(&app_id);
                        }
                    }
                }
            }
        }
    });

    // Task 2: Handle UI "switch to app" requests
    let app_for_switch = Arc::clone(&app);
    tokio::spawn(async move {
        while let Some(app_id) = switch_app_rx.recv().await {
            let mut a = app_for_switch.lock().await;
            a.switch_to_app(&app_id);
            let name = a.source_name.clone();
            a.show_status(format!("Switched to {}", name));
        }
    });

    // Install panic hook
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    // Enter TUI
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &app).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    {
        let a = app.lock().await;
        session::save_session(&a);
    }

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &Arc<Mutex<App>>,
) -> io::Result<()> {
    let mut mouse_captured = true;

    loop {
        {
            let mut app_guard = app.lock().await;

            // Toggle mouse capture for select mode
            if app_guard.select_mode && mouse_captured {
                let _ = execute!(io::stdout(), DisableMouseCapture);
                mouse_captured = false;
            } else if !app_guard.select_mode && !mouse_captured {
                let _ = execute!(io::stdout(), EnableMouseCapture);
                mouse_captured = true;
            }

            terminal.draw(|f| {
                match app_guard.mode {
                    app::AppMode::Help => ui::help::draw_help(f),
                    app::AppMode::Stats => match app_guard.active_stats_tab {
                        app::ViewTab::Logs => ui::logs::stats::draw_stats(f, &mut app_guard),
                        app::ViewTab::Network => {
                            ui::network::stats::draw_network_stats(f, &mut app_guard)
                        }
                    },
                    app::AppMode::MockRuleEdit => {
                        ui::draw(f, &mut app_guard);
                        ui::network::mock_rules::draw_mock_rule_edit(f, &mut app_guard);
                    }
                    _ => ui::draw(f, &mut app_guard),
                }
                // Device picker overlay
                if app_guard.show_device_picker {
                    ui::device_picker::draw_device_picker(f, &mut app_guard, f.area());
                }
            })?;
            if app_guard.should_quit {
                return Ok(());
            }
        }

        if crossterm::event::poll(Duration::from_millis(33))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    let mut app = app.lock().await;
                    event::handle_key(&mut app, key);
                }
                Event::Mouse(mouse) => {
                    let mut app = app.lock().await;
                    event::handle_mouse(&mut app, mouse);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
