//! LCS-based line matching for language-agnostic diffing.
//!
//! Uses a standard O(n·m) dynamic-programming longest common subsequence on
//! line labels to produce a bijective [`Mapping`] between two flat line trees.
//! The root nodes are always mapped to each other; each line in the LCS is
//! mapped to its counterpart.

use crate::mapping::Mapping;
use crate::tree::Tree;

/// Builds a mapping between two flat line trees using longest common subsequence.
///
/// Both trees must have the structure produced by [`crate::line_tree::build_line_tree`]:
/// a single root with zero or more leaf children. The root nodes are always
/// mapped. Lines that appear in the LCS are mapped in order; the rest are left
/// unmapped (producing inserts and deletes in the action generator).
pub fn match_lines(source: &Tree, destination: &Tree) -> Mapping {
    let source_root = source.root();
    let destination_root = destination.root();
    let source_lines = &source.node(source_root).children;
    let destination_lines = &destination.node(destination_root).children;

    let source_labels: Vec<&str> = source_lines
        .iter()
        .map(|&id| source.node(id).label.as_str())
        .collect();
    let destination_labels: Vec<&str> = destination_lines
        .iter()
        .map(|&id| destination.node(id).label.as_str())
        .collect();

    let lcs_pairs = longest_common_subsequence(&source_labels, &destination_labels);

    let mut mapping = Mapping::new();
    mapping.link(source_root, destination_root);
    for (source_index, destination_index) in lcs_pairs {
        mapping.link(
            source_lines[source_index],
            destination_lines[destination_index],
        );
    }
    mapping
}

/// Returns the LCS as a list of `(source_index, destination_index)` pairs.
///
/// Standard O(n·m) DP. For typical source files (thousands of lines) this is
/// fast enough; files large enough to matter hit the file-size limit first.
fn longest_common_subsequence(left: &[&str], right: &[&str]) -> Vec<(usize, usize)> {
    let rows = left.len();
    let columns = right.len();

    // Build the DP table. table[i][j] = LCS length of left[..i] and right[..j].
    let mut table = vec![vec![0u32; columns + 1]; rows + 1];
    for i in 1..=rows {
        for j in 1..=columns {
            table[i][j] = if left[i - 1] == right[j - 1] {
                table[i - 1][j - 1] + 1
            } else {
                table[i - 1][j].max(table[i][j - 1])
            };
        }
    }

    // Backtrack to recover the pairs.
    let mut pairs = Vec::with_capacity(table[rows][columns] as usize);
    let mut i = rows;
    let mut j = columns;
    while i > 0 && j > 0 {
        if left[i - 1] == right[j - 1] {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if table[i - 1][j] >= table[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_tree::build_line_tree;

    #[test]
    fn identical_files_map_all_lines() {
        let source = build_line_tree(b"aaa\nbbb\nccc\n");
        let destination = build_line_tree(b"aaa\nbbb\nccc\n");
        let mapping = match_lines(&source, &destination);
        // Root + 3 lines = 4 nodes all mapped.
        assert_eq!(mapping.len(), 4);
    }

    #[test]
    fn completely_different_files_map_only_roots() {
        let source = build_line_tree(b"aaa\nbbb\n");
        let destination = build_line_tree(b"xxx\nyyy\n");
        let mapping = match_lines(&source, &destination);
        assert_eq!(mapping.len(), 1);
    }

    #[test]
    fn added_lines_are_unmapped() {
        let source = build_line_tree(b"aaa\nccc\n");
        let destination = build_line_tree(b"aaa\nbbb\nccc\n");
        let mapping = match_lines(&source, &destination);
        // Root + aaa + ccc = 3 mapped. "bbb" is unmapped.
        assert_eq!(mapping.len(), 3);
    }

    #[test]
    fn removed_lines_are_unmapped() {
        let source = build_line_tree(b"aaa\nbbb\nccc\n");
        let destination = build_line_tree(b"aaa\nccc\n");
        let mapping = match_lines(&source, &destination);
        assert_eq!(mapping.len(), 3);
    }

    #[test]
    fn duplicate_lines_are_matched_in_order() {
        let source = build_line_tree(b"x\nx\nx\n");
        let destination = build_line_tree(b"x\nx\n");
        let mapping = match_lines(&source, &destination);
        // Root + 2 matched "x" lines. The third source "x" is unmapped.
        assert_eq!(mapping.len(), 3);
    }

    #[test]
    fn lcs_basic() {
        let left = vec!["a", "b", "c", "d"];
        let right = vec!["b", "d"];
        let pairs = longest_common_subsequence(&left, &right);
        assert_eq!(pairs, vec![(1, 0), (3, 1)]);
    }

    #[test]
    fn lcs_empty_inputs() {
        assert!(longest_common_subsequence(&[], &["a"]).is_empty());
        assert!(longest_common_subsequence(&["a"], &[]).is_empty());
        assert!(longest_common_subsequence(&[], &[]).is_empty());
    }
}
