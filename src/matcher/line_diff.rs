//! Two-phase line matching for language-agnostic diffing.
//!
//! **Phase 1: Exact matching.** Lines with identical content are grouped and
//! paired by positional proximity, enabling move detection even when lines
//! change order.
//!
//! **Phase 2: Similarity matching.** Remaining unmatched lines are compared
//! pairwise using normalised edit distance. Pairs above a similarity threshold
//! are linked, producing `update-node` actions instead of delete+insert.

use std::collections::HashMap;

use crate::mapping::Mapping;
use crate::string_distance::normalised_similarity;
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
        let Some(destination_indices) = destination_indices_by_content.get(source_label) else {
            continue;
        };

        // Find the closest unmatched destination with the same content.
        let best_destination = destination_indices
            .iter()
            .copied()
            .filter(|&destination_index| !matched_destinations[destination_index])
            .min_by_key(|&destination_index| {
                (source_index as isize - destination_index as isize).unsigned_abs()
            });

        let Some(destination_index) = best_destination else {
            continue;
        };
        matched_sources[source_index] = true;
        matched_destinations[destination_index] = true;
        mapping.link(
            source_children[source_index],
            destination_children[destination_index],
        );
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

        let Some(position) = best_destination_position else {
            continue;
        };
        let destination_index = unmatched_destination_indices.remove(position);
        mapping.link(
            source_children[*source_index],
            destination_children[destination_index],
        );
    }
}
