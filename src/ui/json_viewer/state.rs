//! Fold state for a JSON tree.
//!
//! State is a parallel `Vec<bool>` indexed by node ID. Leaves are always
//! false and unused; only container nodes' entries matter.

use super::tree::{NodeKind, Tree};

#[derive(Default, Clone)]
pub struct JsonViewerState {
    /// `expanded[id] == true` iff container node `id` is currently expanded.
    /// Length equals `tree.nodes.len()` after `init_state`.
    pub expanded: Vec<bool>,
}

impl JsonViewerState {
    pub fn is_expanded(&self, id: u32) -> bool {
        self.expanded.get(id as usize).copied().unwrap_or(false)
    }
}

/// Create initial state. Every container with `depth <= default_expand_depth`
/// starts expanded; all others collapsed.
pub fn init_state(tree: &Tree, default_expand_depth: u32) -> JsonViewerState {
    let mut expanded = vec![false; tree.nodes.len()];
    for (i, node) in tree.nodes.iter().enumerate() {
        let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);
        if is_container && node.depth <= default_expand_depth {
            expanded[i] = true;
        }
    }
    JsonViewerState { expanded }
}

/// Toggle `node_id`. No-op if `node_id` is out of bounds or is a leaf.
/// Returns `true` iff the state changed.
pub fn toggle(tree: &Tree, state: &mut JsonViewerState, node_id: u32) -> bool {
    let idx = node_id as usize;
    if idx >= state.expanded.len() {
        return false;
    }
    let kind = &tree.nodes[idx].kind;
    if !matches!(kind, NodeKind::Object | NodeKind::Array) {
        return false;
    }
    state.expanded[idx] = !state.expanded[idx];
    true
}

/// Expand every container. Auto-resizes `state.expanded` to match the tree
/// so a stale (shorter) state won't panic — it just extends with `false`.
#[allow(dead_code)]
pub fn expand_all(tree: &Tree, state: &mut JsonViewerState) {
    if state.expanded.len() < tree.nodes.len() {
        state.expanded.resize(tree.nodes.len(), false);
    }
    for (i, node) in tree.nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Object | NodeKind::Array) {
            state.expanded[i] = true;
        }
    }
}

/// Collapse every container **except the root** — the root stays expanded
/// so the panel always shows at least its top-level keys (otherwise the
/// user sees just `{…}` with nothing clickable).
///
/// Auto-resizes `state.expanded` to match the tree, same as `expand_all`.
#[allow(dead_code)]
pub fn collapse_all(tree: &Tree, state: &mut JsonViewerState) {
    if state.expanded.len() < tree.nodes.len() {
        state.expanded.resize(tree.nodes.len(), false);
    }
    for (i, node) in tree.nodes.iter().enumerate() {
        if matches!(node.kind, NodeKind::Object | NodeKind::Array) {
            state.expanded[i] = i == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::parse;
    use super::*;

    #[test]
    fn init_expands_within_depth() {
        // Root object (depth 0) + child object (depth 1) + grandchild (depth 2)
        let t = parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        let s = init_state(&t, 1);
        assert!(s.expanded[0]); // root
        assert!(s.expanded[1]); // a (depth 1)
        assert!(!s.expanded[2]); // b (depth 2)
    }

    #[test]
    fn init_depth_zero_only_root() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let s = init_state(&t, 0);
        assert!(s.expanded[0]);
        assert!(!s.expanded[1]);
    }

    #[test]
    fn toggle_flips_container() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!s.expanded[1]);
        assert!(toggle(&t, &mut s, 1));
        assert!(s.expanded[1]);
        assert!(toggle(&t, &mut s, 1));
        assert!(!s.expanded[1]);
    }

    #[test]
    fn toggle_leaf_is_noop() {
        let t = parse(r#"{"a": 1}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!toggle(&t, &mut s, 1)); // a's value is a leaf number
    }

    #[test]
    fn toggle_out_of_bounds_is_noop() {
        let t = parse(r#"{}"#).unwrap();
        let mut s = init_state(&t, 0);
        assert!(!toggle(&t, &mut s, 99));
    }

    #[test]
    fn expand_all_sets_all_containers() {
        let t = parse(r#"{"a": {"b": {"c": 1}}}"#).unwrap();
        let mut s = init_state(&t, 0);
        expand_all(&t, &mut s);
        assert!(s.expanded[0]);
        assert!(s.expanded[1]);
        assert!(s.expanded[2]);
    }

    #[test]
    fn collapse_all_leaves_root_expanded() {
        let t = parse(r#"{"a": {"b": 1}}"#).unwrap();
        let mut s = init_state(&t, 5);
        collapse_all(&t, &mut s);
        assert!(s.expanded[0]); // root stays
        assert!(!s.expanded[1]); // a collapses
    }

    #[test]
    fn bulk_ops_tolerate_stale_shorter_state() {
        // Build state against a small tree, then use it with a larger tree.
        let small = parse("{}").unwrap();
        let mut s = init_state(&small, 5);
        assert_eq!(s.expanded.len(), 1);
        let big = parse(r#"{"a": {"b": 1}}"#).unwrap();
        // Must not panic; must resize.
        expand_all(&big, &mut s);
        assert_eq!(s.expanded.len(), big.nodes.len());
        assert!(s.expanded[0]); // root
        assert!(s.expanded[1]); // "a" object
        collapse_all(&big, &mut s);
        assert!(s.expanded[0]);
        assert!(!s.expanded[1]);
    }
}
