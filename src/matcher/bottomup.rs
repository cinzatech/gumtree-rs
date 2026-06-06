//! Bottom-up (container) matching phase plus the SimpleGumTree recovery step.
//!
//! After the top-down phase has anchored large isomorphic subtrees, this phase
//! walks `T1` in post-order. For each unmapped node whose subtree already
//! contains anchored descendants, it searches for the best container in `T2`
//! by Dice similarity, then runs a cheap greedy recovery for the remaining
//! unmapped descendants inside the matched pair.

use std::collections::HashMap;

use crate::mapping::Mapping;
use crate::matcher::topdown::dice_coefficient;
use crate::tree::{NodeId, Tree};

/// Minimum dice similarity to accept a bottom-up container match.
pub const DEFAULT_MIN_DICE: f64 = 0.5;
/// Maximum subtree size for which simple-recovery runs (perf guard).
pub const DEFAULT_MAX_SIZE: usize = 1000;

/// Runs the bottom-up phase, extending `mapping` in place.
pub fn match_bottom_up(
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &mut Mapping,
    min_dice: f64,
    max_size: usize,
) {
    // Build a kind → [NodeId] index over T2 so candidate lookup is O(same-kind)
    // instead of O(|T2|) per unmapped node.
    let mut kind_index: HashMap<&str, Vec<NodeId>> = HashMap::new();
    for node in destination_tree.all_nodes() {
        kind_index.entry(&node.kind).or_default().push(node.id);
    }

    let order = source_tree.post_order(source_tree.root());
    for source_node in order {
        if mapping.has_src(source_node) {
            continue;
        }
        if !has_matched_descendant(source_tree, source_node, mapping) {
            continue;
        }
        let Some(destination_node) = find_candidate(
            source_tree,
            source_node,
            destination_tree,
            mapping,
            min_dice,
            &kind_index,
        ) else {
            continue;
        };

        mapping.link(source_node, destination_node);

        let max_subtree_size = source_tree
            .node(source_node)
            .size
            .max(destination_tree.node(destination_node).size);

        if max_subtree_size < max_size {
            recover_simple(
                source_tree,
                source_node,
                destination_tree,
                destination_node,
                mapping,
            );
        }
    }
}

fn has_matched_descendant(tree: &Tree, node_id: NodeId, mapping: &Mapping) -> bool {
    tree.descendants(node_id)
        .iter()
        .any(|descendant| mapping.has_src(*descendant))
}

/// Best unmapped node in T2 with the same kind, by dice similarity.
fn find_candidate<'a>(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &'a Tree,
    mapping: &Mapping,
    min_dice: f64,
    kind_index: &HashMap<&'a str, Vec<NodeId>>,
) -> Option<NodeId> {
    let kind = &source_tree.node(source_node).kind;
    let candidates = kind_index.get(kind.as_str())?;

    candidates
        .iter()
        .copied()
        .filter(|candidate| !mapping.has_dst(*candidate))
        .map(|candidate| {
            let dice = dice_coefficient(
                source_tree,
                source_node,
                destination_tree,
                candidate,
                mapping,
            );
            (candidate, dice)
        })
        .filter(|(_, dice)| *dice >= min_dice)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(node_id, _)| node_id)
}

/// SimpleGumTree's cheap recovery: match remaining unmapped descendants by
/// (kind, label) first, then by (mapped parent + kind + sibling position).
///
/// Exposed so that integrating code (e.g. [`crate::matcher::match_trees`])
/// can invoke recovery as a fallback after both matching phases.
pub fn recover_simple(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &Tree,
    destination_node: NodeId,
    mapping: &mut Mapping,
) {
    let source_descendants = source_tree.descendants(source_node);
    let destination_descendants = destination_tree.descendants(destination_node);

    recover_exact_leaves(
        source_tree,
        destination_tree,
        &source_descendants,
        &destination_descendants,
        mapping,
    );

    recover_inner_nodes(
        source_tree,
        destination_tree,
        &source_descendants,
        &destination_descendants,
        mapping,
    );

    recover_by_parent(source_tree, destination_tree, &source_descendants, mapping);
}

/// Phase 1a: exact (kind, label) histogram pairing for LEAF nodes only.
/// Only match when the candidate is unique — common tokens like `def`,
/// `(`, `)` have multiple candidates and would pollute Dice scores if
/// matched arbitrarily.
fn recover_exact_leaves(
    source_tree: &Tree,
    destination_tree: &Tree,
    source_descendants: &[NodeId],
    destination_descendants: &[NodeId],
    mapping: &mut Mapping,
) {
    let mut destination_buckets: HashMap<(&str, &str), Vec<NodeId>> = destination_descendants
        .iter()
        .copied()
        .filter(|node_id| !mapping.has_dst(*node_id))
        .map(|node_id| (node_id, destination_tree.node(node_id)))
        .filter(|(_, node)| node.children.is_empty())
        .fold(HashMap::new(), |mut acc, (node_id, node)| {
            acc.entry((node.kind.as_str(), node.label.as_str()))
                .or_default()
                .push(node_id);
            acc
        });

    let source_leaf_counts: HashMap<(&str, &str), usize> = source_descendants
        .iter()
        .copied()
        .filter(|node_id| !mapping.has_src(*node_id))
        .map(|node_id| (node_id, source_tree.node(node_id)))
        .filter(|(_, node)| node.children.is_empty())
        .fold(HashMap::new(), |mut acc, (_, node)| {
            *acc.entry((node.kind.as_str(), node.label.as_str()))
                .or_insert(0) += 1;
            acc
        });

    for source_descendant in source_descendants {
        if mapping.has_src(*source_descendant) {
            continue;
        }
        let source_node = source_tree.node(*source_descendant);
        if !source_node.children.is_empty() {
            continue;
        }

        let key = (source_node.kind.as_str(), source_node.label.as_str());
        let source_count = source_leaf_counts.get(&key).copied().unwrap_or(0);

        let Some(bucket) = destination_buckets.get_mut(&key) else {
            continue;
        };
        if bucket.len() == 1 && source_count == 1 {
            mapping.link(*source_descendant, bucket.pop().unwrap());
        }
    }
}

/// Phase 1b: match non-leaf nodes by (kind, label). When multiple
/// candidates share the same key, pick the one with the highest Dice
/// similarity (which reflects how many of their descendants are already
/// matched from Phase 1a).
fn recover_inner_nodes(
    source_tree: &Tree,
    destination_tree: &Tree,
    source_descendants: &[NodeId],
    destination_descendants: &[NodeId],
    mapping: &mut Mapping,
) {
    let mut destination_buckets: HashMap<(&str, &str), Vec<NodeId>> = destination_descendants
        .iter()
        .copied()
        .filter(|node_id| !mapping.has_dst(*node_id))
        .map(|node_id| (node_id, destination_tree.node(node_id)))
        .filter(|(_, node)| !node.children.is_empty())
        .fold(HashMap::new(), |mut acc, (node_id, node)| {
            acc.entry((node.kind.as_str(), node.label.as_str()))
                .or_default()
                .push(node_id);
            acc
        });

    for source_descendant in source_descendants {
        if mapping.has_src(*source_descendant) {
            continue;
        }
        let source_node = source_tree.node(*source_descendant);
        if source_node.children.is_empty() {
            continue;
        }

        let key = (source_node.kind.as_str(), source_node.label.as_str());
        let Some(bucket) = destination_buckets.get_mut(&key) else {
            continue;
        };
        if bucket.is_empty() {
            continue;
        }

        if bucket.len() == 1 {
            let candidate = bucket[0];
            let dice = dice_coefficient(
                source_tree,
                *source_descendant,
                destination_tree,
                candidate,
                mapping,
            );
            if dice > 0.0 {
                bucket.pop();
                mapping.link(*source_descendant, candidate);
            }
            continue;
        }

        let best = bucket
            .iter()
            .enumerate()
            .map(|(index, &candidate)| {
                let dice = dice_coefficient(
                    source_tree,
                    *source_descendant,
                    destination_tree,
                    candidate,
                    mapping,
                );
                (index, dice)
            })
            .filter(|(_, dice)| *dice > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1));

        let Some((best_index, _)) = best else {
            continue;
        };
        let matched = bucket.remove(best_index);
        mapping.link(*source_descendant, matched);
    }
}

/// Phase 2: among still-unmapped, match by parent correspondence + kind.
/// This catches leaves whose label changed (so phase 1 missed them) but
/// whose parents are already mapped.
fn recover_by_parent(
    source_tree: &Tree,
    destination_tree: &Tree,
    source_descendants: &[NodeId],
    mapping: &mut Mapping,
) {
    for source_descendant in source_descendants {
        if mapping.has_src(*source_descendant) {
            continue;
        }

        let source_node = source_tree.node(*source_descendant);
        let Some(parent_id) = source_node.parent else {
            continue;
        };
        let Some(mapped_parent) = mapping.get_dst(parent_id) else {
            continue;
        };

        let kind = &source_node.kind;

        // Same-index sibling first (preserves position when possible).
        let sibling_index = source_tree
            .node(parent_id)
            .children
            .iter()
            .position(|&child_id| child_id == *source_descendant)
            .unwrap();

        let destination_siblings = &destination_tree.node(mapped_parent).children;
        if sibling_index < destination_siblings.len() {
            let candidate = destination_siblings[sibling_index];
            if !mapping.has_dst(candidate) && destination_tree.node(candidate).kind == *kind {
                mapping.link(*source_descendant, candidate);
                continue;
            }
        }

        // Otherwise, first unmapped same-kind sibling.
        for &candidate in destination_siblings {
            if !mapping.has_dst(candidate) && destination_tree.node(candidate).kind == *kind {
                mapping.link(*source_descendant, candidate);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::topdown::{match_top_down, DEFAULT_MIN_HEIGHT};
    use crate::tree::TreeBuilder;

    /// Builds two trees that share a tall stable subtree (so top-down can
    /// anchor on it) plus a section with one label change for bottom-up
    /// recovery to catch.
    ///
    /// Returns (t1, t2, v1, v2) where v1/v2 are the value-leaf nodes whose
    /// labels differ between the trees.
    fn pair_with_label_change() -> (Tree, Tree, NodeId, NodeId) {
        // Stable shared subtree of height 3, plus a (item (key,val)) section
        // where val's label differs.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        // Stable anchor: (anchor (deep (leaf "stable")))
        let source_anchor = source_builder.add("anchor", "", Some(source_root), 0, 0);
        let source_deep = source_builder.add("deep", "", Some(source_anchor), 0, 0);
        let _ = source_builder.add("leaf", "stable", Some(source_deep), 0, 0);
        // Section with label change.
        let source_item = source_builder.add("item", "", Some(source_root), 0, 0);
        let _source_key = source_builder.add("key", "k", Some(source_item), 0, 0);
        let source_value = source_builder.add("val", "old", Some(source_item), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("root", "", None, 0, 0);
        let destination_anchor =
            destination_builder.add("anchor", "", Some(destination_root), 0, 0);
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
        let (source_tree, destination_tree, source_value, destination_value) =
            pair_with_label_change();
        let mut mapping = Mapping::new();
        match_top_down(
            &source_tree,
            &destination_tree,
            &mut mapping,
            DEFAULT_MIN_HEIGHT,
        );
        // Top-down won't map the `val` nodes (their labels differ → different hashes).
        assert!(!mapping.has_src(source_value));
        // But it should anchor the stable subtree (height 3 = > min_height).
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
        // After bottom-up + simple recovery, the val nodes should be linked.
        assert_eq!(mapping.get_dst(source_value), Some(destination_value));
    }

    #[test]
    fn bottom_up_does_nothing_when_no_descendants_match() {
        // Trees with completely disjoint kinds — top-down anchors nothing,
        // so bottom-up has no descendants to bootstrap from.
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
        // Two containers sharing 3 anchored subtrees plus one unique child each.
        // Shared descendants dominate, so dice is well above 0.5 (default) but
        // still below 0.99 (strict).
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
        let destination_container =
            destination_builder.add("ctr", "", Some(destination_root), 0, 0);
        for _ in 0..3 {
            let anchor = destination_builder.add("anchor", "", Some(destination_container), 0, 0);
            let inner = destination_builder.add("inner", "", Some(anchor), 0, 0);
            let _ = destination_builder.add("leaf", "a", Some(inner), 0, 0);
        }
        let _ = destination_builder.add("only_in_2", "y", Some(destination_container), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        // Strict threshold blocks the ctr match (dice ~= 0.9, not >= 0.99).
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
        // Two trees with a stable anchor and identical (kind,label) leaves
        // that bottom-up's recovery should pair up after the container matches.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        // Stable tall anchor.
        let source_anchor_top = source_builder.add("anchor_top", "", Some(source_root), 0, 0);
        let source_anchor_mid = source_builder.add("anchor_mid", "", Some(source_anchor_top), 0, 0);
        let _ = source_builder.add("anchor_leaf", "x", Some(source_anchor_mid), 0, 0);
        // Container with identical anchors.
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
        let destination_container =
            destination_builder.add("ctr", "", Some(destination_root), 0, 0);
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
        // Both "anchor: A" leaves in ctr1 should now be paired with two in ctr2.
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
        // Simulates reordered Python functions: source has [greet, add, think],
        // destination has [add, greet, think]. After top-down matches `add`
        // (identical subtree), recovery must pair greet↔greet and think↔think
        // by their content (matched identifiers), NOT by sibling position.
        //
        // The bug: all function_definition nodes share (kind, label) =
        // ("function_definition", ""), so Phase 1's histogram matching pairs
        // them arbitrarily. This causes children to scatter across functions.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("module", "", None, 0, 100);
        let source_greet_fn =
            source_builder.add("function_definition", "", Some(source_root), 0, 30);
        let _source_greet_id =
            source_builder.add("identifier", "greet", Some(source_greet_fn), 4, 9);
        let source_greet_params =
            source_builder.add("parameters", "", Some(source_greet_fn), 9, 15);
        let _source_greet_name =
            source_builder.add("identifier", "name", Some(source_greet_params), 10, 14);
        let source_add_fn =
            source_builder.add("function_definition", "", Some(source_root), 31, 60);
        let _source_add_id = source_builder.add("identifier", "add", Some(source_add_fn), 35, 38);
        let source_add_params = source_builder.add("parameters", "", Some(source_add_fn), 38, 44);
        let _source_add_a = source_builder.add("identifier", "a", Some(source_add_params), 39, 40);
        let _source_add_b = source_builder.add("identifier", "b", Some(source_add_params), 42, 43);
        let source_think_fn =
            source_builder.add("function_definition", "", Some(source_root), 61, 100);
        let _source_think_id =
            source_builder.add("identifier", "think", Some(source_think_fn), 65, 70);
        let source_think_params =
            source_builder.add("parameters", "", Some(source_think_fn), 70, 78);
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
}
