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
pub fn match_top_down(t1: &Tree, t2: &Tree, mapping: &mut Mapping, min_height: usize) {
    let mut l1 = HeightPQ::new();
    let mut l2 = HeightPQ::new();
    l1.push(t1, t1.root());
    l2.push(t2, t2.root());

    let mut ambiguous: Vec<(NodeId, NodeId)> = Vec::new();

    loop {
        let h1 = l1.peek_height();
        let h2 = l2.peek_height();

        let (top1, top2) = match (h1, h2) {
            (Some(a), Some(b)) => (a, b),
            _ => break,
        };
        let max_h = top1.max(top2);
        if max_h <= min_height {
            break;
        }

        if top1 > top2 {
            for id in l1.pop_max() {
                l1.open(t1, id);
            }
        } else if top2 > top1 {
            for id in l2.pop_max() {
                l2.open(t2, id);
            }
        } else {
            // Equal heights: try to find isomorphic matches at this height.
            let h1_nodes = l1.pop_max();
            let h2_nodes = l2.pop_max();

            let mut matched1: HashSet<NodeId> = HashSet::new();
            let mut matched2: HashSet<NodeId> = HashSet::new();

            // Collect every isomorphic pair.
            let mut iso_pairs: Vec<(NodeId, NodeId)> = Vec::new();
            for &n1 in &h1_nodes {
                for &n2 in &h2_nodes {
                    if is_isomorphic(t1, n1, t2, n2) {
                        iso_pairs.push((n1, n2));
                    }
                }
            }

            // Count occurrences to detect ambiguity.
            let mut n1_count: HashMap<NodeId, usize> = HashMap::new();
            let mut n2_count: HashMap<NodeId, usize> = HashMap::new();
            for &(a, b) in &iso_pairs {
                *n1_count.entry(a).or_insert(0) += 1;
                *n2_count.entry(b).or_insert(0) += 1;
            }

            for (a, b) in &iso_pairs {
                if n1_count[a] == 1 && n2_count[b] == 1 {
                    if !mapping.has_src(*a) && !mapping.has_dst(*b) {
                        map_isomorphic_subtree(t1, *a, t2, *b, mapping);
                    }
                    matched1.insert(*a);
                    matched2.insert(*b);
                } else {
                    ambiguous.push((*a, *b));
                    matched1.insert(*a);
                    matched2.insert(*b);
                }
            }

            // Nodes that didn't match anything: open their children.
            for id in &h1_nodes {
                if !matched1.contains(id) {
                    l1.open(t1, *id);
                }
            }
            for id in &h2_nodes {
                if !matched2.contains(id) {
                    l2.open(t2, *id);
                }
            }
        }
    }

    // Resolve ambiguous candidates by parent-context dice, descending.
    let mut scored: Vec<(f64, NodeId, NodeId)> = ambiguous
        .into_iter()
        .map(|(a, b)| (parent_dice(t1, t2, a, b, mapping), a, b))
        .collect();
    scored.sort_by(|x, y| {
        y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (_, a, b) in scored {
        if !mapping.has_src(a) && !mapping.has_dst(b) {
            map_isomorphic_subtree(t1, a, t2, b, mapping);
        }
    }
}

/// Two nodes are isomorphic if their structural hashes match (which already
/// covers kind, label, and child structure). The kind check is a cheap guard
/// against hash collisions.
fn is_isomorphic(t1: &Tree, n1: NodeId, t2: &Tree, n2: NodeId) -> bool {
    t1.node(n1).hash == t2.node(n2).hash && t1.node(n1).kind == t2.node(n2).kind
}

/// Links two isomorphic subtrees node-by-node in lockstep.
fn map_isomorphic_subtree(
    t1: &Tree,
    n1: NodeId,
    t2: &Tree,
    n2: NodeId,
    mapping: &mut Mapping,
) {
    mapping.link(n1, n2);
    let c1 = t1.node(n1).children.clone();
    let c2 = t2.node(n2).children.clone();
    if c1.len() == c2.len() {
        for (a, b) in c1.iter().zip(c2.iter()) {
            map_isomorphic_subtree(t1, *a, t2, *b, mapping);
        }
    }
}

fn parent_dice(
    t1: &Tree,
    t2: &Tree,
    n1: NodeId,
    n2: NodeId,
    mapping: &Mapping,
) -> f64 {
    match (t1.node(n1).parent, t2.node(n2).parent) {
        (Some(p1), Some(p2)) => dice_coefficient(t1, p1, t2, p2, mapping),
        _ => 0.0,
    }
}

/// Dice similarity between two subtrees, given a partial mapping.
///
/// Defined as `2 * |common| / (|desc(n1)| + |desc(n2)|)` where `common` is the
/// number of descendants of `n1` whose mapped image lies within the descendants
/// of `n2`.
pub fn dice_coefficient(
    t1: &Tree,
    n1: NodeId,
    t2: &Tree,
    n2: NodeId,
    mapping: &Mapping,
) -> f64 {
    let desc1 = t1.descendants(n1);
    let desc2: HashSet<NodeId> = t2.descendants(n2).into_iter().collect();
    if desc1.is_empty() && desc2.is_empty() {
        return 0.0;
    }
    let mut common = 0usize;
    for d in &desc1 {
        if let Some(m) = mapping.get_dst(*d) {
            if desc2.contains(&m) {
                common += 1;
            }
        }
    }
    2.0 * (common as f64) / ((desc1.len() + desc2.len()) as f64)
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

    fn push(&mut self, t: &Tree, id: NodeId) {
        let h = t.node(id).height;
        self.buckets.entry(h).or_default().push(id);
    }

    fn peek_height(&self) -> Option<usize> {
        self.buckets.keys().next_back().copied()
    }

    fn pop_max(&mut self) -> Vec<NodeId> {
        let h = match self.peek_height() {
            Some(h) => h,
            None => return Vec::new(),
        };
        self.buckets.remove(&h).unwrap_or_default()
    }

    fn open(&mut self, t: &Tree, id: NodeId) {
        let children = t.node(id).children.clone();
        for c in children {
            self.push(t, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TreeBuilder;

    /// Builds (r (a x) (b y)) where leaves carry labels x and y.
    fn small_tree(x: &str, y: &str) -> Tree {
        let mut b = TreeBuilder::new();
        let r = b.add("r", "", None, 0, 10);
        let a = b.add("a", "", Some(r), 0, 5);
        let _ax = b.add("leaf", x, Some(a), 1, 2);
        let bb = b.add("b", "", Some(r), 5, 10);
        let _by = b.add("leaf", y, Some(bb), 6, 7);
        b.build(r)
    }

    #[test]
    fn identical_trees_are_fully_mapped() {
        let t1 = small_tree("x", "y");
        let t2 = small_tree("x", "y");
        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);
        // Top-down should map at least the root subtree (height 3).
        assert!(m.has_src(t1.root()));
        assert_eq!(m.get_dst(t1.root()), Some(t2.root()));
        // Since hashes match for the whole tree, every node should be mapped.
        assert_eq!(m.len(), t1.node_count());
    }

    #[test]
    fn completely_different_trees_yield_no_mapping() {
        let mut b1 = TreeBuilder::new();
        let r = b1.add("alpha", "", None, 0, 5);
        let _ = b1.add("alpha_child", "a", Some(r), 0, 1);
        let t1 = b1.build(r);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("beta", "", None, 0, 5);
        let _ = b2.add("beta_child", "b", Some(r2), 0, 1);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);
        assert!(m.is_empty());
    }

    #[test]
    fn shared_subtree_is_anchored() {
        // T1 and T2 share a deep subtree S of height 3 (above min_height=2),
        // but otherwise differ. Top-down should anchor S in both trees.
        // Shared subtree: (sub (mid (x 1)))
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let s1 = b1.add("sub", "", Some(r1), 0, 0);
        let m1 = b1.add("mid", "", Some(s1), 0, 0);
        let _ = b1.add("x", "1", Some(m1), 0, 0);
        let extra = b1.add("extra", "", Some(r1), 0, 0);
        let _ = b1.add("xx", "z", Some(extra), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let other = b2.add("other", "", Some(r2), 0, 0);
        let _ = b2.add("yy", "w", Some(other), 0, 0);
        let s2 = b2.add("sub", "", Some(r2), 0, 0);
        let m2 = b2.add("mid", "", Some(s2), 0, 0);
        let _ = b2.add("x", "1", Some(m2), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);

        // The shared `sub` subtree should be anchored.
        assert_eq!(m.get_dst(s1), Some(s2));
    }

    #[test]
    fn min_height_threshold_excludes_small_subtrees() {
        // The only shared subtree is at height 2; with min_height=2 (i.e.
        // strictly greater than 2), top-down should not match it.
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let small = b1.add("small", "", Some(r1), 0, 0);
        let _ = b1.add("leaf", "v", Some(small), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("Root", "", None, 0, 0); // different kind so roots don't iso
        let small2 = b2.add("small", "", Some(r2), 0, 0);
        let _ = b2.add("leaf", "v", Some(small2), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, 2);
        // small is height 2, threshold strict (max_h <= min_height stops).
        assert!(!m.has_src(small));
    }

    #[test]
    fn lowering_min_height_unlocks_smaller_subtrees() {
        // Same setup, but with min_height=1 the matcher should now anchor the
        // height-2 `small` subtree.
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let small = b1.add("small", "", Some(r1), 0, 0);
        let _ = b1.add("leaf", "v", Some(small), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("Root", "", None, 0, 0);
        let small2 = b2.add("small", "", Some(r2), 0, 0);
        let _ = b2.add("leaf", "v", Some(small2), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, 1);
        assert_eq!(m.get_dst(small), Some(small2));
    }

    #[test]
    fn maps_only_unique_isomorphic_anchors_directly() {
        // T1 and T2 contain a unique large subtree S.
        let mut b1 = TreeBuilder::new();
        let r1 = b1.add("root", "", None, 0, 0);
        let s1 = b1.add("S", "", Some(r1), 0, 0);
        let s1c = b1.add("child", "", Some(s1), 0, 0);
        let _ = b1.add("leaf", "v", Some(s1c), 0, 0);
        let t1 = b1.build(r1);

        let mut b2 = TreeBuilder::new();
        let r2 = b2.add("root", "", None, 0, 0);
        let s2 = b2.add("S", "", Some(r2), 0, 0);
        let s2c = b2.add("child", "", Some(s2), 0, 0);
        let _ = b2.add("leaf", "v", Some(s2c), 0, 0);
        let t2 = b2.build(r2);

        let mut m = Mapping::new();
        match_top_down(&t1, &t2, &mut m, DEFAULT_MIN_HEIGHT);
        assert_eq!(m.get_dst(s1), Some(s2));
    }

    #[test]
    fn dice_coefficient_zero_for_unmatched_subtrees() {
        let t1 = small_tree("x", "y");
        let t2 = small_tree("a", "b");
        let m = Mapping::new();
        assert_eq!(dice_coefficient(&t1, t1.root(), &t2, t2.root(), &m), 0.0);
    }

    #[test]
    fn dice_coefficient_one_when_all_descendants_mapped() {
        let t1 = small_tree("x", "y");
        let t2 = small_tree("x", "y");
        let mut m = Mapping::new();
        // Manually pair every descendant.
        let d1 = t1.descendants(t1.root());
        let d2 = t2.descendants(t2.root());
        for (a, b) in d1.iter().zip(d2.iter()) {
            m.link(*a, *b);
        }
        let dice = dice_coefficient(&t1, t1.root(), &t2, t2.root(), &m);
        assert!((dice - 1.0).abs() < 1e-9);
    }
}
