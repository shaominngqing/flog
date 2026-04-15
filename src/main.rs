//! flog — Flutter Log Viewer.

mod app;
mod cli;
mod domain;
mod event;
pub mod input;
pub mod parser;
mod replay;
mod session;
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
use input::{ClientMessage, FlogServer, ServerEvent};

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
        a.source_name = format!("Listening on port {}", cli.port);
    }

    // Start the WebSocket server
    let mut server = match FlogServer::start(cli.port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start server on port {}: {}", cli.port, e);
            return Err(io::Error::new(io::ErrorKind::AddrInUse, e.to_string()));
        }
    };

    // Store the server handle for downstream communication
    {
        let mut a = app.lock().await;
        a.server_handle = Some(server.handle());
    }

    // Spawn task to process server events
    let app_for_server = Arc::clone(&app);
    tokio::spawn(async move {
        while let Some(event) = server.next_event().await {
            let mut a = app_for_server.lock().await;
            match event {
                ServerEvent::ClientConnected(info) => {
                    a.source_name = format!("{} ({})", info.device, info.app);
                    a.connected = true;
                    a.clients.push(info.clone());
                    a.show_status(format!("Connected: {} - {}", info.device, info.app));
                    // Sync mock rules to newly connected client
                    if let Some(ref handle) = a.server_handle {
                        let json = a.mock_rules.to_json_string();
                        handle.broadcast_mock_sync(json);
                    }
                }
                ServerEvent::ClientDisconnected(id) => {
                    a.clients.retain(|c| c.id != id);
                    if a.clients.is_empty() {
                        a.connected = false;
                        a.source_name = format!("Listening on port {}", a.server_port);
                        a.show_status("Client disconnected".to_string());
                        // Clear session data when all clients disconnect
                        a.clear_session_data();
                    } else {
                        // Update source name to show remaining client
                        if let Some(c) = a.clients.first() {
                            a.source_name = format!("{} ({})", c.device, c.app);
                        }
                    }
                }
                ServerEvent::Message(_, msg) => {
                    dispatch_client_message(&mut a, msg);
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
