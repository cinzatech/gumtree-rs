//! Edit script generation (Chawathe et al.).
//!
//! Given a mapping `M : T1 → T2`, produces a list of [`Action`]s describing
//! how to transform `T1` into `T2`:
//!
//! * **insert-tree / insert-node** — destination nodes with no source counterpart.
//! * **delete-tree / delete-node** — source nodes with no destination counterpart.
//! * **update-node** — same node but different label.
//! * **move-tree** — mapped node whose parent changed, or whose siblings reordered.
//!
//! The algorithm walks T2 in BFS order to emit inserts/updates/moves, then runs
//! an alignment pass over each mapped (w, x) pair to catch sibling reorderings,
//! then walks T1 in pre-order to emit deletes (collapsing to delete-tree where
//! the entire subtree is unmapped).
//!
//! ## Known limitation
//!
//! Positions are emitted as final indices in T2 (i.e. where the node ends up
//! in the destination tree). The Java GumTree tracks positions dynamically as
//! actions are applied, so a strict comparison of the `at` field may diverge
//! even when the action set is semantically equivalent.

use std::collections::{HashMap, HashSet};

use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Insert a fresh subtree (root + all descendants new).
    InsertTree {
        node: NodeId,
        parent: NodeId,
        position: usize,
    },
    /// Insert a single node whose descendants partially survive.
    InsertNode {
        node: NodeId,
        parent: NodeId,
        position: usize,
    },
    /// Delete an entire unmapped subtree.
    DeleteTree { node: NodeId },
    /// Delete a single unmapped node whose descendants survive.
    DeleteNode { node: NodeId },
    /// Change the label of a mapped node.
    Update { node: NodeId, new_label: String },
    /// Move a mapped subtree under a new parent / position.
    MoveTree {
        node: NodeId,
        parent: NodeId,
        position: usize,
    },
}

impl Action {
    pub fn action_str(&self) -> &'static str {
        match self {
            Action::InsertTree { .. } => "insert-tree",
            Action::InsertNode { .. } => "insert-node",
            Action::DeleteTree { .. } => "delete-tree",
            Action::DeleteNode { .. } => "delete-node",
            Action::Update { .. } => "update-node",
            Action::MoveTree { .. } => "move-tree",
        }
    }
}

/// Generates the edit script that transforms `t1` into `t2` according to `mapping`.
pub fn generate_actions(t1: &Tree, t2: &Tree, mapping: &Mapping) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    let mut covered_by_insert_tree: HashSet<NodeId> = HashSet::new();

    // Phase 1: BFS over T2 — emit inserts / updates / moves.
    let bfs = t2.bfs_order(t2.root());
    for x in bfs {
        if x == t2.root() {
            continue;
        }
        if covered_by_insert_tree.contains(&x) {
            continue;
        }
        let y = t2.node(x).parent.expect("non-root has a parent");
        let pos = t2
            .node(y)
            .children
            .iter()
            .position(|&c| c == x)
            .expect("x must be in its parent's children");

        if !mapping.has_dst(x) {
            // x is new.
            let descs = t2.descendants(x);
            let all_new = descs.iter().all(|d| !mapping.has_dst(*d));
            if all_new {
                actions.push(Action::InsertTree {
                    node: x,
                    parent: y,
                    position: pos,
                });
                for d in descs {
                    covered_by_insert_tree.insert(d);
                }
            } else {
                actions.push(Action::InsertNode {
                    node: x,
                    parent: y,
                    position: pos,
                });
            }
        } else {
            let w = mapping.get_src(x).expect("mapped");

            // Label change → update.
            if t1.node(w).label != t2.node(x).label {
                actions.push(Action::Update {
                    node: w,
                    new_label: t2.node(x).label.clone(),
                });
            }

            // Parent mismatch → move-tree.
            let parent_of_w = t1.node(w).parent;
            let expected_parent_in_t1 = mapping.get_src(y);
            if parent_of_w != expected_parent_in_t1 {
                actions.push(Action::MoveTree {
                    node: w,
                    parent: y,
                    position: pos,
                });
            }
        }
    }

    // Phase 2: alignment — within mapped (w, x), order mapped children to match T2.
    for (w, x) in mapping.pairs() {
        align_children(t1, w, t2, x, mapping, &mut actions);
    }
    actions = dedup_moves(actions);

    // Phase 3: pre-order over T1 — emit deletes, collapsing to delete-tree where possible.
    let mut covered_by_delete_tree: HashSet<NodeId> = HashSet::new();
    let pre = t1.pre_order(t1.root());
    for w in pre {
        if covered_by_delete_tree.contains(&w) {
            continue;
        }
        if mapping.has_src(w) {
            continue;
        }
        let descs = t1.descendants(w);
        let all_unmapped = descs.iter().all(|d| !mapping.has_src(*d));
        if all_unmapped {
            actions.push(Action::DeleteTree { node: w });
            for d in descs {
                covered_by_delete_tree.insert(d);
            }
        } else {
            actions.push(Action::DeleteNode { node: w });
        }
    }

    actions
}

fn align_children(
    t1: &Tree,
    w: NodeId,
    t2: &Tree,
    x: NodeId,
    mapping: &Mapping,
    actions: &mut Vec<Action>,
) {
    let w_children = &t1.node(w).children;
    let x_children = &t2.node(x).children;
    if w_children.is_empty() || x_children.is_empty() {
        return;
    }
    let x_pos: HashMap<NodeId, usize> = x_children
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // Collect mapped children of w whose image is a child of x.
    let mut paired: Vec<(NodeId, usize)> = Vec::new(); // (w-child, x-pos)
    for &c in w_children {
        if let Some(c_map) = mapping.get_dst(c) {
            if let Some(&xp) = x_pos.get(&c_map) {
                paired.push((c, xp));
            }
        }
    }
    if paired.len() < 2 {
        return;
    }

    // Already in T1 child order. LIS on x-positions = children that stay in place.
    let x_pos_seq: Vec<usize> = paired.iter().map(|p| p.1).collect();
    let lis = longest_increasing_subsequence(&x_pos_seq);
    let lis_set: HashSet<usize> = lis.into_iter().collect();

    for (i, &(c, xp)) in paired.iter().enumerate() {
        if !lis_set.contains(&i) {
            actions.push(Action::MoveTree {
                node: c,
                parent: x,
                position: xp,
            });
        }
    }
}

/// Returns the indices into `seq` that form one longest strictly-increasing subsequence.
fn longest_increasing_subsequence(seq: &[usize]) -> Vec<usize> {
    let n = seq.len();
    if n == 0 {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::new(); // tails[i] = index ending an LIS of length i+1
    let mut prev: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let v = seq[i];
        let pos = tails
            .binary_search_by(|&t| {
                if seq[t] < v {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_or_else(|p| p);
        if pos > 0 {
            prev[i] = Some(tails[pos - 1]);
        }
        if pos < tails.len() {
            tails[pos] = i;
        } else {
            tails.push(i);
        }
    }
    let mut out = Vec::new();
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        out.push(i);
        cur = prev[i];
    }
    out.reverse();
    out
}

/// Removes duplicate moves for the same node, keeping the first occurrence.
fn dedup_moves(actions: Vec<Action>) -> Vec<Action> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut out = Vec::with_capacity(actions.len());
    for a in actions {
        match &a {
            Action::MoveTree { node, .. } => {
                if seen.insert(*node) {
                    out.push(a);
                }
            }
            _ => out.push(a),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::{match_trees, MatchOptions};
    use crate::tree::TreeBuilder;

    fn diff(t1: &Tree, t2: &Tree) -> (Mapping, Vec<Action>) {
        let m = match_trees(t1, t2, MatchOptions::default());
        let actions = generate_actions(t1, t2, &m);
        (m, actions)
    }

    #[test]
    fn identical_trees_yield_no_actions() {
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "v", Some(a), 0, 0);
        let t1 = b.build(r);

        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "v", Some(a), 0, 0);
        let t2 = b.build(r);

        let (_, actions) = diff(&t1, &t2);
        assert!(actions.is_empty(), "got {:?}", actions);
    }

    #[test]
    fn label_change_emits_update() {
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let v1 = b.add("leaf", "old", Some(a), 0, 0);
        let _ = v1;
        let t1 = b.build(r);

        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "new", Some(a), 0, 0);
        let t2 = b.build(r);

        let (_, actions) = diff(&t1, &t2);
        // Exactly one update-node action.
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Update { new_label, .. } => assert_eq!(new_label, "new"),
            other => panic!("expected Update, got {:?}", other),
        }
    }

    #[test]
    fn pure_insertion_emits_insert_tree() {
        // T1: (root (a 1))
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let t1 = b.build(r);

        // T2: (root (a 1) (b 2))
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let bb = b.add("b", "", Some(r), 0, 0);
        let _ = b.add("leaf", "2", Some(bb), 0, 0);
        let t2 = b.build(r);

        let (_, actions) = diff(&t1, &t2);
        // Expect exactly one insert-tree (rooted at the new `b`); no inserts
        // for its descendants.
        let inserts: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .collect();
        assert_eq!(inserts.len(), 1);
        assert!(matches!(inserts[0], Action::InsertTree { .. }));
    }

    #[test]
    fn pure_deletion_emits_delete_tree() {
        // T1: (root (a 1) (b 2))
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let bb = b.add("b", "", Some(r), 0, 0);
        let _ = b.add("leaf", "2", Some(bb), 0, 0);
        let t1 = b.build(r);

        // T2: (root (a 1))
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let t2 = b.build(r);

        let (_, actions) = diff(&t1, &t2);
        let deletes: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .collect();
        assert_eq!(deletes.len(), 1);
        assert!(matches!(deletes[0], Action::DeleteTree { .. }));
    }

    #[test]
    fn sibling_reorder_emits_move_tree() {
        // T1: (root (a 1) (b 2))
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let bb = b.add("b", "", Some(r), 0, 0);
        let _ = b.add("leaf", "2", Some(bb), 0, 0);
        let t1 = b.build(r);

        // T2: (root (b 2) (a 1)) — swapped order
        let mut b = TreeBuilder::new();
        let r = b.add("root", "", None, 0, 0);
        let bb = b.add("b", "", Some(r), 0, 0);
        let _ = b.add("leaf", "2", Some(bb), 0, 0);
        let a = b.add("a", "", Some(r), 0, 0);
        let _ = b.add("leaf", "1", Some(a), 0, 0);
        let t2 = b.build(r);

        let (_, actions) = diff(&t1, &t2);
        let moves: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::MoveTree { .. }))
            .collect();
        assert!(!moves.is_empty(), "expected at least one move");
        // No inserts or deletes for the moved nodes.
        let inserts = actions
            .iter()
            .filter(|a| matches!(a, Action::InsertTree { .. } | Action::InsertNode { .. }))
            .count();
        let deletes = actions
            .iter()
            .filter(|a| matches!(a, Action::DeleteTree { .. } | Action::DeleteNode { .. }))
            .count();
        assert_eq!(inserts, 0);
        assert_eq!(deletes, 0);
    }

    #[test]
    fn update_action_references_t1_node() {
        // Set up a label change and assert the update action's node id is
        // the T1 node (not the T2 node).
        let mut b = TreeBuilder::new();
        let r1 = b.add("root", "", None, 0, 0);
        let a1 = b.add("a", "", Some(r1), 0, 0);
        let v1 = b.add("leaf", "old", Some(a1), 0, 0);
        let t1 = b.build(r1);

        let mut b = TreeBuilder::new();
        let r2 = b.add("root", "", None, 0, 0);
        let a2 = b.add("a", "", Some(r2), 0, 0);
        let _v2 = b.add("leaf", "new", Some(a2), 0, 0);
        let t2 = b.build(r2);

        let (_, actions) = diff(&t1, &t2);
        for a in &actions {
            if let Action::Update { node, new_label } = a {
                assert_eq!(*node, v1);
                assert_eq!(new_label, "new");
                return;
            }
        }
        panic!("expected an Update action");
    }

    #[test]
    fn empty_unchanged_subtree_does_not_emit_anything() {
        // Two trees with disjoint changes only at one location; the other
        // location is identical and should produce no actions.
        // T1: (root (a 1) (b "x"))
        // T2: (root (a 1) (b "y"))
        let mut b = TreeBuilder::new();
        let r1 = b.add("root", "", None, 0, 0);
        let a1 = b.add("a", "", Some(r1), 0, 0);
        let _ = b.add("leaf", "1", Some(a1), 0, 0);
        let bb1 = b.add("b", "", Some(r1), 0, 0);
        let _ = b.add("leaf", "x", Some(bb1), 0, 0);
        let t1 = b.build(r1);

        let mut b = TreeBuilder::new();
        let r2 = b.add("root", "", None, 0, 0);
        let a2 = b.add("a", "", Some(r2), 0, 0);
        let _ = b.add("leaf", "1", Some(a2), 0, 0);
        let bb2 = b.add("b", "", Some(r2), 0, 0);
        let _ = b.add("leaf", "y", Some(bb2), 0, 0);
        let t2 = b.build(r2);

        let (_, actions) = diff(&t1, &t2);
        // Exactly one update; no moves/inserts/deletes.
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Update { .. }));
    }

    #[test]
    fn lis_finds_increasing_indices() {
        let lis = longest_increasing_subsequence(&[3, 1, 4, 1, 5, 9, 2, 6]);
        // Length of LIS for that input is 4; values at returned indices are
        // strictly increasing in `seq`.
        let seq = [3usize, 1, 4, 1, 5, 9, 2, 6];
        for w in lis.windows(2) {
            assert!(seq[w[0]] < seq[w[1]]);
        }
        assert_eq!(lis.len(), 4);
    }

    #[test]
    fn lis_of_empty_is_empty() {
        assert!(longest_increasing_subsequence(&[]).is_empty());
    }

    #[test]
    fn lis_of_sorted_is_full_length() {
        let lis = longest_increasing_subsequence(&[1, 2, 3, 4]);
        assert_eq!(lis.len(), 4);
    }

    #[test]
    fn lis_of_reverse_is_length_one() {
        let lis = longest_increasing_subsequence(&[4, 3, 2, 1]);
        assert_eq!(lis.len(), 1);
    }
}
