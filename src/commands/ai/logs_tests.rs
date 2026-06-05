use crate::app::App;
use crate::domain::{LogEntry, LogLevel};

use super::{build_logs_list, LogsListOptions};

#[test]
fn logs_list_filters_by_level_tag_search_and_last() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Info, "Network", "boot ok"));
    app.add_entry(LogEntry::new(LogLevel::Error, "Auth", "token expired"));
    app.add_entry(LogEntry::new(
        LogLevel::Error,
        "Network",
        "request timeout one",
    ));
    app.add_entry(LogEntry::new(
        LogLevel::Error,
        "Network",
        "request timeout two",
    ));

    let logs = build_logs_list(
        &app,
        LogsListOptions {
            last: 1,
            level: Some(LogLevel::Error),
            tag: Some("Network".to_string()),
            search: Some("timeout".to_string()),
        },
    );

    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0]["id"], "log#3");
    assert_eq!(logs[0]["level"], "ERROR");
    assert_eq!(logs[0]["tag"], "Network");
    assert_eq!(logs[0]["message"], "request timeout two");
}

#[test]
fn logs_list_uses_preview_instead_of_unbounded_message() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Info, "Long", "x".repeat(700)));

    let logs = build_logs_list(
        &app,
        LogsListOptions {
            last: 10,
            level: None,
            tag: None,
            search: None,
        },
    );

    assert_eq!(logs[0]["message"].as_str().unwrap().chars().count(), 500);
    assert_eq!(logs[0]["message_truncated"], true);
    assert_eq!(logs[0]["original_bytes"], 700);
}
