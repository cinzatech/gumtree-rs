use diffame::tree::{Tree, TreeBuilder};

/// Helper: build (a (b 1) (c 2)).
fn sample_tree() -> Tree {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("a", "", None, 0, 10);
    let branch_b = builder.add("b", "", Some(root_id), 0, 5);
    let _leaf_one = builder.add("leaf", "1", Some(branch_b), 1, 2);
    let branch_c = builder.add("c", "", Some(root_id), 5, 10);
    let _leaf_two = builder.add("leaf", "2", Some(branch_c), 6, 7);
    builder.build(root_id)
}

#[test]
fn builder_links_parent_and_children() {
    let tree = sample_tree();
    let root = tree.root();
    let root_children = &tree.node(root).children;
    assert_eq!(root_children.len(), 2);
    for child_id in root_children {
        assert_eq!(tree.node(*child_id).parent, Some(root));
    }
}

#[test]
fn root_has_no_parent() {
    let tree = sample_tree();
    assert_eq!(tree.node(tree.root()).parent, None);
}

#[test]
fn height_of_leaf_is_one() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("x", "lbl", None, 0, 1);
    let tree = builder.build(root_id);
    assert_eq!(tree.node(root_id).height, 1);
}

#[test]
fn height_of_internal_is_max_child_plus_one() {
    let tree = sample_tree();
    assert_eq!(tree.node(tree.root()).height, 3);
}

#[test]
fn size_includes_node_itself_and_all_descendants() {
    let tree = sample_tree();
    assert_eq!(tree.node(tree.root()).size, 5);
}

#[test]
fn size_of_leaf_is_one() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("x", "", None, 0, 1);
    let tree = builder.build(root_id);
    assert_eq!(tree.node(root_id).size, 1);
}

#[test]
fn hash_equal_for_structurally_identical_trees() {
    let mut builder_a = TreeBuilder::new();
    let root_a = builder_a.add("r", "", None, 0, 0);
    let _child_a = builder_a.add("c", "x", Some(root_a), 0, 0);
    let tree_a = builder_a.build(root_a);

    let mut builder_b = TreeBuilder::new();
    let root_b = builder_b.add("r", "", None, 0, 0);
    let _child_b = builder_b.add("c", "x", Some(root_b), 0, 0);
    let tree_b = builder_b.build(root_b);

    assert_eq!(
        tree_a.node(tree_a.root()).hash,
        tree_b.node(tree_b.root()).hash
    );
}

#[test]
fn hash_differs_when_labels_differ() {
    let mut builder_a = TreeBuilder::new();
    let root_a = builder_a.add("r", "", None, 0, 0);
    let _child_a = builder_a.add("c", "old", Some(root_a), 0, 0);
    let tree_a = builder_a.build(root_a);

    let mut builder_b = TreeBuilder::new();
    let root_b = builder_b.add("r", "", None, 0, 0);
    let _child_b = builder_b.add("c", "new", Some(root_b), 0, 0);
    let tree_b = builder_b.build(root_b);

    assert_ne!(
        tree_a.node(tree_a.root()).hash,
        tree_b.node(tree_b.root()).hash
    );
}

#[test]
fn hash_differs_when_child_order_differs() {
    let mut builder_a = TreeBuilder::new();
    let root_a = builder_a.add("r", "", None, 0, 0);
    let _first_a = builder_a.add("c", "1", Some(root_a), 0, 0);
    let _second_a = builder_a.add("c", "2", Some(root_a), 0, 0);
    let tree_a = builder_a.build(root_a);

    let mut builder_b = TreeBuilder::new();
    let root_b = builder_b.add("r", "", None, 0, 0);
    let _second_b = builder_b.add("c", "2", Some(root_b), 0, 0);
    let _first_b = builder_b.add("c", "1", Some(root_b), 0, 0);
    let tree_b = builder_b.build(root_b);

    assert_ne!(
        tree_a.node(tree_a.root()).hash,
        tree_b.node(tree_b.root()).hash
    );
}

#[test]
fn hash_differs_when_kinds_differ() {
    let mut builder_a = TreeBuilder::new();
    let root_a = builder_a.add("r", "", None, 0, 0);
    let tree_a = builder_a.build(root_a);

    let mut builder_b = TreeBuilder::new();
    let root_b = builder_b.add("R", "", None, 0, 0);
    let tree_b = builder_b.build(root_b);

    assert_ne!(
        tree_a.node(tree_a.root()).hash,
        tree_b.node(tree_b.root()).hash
    );
}

#[test]
fn pre_order_visits_root_first() {
    let tree = sample_tree();
    let order = tree.pre_order(tree.root());
    assert_eq!(order[0], tree.root());
    assert_eq!(order.len(), tree.node_count());
}

#[test]
fn post_order_visits_root_last() {
    let tree = sample_tree();
    let order = tree.post_order(tree.root());
    assert_eq!(*order.last().unwrap(), tree.root());
    assert_eq!(order.len(), tree.node_count());
}

#[test]
fn post_order_visits_children_before_parent() {
    let tree = sample_tree();
    let order = tree.post_order(tree.root());
    for (position, &node_id) in order.iter().enumerate() {
        for child_id in &tree.node(node_id).children {
            let child_position = order
                .iter()
                .position(|&candidate| candidate == *child_id)
                .unwrap();
            assert!(
                child_position < position,
                "child {} should come before parent {}",
                child_id,
                node_id
            );
        }
    }
}

#[test]
fn bfs_groups_by_depth() {
    let tree = sample_tree();
    let order = tree.bfs_order(tree.root());
    let mut prev_depth = 0usize;
    for node_id in order {
        let mut depth = 0;
        let mut current_parent = tree.node(node_id).parent;
        while let Some(parent_id) = current_parent {
            depth += 1;
            current_parent = tree.node(parent_id).parent;
        }
        assert!(depth >= prev_depth);
        prev_depth = depth;
    }
}

#[test]
fn descendants_excludes_self() {
    let tree = sample_tree();
    let descendant_ids = tree.descendants(tree.root());
    assert!(!descendant_ids.contains(&tree.root()));
    assert_eq!(descendant_ids.len(), 4);
}

#[test]
fn descendants_of_leaf_is_empty() {
    let mut builder = TreeBuilder::new();
    let root_id = builder.add("x", "", None, 0, 0);
    let tree = builder.build(root_id);
    assert!(tree.descendants(root_id).is_empty());
}

#[test]
fn position_in_parent_returns_index() {
    let tree = sample_tree();
    let root_children = tree.node(tree.root()).children.clone();
    for (index, child_id) in root_children.iter().enumerate() {
        assert_eq!(tree.position_in_parent(*child_id), Some(index));
    }
}

#[test]
fn position_in_parent_of_root_is_none() {
    let tree = sample_tree();
    assert_eq!(tree.position_in_parent(tree.root()), None);
}

#[test]
fn all_nodes_yields_node_count_nodes() {
    let tree = sample_tree();
    assert_eq!(tree.all_nodes().count(), tree.node_count());
}
