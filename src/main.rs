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

use app::{App, AppMode, LastSourceType, SourceCommand, SourceSelectPhase};
use cli::{Cli, InputMode};
use input::SourceEvent;

/// Dispatch a source event to the app.
fn dispatch_event(app: &mut App, event: SourceEvent) {
    match event {
        SourceEvent::RawLine(line) => app.add_raw_line(&line),
        SourceEvent::RawLineWithTimestamp(line, ts) => app.add_raw_line_with_timestamp(&line, &ts),
        SourceEvent::ParsedEntry(entry) => app.add_entry(entry),
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let app = Arc::new(Mutex::new(App::new()));
    {
        let mut a = app.lock().await;
        session::load_session(&mut a);
        if let Some(level) = cli.level {
            a.filter.min_level = level;
        }
        if let Some(ref tag) = cli.tag {
            a.filter.parse_tag_filter(tag);
        }
        a.invalidate_filter();
    }

    // Set up source command channel
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<SourceCommand>();
    {
        let mut a = app.lock().await;
        a.source_command_tx = Some(cmd_tx.clone());
    }

    // Set up replay channel
    let (replay_tx, mut replay_rx) =
        tokio::sync::mpsc::unbounded_channel::<crate::domain::network::NetworkEntry>();
    {
        let mut a = app.lock().await;
        a.replay_tx = Some(replay_tx);
    }
    {
        let app_for_replay = Arc::clone(&app);
        tokio::spawn(async move {
            while let Some(entry) = replay_rx.recv().await {
                let app_c = Arc::clone(&app_for_replay);
                tokio::spawn(replay::replay_request(app_c, entry));
            }
        });
    }

    // Spawn source manager
    let app_for_manager = Arc::clone(&app);
    tokio::spawn(source_manager(app_for_manager, cmd_rx));

    // Send initial command based on CLI args
    match cli.input_mode() {
        InputMode::Auto => {
            let _ = cmd_tx.send(SourceCommand::ShowSourceSelect);
        }
        InputMode::Adb(serial) => {
            let _ = cmd_tx.send(SourceCommand::ConnectAdb(serial));
        }
        InputMode::VmService(uri) => {
            let _ = cmd_tx.send(SourceCommand::ConnectVm(uri));
        }
        InputMode::Stdin => {
            let app_clone = Arc::clone(&app);
            tokio::spawn(start_stdin(app_clone));
        }
    }

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

// ══════════════════════════════════════
//  Source Manager
// ══════════════════════════════════════

async fn source_manager(
    app: Arc<Mutex<App>>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<SourceCommand>,
) {
    let mut current_task: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(cmd) = cmd_rx.recv().await {
        // Cancel previous source task
        if let Some(handle) = current_task.take() {
            handle.abort();
            let mut a = app.lock().await;
            a.connected = false;
        }

        let app_c = Arc::clone(&app);
        match cmd {
            SourceCommand::ConnectVm(uri) => {
                current_task = Some(tokio::spawn(run_vm_service(app_c, uri)));
            }
            SourceCommand::ConnectAdb(serial) => {
                current_task = Some(tokio::spawn(start_adb(app_c, serial)));
            }
            SourceCommand::AutoDiscover => {
                current_task = Some(tokio::spawn(auto_discover_loop(app_c)));
            }
            SourceCommand::ShowSourceSelect => {
                current_task = Some(tokio::spawn(run_source_selection(app_c)));
            }
        }
    }
}

// ══════════════════════════════════════
//  Source Selection Flow
// ══════════════════════════════════════

async fn run_source_selection(app: Arc<Mutex<App>>) {
    // If the app already has a scanning phase set (e.g. from disconnect),
    // respect it. Otherwise start with ChooseType.
    {
        let mut a = app.lock().await;
        a.mode = AppMode::SourceSelect;
        if a.source_select.phase.is_none() {
            a.source_select.phase = Some(SourceSelectPhase::ChooseType);
            a.source_select.selected_idx = 0;
            a.source_select.items_count = 2;
        }
    }

    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;

        let phase = {
            let a = app.lock().await;
            a.source_select.phase.clone()
        };

        match phase {
            Some(SourceSelectPhase::ScanningVm) => {
                // Scan for VM Services (continuously until found or user cancels)
                let services = input::discover::find_all_vm_services().await;
                let a_guard = app.lock().await;
                // Check if user cancelled while we were scanning
                if a_guard
                    .source_select
                    .phase
                    .as_ref()
                    .map(|p| !matches!(p, SourceSelectPhase::ScanningVm))
                    .unwrap_or(true)
                {
                    continue; // Phase changed — user pressed Esc or picked something else
                }
                drop(a_guard);

                match services.len() {
                    0 => {
                        // Keep scanning — stay in ScanningVm, loop will retry
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    1 => {
                        let ws_url = services[0].ws_url.clone();
                        {
                            app.lock().await.exit_source_select();
                        }
                        run_vm_service(app, ws_url).await;
                        return;
                    }
                    _ => {
                        let mut a = app.lock().await;
                        a.source_select.items_count = services.len();
                        a.source_select.selected_idx = 0;
                        a.source_select.phase = Some(SourceSelectPhase::PickVmService(services));
                    }
                }
            }
            Some(SourceSelectPhase::ScanningAdb) => {
                let devices = input::adb::list_adb_devices().await;
                let a_guard = app.lock().await;
                if a_guard
                    .source_select
                    .phase
                    .as_ref()
                    .map(|p| !matches!(p, SourceSelectPhase::ScanningAdb))
                    .unwrap_or(true)
                {
                    continue;
                }
                drop(a_guard);

                match devices.len() {
                    0 => {
                        // Keep scanning
                        tokio::time::sleep(Duration::from_millis(400)).await;
                    }
                    1 => {
                        let serial = devices[0].serial.clone();
                        {
                            app.lock().await.exit_source_select();
                        }
                        start_adb(app, Some(serial)).await;
                        return;
                    }
                    _ => {
                        let mut a = app.lock().await;
                        a.source_select.items_count = devices.len();
                        a.source_select.selected_idx = 0;
                        a.source_select.phase = Some(SourceSelectPhase::PickAdbDevice(devices));
                    }
                }
            }
            None => return, // User exited
            _ => {}         // ChooseType, PickVmService, PickAdbDevice — wait for user input
        }
    }
}

// ══════════════════════════════════════
//  Source Tasks
// ══════════════════════════════════════

/// Auto-discover VM Service: scan, connect, and on disconnect return to scanning UI.
async fn auto_discover_loop(app: Arc<Mutex<App>>) {
    {
        let mut a = app.lock().await;
        a.source_name = "Scanning...".into();
        a.connected = false;
    }

    if let Some(discovered) = input::discover::find_vm_service().await {
        {
            let mut a = app.lock().await;
            a.source_name = format!("Connecting to {}...", discovered.name);
        }

        if let Ok(mut source) = input::vm_service::VmServiceSource::new(&discovered.ws_url).await {
            let (mock_tx, mut mock_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            {
                let mut a = app.lock().await;
                a.source_name = format!("WS \u{2192} {}", discovered.name);
                a.connected = true;
                a.last_source_type = Some(LastSourceType::Vm);
                a.mock_sync_tx = Some(mock_tx);
                a.show_status(format!("Connected to VM Service ({})", discovered.name));
            }

            loop {
                tokio::select! {
                    event = source.next_event() => {
                        match event {
                            Some(e) => {
                                let mut a = app.lock().await;
                                dispatch_event(&mut a, e);
                            }
                            None => break,
                        }
                    }
                    Some(rules_json) = mock_rx.recv() => {
                        if let Some(iso_id) = source.isolate_id.clone() {
                            source.sync_mock_rules(&iso_id, &rules_json).await;
                        }
                    }
                }
            }

            // Disconnected — return to scanning UI
            {
                let mut a = app.lock().await;
                a.enter_scanning_on_disconnect();
                a.show_status("VM Service disconnected. Scanning...".into());
                a.send_source_command(SourceCommand::ShowSourceSelect);
            }
            return;
        }
    }

    // No service found or connection failed — go to source select
    {
        let mut a = app.lock().await;
        a.enter_scanning_on_disconnect();
        a.send_source_command(SourceCommand::ShowSourceSelect);
    }
}

/// Connect to VM Service; on disconnect, return to scanning UI.
async fn run_vm_service(app: Arc<Mutex<App>>, uri: String) {
    let host = uri.split('/').nth(2).unwrap_or(&uri).to_string();
    match input::vm_service::VmServiceSource::new(&uri).await {
        Ok(mut source) => {
            let (mock_tx, mut mock_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            {
                let mut a = app.lock().await;
                a.source_name = format!("WS \u{2192} {}", host);
                a.connected = true;
                a.last_source_type = Some(LastSourceType::Vm);
                a.mock_sync_tx = Some(mock_tx);
            }
            loop {
                tokio::select! {
                    event = source.next_event() => {
                        match event {
                            Some(e) => {
                                let mut a = app.lock().await;
                                dispatch_event(&mut a, e);
                            }
                            None => break,
                        }
                    }
                    Some(rules_json) = mock_rx.recv() => {
                        if let Some(iso_id) = source.isolate_id.clone() {
                            source.sync_mock_rules(&iso_id, &rules_json).await;
                        }
                    }
                }
            }
            // Disconnected — return to scanning UI
            {
                let mut a = app.lock().await;
                a.enter_scanning_on_disconnect();
                a.show_status("Disconnected. Scanning...".into());
                a.send_source_command(SourceCommand::ShowSourceSelect);
            }
        }
        Err(e) => {
            let mut a = app.lock().await;
            a.source_name = format!("WS failed: {}", e);
            a.enter_scanning_on_disconnect();
            a.send_source_command(SourceCommand::ShowSourceSelect);
        }
    }
}

async fn start_adb(app: Arc<Mutex<App>>, serial: Option<String>) {
    match input::adb::AdbSource::new(serial.as_deref()).await {
        Ok(mut source) => {
            {
                let mut a = app.lock().await;
                a.source_name = format!("ADB \u{2192} {}", source.name());
                a.connected = true;
                a.last_source_type = Some(LastSourceType::Adb);
            }
            while let Some(event) = source.next_event().await {
                let mut a = app.lock().await;
                dispatch_event(&mut a, event);
            }
            // Disconnected — return to scanning UI
            {
                let mut a = app.lock().await;
                a.enter_scanning_on_disconnect();
                a.show_status("ADB disconnected. Scanning...".into());
                a.send_source_command(SourceCommand::ShowSourceSelect);
            }
        }
        Err(e) => {
            let mut a = app.lock().await;
            a.source_name = format!("ADB failed: {}", e);
            a.enter_scanning_on_disconnect();
            a.send_source_command(SourceCommand::ShowSourceSelect);
        }
    }
}


async fn start_stdin(app: Arc<Mutex<App>>) {
    let mut source = input::stdin_source::StdinSource::new();
    {
        let mut a = app.lock().await;
        a.source_name = "stdin".into();
        a.connected = true;
    }
    while let Some(event) = source.next_event().await {
        let mut a = app.lock().await;
        dispatch_event(&mut a, event);
    }
}

async fn dropdown_scan_loop(app: Arc<Mutex<App>>) {
    loop {
        {
            let a = app.lock().await;
            if !a.show_source_dropdown {
                return;
            }
        }

        // Scan VM services
        let vm_services = input::discover::find_all_vm_services().await;
        let adb_devices = input::adb::list_adb_devices().await;

        {
            let mut a = app.lock().await;
            if !a.show_source_dropdown {
                return;
            }
            a.dropdown.discovered_vm = vm_services;
            a.dropdown.discovered_adb = adb_devices;
            a.dropdown.scanning = true; // keep scanning indicator
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &Arc<Mutex<App>>,
) -> io::Result<()> {
    let mut dropdown_task: Option<tokio::task::JoinHandle<()>> = None;
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

            // Check if dropdown scan was requested
            if app_guard.dropdown_scan_requested {
                app_guard.dropdown_scan_requested = false;
                if let Some(h) = dropdown_task.take() {
                    h.abort();
                }
                dropdown_task = Some(tokio::spawn(dropdown_scan_loop(Arc::clone(app))));
            }
            if !app_guard.show_source_dropdown {
                if let Some(h) = dropdown_task.take() {
                    h.abort();
                }
            }

            terminal.draw(|f| match app_guard.mode {
                AppMode::Help => ui::help::draw_help(f),
                AppMode::Stats => match app_guard.active_stats_tab {
                    crate::app::ViewTab::Logs => ui::logs::stats::draw_stats(f, &mut app_guard),
                    crate::app::ViewTab::Network => {
                        ui::network::stats::draw_network_stats(f, &mut app_guard)
                    }
                },
                AppMode::MockRuleEdit => {
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
