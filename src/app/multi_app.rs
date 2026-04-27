//! Multi-app connection management (audit UI-040 + UI-023 ack).
//!
//! flog can be attached to multiple running Flutter apps simultaneously,
//! but the UI shows exactly one at a time. See the invariants documented
//! on [`App`] for the contract this module upholds.

use crate::domain::{FilterState, LogStore};
use crate::input::ConnectorHandle;

use super::{App, DetailState, LogsViewState, NetworkState, SearchState};

/// Info about a connected app.
#[derive(Clone)]
pub struct ConnectedApp {
    pub id: String,           // unique key: "device_id:port" (e.g. "localhost:9753")
    pub device_id: String,    // original device ID for grouping (e.g. "localhost", "1e0e87b2")
    pub port: u16,            // port this app is listening on
    pub device_name: String,  // from device discovery (not hello)
    pub app_name: String,     // from hello
    pub app_version: String,  // from hello
    pub os: String,           // from hello
    pub package_name: String, // from hello
    pub build_mode: String,   // from hello: "debug" / "profile" / "release"
    pub handle: ConnectorHandle,
}

impl App {
    /// Reset all data and UI state to a clean slate.
    ///
    /// Used when switching apps or when the active app disconnects.
    /// Data will be re-delivered by the Dart side's FlogStore on subscribe.
    pub(super) fn reset_session(&mut self) {
        self.store = LogStore::new();
        self.network_store = crate::domain::NetworkStore::new();
        self.network = NetworkState::new();
        self.filter = FilterState::default();
        self.logs = LogsViewState::default();
        self.detail = DetailState::default();
        self.search = SearchState::default();
        self.bookmarks.clear();
        self.invalidate_filter();
    }

    /// Register a new connected app. If it's the first, make it active.
    ///
    /// The Dart side automatically replays its buffer on new WebSocket
    /// connections, so no explicit subscribe is needed for the first app.
    pub fn add_connected_app(&mut self, app_info: ConnectedApp) {
        let id = app_info.id.clone();
        let is_first = self.connected_apps.is_empty();

        // Remove if already exists (reconnection)
        let is_reconnect = self.active_app_id.as_deref() == Some(&id);
        self.connected_apps.retain(|a| a.id != id);
        self.connected_apps.push(app_info);

        if is_reconnect {
            // Reconnecting to active app — clear and let Dart replay fill us.
            self.reset_session();
        } else if is_first || self.active_app_id.is_none() {
            // First app or no active app — activate directly without subscribe.
            // Dart already auto-replays its buffer on new WebSocket connections,
            // so sending subscribe would cause a redundant double-replay.
            self.reset_session();
            self.active_app_id = Some(id.clone());
            self.update_source_name(&id);
        } else {
            // Another app connected while we already have an active one — no-op,
            // just register. User can switch to it manually.
        }
    }

    /// Remove a disconnected app.
    pub fn remove_connected_app(&mut self, id: &str) {
        self.connected_apps.retain(|a| a.id != id);

        if self.active_app_id.as_deref() == Some(id) {
            // Active app disconnected — switch to another if available
            if let Some(next) = self.connected_apps.first() {
                let next_id = next.id.clone();
                self.switch_to_app(&next_id);
            } else {
                self.active_app_id = None;
                self.source_name = format!("Scanning... (port {})", self.server_port);
                self.reset_session();
            }
        }
    }

    /// Switch the UI to view a different app's data.
    ///
    /// Clears all local data and sends a `subscribe` message to the target
    /// app's Dart server, which triggers a full buffer replay. The replayed
    /// messages use the same format as live messages — the TUI cannot and
    /// does not need to distinguish them.
    pub fn switch_to_app(&mut self, id: &str) {
        if self.active_app_id.as_deref() == Some(id) {
            return; // Already active
        }

        // WHY: guard against stale picker selections. Device discovery lists
        // may include ids whose WS connection dropped between poll and click;
        // without this check we'd set `active_app_id` to a ghost and violate
        // invariant 1 (active ∈ connected_apps). `discovered_devices` is
        // NOT authoritative here.
        if !self.connected_apps.iter().any(|a| a.id == id) {
            return;
        }

        // Clear everything — data will come from Dart's FlogStore
        self.reset_session();

        // Switch active app
        self.active_app_id = Some(id.to_string());

        // Request Dart to replay its buffer
        if let Some(handle) = self.get_active_handle() {
            handle.send_subscribe();
        }

        self.update_source_name(id);
    }

    /// Update the source_name display for the given app ID.
    pub(super) fn update_source_name(&mut self, id: &str) {
        if let Some(app_info) = self.connected_apps.iter().find(|a| a.id == id) {
            let dev_name = self
                .discovered_devices
                .get(&app_info.device_id)
                .map(|d| d.name.as_str())
                .unwrap_or(&app_info.device_name);
            if app_info.app_version.is_empty() {
                self.source_name = format!("{} ({})", app_info.app_name, dev_name);
            } else {
                self.source_name = format!(
                    "{} v{} ({})",
                    app_info.app_name, app_info.app_version, dev_name
                );
            }
        }
    }

    /// Get the ConnectorHandle for a specific app (for sending mock/replay).
    pub fn get_active_handle(&self) -> Option<&ConnectorHandle> {
        let id = self.active_app_id.as_deref()?;
        self.connected_apps
            .iter()
            .find(|a| a.id == id)
            .map(|a| &a.handle)
    }
}
