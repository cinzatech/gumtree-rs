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
    let mut b = TreeBuilder::new();
    let mut ids = Vec::with_capacity(spec.len());
    for (kind, label, p, s, e) in spec {
        let parent = if *p < 0 { None } else { Some(ids[*p as usize]) };
        ids.push(b.add(kind, label, parent, *s, *e));
    }
    (b.build(ids[0]), ids)
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
    let (t1, _) = make(&spec);
    let (t2, _) = make(&spec);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    assert_eq!(r.mapping.len(), r.src_tree.node_count());
    assert_eq!(r.mapping.len(), r.dst_tree.node_count());
    assert!(r.actions.is_empty(), "got {:?}", r.actions);
}

#[test]
fn single_label_change_emits_exactly_one_update() {
    let (t1, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "old", 1, 1, 4),
    ]);
    let (t2, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "new", 1, 1, 4),
    ]);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    assert_eq!(r.actions.len(), 1);
    match &r.actions[0] {
        Action::Update { new_label, .. } => assert_eq!(new_label, "new"),
        other => panic!("expected Update, got {:?}", other),
    }
}

#[test]
fn pure_insertion_emits_insert_tree_not_per_node() {
    let (t1, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
    ]);
    let (t2, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("inner", "", 3, 11, 19),
        ("leaf", "2", 4, 12, 13),
    ]);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    // Expect exactly one insert (the whole new subtree).
    let inserts: Vec<&Action> = r
        .actions
        .iter()
        .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
        .collect();
    assert_eq!(inserts.len(), 1);
    assert!(matches!(inserts[0], Action::InsertTree { .. }));
    // No spurious updates or deletes.
    for a in &r.actions {
        assert!(matches!(a, Action::InsertTree { .. }));
    }
}

#[test]
fn pure_deletion_emits_delete_tree_not_per_node() {
    let (t1, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("inner", "", 3, 11, 19),
        ("leaf", "2", 4, 12, 13),
    ]);
    let (t2, _) = make(&[
        ("root", "", -1, 0, 10),
        ("a", "", 0, 0, 5),
        ("leaf", "1", 1, 1, 2),
    ]);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    let deletes: Vec<&Action> = r
        .actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
        .collect();
    assert_eq!(deletes.len(), 1);
    assert!(matches!(deletes[0], Action::DeleteTree { .. }));
}

#[test]
fn move_to_new_parent_emits_move_tree() {
    // T1: (root (a (item v)) (b))
    // T2: (root (a)          (b (item v)))   — `item` moved from a to b
    let (t1, _ids1) = make(&[
        ("root", "", -1, 0, 30),
        ("a", "", 0, 0, 15),
        ("item", "", 1, 1, 14),
        ("leaf", "v", 2, 2, 3),
        ("b", "", 0, 15, 30),
    ]);
    let (t2, _ids2) = make(&[
        ("root", "", -1, 0, 30),
        ("a", "", 0, 0, 5),
        ("b", "", 0, 5, 30),
        ("item", "", 2, 6, 28),
        ("leaf", "v", 3, 7, 8),
    ]);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    let moves: Vec<&Action> = r
        .actions
        .iter()
        .filter(|a| matches!(a, Action::MoveTree { .. }))
        .collect();
    assert!(!moves.is_empty(), "actions: {:?}", r.actions);
    // No inserts or deletes for the moved content.
    let inserts = r
        .actions
        .iter()
        .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
        .count();
    let deletes = r
        .actions
        .iter()
        .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
        .count();
    assert_eq!(inserts, 0);
    assert_eq!(deletes, 0);
}

#[test]
fn mixed_change_produces_each_action_kind() {
    // T1: (root (keep "k") (mod "old") (drop "d"))
    let (t1, _) = make(&[
        ("root", "", -1, 0, 40),
        ("keep", "k", 0, 0, 10),
        ("mod", "old", 0, 10, 20),
        ("drop", "d", 0, 20, 30),
    ]);
    // T2: (root (keep "k") (mod "new") (add "a"))
    let (t2, _) = make(&[
        ("root", "", -1, 0, 40),
        ("keep", "k", 0, 0, 10),
        ("mod", "new", 0, 10, 20),
        ("add", "a", 0, 20, 30),
    ]);

    let r = diff_trees(t1, t2, &DiffOptions::default());
    let kinds: std::collections::HashSet<&str> = r.actions.iter().map(|a| a.action_str()).collect();
    assert!(kinds.contains("update-node"), "{:?}", r.actions);
    assert!(
        kinds.contains("insert-tree") || kinds.contains("insert-node"),
        "{:?}",
        r.actions
    );
    assert!(
        kinds.contains("delete-tree") || kinds.contains("delete-node"),
        "{:?}",
        r.actions
    );
}

#[test]
fn matches_are_bijective() {
    let (t1, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 10),
        ("leaf", "v", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("leaf", "w", 3, 11, 12),
    ]);
    let (t2, _) = make(&[
        ("root", "", -1, 0, 20),
        ("a", "", 0, 0, 10),
        ("leaf", "v", 1, 1, 2),
        ("b", "", 0, 10, 20),
        ("leaf", "w", 3, 11, 12),
    ]);
    let r = diff_trees(t1, t2, &DiffOptions::default());

    let mut srcs = std::collections::HashSet::new();
    let mut dsts = std::collections::HashSet::new();
    for (s, d) in r.mapping.pairs() {
        assert!(srcs.insert(s), "src {} appeared twice", s);
        assert!(dsts.insert(d), "dst {} appeared twice", d);
    }
}

#[test]
fn json_output_round_trip_has_correct_structure() {
    let (t1, _) = make(&[("root", "", -1, 0, 10), ("leaf", "x", 0, 1, 2)]);
    let (t2, _) = make(&[("root", "", -1, 0, 10), ("leaf", "y", 0, 1, 2)]);
    let r = diff_trees(t1, t2, &DiffOptions::default());
    let json = gumtree_rs::format::to_json(&r.src_tree, &r.dst_tree, &r.mapping, &r.actions);

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
    let (t1, _) = make(&[
        ("root", "", -1, 0, 0),
        ("alpha", "", 0, 0, 0),
        ("leaf", "v", 1, 0, 0),
    ]);
    let (t2, _) = make(&[
        ("differs", "", -1, 0, 0),
        ("alpha", "", 0, 0, 0),
        ("leaf", "v", 1, 0, 0),
    ]);

    let strict = DiffOptions {
        match_options: gumtree_rs::matcher::MatchOptions {
            min_height: 5,
            ..Default::default()
        },
    };
    let r_strict = diff_trees(t1.clone(), t2.clone(), &strict);
    let r_default = diff_trees(t1, t2, &DiffOptions::default());

    // With a very high min_height threshold, top-down picks up nothing; only
    // bottom-up might find the alpha subtree via its descendant. Either way
    // the strict result has at most as many mappings as the default.
    assert!(r_strict.mapping.len() <= r_default.mapping.len());
}
