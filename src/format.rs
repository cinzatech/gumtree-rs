//! Formatting utilities: GumTree-style node strings and JSON output.
//!
//! The node string uses GumTree's `Kind: label [start,end]` (or `Kind [start,end]`
//! when label is empty) so consumers familiar with the Java output can read it.

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

/// Serialises the full diff result to a JSON string mirroring GumTree's `-f JSON`
/// output: `{"matches": [...], "actions": [...]}`.
pub fn to_json(t1: &Tree, t2: &Tree, mapping: &Mapping, actions: &[Action]) -> String {
    let mut out = String::new();
    out.push_str("{\n");

    // matches
    out.push_str("  \"matches\": [");
    let pairs = mapping.pairs();
    if !pairs.is_empty() {
        out.push('\n');
        for (i, (src, dst)) in pairs.iter().enumerate() {
            out.push_str("    {");
            out.push_str("\"src\": \"");
            out.push_str(&escape_json(&format_node(t1.node(*src))));
            out.push_str("\", \"dest\": \"");
            out.push_str(&escape_json(&format_node(t2.node(*dst))));
            out.push_str("\"}");
            if i + 1 < pairs.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ");
    }
    out.push_str("],\n");

    // actions
    out.push_str("  \"actions\": [");
    if !actions.is_empty() {
        out.push('\n');
        for (i, action) in actions.iter().enumerate() {
            out.push_str("    ");
            out.push_str(&format_action_json(t1, t2, action));
            if i + 1 < actions.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ");
    }
    out.push_str("]\n");

    out.push('}');
    out
}

fn format_action_json(t1: &Tree, t2: &Tree, action: &Action) -> String {
    match action {
        Action::InsertTree {
            node,
            parent,
            position,
        } => format!(
            "{{\"action\": \"insert-tree\", \"tree\": \"{}\", \"parent\": \"{}\", \"at\": {}}}",
            escape_json(&format_node(t2.node(*node))),
            escape_json(&format_node(t2.node(*parent))),
            position
        ),
        Action::InsertNode {
            node,
            parent,
            position,
        } => format!(
            "{{\"action\": \"insert-node\", \"tree\": \"{}\", \"parent\": \"{}\", \"at\": {}}}",
            escape_json(&format_node(t2.node(*node))),
            escape_json(&format_node(t2.node(*parent))),
            position
        ),
        Action::DeleteTree { node } => format!(
            "{{\"action\": \"delete-tree\", \"tree\": \"{}\"}}",
            escape_json(&format_node(t1.node(*node)))
        ),
        Action::DeleteNode { node } => format!(
            "{{\"action\": \"delete-node\", \"tree\": \"{}\"}}",
            escape_json(&format_node(t1.node(*node)))
        ),
        Action::Update { node, new_label } => format!(
            "{{\"action\": \"update-node\", \"tree\": \"{}\", \"label\": \"{}\"}}",
            escape_json(&format_node(t1.node(*node))),
            escape_json(new_label)
        ),
        Action::MoveTree {
            node,
            parent,
            position,
        } => format!(
            "{{\"action\": \"move-tree\", \"tree\": \"{}\", \"parent\": \"{}\", \"at\": {}}}",
            escape_json(&format_node(t1.node(*node))),
            escape_json(&format_node(t2.node(*parent))),
            position
        ),
    }
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
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
        let s = format_node(t.node(t.root()));
        let json = format!("\"{}\"", escape_json(&s));
        // Quotes inside must be backslash-escaped.
        assert!(json.contains(r#"he said \"ok\\\""#));
    }

    #[test]
    fn json_escapes_control_characters() {
        let escaped = escape_json("a\nb\tc");
        assert!(escaped.contains("\\n"));
        assert!(escaped.contains("\\t"));
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
        // Delete actions have only `action` and `tree`.
        let action_line = s
            .lines()
            .find(|l| l.contains("delete-tree"))
            .expect("delete line");
        assert!(!action_line.contains("\"parent\""));
        assert!(!action_line.contains("\"at\""));
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
        assert!(s.contains("\"matches\": []"));
        assert!(s.contains("\"actions\": []"));
    }
}
