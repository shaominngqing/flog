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
}
