//! Flat-arena JSON tree.
//!
//! Nodes are stored in a single `Vec<FlatNode>` indexed by `u32` ID.
//! Root is always `nodes[0]`. Children IDs are stored on the parent,
//! not contiguous (DFS order means parent < child, but not always
//! consecutive because siblings' subtrees are interleaved).

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Object,
    Array,
}

#[derive(Clone, Debug)]
pub struct FlatNode {
    pub kind: NodeKind,
    pub depth: u32,
    /// Parent node ID; `None` only for the root. Currently used by tests and
    /// kept for future navigation features.
    #[allow(dead_code)]
    pub parent: Option<u32>,
    /// Child node IDs in source order. Empty for leaves.
    pub children: Vec<u32>,
    /// For object entries: the key. For array entries and root: None.
    pub key: Option<String>,
}

#[derive(Clone)]
pub struct Tree {
    pub nodes: Vec<FlatNode>,
}

#[allow(dead_code)]
impl Tree {
    pub fn root(&self) -> &FlatNode {
        &self.nodes[0]
    }
    pub fn node(&self, id: u32) -> &FlatNode {
        &self.nodes[id as usize]
    }
    pub fn is_container(&self, id: u32) -> bool {
        matches!(
            self.nodes[id as usize].kind,
            NodeKind::Object | NodeKind::Array
        )
    }
    pub fn is_empty_container(&self, id: u32) -> bool {
        self.is_container(id) && self.nodes[id as usize].children.is_empty()
    }

    /// Construct a tree from an already-parsed `serde_json::Value`.
    ///
    /// All callers obtain their `Value` via `domain::structured_parser`,
    /// which tries strict JSON first and then a tolerant Dart-Map fallback.
    pub fn from_value(value: &Value) -> Tree {
        let mut nodes: Vec<FlatNode> = Vec::new();
        build(value, None, None, 0, &mut nodes);
        Tree { nodes }
    }
}

/// Reconstruct a `serde_json::Value` for the subtree rooted at `id`.
///
/// Used by `event::actions::extract_node_json` (and tests that characterize it)
/// to copy a JSON subtree to the clipboard. The implementation is placed here
/// so the integration test can call it via the already-public `json_viewer` path
/// without duplicating the logic.
pub fn subtree_to_value(tree: &Tree, id: u32) -> Option<Value> {
    let node = tree.node(id);
    let val = match &node.kind {
        NodeKind::Null => Value::Null,
        NodeKind::Bool(b) => Value::Bool(*b),
        NodeKind::Number(s) => {
            // Re-parse the stored number string back into a serde_json::Number.
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                v
            } else {
                Value::String(s.clone())
            }
        }
        NodeKind::String(s) => Value::String(s.clone()),
        NodeKind::Object => {
            let children = node.children.clone();
            let mut map = serde_json::Map::new();
            for cid in children {
                let child_key = tree.node(cid).key.clone().unwrap_or_default();
                if let Some(child_val) = subtree_to_value(tree, cid) {
                    map.insert(child_key, child_val);
                }
            }
            Value::Object(map)
        }
        NodeKind::Array => {
            let children = node.children.clone();
            let mut arr = Vec::new();
            for cid in children {
                if let Some(child_val) = subtree_to_value(tree, cid) {
                    arr.push(child_val);
                }
            }
            Value::Array(arr)
        }
    };
    Some(val)
}

/// Strict-JSON convenience wrapper. Test-only — runtime callers go through
/// `domain::structured_parser::parse` + `Tree::from_value`.
#[cfg(test)]
pub fn parse(text: &str) -> Result<Tree, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    Ok(Tree::from_value(&value))
}

fn build(
    value: &Value,
    parent: Option<u32>,
    key: Option<String>,
    depth: u32,
    nodes: &mut Vec<FlatNode>,
) -> u32 {
    let my_id = nodes.len() as u32;
    let kind = match value {
        Value::Null => NodeKind::Null,
        Value::Bool(b) => NodeKind::Bool(*b),
        Value::Number(n) => NodeKind::Number(n.to_string()),
        Value::String(s) => NodeKind::String(s.clone()),
        Value::Object(_) => NodeKind::Object,
        Value::Array(_) => NodeKind::Array,
    };
    nodes.push(FlatNode {
        kind,
        depth,
        parent,
        children: Vec::new(),
        key,
    });
    match value {
        Value::Object(map) => {
            let mut child_ids = Vec::with_capacity(map.len());
            for (k, v) in map {
                let cid = build(v, Some(my_id), Some(k.clone()), depth + 1, nodes);
                child_ids.push(cid);
            }
            nodes[my_id as usize].children = child_ids;
        }
        Value::Array(arr) => {
            let mut child_ids = Vec::with_capacity(arr.len());
            for v in arr {
                let cid = build(v, Some(my_id), None, depth + 1, nodes);
                child_ids.push(cid);
            }
            nodes[my_id as usize].children = child_ids;
        }
        _ => {}
    }
    my_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primitive_root() {
        let t = parse("42").unwrap();
        assert_eq!(t.nodes.len(), 1);
        assert_eq!(t.nodes[0].depth, 0);
        assert_eq!(t.nodes[0].kind, NodeKind::Number("42".into()));
        assert!(t.nodes[0].children.is_empty());
    }

    #[test]
    fn parse_flat_object() {
        let t = parse(r#"{"a": 1, "b": "hi"}"#).unwrap();
        assert_eq!(t.nodes.len(), 3);
        assert_eq!(t.nodes[0].kind, NodeKind::Object);
        assert_eq!(t.nodes[0].children, vec![1, 2]);
        assert_eq!(t.nodes[1].key, Some("a".into()));
        assert_eq!(t.nodes[1].kind, NodeKind::Number("1".into()));
        assert_eq!(t.nodes[1].depth, 1);
        assert_eq!(t.nodes[1].parent, Some(0));
        assert_eq!(t.nodes[2].key, Some("b".into()));
        assert_eq!(t.nodes[2].kind, NodeKind::String("hi".into()));
    }

    #[test]
    fn parse_nested_array() {
        let t = parse(r#"{"xs": [true, null]}"#).unwrap();
        // nodes: 0=root object, 1=array xs, 2=true, 3=null
        assert_eq!(t.nodes.len(), 4);
        assert_eq!(t.nodes[1].kind, NodeKind::Array);
        assert_eq!(t.nodes[1].children, vec![2, 3]);
        assert_eq!(t.nodes[2].kind, NodeKind::Bool(true));
        assert_eq!(t.nodes[2].key, None); // array entry has no key
        assert_eq!(t.nodes[2].depth, 2);
        assert_eq!(t.nodes[3].kind, NodeKind::Null);
    }

    #[test]
    fn parse_empty_containers() {
        let t = parse(r#"{"a": [], "b": {}}"#).unwrap();
        assert_eq!(t.nodes.len(), 3);
        assert!(t.is_empty_container(1));
        assert!(t.is_empty_container(2));
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"unterminated":"#).is_err());
    }

    #[test]
    fn number_preserves_string_form() {
        let t = parse("1776684313608").unwrap();
        assert_eq!(t.nodes[0].kind, NodeKind::Number("1776684313608".into()));
    }

    #[test]
    fn parse_preserves_object_key_order() {
        // Insertion order: z, a, m, b. Alphabetical would reorder to a, b, m, z.
        let t = parse(r#"{"z":1,"a":2,"m":3,"b":4}"#).unwrap();
        let keys: Vec<&str> = t.nodes[0]
            .children
            .iter()
            .map(|&cid| t.nodes[cid as usize].key.as_deref().unwrap())
            .collect();
        assert_eq!(keys, vec!["z", "a", "m", "b"]);
    }
}
