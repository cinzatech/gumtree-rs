//! Internal tree representation for AST diffing.
//!
//! Trees are stored in an arena (`Vec<Node>`) with `NodeId` indices into it.
//! Each node carries cached `height`, `size`, and a structural `hash` that allow
//! the matching algorithms to compare subtrees in O(1).
//!
//! Construction goes through [`TreeBuilder`], which lets you build a tree bottom-up
//! while passing parent IDs. Once built, the tree is immutable.

use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

/// Index of a node within a [`Tree`]'s arena.
pub type NodeId = usize;

/// A single node in an AST.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    /// The node "type" (e.g. `block_mapping_pair`, `if_expression`).
    pub kind: String,
    /// Text content for leaves; empty string for internal nodes by convention.
    pub label: String,
    /// Byte offset of the node's start in the source file.
    pub start_byte: usize,
    /// Byte offset just past the node's end.
    pub end_byte: usize,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// 1 for a leaf; max(child.height) + 1 otherwise.
    pub height: usize,
    /// Number of nodes in the subtree rooted here (including this node).
    pub size: usize,
    /// Structural hash combining kind, label, and ordered child hashes.
    pub hash: u64,
}

/// An immutable arena-backed tree.
#[derive(Debug, Clone)]
pub struct Tree {
    nodes: Vec<Node>,
    root: NodeId,
}

impl Tree {
    /// Returns the root node's id.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Returns a reference to the node with the given id.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// Returns the total number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns an iterator over every node, in id order.
    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter()
    }

    /// Pre-order traversal starting at `start` (root before children).
    pub fn pre_order(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size);
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            result.push(id);
            // Push children in reverse so the leftmost child is visited first.
            for &child_id in self.nodes[id].children.iter().rev() {
                stack.push(child_id);
            }
        }
        result
    }

    /// Post-order traversal starting at `start` (children before root).
    pub fn post_order(&self, start: NodeId) -> Vec<NodeId> {
        // Two-pass: reverse pre-order (right-to-left), then reverse the result.
        let mut result = Vec::with_capacity(self.nodes[start].size);
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            result.push(id);
            for &child_id in &self.nodes[id].children {
                stack.push(child_id);
            }
        }
        result.reverse();
        result
    }

    /// Breadth-first traversal starting at `start`.
    pub fn bfs_order(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size);
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some(id) = queue.pop_front() {
            result.push(id);
            for child_id in &self.nodes[id].children {
                queue.push_back(*child_id);
            }
        }
        result
    }

    /// All proper descendants of `start` (excluding `start` itself).
    pub fn descendants(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size.saturating_sub(1));
        let mut stack: Vec<NodeId> = self.nodes[start].children.iter().copied().rev().collect();
        while let Some(id) = stack.pop() {
            result.push(id);
            for &child_id in self.nodes[id].children.iter().rev() {
                stack.push(child_id);
            }
        }
        result
    }

    /// Returns the position of `child` within its parent's children list,
    /// or `None` if `child` is the root or has no parent.
    pub fn position_in_parent(&self, child: NodeId) -> Option<usize> {
        let parent = self.nodes[child].parent?;
        self.nodes[parent]
            .children
            .iter()
            .position(|&candidate| candidate == child)
    }

    fn recompute_metadata(&mut self) {
        // Post-order ensures children are processed before parents.
        let order = self.post_order(self.root);
        for id in order {
            let num_children = self.nodes[id].children.len();

            // Height and size.
            if num_children == 0 {
                self.nodes[id].height = 1;
                self.nodes[id].size = 1;
            } else {
                let mut max_height = 0;
                let mut total_size = 1usize;
                for index in 0..num_children {
                    let child_id = self.nodes[id].children[index];
                    if self.nodes[child_id].height > max_height {
                        max_height = self.nodes[child_id].height;
                    }
                    total_size += self.nodes[child_id].size;
                }
                self.nodes[id].height = max_height + 1;
                self.nodes[id].size = total_size;
            }

            // Structural hash: combines kind, label, and ordered child hashes.
            let mut hasher = DefaultHasher::new();
            self.nodes[id].kind.hash(&mut hasher);
            self.nodes[id].label.hash(&mut hasher);
            for index in 0..num_children {
                let child_id = self.nodes[id].children[index];
                self.nodes[child_id].hash.hash(&mut hasher);
            }
            self.nodes[id].hash = hasher.finish();
        }
    }
}

/// Mutable builder used to construct a [`Tree`] node by node.
#[derive(Debug, Default)]
pub struct TreeBuilder {
    nodes: Vec<Node>,
}

impl TreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node and returns its assigned id.
    ///
    /// `parent` must be a previously-added node (or `None` for the root).
    pub fn add(
        &mut self,
        kind: &str,
        label: &str,
        parent: Option<NodeId>,
        start_byte: usize,
        end_byte: usize,
    ) -> NodeId {
        let id = self.nodes.len();
        let node = Node {
            id,
            kind: kind.to_string(),
            label: label.to_string(),
            start_byte,
            end_byte,
            parent,
            children: Vec::new(),
            height: 0,
            size: 0,
            hash: 0,
        };
        if let Some(parent_id) = parent {
            self.nodes[parent_id].children.push(id);
        }
        self.nodes.push(node);
        id
    }

    /// Finalises the tree. `root` must be a valid id, normally the first node added.
    pub fn build(self, root: NodeId) -> Tree {
        assert!(root < self.nodes.len(), "root id out of bounds");
        let mut tree = Tree {
            nodes: self.nodes,
            root,
        };
        tree.recompute_metadata();
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build (a (b 1) (c 2)).
    fn sample_tree() -> Tree {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("a", "", None, 0, 10);
        let branch_b = builder.add("b", "", Some(root_id), 0, 5);
        let _leaf_one = builder.add("leaf", "1", Some(branch_b), 1, 2);
        let branch_c = builder.add("c", "", Some(root_id), 5, 10);
        let _leaf_two = builder.add("leaf", "2", Some(branch_c), 6, 7);
        builder.build(root_id)
    }

    #[test]
    fn builder_links_parent_and_children() {
        let tree = sample_tree();
        let root = tree.root();
        let root_children = &tree.node(root).children;
        assert_eq!(root_children.len(), 2);
        for child_id in root_children {
            assert_eq!(tree.node(*child_id).parent, Some(root));
        }
    }

    #[test]
    fn root_has_no_parent() {
        let tree = sample_tree();
        assert_eq!(tree.node(tree.root()).parent, None);
    }

    #[test]
    fn height_of_leaf_is_one() {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("x", "lbl", None, 0, 1);
        let tree = builder.build(root_id);
        assert_eq!(tree.node(root_id).height, 1);
    }

    #[test]
    fn height_of_internal_is_max_child_plus_one() {
        let tree = sample_tree();
        // root has children of height 2 → root height is 3.
        assert_eq!(tree.node(tree.root()).height, 3);
    }

    #[test]
    fn size_includes_node_itself_and_all_descendants() {
        let tree = sample_tree();
        // Tree has 5 nodes total.
        assert_eq!(tree.node(tree.root()).size, 5);
    }

    #[test]
    fn size_of_leaf_is_one() {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("x", "", None, 0, 1);
        let tree = builder.build(root_id);
        assert_eq!(tree.node(root_id).size, 1);
    }

    #[test]
    fn hash_equal_for_structurally_identical_trees() {
        let mut builder_a = TreeBuilder::new();
        let root_a = builder_a.add("r", "", None, 0, 0);
        let _child_a = builder_a.add("c", "x", Some(root_a), 0, 0);
        let tree_a = builder_a.build(root_a);

        let mut builder_b = TreeBuilder::new();
        let root_b = builder_b.add("r", "", None, 0, 0);
        let _child_b = builder_b.add("c", "x", Some(root_b), 0, 0);
        let tree_b = builder_b.build(root_b);

        assert_eq!(
            tree_a.node(tree_a.root()).hash,
            tree_b.node(tree_b.root()).hash
        );
    }

    #[test]
    fn hash_differs_when_labels_differ() {
        let mut builder_a = TreeBuilder::new();
        let root_a = builder_a.add("r", "", None, 0, 0);
        let _child_a = builder_a.add("c", "old", Some(root_a), 0, 0);
        let tree_a = builder_a.build(root_a);

        let mut builder_b = TreeBuilder::new();
        let root_b = builder_b.add("r", "", None, 0, 0);
        let _child_b = builder_b.add("c", "new", Some(root_b), 0, 0);
        let tree_b = builder_b.build(root_b);

        assert_ne!(
            tree_a.node(tree_a.root()).hash,
            tree_b.node(tree_b.root()).hash
        );
    }

    #[test]
    fn hash_differs_when_child_order_differs() {
        let mut builder_a = TreeBuilder::new();
        let root_a = builder_a.add("r", "", None, 0, 0);
        let _first_a = builder_a.add("c", "1", Some(root_a), 0, 0);
        let _second_a = builder_a.add("c", "2", Some(root_a), 0, 0);
        let tree_a = builder_a.build(root_a);

        let mut builder_b = TreeBuilder::new();
        let root_b = builder_b.add("r", "", None, 0, 0);
        let _second_b = builder_b.add("c", "2", Some(root_b), 0, 0);
        let _first_b = builder_b.add("c", "1", Some(root_b), 0, 0);
        let tree_b = builder_b.build(root_b);

        assert_ne!(
            tree_a.node(tree_a.root()).hash,
            tree_b.node(tree_b.root()).hash
        );
    }

    #[test]
    fn hash_differs_when_kinds_differ() {
        let mut builder_a = TreeBuilder::new();
        let root_a = builder_a.add("r", "", None, 0, 0);
        let tree_a = builder_a.build(root_a);

        let mut builder_b = TreeBuilder::new();
        let root_b = builder_b.add("R", "", None, 0, 0);
        let tree_b = builder_b.build(root_b);

        assert_ne!(
            tree_a.node(tree_a.root()).hash,
            tree_b.node(tree_b.root()).hash
        );
    }

    #[test]
    fn pre_order_visits_root_first() {
        let tree = sample_tree();
        let order = tree.pre_order(tree.root());
        assert_eq!(order[0], tree.root());
        assert_eq!(order.len(), tree.node_count());
    }

    #[test]
    fn post_order_visits_root_last() {
        let tree = sample_tree();
        let order = tree.post_order(tree.root());
        assert_eq!(*order.last().unwrap(), tree.root());
        assert_eq!(order.len(), tree.node_count());
    }

    #[test]
    fn post_order_visits_children_before_parent() {
        let tree = sample_tree();
        let order = tree.post_order(tree.root());
        // For each non-leaf, ensure every child appears before it.
        for (position, &node_id) in order.iter().enumerate() {
            for child_id in &tree.node(node_id).children {
                let child_position = order
                    .iter()
                    .position(|&candidate| candidate == *child_id)
                    .unwrap();
                assert!(
                    child_position < position,
                    "child {} should come before parent {}",
                    child_id,
                    node_id
                );
            }
        }
    }

    #[test]
    fn bfs_groups_by_depth() {
        let tree = sample_tree();
        let order = tree.bfs_order(tree.root());
        // Depths along BFS must be monotonically non-decreasing.
        let mut prev_depth = 0usize;
        for node_id in order {
            let mut depth = 0;
            let mut current_parent = tree.node(node_id).parent;
            while let Some(parent_id) = current_parent {
                depth += 1;
                current_parent = tree.node(parent_id).parent;
            }
            assert!(depth >= prev_depth);
            prev_depth = depth;
        }
    }

    #[test]
    fn descendants_excludes_self() {
        let tree = sample_tree();
        let descendant_ids = tree.descendants(tree.root());
        assert!(!descendant_ids.contains(&tree.root()));
        // Root has 4 proper descendants (b, 1, c, 2).
        assert_eq!(descendant_ids.len(), 4);
    }

    #[test]
    fn descendants_of_leaf_is_empty() {
        let mut builder = TreeBuilder::new();
        let root_id = builder.add("x", "", None, 0, 0);
        let tree = builder.build(root_id);
        assert!(tree.descendants(root_id).is_empty());
    }

    #[test]
    fn position_in_parent_returns_index() {
        let tree = sample_tree();
        let root_children = tree.node(tree.root()).children.clone();
        for (index, child_id) in root_children.iter().enumerate() {
            assert_eq!(tree.position_in_parent(*child_id), Some(index));
        }
    }

    #[test]
    fn position_in_parent_of_root_is_none() {
        let tree = sample_tree();
        assert_eq!(tree.position_in_parent(tree.root()), None);
    }

    #[test]
    fn all_nodes_yields_node_count_nodes() {
        let tree = sample_tree();
        assert_eq!(tree.all_nodes().count(), tree.node_count());
    }
}
