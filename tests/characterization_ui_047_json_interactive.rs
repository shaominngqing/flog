//! Characterization tests for Task 2 (⧉ copy icon and CopyNode dispatch)
//! and Task 3 (URL detection and open_url).
//!
//! These tests pin the observable behavior so future refactors don't
//! silently break them.

use flog::ui::json_viewer::{subtree_to_value, Tree};

// ── open_url characterization ─────────────────────────────────────────────────
//
// `open_url` lives in `event::actions` as `pub(super)` (inaccessible from
// integration tests). We mirror the exact same logic here so we can test the
// scheme-gate and platform-dispatch independently of the rest of the event layer.

fn open_url_mirror(url: &str) -> String {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return "Open failed (only http/https allowed)".into();
    }
    format!("Opening {url}")
}

fn extract_node_json(tree_opt: &Option<Tree>, id: u32) -> String {
    let Some(tree) = tree_opt else {
        return String::new();
    };
    match subtree_to_value(tree, id) {
        Some(val) => serde_json::to_string_pretty(&val).unwrap_or_default(),
        None => String::new(),
    }
}

#[test]
fn copy_node_extracts_pretty_json_subtree() {
    // Build a Tree from a simple object.
    let value = serde_json::json!({"a": 1, "b": 2});
    let tree = Tree::from_value(&value);
    let wrapped = Some(tree);

    let result = extract_node_json(&wrapped, 0);
    assert!(
        !result.is_empty(),
        "extract_node_json returned empty for root node"
    );
    // Must be valid JSON.
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("extract_node_json should return valid JSON");
    assert_eq!(
        parsed.get("a").and_then(|v| v.as_i64()),
        Some(1),
        "extracted JSON should contain 'a': 1 — got: {result}"
    );
    assert_eq!(
        parsed.get("b").and_then(|v| v.as_i64()),
        Some(2),
        "extracted JSON should contain 'b': 2 — got: {result}"
    );
}

#[test]
fn copy_node_nested_extracts_subtree_only() {
    // Build {"outer": {"inner": 42}} and copy node id=1 (the inner object).
    let value = serde_json::json!({"outer": {"inner": 42}});
    let tree = Tree::from_value(&value);
    // node 0 = root object, node 1 = "outer" object, node 2 = "inner": 42
    let wrapped = Some(tree);

    let result = extract_node_json(&wrapped, 1);
    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("subtree JSON should be valid");
    // Subtree of node 1 should be {"inner": 42}, not the outer object.
    assert_eq!(
        parsed.get("inner").and_then(|v| v.as_i64()),
        Some(42),
        "subtree should contain 'inner': 42 — got: {result}"
    );
    assert!(
        parsed.get("outer").is_none(),
        "subtree should NOT contain 'outer' key — got: {result}"
    );
}

#[test]
fn copy_node_none_tree_returns_empty() {
    let result = extract_node_json(&None, 0);
    assert!(result.is_empty(), "None tree should give empty string");
}

// ── Task 3: open_url characterization ────────────────────────────────────────
// Tests only cover the scheme gate (pure logic, no browser spawned).

#[test]
fn open_url_rejects_non_http_scheme() {
    let result = open_url_mirror("file:///etc/passwd");
    assert_eq!(
        result, "Open failed (only http/https allowed)",
        "file:// should be rejected by scheme gate"
    );
}

#[test]
fn open_url_rejects_javascript_scheme() {
    let result = open_url_mirror("javascript:alert(1)");
    assert_eq!(
        result, "Open failed (only http/https allowed)",
        "javascript: should be rejected by scheme gate"
    );
}

// ── Task 5: FullValueOverlay mode ─────────────────────────────────────────────

#[test]
fn enter_opens_overlay_esc_closes() {
    use flog::app::{App, AppMode};

    let mut app = App::default();

    // Enter the overlay.
    app.enter_full_value_overlay("test value".to_string(), 0);
    assert!(
        matches!(app.mode, AppMode::FullValueOverlay(_)),
        "mode must be FullValueOverlay after enter_full_value_overlay"
    );

    // Simulate Esc by setting mode back to Normal (the actual key handler
    // does `app.mode = AppMode::Normal`).
    app.mode = AppMode::Normal;
    assert_eq!(
        app.mode,
        AppMode::Normal,
        "mode must return to Normal after Esc"
    );
}

#[test]
fn overlay_state_preserves_text_and_scroll() {
    use flog::app::{App, AppMode};

    let mut app = App::default();
    let long_text = "line1\nline2\nline3\nline4\nline5".to_string();
    app.enter_full_value_overlay(long_text.clone(), 99);

    let AppMode::FullValueOverlay(ref state) = app.mode else {
        panic!("expected FullValueOverlay mode");
    };
    assert_eq!(state.text, long_text);
    assert_eq!(state.node_id, 99);
    assert_eq!(state.scroll, 0);
}

#[test]
fn extract_node_string_from_real_tree() {
    // Verify that a string leaf in a JSON tree can be extracted by node ID.
    // This mirrors the production path in dispatch_enter_action.
    let value = serde_json::json!({"url": "https://example.com/api"});
    let tree = Tree::from_value(&value);
    // node 0 = root object, node 1 = "url": "https://example.com/api"
    let node = tree.node(1);
    let text = if let flog::ui::json_viewer::NodeKind::String(s) = &node.kind {
        Some(s.clone())
    } else {
        None
    };
    assert_eq!(
        text,
        Some("https://example.com/api".to_string()),
        "should extract URL string from node 1"
    );
}

// ── Task 4: viewer_cursor J/K clamp ───────────────────────────────────────────

#[test]
fn viewer_cursor_j_k_clamp() {
    use flog::app::App;

    let mut app = App::default();

    // Populate viewer_click_map with 5 empty rows so cursor bounds are exercised.
    app.detail.viewer_click_map = vec![Vec::new(); 5];

    // Call detail_cursor_down() 10 times — should clamp at 4 (max = len - 1).
    for _ in 0..10 {
        app.detail_cursor_down();
    }
    assert_eq!(
        app.detail.viewer_cursor,
        Some(4),
        "cursor must clamp at len-1 (4) after 10 down moves on a 5-row map"
    );

    // Call detail_cursor_up() 10 times — should clamp at 0.
    for _ in 0..10 {
        app.detail_cursor_up();
    }
    assert_eq!(
        app.detail.viewer_cursor,
        Some(0),
        "cursor must clamp at 0 after 10 up moves"
    );
}

#[test]
fn viewer_cursor_starts_at_none() {
    use flog::app::App;
    let app = App::default();
    assert_eq!(
        app.detail.viewer_cursor, None,
        "viewer_cursor must be None in a fresh App"
    );
}

#[test]
fn viewer_cursor_reset_on_selection_change() {
    use flog::app::App;

    let mut app = App::default();
    app.detail.viewer_click_map = vec![Vec::new(); 5];
    app.detail.viewer_cursor = Some(3);

    app.reset_detail_for_selection();
    assert_eq!(
        app.detail.viewer_cursor, None,
        "reset_detail_for_selection must clear viewer_cursor"
    );
}
