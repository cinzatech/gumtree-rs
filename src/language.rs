//! Plug-in interface for tree-sitter grammars.
//!
//! Each language provides a [`LanguageProfile`] describing how to convert its
//! concrete syntax tree into the AST representation used by the matcher.
//!
//! The default implementations keep every named node and assign a label to
//! nodes that have no named children (i.e. leaves in the AST sense).

use tree_sitter::Language;

pub trait LanguageProfile {
    /// The tree-sitter language this profile uses for parsing.
    fn language(&self) -> Language;

    /// Whether to keep a tree-sitter node with the given kind. The default
    /// keeps every named node and discards anonymous tokens.
    fn keep(&self, _kind: &str, is_named: bool) -> bool {
        is_named
    }

    /// Whether the node should carry the raw text as its label. Defaults to
    /// only labelling leaves (no named children).
    fn label(&self, _kind: &str, is_leaf: bool) -> bool {
        is_leaf
    }
}
