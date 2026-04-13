use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::app::App;
use crate::domain::LogLevel;

#[derive(Serialize, Deserialize, Default)]
pub struct SessionData {
    pub min_level: u8,
    pub tag_filter_input: String, // 原始输入字符串，加载时走 parse_tag_filter
    pub search_query: String,
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

pub fn load_session(app: &mut App) {
    let path = session_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let data: SessionData = match toml::from_str(&content) {
        Ok(d) => d,
        Err(_) => return,
    };

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

    app.bookmarks = data.bookmarks.into_iter().collect::<BTreeSet<_>>();

    app.active_tab = match data.active_tab {
        1 => crate::app::ViewTab::Network,
        _ => crate::app::ViewTab::Logs,
    };

    app.invalidate_filter();
}

pub fn save_session(app: &App) {
    // 重建 tag filter 输入字符串
    let tag_filter_input: String = app
        .filter
        .tag_include
        .iter()
        .cloned()
        .chain(app.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
        .collect::<Vec<_>>()
        .join(",");

    let data = SessionData {
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
        bookmarks: app.bookmarks.iter().copied().collect(),
        active_tab: match app.active_tab {
            crate::app::ViewTab::Logs => 0,
            crate::app::ViewTab::Network => 1,
        },
    };

    let path = session_path();
    if let Ok(content) = toml::to_string_pretty(&data) {
        let _ = std::fs::write(path, content);
    }
}
