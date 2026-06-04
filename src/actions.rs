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
pub fn generate_actions(
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    let mut covered_by_insert_tree: HashSet<NodeId> = HashSet::new();

    // Phase 1: BFS over T2 — emit inserts / updates / moves.
    let bfs = destination_tree.bfs_order(destination_tree.root());
    for destination_node in bfs {
        if destination_node == destination_tree.root() {
            continue;
        }
        if covered_by_insert_tree.contains(&destination_node) {
            continue;
        }
        let parent = destination_tree
            .node(destination_node)
            .parent
            .expect("non-root has a parent");
        let position = destination_tree
            .node(parent)
            .children
            .iter()
            .position(|&child_id| child_id == destination_node)
            .expect("node must be in its parent's children");

        if !mapping.has_dst(destination_node) {
            // destination_node is new.
            let descendants = destination_tree.descendants(destination_node);
            let all_new = descendants
                .iter()
                .all(|descendant| !mapping.has_dst(*descendant));
            if all_new {
                actions.push(Action::InsertTree {
                    node: destination_node,
                    parent,
                    position,
                });
                for descendant in descendants {
                    covered_by_insert_tree.insert(descendant);
                }
            } else {
                actions.push(Action::InsertNode {
                    node: destination_node,
                    parent,
                    position,
                });
            }
        } else {
            let source_node = mapping.get_src(destination_node).expect("mapped");

            // Label change → update.
            if source_tree.node(source_node).label != destination_tree.node(destination_node).label
            {
                actions.push(Action::Update {
                    node: source_node,
                    new_label: destination_tree.node(destination_node).label.clone(),
                });
            }

            // Parent mismatch → move-tree.
            let parent_of_source = source_tree.node(source_node).parent;
            let expected_parent_in_source = mapping.get_src(parent);
            if parent_of_source != expected_parent_in_source {
                actions.push(Action::MoveTree {
                    node: source_node,
                    parent,
                    position,
                });
            }
        }
    }

    // Phase 2: alignment — within mapped (w, x), order mapped children to match T2.
    for (source_node, destination_node) in mapping.pairs() {
        align_children(
            source_tree,
            source_node,
            destination_tree,
            destination_node,
            mapping,
            &mut actions,
        );
    }
    actions = dedup_moves(actions);

    // Phase 3: pre-order over T1 — emit deletes, collapsing to delete-tree where possible.
    let mut covered_by_delete_tree: HashSet<NodeId> = HashSet::new();
    let pre_order = source_tree.pre_order(source_tree.root());
    for source_node in pre_order {
        if covered_by_delete_tree.contains(&source_node) {
            continue;
        }
        if mapping.has_src(source_node) {
            continue;
        }
        let descendants = source_tree.descendants(source_node);
        let all_unmapped = descendants
            .iter()
            .all(|descendant| !mapping.has_src(*descendant));
        if all_unmapped {
            actions.push(Action::DeleteTree { node: source_node });
            for descendant in descendants {
                covered_by_delete_tree.insert(descendant);
            }
        } else {
            actions.push(Action::DeleteNode { node: source_node });
        }
    }

    actions
}

fn align_children(
    source_tree: &Tree,
    source_node: NodeId,
    destination_tree: &Tree,
    destination_node: NodeId,
    mapping: &Mapping,
    actions: &mut Vec<Action>,
) {
    let source_children = &source_tree.node(source_node).children;
    let destination_children = &destination_tree.node(destination_node).children;
    if source_children.is_empty() || destination_children.is_empty() {
        return;
    }
    let destination_position_map: HashMap<NodeId, usize> = destination_children
        .iter()
        .enumerate()
        .map(|(index, &node_id)| (node_id, index))
        .collect();

    // Collect mapped children of source whose image is a child of destination.
    let mut paired: Vec<(NodeId, usize)> = Vec::new(); // (source-child, destination-position)
    for &child_id in source_children {
        if let Some(mapped_child) = mapping.get_dst(child_id) {
            if let Some(&destination_position) = destination_position_map.get(&mapped_child) {
                paired.push((child_id, destination_position));
            }
        }
    }
    if paired.len() < 2 {
        return;
    }

    // Already in T1 child order. LIS on x-positions = children that stay in place.
    let destination_positions: Vec<usize> = paired.iter().map(|pair| pair.1).collect();
    let lis = longest_increasing_subsequence(&destination_positions);
    let lis_set: HashSet<usize> = lis.into_iter().collect();

    for (index, &(child_id, destination_position)) in paired.iter().enumerate() {
        if !lis_set.contains(&index) {
            actions.push(Action::MoveTree {
                node: child_id,
                parent: destination_node,
                position: destination_position,
            });
        }
    }
}

/// Returns the indices into `seq` that form one longest strictly-increasing subsequence.
fn longest_increasing_subsequence(sequence: &[usize]) -> Vec<usize> {
    let length = sequence.len();
    if length == 0 {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::new(); // tails[index] = index ending an LIS of length index+1
    let mut predecessors: Vec<Option<usize>> = vec![None; length];
    for index in 0..length {
        let value = sequence[index];
        let insert_position = tails
            .binary_search_by(|&tail_index| {
                if sequence[tail_index] < value {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_or_else(|position| position);
        if insert_position > 0 {
            predecessors[index] = Some(tails[insert_position - 1]);
        }
        if insert_position < tails.len() {
            tails[insert_position] = index;
        } else {
            tails.push(index);
        }
    }
    let mut result = Vec::new();
    let mut current = tails.last().copied();
    while let Some(index) = current {
        result.push(index);
        current = predecessors[index];
    }
    result.reverse();
    result
}

/// Removes duplicate moves for the same node, keeping the first occurrence.
fn dedup_moves(actions: Vec<Action>) -> Vec<Action> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut result = Vec::with_capacity(actions.len());
    for action in actions {
        match &action {
            Action::MoveTree { node, .. } => {
                if seen.insert(*node) {
                    result.push(action);
                }
            }
            _ => result.push(action),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::{match_trees, MatchOptions};
    use crate::tree::TreeBuilder;

    fn diff(source_tree: &Tree, destination_tree: &Tree) -> (Mapping, Vec<Action>) {
        let mapping = match_trees(source_tree, destination_tree, MatchOptions::default());
        let actions = generate_actions(source_tree, destination_tree, &mapping);
        (mapping, actions)
    }

    #[test]
    fn identical_trees_yield_no_actions() {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "v", Some(branch), 0, 0);
        let source_tree = builder.build(root_id);

        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "v", Some(branch), 0, 0);
        let destination_tree = builder.build(root_id);

        let (_, actions) = diff(&source_tree, &destination_tree);
        assert!(actions.is_empty(), "got {:?}", actions);
    }

    #[test]
    fn label_change_emits_update() {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch = builder.add("a", "", Some(root_id), 0, 0);
        let old_leaf = builder.add("leaf", "old", Some(branch), 0, 0);
        let _ = old_leaf;
        let source_tree = builder.build(root_id);

        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "new", Some(branch), 0, 0);
        let destination_tree = builder.build(root_id);

        let (_, actions) = diff(&source_tree, &destination_tree);
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
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let source_tree = builder.build(root_id);

        // T2: (root (a 1) (b 2))
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let branch_b = builder.add("b", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
        let destination_tree = builder.build(root_id);

        let (_, actions) = diff(&source_tree, &destination_tree);
        // Expect exactly one insert-tree (rooted at the new `b`); no inserts
        // for its descendants.
        let inserts: Vec<&Action> = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::InsertTree { .. } | Action::InsertNode { .. }
                )
            })
            .collect();
        assert_eq!(inserts.len(), 1);
        assert!(matches!(inserts[0], Action::InsertTree { .. }));
    }

    #[test]
    fn pure_deletion_emits_delete_tree() {
        // T1: (root (a 1) (b 2))
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let branch_b = builder.add("b", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
        let source_tree = builder.build(root_id);

        // T2: (root (a 1))
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let destination_tree = builder.build(root_id);

        let (_, actions) = diff(&source_tree, &destination_tree);
        let deletes: Vec<&Action> = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::DeleteTree { .. } | Action::DeleteNode { .. }
                )
            })
            .collect();
        assert_eq!(deletes.len(), 1);
        assert!(matches!(deletes[0], Action::DeleteTree { .. }));
    }

    #[test]
    fn sibling_reorder_emits_move_tree() {
        // T1: (root (a 1) (b 2))
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let branch_b = builder.add("b", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
        let source_tree = builder.build(root_id);

        // T2: (root (b 2) (a 1)) — swapped order
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("root", "", None, 0, 0);
        let branch_b = builder.add("b", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "2", Some(branch_b), 0, 0);
        let branch_a = builder.add("a", "", Some(root_id), 0, 0);
        let _ = builder.add("leaf", "1", Some(branch_a), 0, 0);
        let destination_tree = builder.build(root_id);

        let (_, actions) = diff(&source_tree, &destination_tree);
        let moves: Vec<&Action> = actions
            .iter()
            .filter(|action| matches!(action, Action::MoveTree { .. }))
            .collect();
        assert!(!moves.is_empty(), "expected at least one move");
        // No inserts or deletes for the moved nodes.
        let inserts = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::InsertTree { .. } | Action::InsertNode { .. }
                )
            })
            .count();
        let deletes = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::DeleteTree { .. } | Action::DeleteNode { .. }
                )
            })
            .count();
        assert_eq!(inserts, 0);
        assert_eq!(deletes, 0);
    }

    #[test]
    fn update_action_references_t1_node() {
        // Set up a label change and assert the update action's node id is
        // the T1 node (not the T2 node).
        let mut builder = TreeBuilder::new();
        let source_root = builder.add("root", "", None, 0, 0);
        let source_branch = builder.add("a", "", Some(source_root), 0, 0);
        let source_leaf = builder.add("leaf", "old", Some(source_branch), 0, 0);
        let source_tree = builder.build(source_root);

        let mut builder = TreeBuilder::new();
        let destination_root = builder.add("root", "", None, 0, 0);
        let destination_branch = builder.add("a", "", Some(destination_root), 0, 0);
        let _destination_leaf = builder.add("leaf", "new", Some(destination_branch), 0, 0);
        let destination_tree = builder.build(destination_root);

        let (_, actions) = diff(&source_tree, &destination_tree);
        for action in &actions {
            if let Action::Update { node, new_label } = action {
                assert_eq!(*node, source_leaf);
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
        let mut builder = TreeBuilder::new();
        let source_root = builder.add("root", "", None, 0, 0);
        let source_branch_a = builder.add("a", "", Some(source_root), 0, 0);
        let _ = builder.add("leaf", "1", Some(source_branch_a), 0, 0);
        let source_branch_b = builder.add("b", "", Some(source_root), 0, 0);
        let _ = builder.add("leaf", "x", Some(source_branch_b), 0, 0);
        let source_tree = builder.build(source_root);

        let mut builder = TreeBuilder::new();
        let destination_root = builder.add("root", "", None, 0, 0);
        let destination_branch_a = builder.add("a", "", Some(destination_root), 0, 0);
        let _ = builder.add("leaf", "1", Some(destination_branch_a), 0, 0);
        let destination_branch_b = builder.add("b", "", Some(destination_root), 0, 0);
        let _ = builder.add("leaf", "y", Some(destination_branch_b), 0, 0);
        let destination_tree = builder.build(destination_root);

        let (_, actions) = diff(&source_tree, &destination_tree);
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
        for window in lis.windows(2) {
            assert!(seq[window[0]] < seq[window[1]]);
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
