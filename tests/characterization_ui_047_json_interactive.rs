//! Characterization tests for Task 2: ⧉ copy icon and CopyNode dispatch.
//!
//! These tests pin the observable behavior of the copy-subtree feature
//! so future refactors don't silently break it.

use flog::ui::json_viewer::{subtree_to_value, Tree};

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
