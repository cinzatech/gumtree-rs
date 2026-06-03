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
pub fn match_trees(t1: &Tree, t2: &Tree, opts: MatchOptions) -> Mapping {
    let mut mapping = Mapping::new();
    topdown::match_top_down(t1, t2, &mut mapping, opts.min_height);
    bottomup::match_bottom_up(t1, t2, &mut mapping, opts.min_dice, opts.max_size);

    let r1 = t1.root();
    let r2 = t2.root();
    if !mapping.has_src(r1) && !mapping.has_dst(r2) && t1.node(r1).kind == t2.node(r2).kind {
        mapping.link(r1, r2);
        bottomup::recover_simple(t1, r1, t2, r2, &mut mapping);
    }
    mapping
}
