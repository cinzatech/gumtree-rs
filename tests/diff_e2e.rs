//! End-to-end behavioural tests that exercise the full public API.
//!
//! These tests build trees with [`TreeBuilder`] (no tree-sitter required) and
//! verify the resulting diff makes sense at the level of mappings and actions.

use gumtree_rs::actions::Action;
use gumtree_rs::tree::{NodeId, Tree, TreeBuilder};
use gumtree_rs::{diff_trees, DiffOptions};

/// Quick-and-dirty tree builder for tests, accepting a sequence of
/// `(kind, label, parent_index, start, end)` where parent_index is the index
/// of the parent in the same sequence (or -1 for root).
fn make(spec: &[(&str, &str, i32, usize, usize)]) -> (Tree, Vec<NodeId>) {
    let mut builder = TreeBuilder::new();
    let mut ids = Vec::with_capacity(spec.len());
    for (kind, label, parent_index, start, end) in spec {
        let parent = if *parent_index < 0 {
            None
        } else {
            Some(ids[*parent_index as usize])
        };
        ids.push(builder.add(kind, label, parent, *start, *end));
    }
    (builder.build(ids[0]), ids)
}

#[test]
fn identical_trees_produce_full_mapping_and_no_actions() {
    let spec = [
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 10),
        ("leaf", "v", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("leaf", "w", 3, 11, 12),
    ];
    let (source_tree, _) = make(&spec);
    let (destination_tree, _) = make(&spec);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    assert_eq!(result.mapping.len(), result.src_tree.node_count());
    assert_eq!(result.mapping.len(), result.dst_tree.node_count());
    assert!(result.actions.is_empty(), "got {:?}", result.actions);
}

#[test]
fn single_label_change_emits_exactly_one_update() {
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "old", 1, 1, 4),
    ]);
    let (destination_tree, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "new", 1, 1, 4),
    ]);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    assert_eq!(result.actions.len(), 1);
    match &result.actions[0] {
        Action::Update { new_label, .. } => assert_eq!(new_label, "new"),
        other => panic!("expected Update, got {:?}", other),
    }
}

#[test]
fn pure_insertion_emits_insert_tree_not_per_node() {
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
    ]);
    let (destination_tree, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("inner", "", 3, 11, 19),
        ("leaf", "2", 4, 12, 13),
    ]);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    // Expect exactly one insert (the whole new subtree).
    let inserts: Vec<&Action> = result
        .actions
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
    // No spurious updates or deletes.
    for a in &result.actions {
        assert!(matches!(a, Action::InsertTree { .. }));
    }
}

#[test]
fn pure_deletion_emits_delete_tree_not_per_node() {
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("inner", "", 3, 11, 19),
        ("leaf", "2", 4, 12, 13),
    ]);
    let (destination_tree, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
    ]);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    let deletes: Vec<&Action> = result
        .actions
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
fn move_to_new_parent_emits_move_tree() {
    // T1: (root (a (item v)) (b))
    // T2: (root (a)          (b (item v)))   — `item` moved from a to b
    let (source_tree, _source_ids) = make(&[
        ("root", "", -1, 0, 30),
        ("a", "", 0, 0, 15),
        ("item", "", 1, 1, 14),
        ("leaf", "v", 2, 2, 3),
        ("b", "", 0, 15, 30),
    ]);
    let (destination_tree, _destination_ids) = make(&[
        ("root", "", -1, 0, 30),
        ("a", "", 0, 0, 5),
        ("b", "", 0, 5, 30),
        ("item", "", 2, 6, 28),
        ("leaf", "v", 3, 7, 8),
    ]);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    let moves: Vec<&Action> = result
        .actions
        .iter()
        .filter(|action| matches!(action, Action::MoveTree { .. }))
        .collect();
    assert!(!moves.is_empty(), "actions: {:?}", result.actions);
    // No inserts or deletes for the moved content.
    let inserts = result
        .actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::InsertTree { .. } | Action::InsertNode { .. }
            )
        })
        .count();
    let deletes = result
        .actions
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
fn mixed_change_produces_each_action_kind() {
    // T1: (root (keep "k") (mod "old") (drop "d"))
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 40),
        ("keep", "k", 0, 0, 10),
        ("mod", "old", 0, 10, 20),
        ("drop", "d", 0, 20, 30),
    ]);
    // T2: (root (keep "k") (mod "new") (add "a"))
    let (destination_tree, _) = make(&[
        ("root", "", -1, 0, 40),
        ("keep", "k", 0, 0, 10),
        ("mod", "new", 0, 10, 20),
        ("add", "a", 0, 20, 30),
    ]);

    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    let kinds: std::collections::HashSet<&str> = result
        .actions
        .iter()
        .map(|action| action.action_str())
        .collect();
    assert!(kinds.contains("update-node"), "{:?}", result.actions);
    assert!(
        kinds.contains("insert-tree") || kinds.contains("insert-node"),
        "{:?}",
        result.actions
    );
    assert!(
        kinds.contains("delete-tree") || kinds.contains("delete-node"),
        "{:?}",
        result.actions
    );
}

#[test]
fn matches_are_bijective() {
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 10),
        ("leaf", "v", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("leaf", "w", 3, 11, 12),
    ]);
    let (destination_tree, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 10),
        ("leaf", "v", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("leaf", "w", 3, 11, 12),
    ]);
    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());

    let mut srcs = std::collections::HashSet::new();
    let mut dsts = std::collections::HashSet::new();
    for (source, destination) in result.mapping.pairs() {
        assert!(srcs.insert(source), "src {} appeared twice", source);
        assert!(
            dsts.insert(destination),
            "dst {} appeared twice",
            destination
        );
    }
}

#[test]
fn json_output_round_trip_has_correct_structure() {
    let (source_tree, _) = make(&[("root", "", -1, 0, 10), ("leaf", "x", 0, 1, 2)]);
    let (destination_tree, _) = make(&[("root", "", -1, 0, 10), ("leaf", "y", 0, 1, 2)]);
    let result = diff_trees(source_tree, destination_tree, &DiffOptions::default());
    let json = gumtree_rs::format::to_json(
        &result.src_tree,
        &result.dst_tree,
        &result.mapping,
        &result.actions,
    );

    // Look for the expected top-level keys.
    assert!(json.contains("\"matches\""));
    assert!(json.contains("\"actions\""));
    // The update action should reference the new label.
    assert!(json.contains("\"label\": \"y\""));
    assert!(json.contains("\"action\": \"update-node\""));
}

#[test]
fn options_threshold_can_be_overridden() {
    // Two trees sharing only a small (height-2) subtree.
    let (source_tree, _) = make(&[
        ("root", "", -1, 0, 0),
        ("alpha", "", 0, 0, 0),
        ("leaf", "v", 1, 0, 0),
    ]);
    let (destination_tree, _) = make(&[
        ("differs", "", -1, 0, 0),
        ("alpha", "", 0, 0, 0),
        ("leaf", "v", 1, 0, 0),
    ]);

    let strict = DiffOptions {
        match_options: gumtree_rs::matcher::MatchOptions {
            min_height: 5,
            ..Default::default()
        },
        ..Default::default()
    };
    let strict_result = diff_trees(source_tree.clone(), destination_tree.clone(), &strict);
    let default_result = diff_trees(source_tree, destination_tree, &DiffOptions::default());

    // With a very high min_height threshold, top-down picks up nothing; only
    // bottom-up might find the alpha subtree via its descendant. Either way
    // the strict result has at most as many mappings as the default.
    assert!(strict_result.mapping.len() <= default_result.mapping.len());
}
