//! The TUI event-and-render loop.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::Mutex;

use crate::app::{self, App};
use crate::event;
use crate::ui;

pub(crate) async fn run_loop(
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
                // Full-value overlay (drawn last so it overlays everything)
                ui::full_value_overlay::draw_full_value_overlay(f, &app_guard);
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
