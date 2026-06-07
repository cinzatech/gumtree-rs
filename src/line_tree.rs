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
