//! Bidirectional mapping between nodes of two trees.
//!
//! A [`Mapping`] is a bijection: each source node maps to at most one destination
//! node and vice versa. [`Mapping::link`] returns `false` when the requested link
//! would violate that invariant.

use std::collections::HashMap;

use crate::tree::NodeId;

#[derive(Debug, Default, Clone)]
pub struct Mapping {
    src_to_dst: HashMap<NodeId, NodeId>,
    dst_to_src: HashMap<NodeId, NodeId>,
}

impl Mapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of mapped pairs.
    pub fn len(&self) -> usize {
        self.src_to_dst.len()
    }

    pub fn is_empty(&self) -> bool {
        self.src_to_dst.is_empty()
    }

    /// Attempts to add a link. Returns `true` on success, `false` if either side
    /// is already mapped.
    pub fn link(&mut self, src: NodeId, dst: NodeId) -> bool {
        if self.src_to_dst.contains_key(&src) || self.dst_to_src.contains_key(&dst) {
            return false;
        }
        self.src_to_dst.insert(src, dst);
        self.dst_to_src.insert(dst, src);
        true
    }

    /// Looks up the destination corresponding to a source node.
    pub fn get_dst(&self, src: NodeId) -> Option<NodeId> {
        self.src_to_dst.get(&src).copied()
    }

    /// Looks up the source corresponding to a destination node.
    pub fn get_src(&self, dst: NodeId) -> Option<NodeId> {
        self.dst_to_src.get(&dst).copied()
    }

    pub fn has_src(&self, src: NodeId) -> bool {
        self.src_to_dst.contains_key(&src)
    }

    pub fn has_dst(&self, dst: NodeId) -> bool {
        self.dst_to_src.contains_key(&dst)
    }

    /// All `(src, dst)` pairs. Order is implementation-defined.
    pub fn pairs(&self) -> Vec<(NodeId, NodeId)> {
        self.src_to_dst
            .iter()
            .map(|(&source, &destination)| (source, destination))
            .collect()
    }
}
