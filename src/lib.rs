//! GumTree-style AST differencing in Rust.
//!
//! Implements the SimpleGumTree matcher (Falleri & Martinez, ICSE 2024) on top
//! of [tree-sitter](https://tree-sitter.github.io) parsers, plus a Chawathe
//! edit-script generator and a JSON formatter compatible with the upstream
//! Java tool's `-f JSON` output.

pub mod actions;
pub mod format;
pub mod language;
pub mod languages;
pub mod mapping;
pub mod matcher;
pub mod tree;
pub mod ts_convert;

use crate::actions::{generate_actions, Action};
use crate::language::LanguageProfile;
use crate::mapping::Mapping;
use crate::matcher::{match_trees, MatchOptions};
use crate::tree::Tree;

/// The full result of diffing two trees: their internal representations, the
/// final node mapping, and the edit script.
pub struct DiffResult {
    pub src_tree: Tree,
    pub dst_tree: Tree,
    pub mapping: Mapping,
    pub actions: Vec<Action>,
}

/// Options forwarded to the matcher.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffOptions {
    pub match_options: MatchOptions,
}

/// Diffs two already-built internal trees.
pub fn diff_trees(src: Tree, dst: Tree, opts: &DiffOptions) -> DiffResult {
    let mapping = match_trees(&src, &dst, opts.match_options);
    let actions = generate_actions(&src, &dst, &mapping);
    DiffResult {
        src_tree: src,
        dst_tree: dst,
        mapping,
        actions,
    }
}

/// Parses two source buffers with the given language profile, then diffs them.
pub fn diff_sources(
    old_source: &[u8],
    new_source: &[u8],
    profile: &dyn LanguageProfile,
    opts: &DiffOptions,
) -> Result<DiffResult, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&profile.language())
        .map_err(|e| format!("set_language: {}", e))?;

    let old_ts = parser
        .parse(old_source, None)
        .ok_or_else(|| "failed to parse old source".to_string())?;
    let new_ts = parser
        .parse(new_source, None)
        .ok_or_else(|| "failed to parse new source".to_string())?;

    let src = ts_convert::convert(&old_ts, old_source, profile);
    let dst = ts_convert::convert(&new_ts, new_source, profile);
    Ok(diff_trees(src, dst, opts))
}
