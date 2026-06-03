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
        self.pre_order_into(start, &mut result);
        result
    }

    fn pre_order_into(&self, start: NodeId, result: &mut Vec<NodeId>) {
        result.push(start);
        // Clone the children vec to avoid borrow conflicts during recursion.
        let children = self.nodes[start].children.clone();
        for c in children {
            self.pre_order_into(c, result);
        }
    }

    /// Post-order traversal starting at `start` (children before root).
    pub fn post_order(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size);
        self.post_order_into(start, &mut result);
        result
    }

    fn post_order_into(&self, start: NodeId, result: &mut Vec<NodeId>) {
        let children = self.nodes[start].children.clone();
        for c in children {
            self.post_order_into(c, result);
        }
        result.push(start);
    }

    /// Breadth-first traversal starting at `start`.
    pub fn bfs_order(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size);
        let mut queue = VecDeque::new();
        queue.push_back(start);
        while let Some(id) = queue.pop_front() {
            result.push(id);
            for c in &self.nodes[id].children {
                queue.push_back(*c);
            }
        }
        result
    }

    /// All proper descendants of `start` (excluding `start` itself).
    pub fn descendants(&self, start: NodeId) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes[start].size.saturating_sub(1));
        let children = self.nodes[start].children.clone();
        for c in children {
            result.push(c);
            self.descendants_into(c, &mut result);
        }
        result
    }

    fn descendants_into(&self, start: NodeId, result: &mut Vec<NodeId>) {
        let children = self.nodes[start].children.clone();
        for c in children {
            result.push(c);
            self.descendants_into(c, result);
        }
    }

    /// Returns the position of `child` within its parent's children list,
    /// or `None` if `child` is the root or has no parent.
    pub fn position_in_parent(&self, child: NodeId) -> Option<usize> {
        let parent = self.nodes[child].parent?;
        self.nodes[parent].children.iter().position(|&c| c == child)
    }

    fn recompute_metadata(&mut self) {
        // Post-order ensures children are processed before parents.
        let order = self.post_order(self.root);
        for id in order {
            let children = self.nodes[id].children.clone();

            // Height and size.
            if children.is_empty() {
                self.nodes[id].height = 1;
                self.nodes[id].size = 1;
            } else {
                let mut max_h = 0;
                let mut total_size = 1usize;
                for c in &children {
                    if self.nodes[*c].height > max_h {
                        max_h = self.nodes[*c].height;
                    }
                    total_size += self.nodes[*c].size;
                }
                self.nodes[id].height = max_h + 1;
                self.nodes[id].size = total_size;
            }

            // Structural hash: combines kind, label, and ordered child hashes.
            let mut hasher = DefaultHasher::new();
            self.nodes[id].kind.hash(&mut hasher);
            self.nodes[id].label.hash(&mut hasher);
            for c in &children {
                self.nodes[*c].hash.hash(&mut hasher);
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
        if let Some(p) = parent {
            self.nodes[p].children.push(id);
        }
        self.nodes.push(node);
        id
    }

    /// Finalises the tree. `root` must be a valid id, normally the first node added.
    pub fn build(self, root: NodeId) -> Tree {
        assert!(root < self.nodes.len(), "root id out of bounds");
        let mut t = Tree {
            nodes: self.nodes,
            root,
        };
        t.recompute_metadata();
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build (a (b 1) (c 2)).
    fn sample_tree() -> Tree {
        let mut b = TreeBuilder::new();
        let a = b.add("a", "", None, 0, 10);
        let _b1 = b.add("b", "", Some(a), 0, 5);
        let _one = b.add("leaf", "1", Some(_b1), 1, 2);
        let _c = b.add("c", "", Some(a), 5, 10);
        let _two = b.add("leaf", "2", Some(_c), 6, 7);
        b.build(a)
    }

    #[test]
    fn builder_links_parent_and_children() {
        let t = sample_tree();
        let root = t.root();
        let root_children = &t.node(root).children;
        assert_eq!(root_children.len(), 2);
        for c in root_children {
            assert_eq!(t.node(*c).parent, Some(root));
        }
    }

    #[test]
    fn root_has_no_parent() {
        let t = sample_tree();
        assert_eq!(t.node(t.root()).parent, None);
    }

    #[test]
    fn height_of_leaf_is_one() {
        let mut b = TreeBuilder::new();
        let r = b.add("x", "lbl", None, 0, 1);
        let t = b.build(r);
        assert_eq!(t.node(r).height, 1);
    }

    #[test]
    fn height_of_internal_is_max_child_plus_one() {
        let t = sample_tree();
        // root has children of height 2 → root height is 3.
        assert_eq!(t.node(t.root()).height, 3);
    }

    #[test]
    fn size_includes_node_itself_and_all_descendants() {
        let t = sample_tree();
        // Tree has 5 nodes total.
        assert_eq!(t.node(t.root()).size, 5);
    }

    #[test]
    fn size_of_leaf_is_one() {
        let mut b = TreeBuilder::new();
        let r = b.add("x", "", None, 0, 1);
        let t = b.build(r);
        assert_eq!(t.node(r).size, 1);
    }

    #[test]
    fn hash_equal_for_structurally_identical_trees() {
        let mut a = TreeBuilder::new();
        let ar = a.add("r", "", None, 0, 0);
        let _ac = a.add("c", "x", Some(ar), 0, 0);
        let ta = a.build(ar);

        let mut b = TreeBuilder::new();
        let br = b.add("r", "", None, 0, 0);
        let _bc = b.add("c", "x", Some(br), 0, 0);
        let tb = b.build(br);

        assert_eq!(ta.node(ta.root()).hash, tb.node(tb.root()).hash);
    }

    #[test]
    fn hash_differs_when_labels_differ() {
        let mut a = TreeBuilder::new();
        let ar = a.add("r", "", None, 0, 0);
        let _ac = a.add("c", "old", Some(ar), 0, 0);
        let ta = a.build(ar);

        let mut b = TreeBuilder::new();
        let br = b.add("r", "", None, 0, 0);
        let _bc = b.add("c", "new", Some(br), 0, 0);
        let tb = b.build(br);

        assert_ne!(ta.node(ta.root()).hash, tb.node(tb.root()).hash);
    }

    #[test]
    fn hash_differs_when_child_order_differs() {
        let mut a = TreeBuilder::new();
        let ar = a.add("r", "", None, 0, 0);
        let _a1 = a.add("c", "1", Some(ar), 0, 0);
        let _a2 = a.add("c", "2", Some(ar), 0, 0);
        let ta = a.build(ar);

        let mut b = TreeBuilder::new();
        let br = b.add("r", "", None, 0, 0);
        let _b2 = b.add("c", "2", Some(br), 0, 0);
        let _b1 = b.add("c", "1", Some(br), 0, 0);
        let tb = b.build(br);

        assert_ne!(ta.node(ta.root()).hash, tb.node(tb.root()).hash);
    }

    #[test]
    fn hash_differs_when_kinds_differ() {
        let mut a = TreeBuilder::new();
        let ar = a.add("r", "", None, 0, 0);
        let ta = a.build(ar);

        let mut b = TreeBuilder::new();
        let br = b.add("R", "", None, 0, 0);
        let tb = b.build(br);

        assert_ne!(ta.node(ta.root()).hash, tb.node(tb.root()).hash);
    }

    #[test]
    fn pre_order_visits_root_first() {
        let t = sample_tree();
        let order = t.pre_order(t.root());
        assert_eq!(order[0], t.root());
        assert_eq!(order.len(), t.node_count());
    }

    #[test]
    fn post_order_visits_root_last() {
        let t = sample_tree();
        let order = t.post_order(t.root());
        assert_eq!(*order.last().unwrap(), t.root());
        assert_eq!(order.len(), t.node_count());
    }

    #[test]
    fn post_order_visits_children_before_parent() {
        let t = sample_tree();
        let order = t.post_order(t.root());
        // For each non-leaf, ensure every child appears before it.
        for (i, &id) in order.iter().enumerate() {
            for c in &t.node(id).children {
                let cpos = order.iter().position(|&x| x == *c).unwrap();
                assert!(cpos < i, "child {} should come before parent {}", c, id);
            }
        }
    }

    #[test]
    fn bfs_groups_by_depth() {
        let t = sample_tree();
        let order = t.bfs_order(t.root());
        // Depths along BFS must be monotonically non-decreasing.
        let mut prev_depth = 0usize;
        for id in order {
            let mut depth = 0;
            let mut cur = t.node(id).parent;
            while let Some(p) = cur {
                depth += 1;
                cur = t.node(p).parent;
            }
            assert!(depth >= prev_depth);
            prev_depth = depth;
        }
    }

    #[test]
    fn descendants_excludes_self() {
        let t = sample_tree();
        let d = t.descendants(t.root());
        assert!(!d.contains(&t.root()));
        // Root has 4 proper descendants (b, 1, c, 2).
        assert_eq!(d.len(), 4);
    }

    #[test]
    fn descendants_of_leaf_is_empty() {
        let mut b = TreeBuilder::new();
        let r = b.add("x", "", None, 0, 0);
        let t = b.build(r);
        assert!(t.descendants(r).is_empty());
    }

    #[test]
    fn position_in_parent_returns_index() {
        let t = sample_tree();
        let root_children = t.node(t.root()).children.clone();
        for (i, c) in root_children.iter().enumerate() {
            assert_eq!(t.position_in_parent(*c), Some(i));
        }
    }

    #[test]
    fn position_in_parent_of_root_is_none() {
        let t = sample_tree();
        assert_eq!(t.position_in_parent(t.root()), None);
    }

    #[test]
    fn all_nodes_yields_node_count_nodes() {
        let t = sample_tree();
        assert_eq!(t.all_nodes().count(), t.node_count());
    }
}
