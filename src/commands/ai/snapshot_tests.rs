use super::*;
use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus};
use crate::domain::{LogEntry, LogLevel};

#[test]
fn build_snapshot_counts_logs_errors_and_network() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Error, "Repo", "failed".to_string()));
    let mut net = NetworkEntry::new_http(42, "GET".to_string(), "/x".to_string(), String::new());
    net.status = NetworkStatus::Failed;
    app.network_store.push_entry(net);

    let snapshot = build_snapshot(&app, SnapshotBuildOptions::for_tests());

    assert_eq!(snapshot.summary.logs, 1);
    assert_eq!(snapshot.summary.errors, 1);
    assert_eq!(snapshot.summary.network, 1);
    assert_eq!(snapshot.summary.failed_requests, 1);
    assert_eq!(snapshot.logs[0]["id"], "log#0");
    assert_eq!(snapshot.network[0]["id"], "net#42");
    assert_eq!(snapshot.notable[0]["kind"], "error_log");
}

#[test]
fn build_snapshot_respects_last_limit() {
    let mut app = App::new();
    app.add_entry(LogEntry::new(LogLevel::Info, "A", "one".to_string()));
    app.add_entry(LogEntry::new(LogLevel::Info, "A", "two".to_string()));

    let snapshot = build_snapshot(
        &app,
        SnapshotBuildOptions {
            last: 1,
            ..SnapshotBuildOptions::for_tests()
        },
    );

    assert_eq!(snapshot.logs.len(), 1);
    assert_eq!(snapshot.logs[0]["message"], "two");
}
