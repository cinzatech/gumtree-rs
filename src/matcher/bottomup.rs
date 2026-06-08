//! Bottom-up (container) matching phase plus the `SimpleGumTree` recovery step.
//!
//! After the top-down phase has anchored large isomorphic subtrees, this phase
//! walks `T1` in post-order. For each unmapped node whose subtree already
//! contains anchored descendants, it searches for the best container in `T2`
//! by Dice similarity, then runs a cheap greedy recovery for the remaining
//! unmapped descendants inside the matched pair.

use std::collections::HashMap;

use crate::mapping::Mapping;
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

    // Precompute which subtrees contain at least one matched descendant.
    // Post-order propagation: O(n) total instead of O(n²) from per-node
    // descendant collection.
    let has_matched = {
        let mut flags = vec![false; source_tree.node_count()];
        let order = source_tree.post_order(source_tree.root());
        for &id in &order {
            if mapping.has_src(id) {
                // Mark the parent so ancestor nodes see a matched descendant.
                if let Some(parent) = source_tree.node(id).parent {
                    flags[parent] = true;
                }
            } else if flags[id] {
                // This node has a matched descendant; propagate upward.
                if let Some(parent) = source_tree.node(id).parent {
                    flags[parent] = true;
                }
            }
        }
        flags
    };

    let order = source_tree.post_order(source_tree.root());
    for source_node in order {
        if mapping.has_src(source_node) {
            continue;
        }
        if !has_matched[source_node] {
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

/// Best unmapped node in T2 with the same kind, by dice similarity.
///
/// Collects source descendants once and reuses them across all candidates
/// to avoid repeated O(subtree-size) allocations.
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

    // Hoist the source-side descendant collection out of the candidate loop.
    let source_descendants = source_tree.descendants(source_node);

    candidates
        .iter()
        .copied()
        .filter(|candidate| !mapping.has_dst(*candidate))
        .map(|candidate| {
            let dice = dice_with_source_descendants(
                &source_descendants,
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

/// Dice similarity using pre-collected source descendants to avoid redundant
/// allocation when comparing one source node against multiple candidates.
fn dice_with_source_descendants(
    source_descendants: &[NodeId],
    destination_tree: &Tree,
    destination_node: NodeId,
    mapping: &Mapping,
) -> f64 {
    let dest_count = destination_tree.node(destination_node).size - 1;

    if source_descendants.is_empty() && dest_count == 0 {
        return 0.0;
    }

    let dest_member = destination_tree.descendant_set(destination_node);

    let common = source_descendants
        .iter()
        .filter_map(|descendant| mapping.get_dst(*descendant))
        .filter(|&mapped_destination| dest_member[mapped_destination])
        .count();

    let total = source_descendants.len() + dest_count;
    2.0 * (common as f64) / (total as f64)
}

/// `SimpleGumTree`'s cheap recovery: match remaining unmapped descendants by
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
/// Only match when the candidate is unique, common tokens like `def`,
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

        // Collect source descendants once for all candidates in this bucket.
        let src_descs = source_tree.descendants(*source_descendant);

        if bucket.len() == 1 {
            let candidate = bucket[0];
            let dice =
                dice_with_source_descendants(&src_descs, destination_tree, candidate, mapping);
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
                let dice =
                    dice_with_source_descendants(&src_descs, destination_tree, candidate, mapping);
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
