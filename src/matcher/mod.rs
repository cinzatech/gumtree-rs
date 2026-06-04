//! Two-phase tree matching: top-down then bottom-up.
//!
//! See [`topdown::match_top_down`] and [`bottomup::match_bottom_up`] for the
//! individual phases. [`match_trees`] runs both in sequence with default
//! hyperparameters.

pub mod bottomup;
pub mod topdown;

use crate::mapping::Mapping;
use crate::tree::Tree;

/// Hyperparameters for the matcher.
#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    /// Minimum subtree height for top-down anchor matching. Smaller subtrees
    /// are left to the bottom-up phase.
    pub min_height: usize,
    /// Minimum dice similarity to accept a bottom-up container match.
    pub min_dice: f64,
    /// Maximum subtree size for which the simple-recovery step runs.
    pub max_size: usize,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            min_height: topdown::DEFAULT_MIN_HEIGHT,
            min_dice: bottomup::DEFAULT_MIN_DICE,
            max_size: bottomup::DEFAULT_MAX_SIZE,
        }
    }
}

/// Runs both matching phases and returns the resulting mapping.
///
/// If neither phase anchors the roots (a degenerate case that happens with
/// very shallow trees, or when the root subtrees share no large isomorphic
/// regions), and the two roots agree in kind, this function anchors them and
/// performs one pass of simple recovery so that downstream consumers still
/// get a meaningful action set.
pub fn match_trees(source_tree: &Tree, destination_tree: &Tree, options: MatchOptions) -> Mapping {
    let mut mapping = Mapping::new();
    topdown::match_top_down(
        source_tree,
        destination_tree,
        &mut mapping,
        options.min_height,
    );
    bottomup::match_bottom_up(
        source_tree,
        destination_tree,
        &mut mapping,
        options.min_dice,
        options.max_size,
    );

    let source_root = source_tree.root();
    let destination_root = destination_tree.root();
    if !mapping.has_src(source_root)
        && !mapping.has_dst(destination_root)
        && source_tree.node(source_root).kind == destination_tree.node(destination_root).kind
    {
        mapping.link(source_root, destination_root);
        bottomup::recover_simple(
            source_tree,
            source_root,
            destination_tree,
            destination_root,
            &mut mapping,
        );
    }
    mapping
}
