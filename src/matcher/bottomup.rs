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
    t1: &Tree,
    t2: &Tree,
    mapping: &mut Mapping,
    min_dice: f64,
    max_size: usize,
) {
    // Build a kind → [NodeId] index over T2 so candidate lookup is O(same-kind)
    // instead of O(|T2|) per unmapped node.
    let mut kind_index: HashMap<&str, Vec<NodeId>> = HashMap::new();
    for n in t2.all_nodes() {
        kind_index.entry(&n.kind).or_default().push(n.id);
    }

    let order = t1.post_order(t1.root());
    for n1 in order {
        if mapping.has_src(n1) {
            continue;
        }
        if !has_matched_descendant(t1, n1, mapping) {
            continue;
        }
        if let Some(n2) = find_candidate(t1, n1, t2, mapping, min_dice, &kind_index) {
            mapping.link(n1, n2);
            if t1.node(n1).size.max(t2.node(n2).size) < max_size {
                recover_simple(t1, n1, t2, n2, mapping);
            }
        }
    }
}

fn has_matched_descendant(t: &Tree, n: NodeId, mapping: &Mapping) -> bool {
    t.descendants(n).iter().any(|d| mapping.has_src(*d))
}

/// Best unmapped node in T2 with the same kind, by dice similarity.
fn find_candidate<'a>(
    t1: &Tree,
    n1: NodeId,
    t2: &'a Tree,
    mapping: &Mapping,
    min_dice: f64,
    kind_index: &HashMap<&'a str, Vec<NodeId>>,
) -> Option<NodeId> {
    let kind = &t1.node(n1).kind;
    let candidates = kind_index.get(kind.as_str())?;
    let mut best: Option<(NodeId, f64)> = None;
    for &n2 in candidates {
        if mapping.has_dst(n2) {
            continue;
        }
        let d = dice_coefficient(t1, n1, t2, n2, mapping);
        if d < min_dice {
            continue;
        }
        match best {
            None => best = Some((n2, d)),
            Some((_, bd)) if d > bd => best = Some((n2, d)),
            _ => {}
        }
    }
    best.map(|(n, _)| n)
}

/// SimpleGumTree's cheap recovery: match remaining unmapped descendants by
/// (kind, label) first, then by (mapped parent + kind + sibling position).
///
/// Exposed so that integrating code (e.g. [`crate::matcher::match_trees`])
/// can invoke recovery as a fallback after both matching phases.
pub fn recover_simple(t1: &Tree, n1: NodeId, t2: &Tree, n2: NodeId, mapping: &mut Mapping) {
    let d1 = t1.descendants(n1);
    let d2 = t2.descendants(n2);

    // Phase 1: exact (kind, label) histogram pairing.
    let mut by_kind_label: HashMap<(String, String), Vec<NodeId>> = HashMap::new();
    for d in &d2 {
        if mapping.has_dst(*d) {
            continue;
        }
        let n = t2.node(*d);
        by_kind_label
            .entry((n.kind.clone(), n.label.clone()))
            .or_default()
            .push(*d);
    }
    for d in &d1 {
        if mapping.has_src(*d) {
            continue;
        }
        let n = t1.node(*d);
        let key = (n.kind.clone(), n.label.clone());
        if let Some(bucket) = by_kind_label.get_mut(&key) {
            if let Some(m) = bucket.pop() {
                mapping.link(*d, m);
            }
        }
    }

    // Phase 2: among still-unmapped, match by parent correspondence + kind.
    // This catches leaves whose label changed (so phase 1 missed them) but
    // whose parents are already mapped.
    for d in &d1 {
        if mapping.has_src(*d) {
            continue;
        }
        let parent = match t1.node(*d).parent {
            Some(p) => p,
            None => continue,
        };
        let parent_image = match mapping.get_dst(parent) {
            Some(p) => p,
            None => continue,
        };
        let kind = t1.node(*d).kind.clone();

        // Same-index sibling first (preserves position when possible).
        let sibling_idx = t1
            .node(parent)
            .children
            .iter()
            .position(|&c| c == *d)
            .unwrap();
        let candidates = &t2.node(parent_image).children;
        if sibling_idx < candidates.len() {
            let c = candidates[sibling_idx];
            if !mapping.has_dst(c) && t2.node(c).kind == kind {
                mapping.link(*d, c);
                continue;
            }
        }
        // Otherwise, first unmapped same-kind sibling.
        for &c in candidates {
            if !mapping.has_dst(c) && t2.node(c).kind == kind {
                mapping.link(*d, c);
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
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        // Stable anchor: (anchor (deep (leaf "stable")))
        let a1 = b1.add("anchor", "", Some(r1), 0, 0);
        let d1 = b1.add("deep", "", Some(a1), 0, 0);
        let _ = b1.add("leaf", "stable", Some(d1), 0, 0);
        // Section with label change.
        let it1 = b1.add("item", "", Some(r1), 0, 0);
        let _k1 = b1.add("key", "k", Some(it1), 0, 0);
        let v1 = b1.add("val", "old", Some(it1), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let a2 = b2.add("anchor", "", Some(r2), 0, 0);
        let d2 = b2.add("deep", "", Some(a2), 0, 0);
        let _ = b2.add("leaf", "stable", Some(d2), 0, 0);
        let it2 = b2.add("item", "", Some(r2), 0, 0);
        let _k2 = b2.add("key", "k", Some(it2), 0, 0);
        let v2 = b2.add("val", "new", Some(it2), 0, 0);
        let t2 = b2.build(r2);

        (t1, t2, v1, v2)
    }

    #[test]
    fn bottom_up_maps_container_when_descendants_anchor() {
        let (t1, t2, v1, v2) = pair_with_label_change();
        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);
        // Top-down won't map the `val` nodes (their labels differ → different hashes).
        assert!(!m.has_src(v1));
        // But it should anchor the stable subtree (height 3 = > min_height).
        assert!(
            !m.is_empty(),
            "top-down should have anchored the stable subtree"
        );

        match_bottom_up(&t1, &t2, &mut m, DEFAULT_MIN_DICE, DEFAULT_MAX_SIZE);
        // After bottom-up + simple recovery, the val nodes should be linked.
        assert_eq!(m.get_dst(v1), Some(v2));
    }

    #[test]
    fn bottom_up_does_nothing_when_no_descendants_match() {
        // Trees with completely disjoint kinds — top-down anchors nothing,
        // so bottom-up has no descendants to bootstrap from.
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let _ = b1.add("alpha", "a", Some(r1), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let _ = b2.add("beta", "b", Some(r2), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_bottom_up(&t1, &t2, &mut m, DEFAULT_MIN_DICE, DEFAULT_MAX_SIZE);
        assert!(m.is_empty());
    }

    #[test]
    fn min_dice_threshold_blocks_weak_matches() {
        // Two containers sharing 3 anchored subtrees plus one unique child each.
        // Shared descendants dominate, so dice is well above 0.5 (default) but
        // still below 0.99 (strict).
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let c1 = b1.add("ctr", "", Some(r1), 0, 0);
        for _ in 0..3 {
            let an = b1.add("anchor", "", Some(c1), 0, 0);
            let inn = b1.add("inner", "", Some(an), 0, 0);
            let _ = b1.add("leaf", "a", Some(inn), 0, 0);
        }
        let _ = b1.add("only_in_1", "x", Some(c1), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let c2 = b2.add("ctr", "", Some(r2), 0, 0);
        for _ in 0..3 {
            let an = b2.add("anchor", "", Some(c2), 0, 0);
            let inn = b2.add("inner", "", Some(an), 0, 0);
            let _ = b2.add("leaf", "a", Some(inn), 0, 0);
        }
        let _ = b2.add("only_in_2", "y", Some(c2), 0, 0);
        let t2 = b2.build(r2);

        // Strict threshold blocks the ctr match (dice ~= 0.9, not >= 0.99).
        let mut m_strict = Mapping::new();
        match_top_down(&t1, &t2, &mut m_strict, DEFAULT_MIN_HEIGHT);
        match_bottom_up(&t1, &t2, &mut m_strict, 0.99, DEFAULT_MAX_SIZE);
        assert!(!m_strict.has_src(c1));

        // Default threshold accepts the ctr match.
        let mut m_default = Mapping::new();
        match_top_down(&t1, &t2, &mut m_default, DEFAULT_MIN_HEIGHT);
        match_bottom_up(&t1, &t2, &mut m_default, DEFAULT_MIN_DICE, DEFAULT_MAX_SIZE);
        assert_eq!(m_default.get_dst(c1), Some(c2));
    }

    #[test]
    fn recover_pairs_remaining_same_kind_label_nodes() {
        // Two trees with a stable anchor and identical (kind,label) leaves
        // that bottom-up's recovery should pair up after the container matches.
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        // Stable tall anchor.
        let a1 = b1.add("anchor_top", "", Some(r1), 0, 0);
        let a1_mid = b1.add("anchor_mid", "", Some(a1), 0, 0);
        let _ = b1.add("anchor_leaf", "x", Some(a1_mid), 0, 0);
        // Container with identical anchors.
        let ctr1 = b1.add("ctr", "", Some(r1), 0, 0);
        let n1 = b1.add("anchor", "A", Some(ctr1), 0, 0);
        let n2 = b1.add("anchor", "A", Some(ctr1), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let a2 = b2.add("anchor_top", "", Some(r2), 0, 0);
        let a2_mid = b2.add("anchor_mid", "", Some(a2), 0, 0);
        let _ = b2.add("anchor_leaf", "x", Some(a2_mid), 0, 0);
        let ctr2 = b2.add("ctr", "", Some(r2), 0, 0);
        let _ = b2.add("anchor", "A", Some(ctr2), 0, 0);
        let _ = b2.add("anchor", "A", Some(ctr2), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);
        match_bottom_up(&t1, &t2, &mut m, DEFAULT_MIN_DICE, DEFAULT_MAX_SIZE);
        // Both "anchor: A" leaves in ctr1 should now be paired with two in ctr2.
        assert!(m.has_src(n1), "first anchor should be mapped");
        assert!(m.has_src(n2), "second anchor should be mapped");
    }
}
