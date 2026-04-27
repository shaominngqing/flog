//! Device-discovery task + the UI's `switch_to_app` channel handler.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};

use crate::app::{self, App};
use crate::input::{connect, connect_stream, ConnectorEvent};
use crate::transport;

use super::dispatch::dispatch_client_message;

// ── Per-connection reconnect backoff (TRANS-008) ────────────────────────
//
// WHY: the connector retries a failed WS connect with exponential backoff:
// each failure doubles the delay, capped at the maximum; a successful
// connection resets the delay back to the initial value.
//
// Values: 2s → 4s → 8s → 16s → 30s (cap). 2s is short enough that a flaky
// handshake recovers quickly; 30s is long enough that a dead device doesn't
// burn log noise or CPU. Flutter's own `flutter run` cycle (hot reload /
// hot restart) settles in under 30s, so an app restart will reconnect
// on the following poll.

/// Initial delay before the first retry after a failed connection.
pub(crate) const RECONNECT_INITIAL_DELAY_SECS: u64 = 2;
/// Cap on the exponential backoff so the delay never grows unbounded.
pub(crate) const RECONNECT_MAX_DELAY_SECS: u64 = 30;
/// Multiplier applied to the delay after each failure.
pub(crate) const RECONNECT_BACKOFF_FACTOR: u64 = 2;

/// Number of ports to scan per device (flog_dart binds `port..port+9`).
const PORT_SCAN_RANGE: u16 = 10;

/// Spawn the device-discovery → per-connection task fanout.
///
/// Owns the `active_tasks` + `adb_forwards` maps so they're cleaned up
/// when the process exits.
pub(crate) fn spawn_device_discovery(
    app: Arc<Mutex<App>>,
    mut device_rx: mpsc::UnboundedReceiver<transport::DeviceEvent>,
    base_port: u16,
) {
    // Track active connection tasks: key = "device_id:port"
    let active_tasks: Arc<Mutex<std::collections::HashMap<String, tokio::task::JoinHandle<()>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    // Track active adb forwards for cleanup on task abort: key = "device_id:port"
    let adb_forwards: Arc<Mutex<std::collections::HashMap<String, (String, u16)>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    let app_for_discovery = Arc::clone(&app);
    let active_tasks_c = Arc::clone(&active_tasks);
    let adb_forwards_c = Arc::clone(&adb_forwards);
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
                            connection_task(device, target_port, task_key_c, app_c, adb_fwd).await;
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
}

/// One retry loop for a single (device, port) pair. Exits only on
/// unrecoverable failure; otherwise reconnects forever with the
/// TRANS-008 exponential backoff.
async fn connection_task(
    device: transport::device_monitor::Device,
    target_port: u16,
    task_key_c: String,
    app_c: Arc<Mutex<App>>,
    adb_fwd: Arc<Mutex<std::collections::HashMap<String, (String, u16)>>>,
) {
    let mut retry_delay_secs: u64 = RECONNECT_INITIAL_DELAY_SECS;
    loop {
        // Track adb forward for cleanup
        let mut adb_forward_info: Option<(String, u16)> = None;

        // TRANS-009: `resolve_transport_addr` turns the device kind into
        // a structured plan; the match below then runs the platform-
        // specific shell-out side effects symmetrically.
        let plan = match transport::resolve_transport_addr(&device, target_port) {
            Ok(plan) => plan,
            Err(e) => {
                // No variants today; future-proof.
                tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
                retry_delay_secs =
                    (retry_delay_secs * RECONNECT_BACKOFF_FACTOR).min(RECONNECT_MAX_DELAY_SECS);
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
                        adb_forward_info = Some((serial.clone(), local_port));
                        adb_fwd
                            .lock()
                            .await
                            .insert(task_key_c.clone(), (serial.clone(), local_port));
                        let url = format!("ws://localhost:{}", local_port);
                        connect(&url).await.map_err(|e| e.to_string())
                    }
                    None => Err("adb forward failed".to_string()),
                }
            }
            transport::TransportAddr::Usbmuxd {
                device_id: uid,
                port,
            } => match transport::usbmuxd::connect_device(uid, port).await {
                Ok(tunnel) => {
                    let url = format!("ws://localhost:{}", port);
                    connect_stream(tunnel, &url)
                        .await
                        .map_err(|e| e.to_string())
                }
                Err(e) => Err(e.to_string()),
            },
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
                        a.show_status(format!("Connected: {} ({})", info.app, device_name));
                        let json = a.mock_rules.to_json_string();
                        handle.send_mock_sync(json);
                    }
                    ConnectorEvent::Disconnected => {
                        if let Some((ref serial, local_port)) = adb_forward_info {
                            transport::adb::remove_forward(serial, local_port).await;
                            adb_fwd.lock().await.remove(&task_key_c);
                        }
                        a.remove_connected_app(&task_key_c);
                        a.show_status(format!("Disconnected: {}", device.name));
                        break;
                    }
                    ConnectorEvent::Message(msg) => {
                        // TRANS-010 (A-class ack): inactive-app messages
                        // are intentionally dropped here. Each flog_dart
                        // instance buffers its own log/network entries
                        // via FlogStore; when the user switches
                        // active_app_id we subscribe() which replays the
                        // buffer.
                        if a.active_app_id.as_deref() == Some(task_key_c.as_str()) {
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

        // Retry with exponential backoff (2s → 4s → 8s → 16s → 30s cap).
        // TRANS-008.
        //
        // TRANS-011 (A-class ack): the retry loop is intentionally
        // unlogged per-attempt — the reader/writer task exit-cause
        // eprintln!s from TRANS-006 already tell the user the connection
        // dropped, and spamming status bar toasts on every 2s–30s poll
        // cycle would drown out real events. Observability upgrade
        // (retry_count on ConnectedApp) is deferred to Phase 3.5.
        tokio::time::sleep(Duration::from_secs(retry_delay_secs)).await;
        retry_delay_secs =
            (retry_delay_secs * RECONNECT_BACKOFF_FACTOR).min(RECONNECT_MAX_DELAY_SECS);
    }
}

/// Spawn the handler that pulls UI "switch to this app" requests off a
/// channel and applies them to the shared app state.
pub(crate) fn spawn_switch_app_handler(
    app: Arc<Mutex<App>>,
    mut switch_app_rx: mpsc::UnboundedReceiver<String>,
) {
    tokio::spawn(async move {
        while let Some(app_id) = switch_app_rx.recv().await {
            let mut a = app.lock().await;
            a.switch_to_app(&app_id);
            let name = a.source_name.clone();
            a.show_status(format!("Switched to {}", name));
        }
    });
}
