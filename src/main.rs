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
use input::{ClientMessage, ConnectorEvent, connect};

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

    // Shared list of discovered devices
    let discovered: Arc<Mutex<Vec<transport::Device>>> = Arc::new(Mutex::new(Vec::new()));

    // Channel for the connector to request "try connecting to available devices"
    let (try_connect_tx, mut try_connect_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Task 1: Listen for device events, maintain discovered list, trigger connection attempts
    let discovered_c = Arc::clone(&discovered);
    let try_connect_tx_c = try_connect_tx.clone();
    tokio::spawn(async move {
        while let Some(event) = device_rx.recv().await {
            let mut devs = discovered_c.lock().await;
            match event {
                transport::DeviceEvent::Added(device) => {
                    if !devs.iter().any(|d| d.id == device.id) {
                        devs.push(device);
                        let _ = try_connect_tx_c.send(());
                    }
                }
                transport::DeviceEvent::Removed(id) => {
                    devs.retain(|d| d.id != id);
                }
            }
        }
    });

    // Task 2: Connection manager — tries to connect to discovered devices
    let app_for_connector = Arc::clone(&app);
    let discovered_c2 = Arc::clone(&discovered);
    let port = cli.port;
    tokio::spawn(async move {
        loop {
            // Wait for a trigger (new device discovered, or reconnect after disconnect)
            let _ = try_connect_rx.recv().await;
            // Drain any extra triggers
            while try_connect_rx.try_recv().is_ok() {}

            // Already connected? Skip.
            {
                let a = app_for_connector.lock().await;
                if a.connected {
                    continue;
                }
            }

            // Try each discovered device
            let devices = {
                let devs = discovered_c2.lock().await;
                devs.clone()
            };

            for device in &devices {
                let ws_url = match device.connection_method() {
                    transport::ConnectionMethod::Localhost => {
                        format!("ws://localhost:{}", port)
                    }
                    transport::ConnectionMethod::AdbForward { ref serial } => {
                        match transport::adb::setup_forward(serial, port).await {
                            Some(local_port) => format!("ws://localhost:{}", local_port),
                            None => continue,
                        }
                    }
                    transport::ConnectionMethod::Usbmuxd { .. } => {
                        continue; // TODO
                    }
                };

                if let Ok((mut event_rx, handle)) = connect(&ws_url).await {
                    {
                        let mut a = app_for_connector.lock().await;
                        a.connector_handle = Some(handle.clone());
                    }

                    // Start flutter logs
                    let app_for_logs = Arc::clone(&app_for_connector);
                    let log_dev_id = device.id.clone();
                    let logs_handle = tokio::spawn(async move {
                        let dev_arg = if log_dev_id == "localhost" { None } else { Some(log_dev_id.as_str()) };
                        if let Ok(mut logs) = transport::flutter_logs::FlutterLogs::start(dev_arg).await {
                            while let Some(line) = logs.next_line().await {
                                let mut a = app_for_logs.lock().await;
                                a.add_raw_line(&line);
                            }
                        }
                    });

                    // Process events until disconnect
                    while let Some(evt) = event_rx.recv().await {
                        let mut a = app_for_connector.lock().await;
                        match evt {
                            ConnectorEvent::Connected(info) => {
                                a.source_name = format!("{} ({})", info.device, info.app);
                                a.connected = true;
                                a.clients.push(info.clone());
                                a.show_status(format!("Connected: {}", info.device));
                                let json = a.mock_rules.to_json_string();
                                handle.send_mock_sync(json);
                            }
                            ConnectorEvent::Disconnected => {
                                a.clients.clear();
                                a.connected = false;
                                a.connector_handle = None;
                                a.source_name = "Scanning...".to_string();
                                a.show_status("Disconnected".to_string());
                                a.clear_session_data();
                                logs_handle.abort();
                                break;
                            }
                            ConnectorEvent::Message(msg) => {
                                dispatch_client_message(&mut a, msg);
                            }
                        }
                    }

                    // Disconnected — trigger reconnection to try other devices
                    let _ = try_connect_tx.send(());
                    break; // Exit device loop, wait for trigger
                }
            }
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

            terminal.draw(|f| match app_guard.mode {
                app::AppMode::Help => ui::help::draw_help(f),
                app::AppMode::Stats => match app_guard.active_stats_tab {
                    app::ViewTab::Logs => ui::logs::stats::draw_stats(f, &mut app_guard),
                    app::ViewTab::Network => {
                        ui::network::stats::draw_network_stats(f, &mut app_guard)
                    }
                },
                app::AppMode::MockRuleEdit => {
                    // Draw normal Network view underneath, then editor overlay on top
                    ui::draw(f, &mut app_guard);
                    ui::network::mock_rules::draw_mock_rule_edit(f, &mut app_guard);
                }
                _ => ui::draw(f, &mut app_guard),
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
