use gumtree_rs::mapping::Mapping;
use gumtree_rs::matcher::bottomup::{
    match_bottom_up, recover_simple, DEFAULT_MAX_SIZE, DEFAULT_MIN_DICE,
};
use gumtree_rs::matcher::topdown::{match_top_down, DEFAULT_MIN_HEIGHT};
use gumtree_rs::tree::{NodeId, Tree, TreeBuilder};

/// Builds two trees that share a tall stable subtree (so top-down can
/// anchor on it) plus a section with one label change for bottom-up
/// recovery to catch.
///
/// Returns (t1, t2, v1, v2) where v1/v2 are the value-leaf nodes whose
/// labels differ between the trees.
fn pair_with_label_change() -> (Tree, Tree, NodeId, NodeId) {
    let mut source_builder = TreeBuilder::new();
    let source_root = source_builder.add("root", "", None, 0, 0);
    let source_anchor = source_builder.add("anchor", "", Some(source_root), 0, 0);
    let source_deep = source_builder.add("deep", "", Some(source_anchor), 0, 0);
    let _ = source_builder.add("leaf", "stable", Some(source_deep), 0, 0);
    let source_item = source_builder.add("item", "", Some(source_root), 0, 0);
    let _source_key = source_builder.add("key", "k", Some(source_item), 0, 0);
    let source_value = source_builder.add("val", "old", Some(source_item), 0, 0);
    let source_tree = source_builder.build(source_root);

    let mut destination_builder = TreeBuilder::new();
    let destination_root = destination_builder.add("root", "", None, 0, 0);
    let destination_anchor = destination_builder.add("anchor", "", Some(destination_root), 0, 0);
    let destination_deep = destination_builder.add("deep", "", Some(destination_anchor), 0, 0);
    let _ = destination_builder.add("leaf", "stable", Some(destination_deep), 0, 0);
    let destination_item = destination_builder.add("item", "", Some(destination_root), 0, 0);
    let _destination_key = destination_builder.add("key", "k", Some(destination_item), 0, 0);
    let destination_value = destination_builder.add("val", "new", Some(destination_item), 0, 0);
    let destination_tree = destination_builder.build(destination_root);

    (
        source_tree,
        destination_tree,
        source_value,
        destination_value,
    )
}

#[test]
fn bottom_up_maps_container_when_descendants_anchor() {
    let (source_tree, destination_tree, source_value, destination_value) = pair_with_label_change();
    let mut mapping = Mapping::new();
    match_top_down(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_HEIGHT,
    );
    assert!(!mapping.has_src(source_value));
    assert!(
        !mapping.is_empty(),
        "top-down should have anchored the stable subtree"
    );

    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_DICE,
        DEFAULT_MAX_SIZE,
    );
    assert_eq!(mapping.get_dst(source_value), Some(destination_value));
}

#[test]
fn bottom_up_does_nothing_when_no_descendants_match() {
    let mut source_builder = TreeBuilder::new();
    let source_root = source_builder.add("root", "", None, 0, 0);
    let _ = source_builder.add("alpha", "a", Some(source_root), 0, 0);
    let source_tree = source_builder.build(source_root);

    let mut destination_builder = TreeBuilder::new();
    let destination_root = destination_builder.add("root", "", None, 0, 0);
    let _ = destination_builder.add("beta", "b", Some(destination_root), 0, 0);
    let destination_tree = destination_builder.build(destination_root);

    let mut mapping = Mapping::new();
    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_DICE,
        DEFAULT_MAX_SIZE,
    );
    assert!(mapping.is_empty());
}

#[test]
fn min_dice_threshold_blocks_weak_matches() {
    let mut source_builder = TreeBuilder::new();
    let source_root = source_builder.add("root", "", None, 0, 0);
    let source_container = source_builder.add("ctr", "", Some(source_root), 0, 0);
    for _ in 0..3 {
        let anchor = source_builder.add("anchor", "", Some(source_container), 0, 0);
        let inner = source_builder.add("inner", "", Some(anchor), 0, 0);
        let _ = source_builder.add("leaf", "a", Some(inner), 0, 0);
    }
    let _ = source_builder.add("only_in_1", "x", Some(source_container), 0, 0);
    let source_tree = source_builder.build(source_root);

    let mut destination_builder = TreeBuilder::new();
    let destination_root = destination_builder.add("root", "", None, 0, 0);
    let destination_container = destination_builder.add("ctr", "", Some(destination_root), 0, 0);
    for _ in 0..3 {
        let anchor = destination_builder.add("anchor", "", Some(destination_container), 0, 0);
        let inner = destination_builder.add("inner", "", Some(anchor), 0, 0);
        let _ = destination_builder.add("leaf", "a", Some(inner), 0, 0);
    }
    let _ = destination_builder.add("only_in_2", "y", Some(destination_container), 0, 0);
    let destination_tree = destination_builder.build(destination_root);

    // Strict threshold blocks the ctr match.
    let mut strict_mapping = Mapping::new();
    match_top_down(
        &source_tree,
        &destination_tree,
        &mut strict_mapping,
        DEFAULT_MIN_HEIGHT,
    );
    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut strict_mapping,
        0.99,
        DEFAULT_MAX_SIZE,
    );
    assert!(!strict_mapping.has_src(source_container));

    // Default threshold accepts the ctr match.
    let mut default_mapping = Mapping::new();
    match_top_down(
        &source_tree,
        &destination_tree,
        &mut default_mapping,
        DEFAULT_MIN_HEIGHT,
    );
    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut default_mapping,
        DEFAULT_MIN_DICE,
        DEFAULT_MAX_SIZE,
    );
    assert_eq!(
        default_mapping.get_dst(source_container),
        Some(destination_container)
    );
}

#[test]
fn recover_pairs_remaining_same_kind_label_nodes() {
    let mut source_builder = TreeBuilder::new();
    let source_root = source_builder.add("root", "", None, 0, 0);
    let source_anchor_top = source_builder.add("anchor_top", "", Some(source_root), 0, 0);
    let source_anchor_mid = source_builder.add("anchor_mid", "", Some(source_anchor_top), 0, 0);
    let _ = source_builder.add("anchor_leaf", "x", Some(source_anchor_mid), 0, 0);
    let source_container = source_builder.add("ctr", "", Some(source_root), 0, 0);
    let first_anchor = source_builder.add("anchor", "A", Some(source_container), 0, 0);
    let second_anchor = source_builder.add("anchor", "A", Some(source_container), 0, 0);
    let source_tree = source_builder.build(source_root);

    let mut destination_builder = TreeBuilder::new();
    let destination_root = destination_builder.add("root", "", None, 0, 0);
    let destination_anchor_top =
        destination_builder.add("anchor_top", "", Some(destination_root), 0, 0);
    let destination_anchor_mid =
        destination_builder.add("anchor_mid", "", Some(destination_anchor_top), 0, 0);
    let _ = destination_builder.add("anchor_leaf", "x", Some(destination_anchor_mid), 0, 0);
    let destination_container = destination_builder.add("ctr", "", Some(destination_root), 0, 0);
    let _ = destination_builder.add("anchor", "A", Some(destination_container), 0, 0);
    let _ = destination_builder.add("anchor", "A", Some(destination_container), 0, 0);
    let destination_tree = destination_builder.build(destination_root);

    let mut mapping = Mapping::new();
    match_top_down(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_HEIGHT,
    );
    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_DICE,
        DEFAULT_MAX_SIZE,
    );
    assert!(
        mapping.has_src(first_anchor),
        "first anchor should be mapped"
    );
    assert!(
        mapping.has_src(second_anchor),
        "second anchor should be mapped"
    );
}

#[test]
fn recover_simple_matches_containers_by_content_not_position() {
    let mut source_builder = TreeBuilder::new();
    let source_root = source_builder.add("module", "", None, 0, 100);
    let source_greet_fn = source_builder.add("function_definition", "", Some(source_root), 0, 30);
    let _source_greet_id = source_builder.add("identifier", "greet", Some(source_greet_fn), 4, 9);
    let source_greet_params = source_builder.add("parameters", "", Some(source_greet_fn), 9, 15);
    let _source_greet_name =
        source_builder.add("identifier", "name", Some(source_greet_params), 10, 14);
    let source_add_fn = source_builder.add("function_definition", "", Some(source_root), 31, 60);
    let _source_add_id = source_builder.add("identifier", "add", Some(source_add_fn), 35, 38);
    let source_add_params = source_builder.add("parameters", "", Some(source_add_fn), 38, 44);
    let _source_add_a = source_builder.add("identifier", "a", Some(source_add_params), 39, 40);
    let _source_add_b = source_builder.add("identifier", "b", Some(source_add_params), 42, 43);
    let source_think_fn = source_builder.add("function_definition", "", Some(source_root), 61, 100);
    let _source_think_id = source_builder.add("identifier", "think", Some(source_think_fn), 65, 70);
    let source_think_params = source_builder.add("parameters", "", Some(source_think_fn), 70, 78);
    let _source_think_about =
        source_builder.add("identifier", "about", Some(source_think_params), 71, 76);
    let source_tree = source_builder.build(source_root);

    let mut destination_builder = TreeBuilder::new();
    let destination_root = destination_builder.add("module", "", None, 0, 100);
    let destination_add_fn =
        destination_builder.add("function_definition", "", Some(destination_root), 0, 30);
    let _destination_add_id =
        destination_builder.add("identifier", "add", Some(destination_add_fn), 4, 7);
    let destination_add_params =
        destination_builder.add("parameters", "", Some(destination_add_fn), 7, 13);
    let _destination_add_a =
        destination_builder.add("identifier", "a", Some(destination_add_params), 8, 9);
    let _destination_add_b =
        destination_builder.add("identifier", "b", Some(destination_add_params), 11, 12);
    let destination_greet_fn =
        destination_builder.add("function_definition", "", Some(destination_root), 31, 60);
    let _destination_greet_id =
        destination_builder.add("identifier", "greet", Some(destination_greet_fn), 35, 40);
    let destination_greet_params =
        destination_builder.add("parameters", "", Some(destination_greet_fn), 40, 48);
    let _destination_greet_person = destination_builder.add(
        "identifier",
        "person",
        Some(destination_greet_params),
        41,
        47,
    );
    let destination_think_fn =
        destination_builder.add("function_definition", "", Some(destination_root), 61, 100);
    let _destination_think_id =
        destination_builder.add("identifier", "think", Some(destination_think_fn), 65, 70);
    let destination_think_params =
        destination_builder.add("parameters", "", Some(destination_think_fn), 70, 80);
    let _destination_think_thought = destination_builder.add(
        "identifier",
        "thought",
        Some(destination_think_params),
        71,
        78,
    );
    let destination_tree = destination_builder.build(destination_root);

    let mut mapping = Mapping::new();
    match_top_down(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_HEIGHT,
    );
    assert!(mapping.has_src(source_add_fn), "top-down should anchor add");

    match_bottom_up(
        &source_tree,
        &destination_tree,
        &mut mapping,
        DEFAULT_MIN_DICE,
        DEFAULT_MAX_SIZE,
    );

    if !mapping.has_src(source_tree.root()) {
        mapping.link(source_tree.root(), destination_tree.root());
        recover_simple(
            &source_tree,
            source_tree.root(),
            &destination_tree,
            destination_tree.root(),
            &mut mapping,
        );
    }

    assert_eq!(
        mapping.get_dst(source_greet_fn),
        Some(destination_greet_fn),
        "greet's function_definition should map to greet's, not another function"
    );
    assert_eq!(
        mapping.get_dst(source_think_fn),
        Some(destination_think_fn),
        "think's function_definition should map to think's, not another function"
    );
}
