//! Line-based tree construction for language-agnostic diffing.
//!
//! Builds a flat two-level tree from raw source bytes: a single `"file"` root
//! with one `"line"` leaf per line. Each leaf carries the line content as its
//! label and accurate byte offsets, so the standard action generator and JSON
//! formatter work without modification.

use crate::tree::{Tree, TreeBuilder};

/// Builds a flat `file → line*` tree from raw source bytes.
///
/// Lines are split on `\n`. A trailing newline does not produce an empty
/// trailing line (matching the convention of `wc -l` and `diff`).
pub fn build_line_tree(source: &[u8]) -> Tree {
    let source_text = String::from_utf8_lossy(source);
    let mut builder = TreeBuilder::new();
    let root = builder.add("file", "", None, 0, source.len());

    let mut byte_offset = 0;
    for raw_line in source_text.split('\n') {
        let line_start = byte_offset;
        let line_end = byte_offset + raw_line.len();

        // Skip the phantom empty line after a trailing newline.
        if line_start == source.len() && raw_line.is_empty() {
            break;
        }

        let label = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        builder.add("line", label, Some(root), line_start, line_end);

        // Advance past the content plus the '\n' delimiter.
        byte_offset = line_end + 1;
    }

    builder.build(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_root_only() {
        let tree = build_line_tree(b"");
        assert_eq!(tree.node_count(), 1);
        assert_eq!(tree.node(tree.root()).kind, "file");
        assert!(tree.node(tree.root()).children.is_empty());
    }

    #[test]
    fn single_line_without_newline() {
        let tree = build_line_tree(b"hello");
        assert_eq!(tree.node_count(), 2);
        let line = tree.node(tree.node(tree.root()).children[0]);
        assert_eq!(line.kind, "line");
        assert_eq!(line.label, "hello");
        assert_eq!(line.start_byte, 0);
        assert_eq!(line.end_byte, 5);
    }

    #[test]
    fn trailing_newline_does_not_create_empty_line() {
        let tree = build_line_tree(b"aaa\nbbb\n");
        let children = &tree.node(tree.root()).children;
        assert_eq!(children.len(), 2);
        assert_eq!(tree.node(children[0]).label, "aaa");
        assert_eq!(tree.node(children[1]).label, "bbb");
    }

    #[test]
    fn byte_offsets_are_accurate() {
        let tree = build_line_tree(b"ab\ncde\nf");
        let children = &tree.node(tree.root()).children;
        assert_eq!(children.len(), 3);

        let first = tree.node(children[0]);
        assert_eq!((first.start_byte, first.end_byte), (0, 2));

        let second = tree.node(children[1]);
        assert_eq!((second.start_byte, second.end_byte), (3, 6));

        let third = tree.node(children[2]);
        assert_eq!((third.start_byte, third.end_byte), (7, 8));
    }

    #[test]
    fn carriage_returns_are_stripped_from_labels() {
        let tree = build_line_tree(b"line one\r\nline two\r\n");
        let children = &tree.node(tree.root()).children;
        assert_eq!(tree.node(children[0]).label, "line one");
        assert_eq!(tree.node(children[1]).label, "line two");
    }

    #[test]
    fn blank_lines_in_the_middle_are_preserved() {
        let tree = build_line_tree(b"a\n\nb");
        let children = &tree.node(tree.root()).children;
        assert_eq!(children.len(), 3);
        assert_eq!(tree.node(children[0]).label, "a");
        assert_eq!(tree.node(children[1]).label, "");
        assert_eq!(tree.node(children[2]).label, "b");
    }
}
