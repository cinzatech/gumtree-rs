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
    let root_id = walk(&ts_tree.root_node(), source, &mut builder, None, profile);
    builder.build(root_id)
}

fn walk(
    ts_node: &TSNode,
    source: &[u8],
    builder: &mut TreeBuilder,
    parent: Option<NodeId>,
    profile: &dyn LanguageProfile,
) -> NodeId {
    let kind = ts_node.kind();

    // Determine whether this node will have any kept named children.
    let has_kept_child = {
        let mut cursor = ts_node.walk();
        let mut found = false;
        for child in ts_node.named_children(&mut cursor) {
            if profile.keep(child.kind(), child.is_named()) {
                found = true;
                break;
            }
        }
        found
    };

    let label = if profile.label(kind, !has_kept_child) {
        ts_node.utf8_text(source).unwrap_or("").to_string()
    } else {
        String::new()
    };

    let id = builder.add(
        kind,
        &label,
        parent,
        ts_node.start_byte(),
        ts_node.end_byte(),
    );

    let mut cursor = ts_node.walk();
    for child in ts_node.named_children(&mut cursor) {
        if profile.keep(child.kind(), child.is_named()) {
            walk(&child, source, builder, Some(id), profile);
        }
    }

    id
}
