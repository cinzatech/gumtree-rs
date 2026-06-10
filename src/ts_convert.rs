//! Conversion from a tree-sitter `Tree` into our internal AST representation.
//!
//! Filtering and labelling decisions are delegated to the [`LanguageProfile`].

use tree_sitter::{Node as TSNode, Tree as TSTree};

use crate::language::LanguageProfile;
use crate::tree::{NodeId, Tree, TreeBuilder};

/// Converts a tree-sitter tree to an internal [`Tree`], using `profile` to
/// decide which nodes survive and which carry labels.
pub fn convert(ts_tree: &TSTree, source: &[u8], profile: &dyn LanguageProfile) -> Tree {
    let mut builder = TreeBuilder::new();
    let ts_root = ts_tree.root_node();

    // Iterative DFS using an explicit work stack.
    // Each entry is (ts_node, parent_id_in_builder).
    let root_id = add_node(&ts_root, source, &mut builder, None, profile);
    let mut stack: Vec<(TSNode, NodeId)> = collect_kept_children(&ts_root, profile)
        .into_iter()
        .rev()
        .map(|child| (child, root_id))
        .collect();

    while let Some((ts_node, parent_id)) = stack.pop() {
        let id = add_node(&ts_node, source, &mut builder, Some(parent_id), profile);
        for child in collect_kept_children(&ts_node, profile).into_iter().rev() {
            stack.push((child, id));
        }
    }

    builder.build(root_id)
}

/// Creates a builder node for a single tree-sitter node.
fn add_node(
    ts_node: &TSNode,
    source: &[u8],
    builder: &mut TreeBuilder,
    parent: Option<NodeId>,
    profile: &dyn LanguageProfile,
) -> NodeId {
    let kind = ts_node.kind();
    // Anonymous leaves are only kept alongside a kept named child, so a
    // node is a leaf (and gets a label) iff it has no kept named child.
    let has_kept_child = has_kept_named_child(ts_node, profile);
    let label = if profile.label(kind, !has_kept_child) {
        ts_node.utf8_text(source).unwrap_or("").to_string()
    } else {
        String::new()
    };
    builder.add(
        kind,
        &label,
        parent,
        ts_node.start_byte(),
        ts_node.end_byte(),
    )
}

/// Returns the kept children of a tree-sitter node, in order.
///
/// Named nodes are kept per the profile.  Anonymous leaf tokens (keywords,
/// operators, punctuation) are kept only when the node also has a kept
/// named child: they then participate in matching and receive proper
/// Inserted/Deleted/Updated coloring.  Nodes whose children are all
/// anonymous (e.g. `line_comment` wrapping `//`) keep no children and so
/// remain labeled leaves carrying their full text.
fn collect_kept_children<'a>(
    ts_node: &TSNode<'a>,
    profile: &dyn LanguageProfile,
) -> Vec<TSNode<'a>> {
    if !has_kept_named_child(ts_node, profile) {
        return Vec::new();
    }
    let mut cursor = ts_node.walk();
    ts_node
        .children(&mut cursor)
        .filter(|child| profile.keep(child.kind(), child.is_named()) || child.child_count() == 0)
        .collect()
}

/// Whether the node has at least one named child kept by the profile.
fn has_kept_named_child(ts_node: &TSNode, profile: &dyn LanguageProfile) -> bool {
    let mut cursor = ts_node.walk();
    let result = ts_node
        .named_children(&mut cursor)
        .any(|child| profile.keep(child.kind(), child.is_named()));
    result
}
