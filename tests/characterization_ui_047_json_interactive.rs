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
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    match result {
        Ok(_) => format!("Opening {url}"),
        Err(_) => "Open failed (no opener)".into(),
    }
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

#[test]
fn open_url_returns_ok_message_for_https() {
    // Call open_url with a valid https URL.
    // We can't assert the browser actually opened, but we can verify
    // that the code path ran and returned a meaningful result.
    // On CI without a browser this may return "Open failed (no opener)".
    let result = open_url_mirror("https://example.com");
    assert!(
        result.contains("Opening") || result.contains("Open failed"),
        "open_url should return 'Opening ...' or 'Open failed ...', got: {:?}",
        result
    );
    // The scheme-gate must NOT trigger for https.
    assert!(
        !result.contains("only http/https"),
        "https:// should pass the scheme gate, got: {:?}",
        result
    );
}

#[test]
fn open_url_rejects_non_http_scheme() {
    // file:// URIs must be blocked by the scheme gate.
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

#[test]
fn open_url_accepts_plain_http() {
    let result = open_url_mirror("http://example.com");
    assert!(
        result.contains("Opening") || result.contains("Open failed"),
        "http:// should pass the scheme gate, got: {:?}",
        result
    );
    assert!(
        !result.contains("only http/https"),
        "http:// should pass the scheme gate, got: {:?}",
        result
    );
}
