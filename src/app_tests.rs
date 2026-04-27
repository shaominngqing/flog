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
fn app_logs_accessor_reads_top_level_fields() {
    // Phase 3 Step 3.10: `app.logs()` projects the top-level fields into
    // a LogsViewState snapshot. Verify projection is identity.
    let mut app = App::new();
    app.selected = 7;
    app.scroll_offset = 3;
    app.auto_scroll = false;

    let s = app.logs();
    assert_eq!(s.selected, 7);
    assert_eq!(s.scroll_offset, 3);
    assert!(!s.auto_scroll);
}

#[test]
fn app_logs_reflects_mutations_to_top_level_fields() {
    // Mutation via the top-level App fields (the existing API) must
    // flow into the projected LogsViewState snapshot.
    let mut app = App::new();
    assert_eq!(app.logs(), LogsViewState::default());

    app.selected = 42;
    assert_eq!(app.logs().selected, 42);

    app.scroll_offset = 100;
    assert_eq!(app.logs().scroll_offset, 100);

    app.auto_scroll = false;
    assert!(!app.logs().auto_scroll);
}
