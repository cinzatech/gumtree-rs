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

use crate::lis::longest_increasing_subsequence;
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
            continue;
        }

        let source_node = mapping.get_src(destination_node).expect("mapped");

        // Label change → update.
        if source_tree.node(source_node).label != destination_tree.node(destination_node).label {
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
