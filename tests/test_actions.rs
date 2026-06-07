use gumtree_rs::actions::{generate_actions, Action};
use gumtree_rs::mapping::Mapping;
use gumtree_rs::matcher::{match_trees, MatchOptions};
use gumtree_rs::tree::Tree;
use gumtree_rs::tree::TreeBuilder;

fn diff(source_tree: &Tree, destination_tree: &Tree) -> (Mapping, Vec<Action>) {
    let mapping = match_trees(source_tree, destination_tree, MatchOptions::default());
    let actions = generate_actions(source_tree, destination_tree, &mapping);
    (mapping, actions)
}

#[test]
fn identical_trees_yield_no_actions() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "v", Some(branch), 0, 0);
    let source_tree = builder.build(root_id);

    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "v", Some(branch), 0, 0);
    let destination_tree = builder.build(root_id);

    let (_, actions) = diff(&source_tree, &destination_tree);
    assert!(actions.is_empty(), "got {:?}", actions);
}

#[test]
fn label_change_emits_update() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch = builder.add("a", "", Some(root_id), 0, 0);
    let old_leaf = builder.add("leaf", "old", Some(branch), 0, 0);
    let _ = old_leaf;
    let source_tree = builder.build(root_id);

    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "new", Some(branch), 0, 0);
    let destination_tree = builder.build(root_id);

    let (_, actions) = diff(&source_tree, &destination_tree);
    // Exactly one update-node action.
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::Update { new_label, .. } => assert_eq!(new_label, "new"),
        other => panic!("expected Update, got {:?}", other),
    }
}

#[test]
fn pure_insertion_emits_insert_tree() {
    // T1: (root (a 1))
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let source_tree = builder.build(root_id);

    // T2: (root (a 1) (b 2))
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let branch_b = builder.add("b", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
    let destination_tree = builder.build(root_id);

    let (_, actions) = diff(&source_tree, &destination_tree);
    let inserts: Vec<&Action> = actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::InsertTree { .. } | Action::InsertNode { .. }
            )
        })
        .collect();
    assert_eq!(inserts.len(), 1);
    assert!(matches!(inserts[0], Action::InsertTree { .. }));
}

#[test]
fn pure_deletion_emits_delete_tree() {
    // T1: (root (a 1) (b 2))
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let branch_b = builder.add("b", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
    let source_tree = builder.build(root_id);

    // T2: (root (a 1))
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let destination_tree = builder.build(root_id);

    let (_, actions) = diff(&source_tree, &destination_tree);
    let deletes: Vec<&Action> = actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::DeleteTree { .. } | Action::DeleteNode { .. }
            )
        })
        .collect();
    assert_eq!(deletes.len(), 1);
    assert!(matches!(deletes[0], Action::DeleteTree { .. }));
}

#[test]
fn sibling_reorder_emits_move_tree() {
    // T1: (root (a 1) (b 2))
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let branch_b = builder.add("b", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
    let source_tree = builder.build(root_id);

    // T2: (root (b 2) (a 1)) — swapped order
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("root", "", None, 0, 0);
    let branch_b = builder.add("b", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
    let branch_a = builder.add("a", "", Some(root_id), 0, 0);
    let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
    let destination_tree = builder.build(root_id);

    let (_, actions) = diff(&source_tree, &destination_tree);
    let moves: Vec<&Action> = actions
        .iter()
        .filter(|action| matches!(action, Action::MoveTree { .. }))
        .collect();
    assert!(!moves.is_empty(), "expected at least one move");
    let inserts = actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::InsertTree { .. } | Action::InsertNode { .. }
            )
        })
        .count();
    let deletes = actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::DeleteTree { .. } | Action::DeleteNode { .. }
            )
        })
        .count();
    assert_eq!(inserts, 0);
    assert_eq!(deletes, 0);
}

#[test]
fn update_action_references_t1_node() {
    let mut builder = TreeBuilder::new();
    let source_root = builder.add("root", "", None, 0, 0);
    let source_branch = builder.add("a", "", Some(source_root), 0, 0);
    let source_leaf = builder.add("leaf", "old", Some(source_branch), 0, 0);
    let source_tree = builder.build(source_root);

    let mut builder = TreeBuilder::new();
    let destination_root = builder.add("root", "", None, 0, 0);
    let destination_branch = builder.add("a", "", Some(destination_root), 0, 0);
    let _destination_leaf = builder.add("leaf", "new", Some(destination_branch), 0, 0);
    let destination_tree = builder.build(destination_root);

    let (_, actions) = diff(&source_tree, &destination_tree);
    for action in &actions {
        if let Action::Update { node, new_label } = action {
            assert_eq!(*node, source_leaf);
            assert_eq!(new_label, "new");
            return;
        }
    }
    panic!("expected an Update action");
}

#[test]
fn empty_unchanged_subtree_does_not_emit_anything() {
    // T1: (root (a 1) (b "x"))
    // T2: (root (a 1) (b "y"))
    let mut builder = TreeBuilder::new();
    let source_root = builder.add("root", "", None, 0, 0);
    let source_branch_a = builder.add("a", "", Some(source_root), 0, 0);
    let _ = builder.add("leaf", "1", Some(source_branch_a), 0, 0);
    let source_branch_b = builder.add("b", "", Some(source_root), 0, 0);
    let _ = builder.add("leaf", "x", Some(source_branch_b), 0, 0);
    let source_tree = builder.build(source_root);

    let mut builder = TreeBuilder::new();
    let destination_root = builder.add("root", "", None, 0, 0);
    let destination_branch_a = builder.add("a", "", Some(destination_root), 0, 0);
    let _ = builder.add("leaf", "1", Some(destination_branch_a), 0, 0);
    let destination_branch_b = builder.add("b", "", Some(destination_root), 0, 0);
    let _ = builder.add("leaf", "y", Some(destination_branch_b), 0, 0);
    let destination_tree = builder.build(destination_root);

    let (_, actions) = diff(&source_tree, &destination_tree);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Update { .. }));
}
