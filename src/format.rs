//! Formatting utilities: GumTree-style node strings and JSON output.
//!
//! The node string uses GumTree's `Kind: label [start,end]` (or `Kind [start,end]`
//! when label is empty) so consumers familiar with the Java output can read it.

use serde::Serialize;

use crate::actions::Action;
use crate::mapping::Mapping;
use crate::tree::{Node, Tree};

/// Returns the GumTree-style display string for a node.
pub fn format_node(node: &Node) -> String {
    if node.label.is_empty() {
        format!("{} [{},{}]", node.kind, node.start_byte, node.end_byte)
    } else {
        format!(
            "{}: {} [{},{}]",
            node.kind, node.label, node.start_byte, node.end_byte
        )
    }
}

// ----- Serializable output types -----

#[derive(Serialize)]
struct DiffOutput {
    matches: Vec<MatchEntry>,
    actions: Vec<ActionEntry>,
}

#[derive(Serialize)]
struct MatchEntry {
    src: String,
    dest: String,
}

#[derive(Serialize)]
struct ActionEntry {
    action: &'static str,
    tree: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Serialises the full diff result to a JSON string mirroring GumTree's `-f JSON`
/// output: `{"matches": [...], "actions": [...]}`.
pub fn to_json(t1: &Tree, t2: &Tree, mapping: &Mapping, actions: &[Action]) -> String {
    let matches: Vec<MatchEntry> = mapping
        .pairs()
        .iter()
        .map(|(src, dst)| MatchEntry {
            src: format_node(t1.node(*src)),
            dest: format_node(t2.node(*dst)),
        })
        .collect();

    let action_entries: Vec<ActionEntry> =
        actions.iter().map(|a| format_action(t1, t2, a)).collect();

    let output = DiffOutput {
        matches,
        actions: action_entries,
    };

    serde_json::to_string_pretty(&output).expect("serialization cannot fail for string data")
}

fn format_action(t1: &Tree, t2: &Tree, action: &Action) -> ActionEntry {
    match action {
        Action::InsertTree {
            node,
            parent,
            position,
        } => ActionEntry {
            action: "insert-tree",
            tree: format_node(t2.node(*node)),
            parent: Some(format_node(t2.node(*parent))),
            at: Some(*position),
            label: None,
        },
        Action::InsertNode {
            node,
            parent,
            position,
        } => ActionEntry {
            action: "insert-node",
            tree: format_node(t2.node(*node)),
            parent: Some(format_node(t2.node(*parent))),
            at: Some(*position),
            label: None,
        },
        Action::DeleteTree { node } => ActionEntry {
            action: "delete-tree",
            tree: format_node(t1.node(*node)),
            parent: None,
            at: None,
            label: None,
        },
        Action::DeleteNode { node } => ActionEntry {
            action: "delete-node",
            tree: format_node(t1.node(*node)),
            parent: None,
            at: None,
            label: None,
        },
        Action::Update { node, new_label } => ActionEntry {
            action: "update-node",
            tree: format_node(t1.node(*node)),
            parent: None,
            at: None,
            label: Some(new_label.clone()),
        },
        Action::MoveTree {
            node,
            parent,
            position,
        } => ActionEntry {
            action: "move-tree",
            tree: format_node(t1.node(*node)),
            parent: Some(format_node(t2.node(*parent))),
            at: Some(*position),
            label: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TreeBuilder;

    fn one_node(kind: &str, label: &str) -> Tree {
        let mut b = TreeBuilder::new();
        let r = b.add(kind, label, None, 3, 14);
        b.build(r)
    }

    #[test]
    fn format_node_without_label() {
        let t = one_node("YamlHash", "");
        assert_eq!(format_node(t.node(t.root())), "YamlHash [3,14]");
    }

    #[test]
    fn format_node_with_label() {
        let t = one_node("YamlValue", "hello");
        assert_eq!(format_node(t.node(t.root())), "YamlValue: hello [3,14]");
    }

    #[test]
    fn json_escapes_quotes_and_backslashes() {
        let t = one_node("Leaf", r#"he said "ok\""#);
        let m = Mapping::new();
        let actions: Vec<Action> = vec![];
        let json = to_json(&t, &t, &m, &actions);
        // serde_json handles escaping; verify the output is valid JSON.
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(parsed.is_object());
    }

    #[test]
    fn json_escapes_control_characters() {
        let t1 = one_node("Leaf", "a\nb\tc");
        let t2 = one_node("Leaf", "a\nb\tc");
        let mut m = Mapping::new();
        m.link(t1.root(), t2.root());
        let actions: Vec<Action> = vec![];
        let json = to_json(&t1, &t2, &m, &actions);
        assert!(json.contains("\\n"));
        assert!(json.contains("\\t"));
    }

    #[test]
    fn to_json_has_matches_and_actions_keys() {
        let t1 = one_node("X", "");
        let t2 = one_node("X", "");
        let mut m = Mapping::new();
        m.link(t1.root(), t2.root());
        let actions: Vec<Action> = vec![];
        let s = to_json(&t1, &t2, &m, &actions);
        assert!(s.contains("\"matches\""));
        assert!(s.contains("\"actions\""));
        assert!(s.starts_with('{'));
        assert!(s.ends_with('}'));
    }

    #[test]
    fn to_json_action_includes_at_for_move() {
        let mut b = TreeBuilder::new();
        let r1 = b.add("R", "", None, 0, 10);
        let c1 = b.add("C", "", Some(r1), 0, 5);
        let t1 = b.build(r1);

        let mut b = TreeBuilder::new();
        let r2 = b.add("R", "", None, 0, 10);
        let c2 = b.add("C", "", Some(r2), 5, 10);
        let t2 = b.build(r2);

        let mut m = Mapping::new();
        m.link(r1, r2);
        m.link(c1, c2);
        let actions = vec![Action::MoveTree {
            node: c1,
            parent: r2,
            position: 0,
        }];
        let s = to_json(&t1, &t2, &m, &actions);
        assert!(s.contains("\"at\": 0"));
        assert!(s.contains("\"action\": \"move-tree\""));
    }

    #[test]
    fn to_json_action_includes_label_for_update() {
        let mut b = TreeBuilder::new();
        let r1 = b.add("R", "old", None, 0, 3);
        let t1 = b.build(r1);

        let mut b = TreeBuilder::new();
        let r2 = b.add("R", "new", None, 0, 3);
        let t2 = b.build(r2);

        let mut m = Mapping::new();
        m.link(r1, r2);
        let actions = vec![Action::Update {
            node: r1,
            new_label: "new".to_string(),
        }];
        let s = to_json(&t1, &t2, &m, &actions);
        assert!(s.contains("\"label\": \"new\""));
        assert!(s.contains("\"action\": \"update-node\""));
    }

    #[test]
    fn to_json_action_omits_parent_for_delete() {
        let mut b = TreeBuilder::new();
        let r = b.add("R", "", None, 0, 0);
        let t1 = b.build(r);

        let mut b = TreeBuilder::new();
        let r = b.add("R", "", None, 0, 0);
        let t2 = b.build(r);

        let actions = vec![Action::DeleteTree { node: t1.root() }];
        let m = Mapping::new();
        let s = to_json(&t1, &t2, &m, &actions);
        // Delete actions have only `action` and `tree` — no parent or at.
        let parsed: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        let action_obj = &parsed["actions"][0];
        assert!(action_obj.get("parent").is_none());
        assert!(action_obj.get("at").is_none());
    }

    #[test]
    fn empty_diff_produces_empty_arrays() {
        let mut b = TreeBuilder::new();
        let r = b.add("R", "", None, 0, 0);
        let t1 = b.build(r);
        let mut b = TreeBuilder::new();
        let r = b.add("R", "", None, 0, 0);
        let t2 = b.build(r);

        let m = Mapping::new();
        let actions: Vec<Action> = vec![];
        let s = to_json(&t1, &t2, &m, &actions);
        let parsed: serde_json::Value = serde_json::from_str(&s).expect("valid JSON");
        assert!(parsed["matches"].as_array().unwrap().is_empty());
        assert!(parsed["actions"].as_array().unwrap().is_empty());
    }
}
