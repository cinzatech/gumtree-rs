//! Line-pairing algorithm: builds a bijective destination-line → source-line mapping.
//!
//! This module is independent of any rendering concern and can be reused by
//! any output format that needs to align old and new lines.

use std::collections::{HashMap, HashSet};

use crate::mapping::Mapping;
use crate::string_distance::normalised_similarity;
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
        phase2_text_match(candidates, source_lines, destination_lines);
    let dst_to_src = phase3_blanks(result, used_source_lines, source_lines, destination_lines);
    let moved_dst_lines = detect_moved_destination_lines(&dst_to_src, destination_lines.len());
    LinePairing {
        dst_to_src,
        moved_dst_lines,
    }
}

/// Phase 1: vote using unique leaf nodes weighted by inverse label frequency.
fn phase1_vote<'a>(
    source_lines: &[FileLine<'a>],
    destination_lines: &[FileLine<'a>],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> Vec<(usize, usize, f64)> {
    let destination_label_frequency: HashMap<&str, usize> = destination_tree
        .all_nodes()
        .filter(|n| n.children.is_empty())
        .fold(HashMap::new(), |mut acc, n| {
            *acc.entry(n.label.as_str()).or_insert(0) += 1;
            acc
        });

    let mut weighted_votes: HashMap<usize, HashMap<usize, f64>> = HashMap::new();

    for destination_node in destination_tree.all_nodes() {
        if !destination_node.children.is_empty() {
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

/// Phase 2: fill gaps between anchors by matching lines with identical text,
/// then pair remaining lines positionally.
fn phase2_text_match(
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

    // Collect anchors sorted by destination index to define gap boundaries.
    let mut anchors: Vec<(usize, usize)> = result.iter().map(|(&d, &s)| (d, s)).collect();
    anchors.sort_by_key(|&(d, _)| d);

    let mut gaps: Vec<(usize, usize, usize, usize)> = Vec::new();
    if anchors.is_empty() {
        gaps.push((0, dst_count, 0, src_count));
    } else {
        let (fd, fs) = anchors[0];
        if fd > 0 || fs > 0 {
            gaps.push((0, fd, 0, fs));
        }
        for w in anchors.windows(2) {
            let (d1, s1) = w[0];
            let (d2, s2) = w[1];
            if s1 < s2 {
                gaps.push((d1 + 1, d2, s1 + 1, s2));
            }
        }
        let (ld, ls) = *anchors.last().unwrap();
        if ld + 1 < dst_count || ls + 1 < src_count {
            gaps.push((ld + 1, dst_count, ls + 1, src_count));
        }
    }

    // First pass, per gap: match by identical text.
    for &(dst_from, dst_to, src_from, src_to) in &gaps {
        let mut available: Vec<usize> = (src_from..src_to)
            .filter(|s| !used_source_lines.contains(s))
            .collect();
        let unmatched_dst: Vec<usize> = (dst_from..dst_to)
            .filter(|d| !result.contains_key(d))
            .collect();
        for d in unmatched_dst {
            let text = destination_lines[d].text;
            if let Some(pos) = available.iter().position(|&s| source_lines[s].text == text) {
                let s = available.remove(pos);
                result.insert(d, s);
                used_source_lines.insert(s);
            }
        }
    }

    // Second pass, global: anchors from reordered code are non-monotonic,
    // so some lines fall outside every gap.  Pair any remaining line whose
    // exact text occurs exactly once among the unmatched lines of each side
    // (e.g. `return a + b` of a moved function).  Runs before similarity
    // matching so identical lines claim their partners first.
    let leftover_dst: Vec<usize> = (0..dst_count)
        .filter(|d| !result.contains_key(d) && !destination_lines[*d].text.trim().is_empty())
        .collect();
    let leftover_src: Vec<usize> = (0..src_count)
        .filter(|s| !used_source_lines.contains(s) && !source_lines[*s].text.trim().is_empty())
        .collect();
    let mut src_by_text: HashMap<&str, Vec<usize>> = HashMap::new();
    for &s in &leftover_src {
        src_by_text.entry(source_lines[s].text).or_default().push(s);
    }
    let mut dst_text_count: HashMap<&str, usize> = HashMap::new();
    for &d in &leftover_dst {
        *dst_text_count.entry(destination_lines[d].text).or_insert(0) += 1;
    }
    for &d in &leftover_dst {
        let text = destination_lines[d].text;
        if dst_text_count[text] != 1 {
            continue;
        }
        if let Some(sources) = src_by_text.get(text) {
            if let [s] = sources[..] {
                result.insert(d, s);
                used_source_lines.insert(s);
            }
        }
    }

    // Third pass, per gap: pair remaining lines by text similarity, best
    // match first, so `.collect()` pairs with `.collect();` rather than
    // with whatever line happens to sit at the same offset.  Dissimilar
    // lines stay unpaired and render as whole-line delete/insert.
    for &(dst_from, dst_to, src_from, src_to) in &gaps {
        let available: Vec<usize> = (src_from..src_to)
            .filter(|s| !used_source_lines.contains(s))
            .collect();
        let remaining_dst: Vec<usize> = (dst_from..dst_to)
            .filter(|d| !result.contains_key(d))
            .collect();
        let mut scored: Vec<(f64, usize, usize)> = Vec::new();
        for &d in &remaining_dst {
            for &s in &available {
                let similarity = normalised_similarity(
                    destination_lines[d].text.trim(),
                    source_lines[s].text.trim(),
                );
                if similarity >= SIMILARITY_THRESHOLD {
                    scored.push((similarity, d, s));
                }
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (_, d, s) in scored {
            if result.contains_key(&d) || used_source_lines.contains(&s) {
                continue;
            }
            result.insert(d, s);
            used_source_lines.insert(s);
        }
    }

    (result, used_source_lines)
}

/// Minimum trimmed-text similarity for two non-identical lines to pair.
/// Low enough that `print(about)` still pairs with its expanded successor
/// (≈0.35), high enough that unrelated lines like `result` and
/// `.collect()` (≈0.2) stay apart.
const SIMILARITY_THRESHOLD: f64 = 0.3;

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
