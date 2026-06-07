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
    // T2: (root (a)          (b (item v)))     `item` moved from a to b
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

// ---------------------------------------------------------------------------
// Line-based diff (language-agnostic fallback) end-to-end tests
// ---------------------------------------------------------------------------

mod line_diff {
    use gumtree_rs::actions::Action;
    use gumtree_rs::{diff_lines, DiffOptions};

    #[test]
    fn identical_files_produce_no_actions() {
        let source = b"aaa\nbbb\nccc\n";
        let result = diff_lines(source, source, &DiffOptions::default()).unwrap();
        assert!(result.actions.is_empty(), "got {:?}", result.actions);
    }

    #[test]
    fn added_lines_produce_insert_actions() {
        let old = b"aaa\nccc\n";
        let new = b"aaa\nbbb\nccc\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let inserts: Vec<_> = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .collect();
        assert!(
            !inserts.is_empty(),
            "expected inserts, got {:?}",
            result.actions
        );

        // The inserted node in the destination tree should carry the new line's content.
        let inserted_label = match &inserts[0] {
            Action::InsertTree { node, .. } | Action::InsertNode { node, .. } => {
                result.dst_tree.node(*node).label.clone()
            }
            _ => unreachable!(),
        };
        assert_eq!(inserted_label, "bbb");

        // No deletes, the old lines are still present.
        let deletes = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(deletes, 0);
    }

    #[test]
    fn removed_lines_produce_delete_actions() {
        let old = b"aaa\nbbb\nccc\n";
        let new = b"aaa\nccc\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let deletes: Vec<_> = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .collect();
        assert!(
            !deletes.is_empty(),
            "expected deletes, got {:?}",
            result.actions
        );

        let deleted_label = match &deletes[0] {
            Action::DeleteTree { node } | Action::DeleteNode { node } => {
                result.src_tree.node(*node).label.clone()
            }
            _ => unreachable!(),
        };
        assert_eq!(deleted_label, "bbb");
    }

    #[test]
    fn completely_different_files_delete_all_old_and_insert_all_new() {
        let old = b"aaa\nbbb\n";
        let new = b"xxx\nyyy\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let insert_count = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .count();
        let delete_count = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(delete_count, 2);
        assert_eq!(insert_count, 2);
    }

    #[test]
    fn empty_files_produce_no_actions() {
        let result = diff_lines(b"", b"", &DiffOptions::default()).unwrap();
        assert!(result.actions.is_empty());
    }

    #[test]
    fn empty_to_nonempty_inserts_all_lines() {
        let result = diff_lines(b"", b"hello\nworld\n", &DiffOptions::default()).unwrap();
        let insert_count = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .count();
        assert_eq!(insert_count, 2);
    }

    #[test]
    fn nonempty_to_empty_deletes_all_lines() {
        let result = diff_lines(b"hello\nworld\n", b"", &DiffOptions::default()).unwrap();
        let delete_count = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(delete_count, 2);
    }

    #[test]
    fn mappings_are_bijective() {
        let old = b"aaa\nbbb\nccc\nddd\n";
        let new = b"aaa\nxxx\nccc\nyyy\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let mut sources = std::collections::HashSet::new();
        let mut destinations = std::collections::HashSet::new();
        for (source, destination) in result.mapping.pairs() {
            assert!(sources.insert(source), "source {} mapped twice", source);
            assert!(
                destinations.insert(destination),
                "destination {} mapped twice",
                destination
            );
        }
    }

    #[test]
    fn json_output_uses_standard_vocabulary() {
        let old = b"aaa\nbbb\n";
        let new = b"aaa\nccc\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();
        let json = gumtree_rs::format::to_json(
            &result.src_tree,
            &result.dst_tree,
            &result.mapping,
            &result.actions,
        );

        assert!(json.contains("\"matches\""), "missing matches key");
        assert!(json.contains("\"actions\""), "missing actions key");
        // The standard action names should appear, not any line-diff-specific ones.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let actions = parsed["actions"].as_array().unwrap();
        for action in actions {
            let action_name = action["action"].as_str().unwrap();
            assert!(
                [
                    "insert-tree",
                    "insert-node",
                    "delete-tree",
                    "delete-node",
                    "update-node",
                    "move-tree"
                ]
                .contains(&action_name),
                "unexpected action type: {}",
                action_name
            );
        }
    }

    #[test]
    fn file_size_limit_is_enforced() {
        let options = DiffOptions {
            max_file_size: 5,
            ..DiffOptions::default()
        };
        let result = diff_lines(b"this is too long", b"short", &options);
        match result {
            Err(message) => assert!(
                message.contains("max file size"),
                "unexpected error: {}",
                message
            ),
            Ok(_) => panic!("expected an error for oversized input"),
        }
    }

    #[test]
    fn swapped_lines_produce_move_not_insert_delete() {
        let old = b"Foo\nBar\nBaz\n";
        let new = b"Foo\nBaz\nBar\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let moves: Vec<_> = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::MoveTree { .. }))
            .collect();
        assert!(
            !moves.is_empty(),
            "expected move-tree, got {:?}",
            result.actions
        );

        // No inserts or deletes, both lines are still present, just reordered.
        let inserts = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .count();
        let deletes = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(inserts, 0, "unexpected inserts: {:?}", result.actions);
        assert_eq!(deletes, 0, "unexpected deletes: {:?}", result.actions);
    }

    #[test]
    fn similar_line_produces_update_not_insert_delete() {
        let old = b"Foo\nBarbaz\nBaz\n";
        let new = b"Foo\nBar baz\nBaz\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let updates: Vec<_> = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::Update { .. }))
            .collect();
        assert!(
            !updates.is_empty(),
            "expected update-node, got {:?}",
            result.actions
        );

        match &updates[0] {
            Action::Update { new_label, .. } => assert_eq!(new_label, "Bar baz"),
            other => panic!("expected Update, got {:?}", other),
        }

        // No inserts or deletes for the modified line.
        let inserts = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .count();
        let deletes = result
            .actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(inserts, 0, "unexpected inserts: {:?}", result.actions);
        assert_eq!(deletes, 0, "unexpected deletes: {:?}", result.actions);
    }

    #[test]
    fn move_and_update_combined() {
        // Line moves AND another line changes content.
        let old = b"alpha\nbeta_value\ngamma\n";
        let new = b"gamma\nbeta_Value\nalpha\n";
        let result = diff_lines(old, new, &DiffOptions::default()).unwrap();

        let action_kinds: std::collections::HashSet<&str> =
            result.actions.iter().map(|a| a.action_str()).collect();

        // "alpha" and "gamma" moved; "beta_value" → "beta_Value" is an update.
        assert!(
            action_kinds.contains("move-tree"),
            "expected move-tree: {:?}",
            result.actions
        );
        assert!(
            action_kinds.contains("update-node"),
            "expected update-node: {:?}",
            result.actions
        );

        // No inserts or deletes, everything is accounted for by moves and updates.
        assert!(
            !action_kinds.contains("insert-tree") && !action_kinds.contains("insert-node"),
            "unexpected inserts: {:?}",
            result.actions
        );
        assert!(
            !action_kinds.contains("delete-tree") && !action_kinds.contains("delete-node"),
            "unexpected deletes: {:?}",
            result.actions
        );
    }
}
