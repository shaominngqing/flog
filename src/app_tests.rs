use super::*;

#[test]
fn logs_view_state_default_matches_initial_app_state() {
    // LogsViewState::default mirrors the values assigned in App::new():
    //   selected = 0, scroll_offset = 0, auto_scroll = true.
    let s = LogsViewState::default();
    assert_eq!(s.selected, 0);
    assert_eq!(s.scroll_offset, 0);
    assert!(s.auto_scroll);
}

#[test]
fn app_logs_field_reflects_direct_mutations() {
    // Phase 4 UI-003 completion: `app.logs` is the direct owner of the
    // Logs-tab viewport fields. Verify round-trip.
    let mut app = App::new();
    app.logs.selected = 7;
    app.logs.scroll_offset = 3;
    app.logs.auto_scroll = false;

    assert_eq!(app.logs.selected, 7);
    assert_eq!(app.logs.scroll_offset, 3);
    assert!(!app.logs.auto_scroll);
}

#[test]
fn app_logs_starts_at_default() {
    // App::new initializes `logs` to LogsViewState::default().
    let app = App::new();
    assert_eq!(app.logs, LogsViewState::default());
}
