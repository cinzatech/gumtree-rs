//! Top-down (anchor) matching phase.
//!
//! Walks both trees from highest subtrees downward, mapping isomorphic
//! subtrees that share a structural hash. Ambiguous candidates are deferred
//! and resolved at the end using parent-context dice similarity.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

/// Default minimum height for top-down anchor matching.
pub const DEFAULT_MIN_HEIGHT: usize = 2;

/// Runs the top-down phase, extending `mapping` in place.
///
/// Subtrees whose height is at most `min_height` are deferred to the
/// bottom-up phase.
pub fn match_top_down(source_tree: &Tree, destination_tree: &Tree, mapping: &mut Mapping, min_height: usize) {
    let mut source_queue = HeightPQ::new();
    let mut destination_queue = HeightPQ::new();
    source_queue.push(source_tree, source_tree.root());
    destination_queue.push(destination_tree, destination_tree.root());

    let mut ambiguous: Vec<(NodeId, NodeId)> = Vec::new();

    loop {
        let source_height = source_queue.peek_height();
        let destination_height = destination_queue.peek_height();

        let (source_top, destination_top) = match (source_height, destination_height) {
            (Some(source), Some(destination)) => (source, destination),
            _ => break,
        };
        let max_height = source_top.max(destination_top);
        if max_height <= min_height {
            break;
        }

        if source_top > destination_top {
            for node_id in source_queue.pop_max() {
                source_queue.open(source_tree, node_id);
            }
        } else if destination_top > source_top {
            for node_id in destination_queue.pop_max() {
                destination_queue.open(destination_tree, node_id);
            }
        } else {
            // Equal heights: try to find isomorphic matches at this height.
            let source_nodes = source_queue.pop_max();
            let destination_nodes = destination_queue.pop_max();

            let mut matched_sources: HashSet<NodeId> = HashSet::new();
            let mut matched_destinations: HashSet<NodeId> = HashSet::new();

            // Collect every isomorphic pair.
            let mut iso_pairs: Vec<(NodeId, NodeId)> = Vec::new();
            for &source_node in &source_nodes {
                for &destination_node in &destination_nodes {
                    if is_isomorphic(source_tree, source_node, destination_tree, destination_node) {
                        iso_pairs.push((source_node, destination_node));
                    }
                }
            }

            // Count occurrences to detect ambiguity.
            let mut source_count: HashMap<NodeId, usize> = HashMap::new();
            let mut destination_count: HashMap<NodeId, usize> = HashMap::new();
            for &(source, destination) in &iso_pairs {
                *source_count.entry(source).or_insert(0) += 1;
                *destination_count.entry(destination).or_insert(0) += 1;
            }

            for (source, destination) in &iso_pairs {
                if source_count[source] == 1 && destination_count[destination] == 1 {
                    if !mapping.has_src(*source) && !mapping.has_dst(*destination) {
                        map_isomorphic_subtree(source_tree, *source, destination_tree, *destination, mapping);
                    }
                    matched_sources.insert(*source);
                    matched_destinations.insert(*destination);
                } else {
                    ambiguous.push((*source, *destination));
                    matched_sources.insert(*source);
                    matched_destinations.insert(*destination);
                }
            }

            // Nodes that didn't match anything: open their children.
            for node_id in &source_nodes {
                if !matched_sources.contains(node_id) {
                    source_queue.open(source_tree, *node_id);
                }
            }
            for node_id in &destination_nodes {
                if !matched_destinations.contains(node_id) {
                    destination_queue.open(destination_tree, *node_id);
                }
            }
        }
    }

    // Resolve ambiguous candidates by parent-context dice, descending.
    let mut scored: Vec<(f64, NodeId, NodeId)> = ambiguous
        .into_iter()
        .map(|(source, destination)| (parent_dice(source_tree, destination_tree, source, destination, mapping), source, destination))
        .collect();
    scored.sort_by(|left, right| right.0.partial_cmp(&left.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, source, destination) in scored {
        if !mapping.has_src(source) && !mapping.has_dst(destination) {
            map_isomorphic_subtree(source_tree, source, destination_tree, destination, mapping);
        }
    }
}

/// Two nodes are isomorphic if their structural hashes match (which already
/// covers kind, label, and child structure). The kind check is a cheap guard
/// against hash collisions.
fn is_isomorphic(source_tree: &Tree, source_node: NodeId, destination_tree: &Tree, destination_node: NodeId) -> bool {
    source_tree.node(source_node).hash == destination_tree.node(destination_node).hash && source_tree.node(source_node).kind == destination_tree.node(destination_node).kind
}

/// Links two isomorphic subtrees node-by-node in lockstep.
fn map_isomorphic_subtree(source_tree: &Tree, source_node: NodeId, destination_tree: &Tree, destination_node: NodeId, mapping: &mut Mapping) {
    mapping.link(source_node, destination_node);
    let source_children = source_tree.node(source_node).children.clone();
    let destination_children = destination_tree.node(destination_node).children.clone();
    if source_children.len() == destination_children.len() {
        for (source_child, destination_child) in source_children.iter().zip(destination_children.iter()) {
            map_isomorphic_subtree(source_tree, *source_child, destination_tree, *destination_child, mapping);
        }
    }
}

fn parent_dice(source_tree: &Tree, destination_tree: &Tree, source_node: NodeId, destination_node: NodeId, mapping: &Mapping) -> f64 {
    match (source_tree.node(source_node).parent, destination_tree.node(destination_node).parent) {
        (Some(source_parent), Some(destination_parent)) => dice_coefficient(source_tree, source_parent, destination_tree, destination_parent, mapping),
        _ => 0.0,
    }
}

/// Dice similarity between two subtrees, given a partial mapping.
///
/// Defined as `2 * |common| / (|desc(n1)| + |desc(n2)|)` where `common` is the
/// number of descendants of `n1` whose mapped image lies within the descendants
/// of `n2`.
pub fn dice_coefficient(source_tree: &Tree, source_node: NodeId, destination_tree: &Tree, destination_node: NodeId, mapping: &Mapping) -> f64 {
    let source_descendants = source_tree.descendants(source_node);
    let destination_descendants: HashSet<NodeId> = destination_tree.descendants(destination_node).into_iter().collect();
    if source_descendants.is_empty() && destination_descendants.is_empty() {
        return 0.0;
    }
    let mut common = 0usize;
    for descendant in &source_descendants {
        if let Some(mapped_destination) = mapping.get_dst(*descendant) {
            if destination_descendants.contains(&mapped_destination) {
                common += 1;
            }
        }
    }
    2.0 * (common as f64) / ((source_descendants.len() + destination_descendants.len()) as f64)
}

/// Priority queue keyed by node height, with max-heap behaviour.
struct HeightPQ {
    buckets: BTreeMap<usize, Vec<NodeId>>,
}

impl HeightPQ {
    fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
        }
    }

    fn push(&mut self, tree: &Tree, node_id: NodeId) {
        let height = tree.node(node_id).height;
        self.buckets.entry(height).or_default().push(node_id);
    }

    fn peek_height(&self) -> Option<usize> {
        self.buckets.keys().next_back().copied()
    }

    fn pop_max(&mut self) -> Vec<NodeId> {
        let max_height = match self.peek_height() {
            Some(height) => height,
            None => return Vec::new(),
        };
        self.buckets.remove(&max_height).unwrap_or_default()
    }

    fn open(&mut self, tree: &Tree, node_id: NodeId) {
        let children = tree.node(node_id).children.clone();
        for child_id in children {
            self.push(tree, child_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TreeBuilder;

    /// Builds (r (a x) (b y)) where leaves carry labels x and y.
    fn small_tree(left_label: &str, right_label: &str) -> Tree {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("r", "", None, 0, 10);
        let left_branch = builder.add("a", "", Some(root_id), 0, 5);
        let _left_leaf = builder.add("leaf", left_label, Some(left_branch), 1, 2);
        let right_branch = builder.add("b", "", Some(root_id), 5, 10);
        let _right_leaf = builder.add("leaf", right_label, Some(right_branch), 6, 7);
        builder.build(root_id)
    }

    #[test]
    fn identical_trees_are_fully_mapped() {
        let source_tree = small_tree("x", "y");
        let destination_tree = small_tree("x", "y");
        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, DEFAULT_MIN_HEIGHT);
        // Top-down should map at least the root subtree (height 3).
        assert!(mapping.has_src(source_tree.root()));
        assert_eq!(mapping.get_dst(source_tree.root()), Some(destination_tree.root()));
        // Since hashes match for the whole tree, every node should be mapped.
        assert_eq!(mapping.len(), source_tree.node_count());
    }

    #[test]
    fn completely_different_trees_yield_no_mapping() {
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("alpha", "", None, 0, 5);
        let _ = source_builder.add("alpha_child", "a", Some(source_root), 0, 1);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("beta", "", None, 0, 5);
        let _ = destination_builder.add("beta_child", "b", Some(destination_root), 0, 1);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, DEFAULT_MIN_HEIGHT);
        assert!(mapping.is_empty());
    }

    #[test]
    fn shared_subtree_is_anchored() {
        // T1 and T2 share a deep subtree S of height 3 (above min_height=2),
        // but otherwise differ. Top-down should anchor S in both trees.
        // Shared subtree: (sub (mid (x 1)))
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        let source_sub = source_builder.add("sub", "", Some(source_root), 0, 0);
        let source_mid = source_builder.add("mid", "", Some(source_sub), 0, 0);
        let _ = source_builder.add("x", "1", Some(source_mid), 0, 0);
        let source_extra = source_builder.add("extra", "", Some(source_root), 0, 0);
        let _ = source_builder.add("xx", "z", Some(source_extra), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("root", "", None, 0, 0);
        let other = destination_builder.add("other", "", Some(destination_root), 0, 0);
        let _ = destination_builder.add("yy", "w", Some(other), 0, 0);
        let destination_sub = destination_builder.add("sub", "", Some(destination_root), 0, 0);
        let destination_mid = destination_builder.add("mid", "", Some(destination_sub), 0, 0);
        let _ = destination_builder.add("x", "1", Some(destination_mid), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, DEFAULT_MIN_HEIGHT);

        // The shared `sub` subtree should be anchored.
        assert_eq!(mapping.get_dst(source_sub), Some(destination_sub));
    }

    #[test]
    fn min_height_threshold_excludes_small_subtrees() {
        // The only shared subtree is at height 2; with min_height=2 (i.e.
        // strictly greater than 2), top-down should not match it.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        let small_subtree = source_builder.add("small", "", Some(source_root), 0, 0);
        let _ = source_builder.add("leaf", "v", Some(small_subtree), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("Root", "", None, 0, 0); // different kind so roots don't iso
        let small_subtree_dest = destination_builder.add("small", "", Some(destination_root), 0, 0);
        let _ = destination_builder.add("leaf", "v", Some(small_subtree_dest), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, 2);
        // small is height 2, threshold strict (max_height <= min_height stops).
        assert!(!mapping.has_src(small_subtree));
    }

    #[test]
    fn lowering_min_height_unlocks_smaller_subtrees() {
        // Same setup, but with min_height=1 the matcher should now anchor the
        // height-2 `small` subtree.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        let small_subtree = source_builder.add("small", "", Some(source_root), 0, 0);
        let _ = source_builder.add("leaf", "v", Some(small_subtree), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("Root", "", None, 0, 0);
        let small_subtree_dest = destination_builder.add("small", "", Some(destination_root), 0, 0);
        let _ = destination_builder.add("leaf", "v", Some(small_subtree_dest), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, 1);
        assert_eq!(mapping.get_dst(small_subtree), Some(small_subtree_dest));
    }

    #[test]
    fn maps_only_unique_isomorphic_anchors_directly() {
        // T1 and T2 contain a unique large subtree S.
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        let source_subtree = source_builder.add("S", "", Some(source_root), 0, 0);
        let source_child = source_builder.add("child", "", Some(source_subtree), 0, 0);
        let _ = source_builder.add("leaf", "v", Some(source_child), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("root", "", None, 0, 0);
        let destination_subtree = destination_builder.add("S", "", Some(destination_root), 0, 0);
        let destination_child = destination_builder.add("child", "", Some(destination_subtree), 0, 0);
        let _ = destination_builder.add("leaf", "v", Some(destination_child), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(&source_tree, &destination_tree, &mut mapping, DEFAULT_MIN_HEIGHT);
        assert_eq!(mapping.get_dst(source_subtree), Some(destination_subtree));
    }

    #[test]
    fn dice_coefficient_zero_for_unmatched_subtrees() {
        let source_tree = small_tree("x", "y");
        let destination_tree = small_tree("a", "b");
        let mapping = Mapping::new();
        assert_eq!(dice_coefficient(&source_tree, source_tree.root(), &destination_tree, destination_tree.root(), &mapping), 0.0);
    }

    #[test]
    fn dice_coefficient_one_when_all_descendants_mapped() {
        let source_tree = small_tree("x", "y");
        let destination_tree = small_tree("x", "y");
        let mut mapping = Mapping::new();
        // Manually pair every descendant.
        let source_descendants = source_tree.descendants(source_tree.root());
        let destination_descendants = destination_tree.descendants(destination_tree.root());
        for (source, destination) in source_descendants.iter().zip(destination_descendants.iter()) {
            mapping.link(*source, *destination);
        }
        let dice = dice_coefficient(&source_tree, source_tree.root(), &destination_tree, destination_tree.root(), &mapping);
        assert!((dice - 1.0).abs() < 1e-9);
    }
}
