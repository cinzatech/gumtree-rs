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
        self.src_to_dst.iter().map(|(&a, &b)| (a, b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_mapping_is_empty() {
        let m = Mapping::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert!(m.pairs().is_empty());
    }

    #[test]
    fn link_creates_bidirectional_lookup() {
        let mut m = Mapping::new();
        assert!(m.link(3, 7));
        assert_eq!(m.get_dst(3), Some(7));
        assert_eq!(m.get_src(7), Some(3));
        assert!(m.has_src(3));
        assert!(m.has_dst(7));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn link_rejects_when_src_already_mapped() {
        let mut m = Mapping::new();
        assert!(m.link(3, 7));
        // Same src to a different dst: should be rejected.
        assert!(!m.link(3, 9));
        assert_eq!(m.get_dst(3), Some(7));
        assert!(!m.has_dst(9));
    }

    #[test]
    fn link_rejects_when_dst_already_mapped() {
        let mut m = Mapping::new();
        assert!(m.link(3, 7));
        // A different src to the same dst: should be rejected.
        assert!(!m.link(5, 7));
        assert_eq!(m.get_src(7), Some(3));
        assert!(!m.has_src(5));
    }

    #[test]
    fn link_rejects_duplicate_pair() {
        let mut m = Mapping::new();
        assert!(m.link(3, 7));
        // Re-linking the same pair: also rejected (since src already mapped).
        assert!(!m.link(3, 7));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn lookups_on_unmapped_return_none() {
        let mut m = Mapping::new();
        m.link(1, 2);
        assert_eq!(m.get_dst(99), None);
        assert_eq!(m.get_src(99), None);
        assert!(!m.has_src(99));
        assert!(!m.has_dst(99));
    }

    #[test]
    fn pairs_yields_every_link() {
        let mut m = Mapping::new();
        m.link(1, 10);
        m.link(2, 20);
        m.link(3, 30);
        let mut pairs = m.pairs();
        pairs.sort();
        assert_eq!(pairs, vec![(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn supports_zero_node_ids() {
        // NodeId is usize; 0 is a valid id (typically the root).
        let mut m = Mapping::new();
        assert!(m.link(0, 0));
        assert_eq!(m.get_dst(0), Some(0));
        assert_eq!(m.get_src(0), Some(0));
    }

    #[test]
    fn clone_is_independent() {
        let mut m = Mapping::new();
        m.link(1, 10);
        let cloned = m.clone();
        m.link(2, 20);
        assert_eq!(cloned.len(), 1);
        assert_eq!(m.len(), 2);
    }
}
