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
use input::{ClientMessage, ConnectorEvent, connect, connect_stream};

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
            let log_level = domain::LogLevel::from_str(&level).unwrap_or(domain::LogLevel::Info);
            let mut entry = domain::LogEntry::new(log_level, tag, message);
            entry.error = error;
            entry.stacktrace = stack_trace;
            if let Some(ts) = timestamp {
                // Convert milliseconds to readable timestamp
                let secs = ts / 1000;
                let millis = ts % 1000;
                entry.timestamp = format!(
                    "{:02}:{:02}:{:02}.{:03}",
                    (secs % 86400) / 3600,
                    (secs % 3600) / 60,
                    secs % 60,
                    millis
                );
            }
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

    // Track active connection tasks per device
    let active_tasks: Arc<Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Task 1: Device discovery → spawn per-device connection tasks
    let app_for_discovery = Arc::clone(&app);
    let active_tasks_c = Arc::clone(&active_tasks);
    let port = cli.port;
    tokio::spawn(async move {
        while let Some(event) = device_rx.recv().await {
            match event {
                transport::DeviceEvent::Added(device) => {
                    // Sync to App for UI
                    {
                        let mut a = app_for_discovery.lock().await;
                        if !a.discovered_devices.iter().any(|d| d.id == device.id) {
                            a.discovered_devices.push(device.clone());
                        }
                    }

                    // Skip if we already have a task for this device
                    {
                        let tasks = active_tasks_c.lock().await;
                        if tasks.contains_key(&device.id) {
                            continue;
                        }
                    }

                    // Spawn a persistent connection task for this device
                    let device_id = device.id.clone();
                    let app_c = Arc::clone(&app_for_discovery);
                    let tasks_c = Arc::clone(&active_tasks_c);

                    let task = tokio::spawn(async move {
                        loop {
                            // Build WS URL based on device type
                            let ws_result = match device.connection_method() {
                                transport::ConnectionMethod::Localhost => {
                                    let url = format!("ws://localhost:{}", port);
                                    connect(&url).await.map_err(|e| e.to_string())
                                }
                                transport::ConnectionMethod::AdbForward { ref serial } => {
                                    match transport::adb::setup_forward(serial, port).await {
                                        Some(local_port) => {
                                            let url = format!("ws://localhost:{}", local_port);
                                            connect(&url).await.map_err(|e| e.to_string())
                                        }
                                        None => Err("adb forward failed".to_string()),
                                    }
                                }
                                transport::ConnectionMethod::Usbmuxd { device_id: uid } => {
                                    match transport::usbmuxd::connect_device(uid, port).await {
                                        Ok(tunnel) => {
                                            let url = format!("ws://localhost:{}", port);
                                            connect_stream(tunnel, &url).await.map_err(|e| e.to_string())
                                        }
                                        Err(e) => Err(e.to_string()),
                                    }
                                }
                            };

                            if let Ok((mut event_rx, handle)) = ws_result {
                                // Start flutter logs for this device
                                let app_for_logs = Arc::clone(&app_c);
                                let log_dev_id = device.id.clone();
                                let logs_handle = tokio::spawn(async move {
                                    let dev_arg = if log_dev_id == "localhost" { None } else { Some(log_dev_id.as_str()) };
                                    if let Ok(mut logs) = transport::flutter_logs::FlutterLogs::start(dev_arg).await {
                                        while let Some(line) = logs.next_line().await {
                                            let mut a = app_for_logs.lock().await;
                                            // Only add to active app
                                            if a.active_app_id.as_deref() == Some(&log_dev_id) {
                                                a.add_raw_line(&line);
                                            }
                                        }
                                    }
                                });

                                // Process events
                                while let Some(evt) = event_rx.recv().await {
                                    let mut a = app_c.lock().await;
                                    match evt {
                                        ConnectorEvent::Connected(info) => {
                                            let app_info = app::ConnectedApp {
                                                id: device.id.clone(),
                                                device_name: info.device.clone(),
                                                app_name: info.app.clone(),
                                                app_version: info.app_version.clone(),
                                                os: info.os.clone(),
                                                handle: handle.clone(),
                                            };
                                            a.add_connected_app(app_info);
                                            a.show_status(format!("Connected: {}", info.device));
                                            // Sync mock rules
                                            let json = a.mock_rules.to_json_string();
                                            handle.send_mock_sync(json);
                                        }
                                        ConnectorEvent::Disconnected => {
                                            a.remove_connected_app(&device.id);
                                            a.show_status(format!("Disconnected: {}", device.name));
                                            logs_handle.abort();
                                            break;
                                        }
                                        ConnectorEvent::Message(msg) => {
                                            // Only process messages for the active app
                                            if a.active_app_id.as_deref() == Some(&device.id) {
                                                dispatch_client_message(&mut a, msg);
                                            }
                                            // For non-active apps, messages are dropped
                                            // (their session will be populated when they become active)
                                        }
                                    }
                                }
                            }

                            // Retry after delay
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        }
                    });

                    active_tasks_c.lock().await.insert(device_id, task);
                }
                transport::DeviceEvent::Removed(id) => {
                    // Sync to App
                    {
                        let mut a = app_for_discovery.lock().await;
                        a.discovered_devices.retain(|d| d.id != id);
                    }
                    // Cancel connection task
                    if let Some(task) = active_tasks_c.lock().await.remove(&id) {
                        task.abort();
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
                    ui::source_select::draw_device_picker(f, &mut app_guard, f.area());
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
