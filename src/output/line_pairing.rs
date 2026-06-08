//! Line-pairing algorithm: builds a bijective destination-line → source-line mapping.
//!
//! This module is independent of any rendering concern and can be reused by
//! any output format that needs to align old and new lines.

use std::collections::{HashMap, HashSet};

use crate::mapping::Mapping;
use crate::tree::Tree;

/// A single logical line within a source buffer.
#[derive(Debug)]
pub struct FileLine<'a> {
    pub text: &'a str,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// The result of pairing source and destination lines.
pub struct LinePairing {
    /// Maps destination line index → source line index.
    pub dst_to_src: HashMap<usize, usize>,
    /// Destination lines that belong to moved (reordered) blocks.
    pub moved_dst_lines: HashSet<usize>,
}

/// Splits a byte buffer into [`FileLine`]s on `\n` boundaries.
///
/// Uses lossy UTF-8 conversion so that non-UTF-8 input degrades gracefully
/// (matching `build_line_tree`'s behaviour) instead of panicking.
#[must_use]
pub fn split_into_lines(bytes: &[u8]) -> Vec<FileLine<'_>> {
    // Safety: we need a &str that lives as long as `bytes`.  For pure-ASCII
    // and valid-UTF-8 inputs (the overwhelming majority) `from_utf8` succeeds
    // and we borrow zero-copy.  For the rare invalid case we fall back to
    // `from_utf8_lossy`, which may allocate — but the returned `Cow` is
    // converted to an owned `String` whose lifetime we cannot return as a
    // `&str` tied to `bytes`.  So we leak the allocation into a &'static str.
    // This only happens for genuinely broken files and the total leaked size
    // equals the file size, bounded by `max_file_size`.
    let text: &str = if let Ok(valid) = std::str::from_utf8(bytes) {
        valid
    } else {
        let owned = String::from_utf8_lossy(bytes).into_owned();
        // Leak is bounded by max_file_size and only triggers for
        // non-UTF-8 files — an uncommon edge case.
        Box::leak(owned.into_boxed_str())
    };
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let end = offset + line.len();
        lines.push(FileLine {
            text: line,
            start_byte: offset,
            end_byte: end,
        });
        offset = end + 1;
    }
    if lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    lines
}

/// Returns the line index that contains the given byte offset.
#[must_use]
pub fn line_index_at_byte(lines: &[FileLine], byte_offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| byte_offset >= line.start_byte && byte_offset <= line.end_byte)
}

/// Builds a complete [`LinePairing`] from the AST-level mapping.
#[must_use]
pub fn build_line_pairing<'a>(
    source_lines: &[FileLine<'a>],
    destination_lines: &[FileLine<'a>],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> LinePairing {
    let candidates = phase1_vote(
        source_lines,
        destination_lines,
        source_tree,
        destination_tree,
        mapping,
    );
    let (result, used_source_lines) =
        phase2_interpolate(candidates, source_lines, destination_lines);
    let dst_to_src = phase3_blanks(result, used_source_lines, source_lines, destination_lines);
    let moved_dst_lines = detect_moved_destination_lines(&dst_to_src, destination_lines.len());
    LinePairing {
        dst_to_src,
        moved_dst_lines,
    }
}

/// Phase 1: vote using identifier nodes weighted by inverse label frequency.
fn phase1_vote<'a>(
    source_lines: &[FileLine<'a>],
    destination_lines: &[FileLine<'a>],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> Vec<(usize, usize, f64)> {
    let destination_label_frequency: HashMap<&str, usize> = destination_tree
        .all_nodes()
        .filter(|n| n.children.is_empty() && n.kind == "identifier")
        .fold(HashMap::new(), |mut acc, n| {
            *acc.entry(n.label.as_str()).or_insert(0) += 1;
            acc
        });

    let mut weighted_votes: HashMap<usize, HashMap<usize, f64>> = HashMap::new();

    for destination_node in destination_tree.all_nodes() {
        if !destination_node.children.is_empty() || destination_node.kind != "identifier" {
            continue;
        }
        let frequency = destination_label_frequency
            .get(destination_node.label.as_str())
            .copied()
            .unwrap_or(1);
        if frequency != 1 {
            continue;
        }

        let Some(source_node_id) = mapping.get_src(destination_node.id) else {
            continue;
        };
        let source_node = source_tree.node(source_node_id);

        let (Some(destination_line), Some(source_line)) = (
            line_index_at_byte(destination_lines, destination_node.start_byte),
            line_index_at_byte(source_lines, source_node.start_byte),
        ) else {
            continue;
        };

        *weighted_votes
            .entry(destination_line)
            .or_default()
            .entry(source_line)
            .or_insert(0.0) += 1.0;
    }

    let mut candidates: Vec<(usize, usize, f64)> = weighted_votes
        .into_iter()
        .filter_map(|(destination_line, line_votes)| {
            line_votes
                .into_iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(source_line, weight)| (destination_line, source_line, weight))
        })
        .collect();

    candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
    candidates
}

/// Phase 2: fill gaps by proximity interpolation.
///
/// Uses a single forward sweep followed by a backward sweep to fill gaps in
/// O(n), replacing the previous `while changed` fixed-point loop that was
/// O(n²) in the worst case.
fn phase2_interpolate(
    candidates: Vec<(usize, usize, f64)>,
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
) -> (HashMap<usize, usize>, HashSet<usize>) {
    let mut result: HashMap<usize, usize> = HashMap::new();
    let mut used_source_lines: HashSet<usize> = HashSet::new();

    for (destination_line, source_line, _) in candidates {
        if used_source_lines.contains(&source_line) {
            continue;
        }
        result.insert(destination_line, source_line);
        used_source_lines.insert(source_line);
    }

    let dst_count = destination_lines.len();
    let src_count = source_lines.len();

    // Precompute next_below[d] = nearest initially-mapped destination line
    // strictly after d, so the forward sweep can look ahead in O(1).
    let mut next_below: Vec<Option<(usize, usize)>> = vec![None; dst_count];
    {
        let mut upcoming: Option<(usize, usize)> = None;
        for d in (0..dst_count).rev() {
            next_below[d] = upcoming;
            if let Some(&s) = result.get(&d) {
                upcoming = Some((d, s));
            }
        }
    }

    // Forward sweep: handles between-anchor interpolation (Case 1) and
    // forward edge extension (Case 2).  Tracks `last_mapped` incrementally
    // so each fill chains into the next without rescanning.
    let mut last_mapped: Option<(usize, usize)> = None;
    let mut first_mapped: Option<usize> = None;
    for d in 0..dst_count {
        if result.contains_key(&d) {
            if first_mapped.is_none() {
                first_mapped = Some(d);
            }
            last_mapped = Some((d, result[&d]));
            continue;
        }

        let above = last_mapped;
        let below = next_below[d];

        let inferred_source = match (above, below) {
            (Some((above_dst, above_src)), Some((_below_dst, below_src)))
                if above_src < below_src =>
            {
                let offset = d - above_dst;
                let candidate = above_src + offset;
                (candidate < below_src && candidate < src_count).then_some(candidate)
            }
            (Some((above_dst, above_src)), _) => {
                let offset = d - above_dst;
                let candidate = above_src + offset;
                (candidate < src_count && offset <= 1).then_some(candidate)
            }
            // Case 3 during forward sweep: only for the line immediately
            // before the first anchor (offset == 1).
            (None, Some((below_dst, below_src))) => {
                let offset = below_dst - d;
                (below_src >= offset && offset <= 1).then_some(below_src - offset)
            }
            _ => None,
        };

        if let Some(source_line) = inferred_source {
            if !used_source_lines.contains(&source_line) {
                result.insert(d, source_line);
                used_source_lines.insert(source_line);
                last_mapped = Some((d, source_line));
                if first_mapped.is_none() {
                    first_mapped = Some(d);
                }
            }
        }
    }

    // Backward sweep: extends from the earliest mapped line toward the start.
    // Only lines before the first mapped line lack an "above" anchor, so only
    // they can benefit from Case 3 (below-only, offset <= 1) chaining.
    if let Some(first) = first_mapped {
        let mut next_anchor: Option<(usize, usize)> = Some((first, result[&first]));
        for d in (0..first).rev() {
            if let Some((below_dst, below_src)) = next_anchor {
                let offset = below_dst - d;
                if below_src >= offset && offset <= 1 {
                    let candidate = below_src - offset;
                    if !used_source_lines.contains(&candidate) {
                        result.insert(d, candidate);
                        used_source_lines.insert(candidate);
                        next_anchor = Some((d, candidate));
                    }
                }
            }
        }
    }

    (result, used_source_lines)
}

/// Phase 3: pair remaining blank lines positionally.
fn phase3_blanks(
    mut result: HashMap<usize, usize>,
    mut used_source_lines: HashSet<usize>,
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
) -> HashMap<usize, usize> {
    let mut unmatched_source_blanks = (0..source_lines.len())
        .filter(|i| !used_source_lines.contains(i) && source_lines[*i].text.trim().is_empty())
        .collect::<Vec<_>>()
        .into_iter();

    let unmatched_destination_blanks: Vec<usize> = (0..destination_lines.len())
        .filter(|i| !result.contains_key(i) && destination_lines[*i].text.trim().is_empty())
        .collect();

    for destination_blank in unmatched_destination_blanks {
        let Some(source_blank) = unmatched_source_blanks.next() else {
            break;
        };
        result.insert(destination_blank, source_blank);
        used_source_lines.insert(source_blank);
    }

    result
}

/// Detects which destination lines belong to moved blocks by finding inversions
/// in the source-line sequence.
fn detect_moved_destination_lines(
    line_mapping: &HashMap<usize, usize>,
    destination_line_count: usize,
) -> HashSet<usize> {
    let mut blocks: Vec<(Vec<usize>, usize)> = Vec::new();

    for destination_line in 0..destination_line_count {
        let Some(&source_line) = line_mapping.get(&destination_line) else {
            continue;
        };
        let extends_previous = blocks
            .last()
            .is_some_and(|(dst_lines, first_src)| source_line == first_src + dst_lines.len());
        if extends_previous {
            blocks.last_mut().unwrap().0.push(destination_line);
        } else {
            blocks.push((vec![destination_line], source_line));
        }
    }

    if blocks.len() <= 1 {
        return HashSet::new();
    }

    let source_representatives: Vec<usize> = blocks.iter().map(|(_, src)| *src).collect();
    let mut moved_block_indices: HashSet<usize> = HashSet::new();

    for i in 0..source_representatives.len() {
        for j in (i + 1)..source_representatives.len() {
            if source_representatives[i] <= source_representatives[j] {
                continue;
            }
            moved_block_indices.insert(i);
            moved_block_indices.insert(j);
        }
    }

    let mut moved_lines: HashSet<usize> = HashSet::new();
    for block_index in moved_block_indices {
        for &destination_line in &blocks[block_index].0 {
            moved_lines.insert(destination_line);
        }
    }
    moved_lines
}
