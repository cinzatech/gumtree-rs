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
pub fn match_top_down(
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &mut Mapping,
    min_height: usize,
) {
    let mut source_queue = HeightPQ::new();
    let mut destination_queue = HeightPQ::new();
    source_queue.push(source_tree, source_tree.root());
    destination_queue.push(destination_tree, destination_tree.root());

    let mut ambiguous: Vec<(NodeId, NodeId)> = Vec::new();

    while let Some(source_height) = source_queue.peek_height() {
        let Some(destination_height) = destination_queue.peek_height() else {
            break;
        };

        let max_height = source_height.max(destination_height);
        if max_height <= min_height {
            break;
        }

        if source_height > destination_height {
            for node_id in source_queue.pop_max() {
                source_queue.open(source_tree, node_id);
            }
        } else if destination_height > source_height {
            for node_id in destination_queue.pop_max() {
                destination_queue.open(destination_tree, node_id);
            }
        } else {
            let source_nodes = source_queue.pop_max();
            let destination_nodes = destination_queue.pop_max();
            let (matched_sources, matched_destinations) = match_at_height(
                source_tree,
                destination_tree,
                &source_nodes,
                &destination_nodes,
                mapping,
                &mut ambiguous,
            );

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
        .map(|(source_node, destination_node)| {
            let score = parent_dice(
                source_tree,
                destination_tree,
                source_node,
                destination_node,
                mapping,
            );
            (score, source_node, destination_node)
        })
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));

    for (_, source_node, destination_node) in scored {
        if !mapping.has_src(source_node) && !mapping.has_dst(destination_node) {
            map_isomorphic_subtree(
                source_tree,
                source_node,
                destination_tree,
                destination_node,
                mapping,
            );
        }
    }
}

/// Attempts to match isomorphic nodes at the same height level.
/// Unique matches are linked immediately; ambiguous ones are deferred.
/// Returns the sets of source and destination nodes that were matched
/// (either uniquely or ambiguously) so the caller knows which nodes NOT to open.
fn match_at_height(
    source_tree: &Tree,
    destination_tree: &Tree,
    source_nodes: &[NodeId],
    destination_nodes: &[NodeId],
    mapping: &mut Mapping,
    ambiguous: &mut Vec<(NodeId, NodeId)>,
) -> (HashSet<NodeId>, HashSet<NodeId>) {
    // Collect every isomorphic pair.
    let iso_pairs: Vec<(NodeId, NodeId)> = source_nodes
        .iter()
        .flat_map(|&source_node| {
            destination_nodes
                .iter()
                .copied()
                .filter(move |&destination_node| {
                    is_isomorphic(source_tree, source_node, destination_tree, destination_node)
                })
                .map(move |destination_node| (source_node, destination_node))
        })
        .collect();

    // Count occurrences to detect ambiguity.
    let mut source_count: HashMap<NodeId, usize> = HashMap::new();
    let mut destination_count: HashMap<NodeId, usize> = HashMap::new();
    for &(source_node, destination_node) in &iso_pairs {
        *source_count.entry(source_node).or_insert(0) += 1;
        *destination_count.entry(destination_node).or_insert(0) += 1;
    }

    let mut matched_sources: HashSet<NodeId> = HashSet::new();
    let mut matched_destinations: HashSet<NodeId> = HashSet::new();

    for (source_node, destination_node) in &iso_pairs {
        matched_sources.insert(*source_node);
        matched_destinations.insert(*destination_node);

        let is_unique = source_count[source_node] == 1 && destination_count[destination_node] == 1;
        if !is_unique {
            ambiguous.push((*source_node, *destination_node));
            continue;
        }
        if mapping.has_src(*source_node) || mapping.has_dst(*destination_node) {
            continue;
        }
        map_isomorphic_subtree(
            source_tree,
            *source_node,
            destination_tree,
            *destination_node,
            mapping,
        );
    }

    (matched_sources, matched_destinations)
}

/// Two nodes are isomorphic if their structural hashes match (which already
/// covers kind, label, and child structure). The kind check is a cheap guard
/// against hash collisions.
fn is_isomorphic(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &Tree,
    destination_node: NodeId,
) -> bool {
    let source_data = source_tree.node(source_node);
    let destination_data = destination_tree.node(destination_node);
    source_data.hash == destination_data.hash && source_data.kind == destination_data.kind
}

/// Links two isomorphic subtrees node-by-node in lockstep.
fn map_isomorphic_subtree(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &Tree,
    destination_node: NodeId,
    mapping: &mut Mapping,
) {
    let mut stack = vec![(source_node, destination_node)];
    while let Some((current_source, current_destination)) = stack.pop() {
        mapping.link(current_source, current_destination);
        let source_children = &source_tree.node(current_source).children;
        let destination_children = &destination_tree.node(current_destination).children;
        if source_children.len() != destination_children.len() {
            continue;
        }
        // Push in reverse so leftmost pair is processed first.
        for (source_child, destination_child) in source_children
            .iter()
            .zip(destination_children.iter())
            .rev()
        {
            stack.push((*source_child, *destination_child));
        }
    }
}

fn parent_dice(
    source_tree: &Tree,
    destination_tree: &Tree,
    source_node: NodeId,
    destination_node: NodeId,
    mapping: &Mapping,
) -> f64 {
    let source_parent = source_tree.node(source_node).parent;
    let destination_parent = destination_tree.node(destination_node).parent;
    match (source_parent, destination_parent) {
        (Some(source_parent_id), Some(destination_parent_id)) => dice_coefficient(
            source_tree,
            source_parent_id,
            destination_tree,
            destination_parent_id,
            mapping,
        ),
        _ => 0.0,
    }
}

/// Dice similarity between two subtrees, given a partial mapping.
///
/// Defined as `2 * |common| / (|desc(n1)| + |desc(n2)|)` where `common` is the
/// number of descendants of `n1` whose mapped image lies within the descendants
/// of `n2`.
pub fn dice_coefficient(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &Tree,
    destination_node: NodeId,
    mapping: &Mapping,
) -> f64 {
    let source_descendants = source_tree.descendants(source_node);
    let dest_count = destination_tree.node(destination_node).size - 1;

    if source_descendants.is_empty() && dest_count == 0 {
        return 0.0;
    }

    // Vec<bool> membership lookup: O(1) per test, no hashing, cache-friendly.
    let dest_member = destination_tree.descendant_set(destination_node);

    let common = source_descendants
        .iter()
        .filter_map(|descendant| mapping.get_dst(*descendant))
        .filter(|&mapped_destination| dest_member[mapped_destination])
        .count();

    let total = source_descendants.len() + dest_count;
    2.0 * (common as f64) / (total as f64)
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
        let Some(max_height) = self.peek_height() else {
            return Vec::new();
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
        match_top_down(
            &source_tree,
            &destination_tree,
            &mut mapping,
            DEFAULT_MIN_HEIGHT,
        );
        assert!(mapping.has_src(source_tree.root()));
        assert_eq!(
            mapping.get_dst(source_tree.root()),
            Some(destination_tree.root())
        );
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
        match_top_down(
            &source_tree,
            &destination_tree,
            &mut mapping,
            DEFAULT_MIN_HEIGHT,
        );
        assert!(mapping.is_empty());
    }

    #[test]
    fn shared_subtree_is_anchored() {
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
        match_top_down(
            &source_tree,
            &destination_tree,
            &mut mapping,
            DEFAULT_MIN_HEIGHT,
        );

        assert_eq!(mapping.get_dst(source_sub), Some(destination_sub));
    }

    #[test]
    fn min_height_threshold_excludes_small_subtrees() {
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
        match_top_down(&source_tree, &destination_tree, &mut mapping, 2);
        assert!(!mapping.has_src(small_subtree));
    }

    #[test]
    fn lowering_min_height_unlocks_smaller_subtrees() {
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
        let mut source_builder = TreeBuilder::new();
        let source_root = source_builder.add("root", "", None, 0, 0);
        let source_subtree = source_builder.add("S", "", Some(source_root), 0, 0);
        let source_child = source_builder.add("child", "", Some(source_subtree), 0, 0);
        let _ = source_builder.add("leaf", "v", Some(source_child), 0, 0);
        let source_tree = source_builder.build(source_root);

        let mut destination_builder = TreeBuilder::new();
        let destination_root = destination_builder.add("root", "", None, 0, 0);
        let destination_subtree = destination_builder.add("S", "", Some(destination_root), 0, 0);
        let destination_child =
            destination_builder.add("child", "", Some(destination_subtree), 0, 0);
        let _ = destination_builder.add("leaf", "v", Some(destination_child), 0, 0);
        let destination_tree = destination_builder.build(destination_root);

        let mut mapping = Mapping::new();
        match_top_down(
            &source_tree,
            &destination_tree,
            &mut mapping,
            DEFAULT_MIN_HEIGHT,
        );
        assert_eq!(mapping.get_dst(source_subtree), Some(destination_subtree));
    }

    #[test]
    fn dice_coefficient_zero_for_unmatched_subtrees() {
        let source_tree = small_tree("x", "y");
        let destination_tree = small_tree("a", "b");
        let mapping = Mapping::new();
        assert_eq!(
            dice_coefficient(
                &source_tree,
                source_tree.root(),
                &destination_tree,
                destination_tree.root(),
                &mapping
            ),
            0.0
        );
    }

    #[test]
    fn dice_coefficient_one_when_all_descendants_mapped() {
        let source_tree = small_tree("x", "y");
        let destination_tree = small_tree("x", "y");
        let mut mapping = Mapping::new();
        let source_descendants = source_tree.descendants(source_tree.root());
        let destination_descendants = destination_tree.descendants(destination_tree.root());
        for (source_node, destination_node) in source_descendants
            .iter()
            .zip(destination_descendants.iter())
        {
            mapping.link(*source_node, *destination_node);
        }
        let dice = dice_coefficient(
            &source_tree,
            source_tree.root(),
            &destination_tree,
            destination_tree.root(),
            &mapping,
        );
        assert!((dice - 1.0).abs() < 1e-9);
    }
}
