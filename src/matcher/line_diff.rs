//! Two-phase line matching for language-agnostic diffing.
//!
//! **Phase 1 — Exact matching.** Lines with identical content are grouped and
//! paired by positional proximity, enabling move detection even when lines
//! change order.
//!
//! **Phase 2 — Similarity matching.** Remaining unmatched lines are compared
//! pairwise using normalised edit distance. Pairs above a similarity threshold
//! are linked, producing `update-node` actions instead of delete+insert.

use std::collections::HashMap;

use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

/// Minimum normalised similarity (0.0–1.0) for two lines to be considered a
/// fuzzy match. Lines below this threshold stay unmatched.
const MIN_SIMILARITY: f64 = 0.5;

/// Builds a mapping between two flat line trees.
///
/// Both trees must have the structure produced by [`crate::line_tree::build_line_tree`]:
/// a single root with zero or more leaf children.
pub fn match_lines(source: &Tree, destination: &Tree) -> Mapping {
    let source_root = source.root();
    let destination_root = destination.root();
    let source_children = &source.node(source_root).children;
    let destination_children = &destination.node(destination_root).children;

    let source_labels: Vec<&str> = source_children
        .iter()
        .map(|&id| source.node(id).label.as_str())
        .collect();
    let destination_labels: Vec<&str> = destination_children
        .iter()
        .map(|&id| destination.node(id).label.as_str())
        .collect();

    let mut mapping = Mapping::new();
    mapping.link(source_root, destination_root);

    let mut matched_sources = vec![false; source_children.len()];
    let mut matched_destinations = vec![false; destination_children.len()];

    match_exact_content(
        source_children,
        destination_children,
        &source_labels,
        &destination_labels,
        &mut matched_sources,
        &mut matched_destinations,
        &mut mapping,
    );

    match_similar_content(
        source_children,
        destination_children,
        &source_labels,
        &destination_labels,
        &matched_sources,
        &matched_destinations,
        &mut mapping,
    );

    mapping
}

/// Phase 1: pairs lines with identical content by positional proximity.
///
/// For each distinct content string that appears in both files, the source and
/// destination occurrences are paired greedily: each source occurrence is
/// matched to the closest unmatched destination occurrence. This preserves
/// order for lines that didn't move and correctly detects moves for those that
/// did.
fn match_exact_content(
    source_children: &[NodeId],
    destination_children: &[NodeId],
    source_labels: &[&str],
    destination_labels: &[&str],
    matched_sources: &mut [bool],
    matched_destinations: &mut [bool],
    mapping: &mut Mapping,
) {
    // Group destination indices by content for quick lookup.
    let mut destination_indices_by_content: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, &label) in destination_labels.iter().enumerate() {
        destination_indices_by_content
            .entry(label)
            .or_default()
            .push(index);
    }

    // For each source line, try to find an exact match in the destination.
    // Process source lines in order; within each content group, pick the
    // closest available destination index.
    for (source_index, &source_label) in source_labels.iter().enumerate() {
        let destination_indices = match destination_indices_by_content.get(source_label) {
            Some(indices) => indices,
            None => continue,
        };

        // Find the closest unmatched destination with the same content.
        let best_destination = destination_indices
            .iter()
            .copied()
            .filter(|&destination_index| !matched_destinations[destination_index])
            .min_by_key(|&destination_index| {
                (source_index as isize - destination_index as isize).unsigned_abs()
            });

        if let Some(destination_index) = best_destination {
            matched_sources[source_index] = true;
            matched_destinations[destination_index] = true;
            mapping.link(
                source_children[source_index],
                destination_children[destination_index],
            );
        }
    }
}

/// Phase 2: pairs remaining unmatched lines by content similarity.
///
/// For every unmatched source line, finds the most similar unmatched
/// destination line. If the normalised similarity meets [`MIN_SIMILARITY`],
/// the pair is linked. The action generator then emits `update-node` for the
/// label change rather than a separate delete and insert.
fn match_similar_content(
    source_children: &[NodeId],
    destination_children: &[NodeId],
    source_labels: &[&str],
    destination_labels: &[&str],
    matched_sources: &[bool],
    matched_destinations: &[bool],
    mapping: &mut Mapping,
) {
    let unmatched_source_indices: Vec<usize> = matched_sources
        .iter()
        .enumerate()
        .filter(|(_, &is_matched)| !is_matched)
        .map(|(index, _)| index)
        .collect();

    let mut unmatched_destination_indices: Vec<usize> = matched_destinations
        .iter()
        .enumerate()
        .filter(|(_, &is_matched)| !is_matched)
        .map(|(index, _)| index)
        .collect();

    for source_index in &unmatched_source_indices {
        let source_label = source_labels[*source_index];

        let mut best_destination_position: Option<usize> = None;
        let mut best_similarity: f64 = MIN_SIMILARITY;

        for (position, &destination_index) in unmatched_destination_indices.iter().enumerate() {
            let destination_label = destination_labels[destination_index];
            let similarity = normalised_similarity(source_label, destination_label);
            if similarity > best_similarity {
                best_similarity = similarity;
                best_destination_position = Some(position);
            }
        }

        if let Some(position) = best_destination_position {
            let destination_index = unmatched_destination_indices.remove(position);
            mapping.link(
                source_children[*source_index],
                destination_children[destination_index],
            );
        }
    }
}

/// Returns a similarity score between 0.0 and 1.0 for two strings.
///
/// Uses `1.0 - (edit_distance / max_length)`. Two empty strings are defined
/// as identical (1.0).
fn normalised_similarity(left: &str, right: &str) -> f64 {
    let max_length = left.len().max(right.len());
    if max_length == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(left, right);
    1.0 - (distance as f64 / max_length as f64)
}

/// Standard O(n·m) Levenshtein distance on bytes.
fn levenshtein_distance(left: &str, right: &str) -> usize {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let right_length = right_bytes.len();

    // Single-row DP; only the previous row is needed.
    let mut previous_row: Vec<usize> = (0..=right_length).collect();
    let mut current_row = vec![0; right_length + 1];

    for (left_index, &left_byte) in left_bytes.iter().enumerate() {
        current_row[0] = left_index + 1;
        for (right_index, &right_byte) in right_bytes.iter().enumerate() {
            let substitution_cost = if left_byte == right_byte { 0 } else { 1 };
            current_row[right_index + 1] = (previous_row[right_index] + substitution_cost)
                .min(previous_row[right_index + 1] + 1)
                .min(current_row[right_index] + 1);
        }
        std::mem::swap(&mut previous_row, &mut current_row);
    }

    previous_row[right_length]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::line_tree::build_line_tree;

    // --- Unit tests for helpers ---

    #[test]
    fn levenshtein_identical_strings() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert_eq!(levenshtein_distance("abc", "xyz"), 3);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn levenshtein_single_edit() {
        assert_eq!(levenshtein_distance("kitten", "sitten"), 1);
        assert_eq!(levenshtein_distance("abc", "abcd"), 1);
    }

    #[test]
    fn similarity_identical() {
        assert!((normalised_similarity("hello", "hello") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_empty_strings() {
        assert!((normalised_similarity("", "") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_completely_different() {
        assert!((normalised_similarity("abc", "xyz")).abs() < f64::EPSILON);
    }

    #[test]
    fn similarity_partial_match() {
        // "Barbaz" vs "Bar baz": distance 1, length 7 → similarity ≈ 0.857
        let score = normalised_similarity("Barbaz", "Bar baz");
        assert!(score > 0.8);
        assert!(score < 0.9);
    }

    // --- Matching behaviour tests ---

    #[test]
    fn identical_files_map_all_lines() {
        let source = build_line_tree(b"aaa\nbbb\nccc\n");
        let destination = build_line_tree(b"aaa\nbbb\nccc\n");
        let mapping = match_lines(&source, &destination);
        assert_eq!(mapping.len(), 4);
    }

    #[test]
    fn completely_different_files_map_only_roots() {
        let source = build_line_tree(b"aaa\nbbb\n");
        let destination = build_line_tree(b"xxx\nyyy\n");
        let mapping = match_lines(&source, &destination);
        // Only roots; "aaa"↔"xxx" similarity is 0.0, below threshold.
        assert_eq!(mapping.len(), 1);
    }

    #[test]
    fn added_lines_are_unmapped() {
        let source = build_line_tree(b"aaa\nccc\n");
        let destination = build_line_tree(b"aaa\nbbb\nccc\n");
        let mapping = match_lines(&source, &destination);
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
    fn swapped_lines_are_both_matched() {
        let source = build_line_tree(b"Foo\nBar\nBaz\n");
        let destination = build_line_tree(b"Foo\nBaz\nBar\n");
        let mapping = match_lines(&source, &destination);
        // Root + all 3 lines matched (Foo stays, Bar and Baz swap).
        assert_eq!(mapping.len(), 4);
    }

    #[test]
    fn similar_lines_are_matched() {
        let source = build_line_tree(b"Barbaz\n");
        let destination = build_line_tree(b"Bar baz\n");
        let mapping = match_lines(&source, &destination);
        // Root + the fuzzy-matched line.
        assert_eq!(mapping.len(), 2);
    }

    #[test]
    fn dissimilar_lines_are_not_matched() {
        let source = build_line_tree(b"aaa\n");
        let destination = build_line_tree(b"zzz\n");
        let mapping = match_lines(&source, &destination);
        // Only root; similarity is 0.
        assert_eq!(mapping.len(), 1);
    }

    #[test]
    fn duplicate_lines_are_matched_by_proximity() {
        let source = build_line_tree(b"x\nx\nx\n");
        let destination = build_line_tree(b"x\nx\n");
        let mapping = match_lines(&source, &destination);
        // Root + 2 matched "x" lines.
        assert_eq!(mapping.len(), 3);
    }
}
