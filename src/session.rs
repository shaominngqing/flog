use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::app::App;
use crate::domain::LogLevel;

#[derive(Serialize, Deserialize, Default)]
pub struct SessionData {
    pub min_level: u8,
    pub tag_filter_input: String, // 原始输入字符串，加载时走 parse_tag_filter
    pub search_query: String,
    #[serde(default)]
    pub exclude_query: String,
    pub bookmarks: Vec<usize>,
    pub active_tab: u8, // 0 = Logs, 1 = Network
}

fn session_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("flog");
    let _ = std::fs::create_dir_all(&config_dir);
    config_dir.join("session.toml")
}

/// Build a `SessionData` from the current `App` state.
///
/// Pure / deterministic — extracted from `save_session` so the u8 level
/// mapping (DOM-021) and tag filter reconstruction (DOM-022) can be tested
/// without touching the filesystem.
pub fn session_data_from_app(app: &App) -> SessionData {
    let tag_filter_input: String = app
        .filter
        .tag_include
        .iter()
        .cloned()
        .chain(app.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
        .collect::<Vec<_>>()
        .join(",");

    SessionData {
        min_level: match app.filter.min_level {
            LogLevel::System => 0,
            LogLevel::Verbose => 1,
            LogLevel::Debug => 2,
            LogLevel::Info => 3,
            LogLevel::Warning => 4,
            LogLevel::Error => 5,
        },
        tag_filter_input,
        search_query: app.filter.search_query.clone(),
        exclude_query: app.filter.exclude_query.clone(),
        bookmarks: app.bookmarks.iter().copied().collect(),
        active_tab: match app.active_tab {
            crate::app::ViewTab::Logs => 0,
            crate::app::ViewTab::Network => 1,
        },
    }
}

/// Apply a `SessionData` to an `App` in place.
///
/// Pure / deterministic — extracted from `load_session` so the u8 level
/// mapping (DOM-021) and tag filter reconstruction (DOM-022) can be tested
/// without touching the filesystem.
pub fn apply_session_data(app: &mut App, data: SessionData) {
    app.filter.min_level = match data.min_level {
        0 => LogLevel::System,
        1 => LogLevel::Verbose,
        2 => LogLevel::Debug,
        3 => LogLevel::Info,
        4 => LogLevel::Warning,
        5 => LogLevel::Error,
        _ => LogLevel::System,
    };

    if !data.tag_filter_input.is_empty() {
        app.filter.parse_tag_filter(&data.tag_filter_input);
    }

    if !data.search_query.is_empty() {
        app.filter.set_search(&data.search_query);
    }

    if !data.exclude_query.is_empty() {
        app.filter.set_exclude(&data.exclude_query);
    }

    app.bookmarks = data.bookmarks.into_iter().collect::<BTreeSet<_>>();

    app.active_tab = match data.active_tab {
        1 => crate::app::ViewTab::Network,
        _ => crate::app::ViewTab::Logs,
    };

    app.invalidate_filter();
}

/// Load session state from a specific path. Returns `Ok(())` when the
/// session was applied, `Err(_)` when the file is missing or malformed.
/// The `App` is left unchanged on error.
pub fn load_session_from_path(app: &mut App, path: &Path) -> Result<(), SessionLoadError> {
    let content = std::fs::read_to_string(path).map_err(SessionLoadError::Io)?;
    let data: SessionData = toml::from_str(&content).map_err(SessionLoadError::Parse)?;
    apply_session_data(app, data);
    Ok(())
}

/// Save session state to a specific path.
pub fn save_session_to_path(app: &App, path: &Path) -> std::io::Result<()> {
    let data = session_data_from_app(app);
    let content =
        toml::to_string_pretty(&data).map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::write(path, content)
}

/// Errors returned by `load_session_from_path`.
#[derive(Debug)]
pub enum SessionLoadError {
    Io(#[allow(dead_code)] std::io::Error),
    Parse(#[allow(dead_code)] toml::de::Error),
}

pub fn load_session(app: &mut App) {
    let _ = load_session_from_path(app, &session_path());
}

pub fn save_session(app: &App) {
    let _ = save_session_to_path(app, &session_path());
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
