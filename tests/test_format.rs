use diffame::actions::Action;
use diffame::format::{format_node, to_json};
use diffame::mapping::Mapping;
use diffame::tree::{Tree, TreeBuilder};

fn one_node(kind: &str, label: &str) -> Tree {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add(kind, label, None, 3, 14);
    builder.build(root_id)
}

#[test]
fn format_node_without_label() {
    let tree = one_node("YamlHash", "");
    assert_eq!(format_node(tree.node(tree.root())), "YamlHash [3,14]");
}

#[test]
fn format_node_with_label() {
    let tree = one_node("YamlValue", "hello");
    assert_eq!(
        format_node(tree.node(tree.root())),
        "YamlValue: hello [3,14]"
    );
}

#[test]
fn json_escapes_quotes_and_backslashes() {
    let tree = one_node("Leaf", r#"he said "ok\""#);
    let mapping = Mapping::new();
    let actions: Vec<Action> = vec![];
    let json = to_json(&tree, &tree, &mapping, &actions);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed.is_object());
}

#[test]
fn json_escapes_control_characters() {
    let source_tree = one_node("Leaf", "a\nb\tc");
    let destination_tree = one_node("Leaf", "a\nb\tc");
    let mut mapping = Mapping::new();
    mapping.link(source_tree.root(), destination_tree.root());
    let actions: Vec<Action> = vec![];
    let json = to_json(&source_tree, &destination_tree, &mapping, &actions);
    assert!(json.contains("\\n"));
    assert!(json.contains("\\t"));
}

#[test]
fn to_json_has_matches_and_actions_keys() {
    let source_tree = one_node("X", "");
    let destination_tree = one_node("X", "");
    let mut mapping = Mapping::new();
    mapping.link(source_tree.root(), destination_tree.root());
    let actions: Vec<Action> = vec![];
    let json_output = to_json(&source_tree, &destination_tree, &mapping, &actions);
    assert!(json_output.contains("\"matches\""));
    assert!(json_output.contains("\"actions\""));
    assert!(json_output.starts_with('{'));
    assert!(json_output.ends_with('}'));
}

#[test]
fn to_json_action_includes_at_for_move() {
    let mut builder = TreeBuilder::new();
    let source_root = builder.add("R", "", None, 0, 10);
    let source_child = builder.add("C", "", Some(source_root), 0, 5);
    let source_tree = builder.build(source_root);

    let mut builder = TreeBuilder::new();
    let destination_root = builder.add("R", "", None, 0, 10);
    let destination_child = builder.add("C", "", Some(destination_root), 5, 10);
    let destination_tree = builder.build(destination_root);

    let mut mapping = Mapping::new();
    mapping.link(source_root, destination_root);
    mapping.link(source_child, destination_child);
    let actions = vec![Action::MoveTree {
        node: source_child,
        parent: destination_root,
        position: 0,
    }];
    let json_output = to_json(&source_tree, &destination_tree, &mapping, &actions);
    assert!(json_output.contains("\"at\": 0"));
    assert!(json_output.contains("\"action\": \"move-tree\""));
}

#[test]
fn to_json_action_includes_label_for_update() {
    let mut builder = TreeBuilder::new();
    let source_root = builder.add("R", "old", None, 0, 3);
    let source_tree = builder.build(source_root);

    let mut builder = TreeBuilder::new();
    let destination_root = builder.add("R", "new", None, 0, 3);
    let destination_tree = builder.build(destination_root);

    let mut mapping = Mapping::new();
    mapping.link(source_root, destination_root);
    let actions = vec![Action::Update {
        node: source_root,
        new_label: "new".to_string(),
    }];
    let json_output = to_json(&source_tree, &destination_tree, &mapping, &actions);
    assert!(json_output.contains("\"label\": \"new\""));
    assert!(json_output.contains("\"action\": \"update-node\""));
}

#[test]
fn to_json_action_omits_parent_for_delete() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("R", "", None, 0, 0);
    let source_tree = builder.build(root_id);

    let mut builder = TreeBuilder::new();
    let root_id = builder.add("R", "", None, 0, 0);
    let destination_tree = builder.build(root_id);

    let actions = vec![Action::DeleteTree {
        node: source_tree.root(),
    }];
    let mapping = Mapping::new();
    let json_output = to_json(&source_tree, &destination_tree, &mapping, &actions);
    let parsed: serde_json::Value = serde_json::from_str(&json_output).expect("valid JSON");
    let action_obj = &parsed["actions"][0];
    assert!(action_obj.get("parent").is_none());
    assert!(action_obj.get("at").is_none());
}

#[test]
fn empty_diff_produces_empty_arrays() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("R", "", None, 0, 0);
    let source_tree = builder.build(root_id);
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("R", "", None, 0, 0);
    let destination_tree = builder.build(root_id);

    let mapping = Mapping::new();
    let actions: Vec<Action> = vec![];
    let json_output = to_json(&source_tree, &destination_tree, &mapping, &actions);
    let parsed: serde_json::Value = serde_json::from_str(&json_output).expect("valid JSON");
    assert!(parsed["matches"].as_array().unwrap().is_empty());
    assert!(parsed["actions"].as_array().unwrap().is_empty());
}
