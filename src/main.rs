//! flog — Flutter Log Viewer.

mod app;
mod cli;
mod commands;
mod domain;
mod event;
pub mod input;
pub mod parser;
mod replay;
mod run;
mod session;
mod transport;
mod ui;

use std::io;
use std::sync::Arc;

use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::Mutex;

use app::App;
use cli::Cli;

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    if let Some(command) = cli.command {
        return commands::run(command).await;
    }

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
    let device_rx = transport::start_discovery(cli.port);

    // Channel for UI to request switching to a specific app
    let (switch_app_tx, switch_app_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let mut a = app.lock().await;
        a.connect_device_tx = Some(switch_app_tx.clone());
    }

    run::spawn_device_discovery(Arc::clone(&app), device_rx, cli.port);
    run::spawn_switch_app_handler(Arc::clone(&app), switch_app_rx);

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

    let result = run::run_loop(&mut terminal, &app).await;

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
