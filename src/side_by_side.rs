//! Side-by-side colored diff output.
//!
//! Renders the diff result as a two-column view with colors:
//!
//! * **Cyan** line numbers — the line belongs to a moved block.
//! * **Red** content on the left — deleted text.
//! * **Green** content on the right — inserted text.
//! * **Yellow** content on both sides — updated (changed but matched) text.
//! * Default — unchanged text.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use colored::Colorize;

use crate::actions::Action;
use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

/// A single line of source text with its byte range.
struct SourceLine {
    text: String,
    start_byte: usize,
    end_byte: usize,
}

/// Splits raw bytes into lines, recording byte offsets for each.
fn split_into_lines(bytes: &[u8]) -> Vec<SourceLine> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let end = offset + line.len();
        lines.push(SourceLine {
            text: line.to_string(),
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

/// Returns the 0-based line index whose byte range contains `byte_offset`.
fn line_index_at_byte(lines: &[SourceLine], byte_offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| byte_offset >= line.start_byte && byte_offset <= line.end_byte)
}

/// Builds a destination-line → source-line mapping by voting over mapped leaf
/// nodes. Each destination leaf that has a source counterpart casts a vote for
/// the pair (destination_line, source_line). The source line with the most
/// votes wins for each destination line.
fn build_line_mapping(
    source_lines: &[SourceLine],
    destination_lines: &[SourceLine],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> HashMap<usize, usize> {
    // Accumulate votes: for each destination line, which source line do its
    // mapped leaf nodes point to most often?
    let mut votes: HashMap<usize, HashMap<usize, usize>> = HashMap::new();

    for destination_node in destination_tree.all_nodes() {
        if !destination_node.children.is_empty() {
            continue;
        }
        if let Some(source_node_id) = mapping.get_src(destination_node.id) {
            let source_node = source_tree.node(source_node_id);
            if let (Some(destination_line), Some(source_line)) = (
                line_index_at_byte(destination_lines, destination_node.start_byte),
                line_index_at_byte(source_lines, source_node.start_byte),
            ) {
                *votes
                    .entry(destination_line)
                    .or_default()
                    .entry(source_line)
                    .or_insert(0) += 1;
            }
        }
    }

    // Pick the best source line for each destination line.
    let mut candidates: Vec<(usize, usize, usize)> = votes
        .into_iter()
        .filter_map(|(destination_line, line_votes)| {
            line_votes
                .into_iter()
                .max_by_key(|&(_, count)| count)
                .map(|(source_line, count)| (destination_line, source_line, count))
        })
        .collect();

    // Sort by vote count descending so higher-confidence pairings win.
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.2));

    // Greedily assign, keeping the mapping bijective.
    let mut result: HashMap<usize, usize> = HashMap::new();
    let mut used_source_lines: HashSet<usize> = HashSet::new();
    for (destination_line, source_line, _) in candidates {
        if !used_source_lines.contains(&source_line) {
            result.insert(destination_line, source_line);
            used_source_lines.insert(source_line);
        }
    }

    result
}

/// Collects every source-tree node ID involved in a `MoveTree` action,
/// including all descendants of the moved root.
fn collect_moved_source_nodes(source_tree: &Tree, actions: &[Action]) -> HashSet<NodeId> {
    let mut moved = HashSet::new();
    for action in actions {
        if let Action::MoveTree { node, .. } = action {
            for descendant in source_tree.pre_order(*node) {
                moved.insert(descendant);
            }
        }
    }
    moved
}

/// Returns true if any leaf node on the given source line belongs to the set.
fn source_line_touches_node_set(
    source_tree: &Tree,
    line: &SourceLine,
    node_set: &HashSet<NodeId>,
) -> bool {
    source_tree.all_nodes().any(|node| {
        node.children.is_empty()
            && node.start_byte >= line.start_byte
            && node.start_byte <= line.end_byte
            && node_set.contains(&node.id)
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RowKind {
    Unchanged,
    Inserted,
    Deleted,
    Updated,
}

struct OutputRow {
    source_line_number: Option<usize>,
    source_text: Option<String>,
    destination_line_number: Option<usize>,
    destination_text: Option<String>,
    kind: RowKind,
    is_moved: bool,
}

/// Builds the ordered list of output rows by walking destination lines and
/// interleaving deleted source lines at their natural position.
fn build_output_rows(
    source_lines: &[SourceLine],
    destination_lines: &[SourceLine],
    line_mapping: &HashMap<usize, usize>,
    moved_source_nodes: &HashSet<NodeId>,
    source_tree: &Tree,
) -> Vec<OutputRow> {
    let reverse_mapping: HashMap<usize, usize> = line_mapping
        .iter()
        .map(|(&destination, &source)| (source, destination))
        .collect();

    let mut rows: Vec<OutputRow> = Vec::new();
    let mut emitted_source_lines: HashSet<usize> = HashSet::new();
    let mut last_source_line: Option<usize> = None;

    for (destination_index, destination_line) in destination_lines.iter().enumerate() {
        if let Some(&source_index) = line_mapping.get(&destination_index) {
            // Emit deleted source lines that fall between the previous matched
            // source line and this one.
            let gap_start = last_source_line.map_or(0, |l| l + 1);
            if source_index >= gap_start {
                for (gap, gap_line) in source_lines
                    .iter()
                    .enumerate()
                    .take(source_index)
                    .skip(gap_start)
                {
                    if !reverse_mapping.contains_key(&gap) && !emitted_source_lines.contains(&gap) {
                        rows.push(OutputRow {
                            source_line_number: Some(gap + 1),
                            source_text: Some(gap_line.text.clone()),
                            destination_line_number: None,
                            destination_text: None,
                            kind: RowKind::Deleted,
                            is_moved: false,
                        });
                        emitted_source_lines.insert(gap);
                    }
                }
            }

            let source_text = &source_lines[source_index].text;
            let destination_text = &destination_line.text;
            let is_moved = source_line_touches_node_set(
                source_tree,
                &source_lines[source_index],
                moved_source_nodes,
            );
            let kind = if source_text == destination_text {
                RowKind::Unchanged
            } else {
                RowKind::Updated
            };

            rows.push(OutputRow {
                source_line_number: Some(source_index + 1),
                source_text: Some(source_text.clone()),
                destination_line_number: Some(destination_index + 1),
                destination_text: Some(destination_text.clone()),
                kind,
                is_moved,
            });
            emitted_source_lines.insert(source_index);
            last_source_line = Some(source_index);
        } else {
            rows.push(OutputRow {
                source_line_number: None,
                source_text: None,
                destination_line_number: Some(destination_index + 1),
                destination_text: Some(destination_line.text.clone()),
                kind: RowKind::Inserted,
                is_moved: false,
            });
        }
    }

    // Trailing deleted source lines.
    for (source_index, source_line) in source_lines.iter().enumerate() {
        if !emitted_source_lines.contains(&source_index)
            && !reverse_mapping.contains_key(&source_index)
        {
            rows.push(OutputRow {
                source_line_number: Some(source_index + 1),
                source_text: Some(source_line.text.clone()),
                destination_line_number: None,
                destination_text: None,
                kind: RowKind::Deleted,
                is_moved: false,
            });
        }
    }

    rows
}

/// Groups rows into hunks, each containing at most `context` lines of
/// surrounding unchanged rows. Returns `(start, end)` index pairs into the
/// rows slice.
fn extract_hunks(rows: &[OutputRow], context: usize) -> Vec<(usize, usize)> {
    let changed_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind != RowKind::Unchanged)
        .map(|(index, _)| index)
        .collect();

    if changed_indices.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &index in &changed_indices {
        let start = index.saturating_sub(context);
        let end = (index + context + 1).min(rows.len());
        if let Some(last) = ranges.last_mut() {
            if start <= last.1 {
                last.1 = end;
                continue;
            }
        }
        ranges.push((start, end));
    }

    ranges
}

/// Renders one output row as a single line of side-by-side text with ANSI
/// color codes.
fn render_row(
    row: &OutputRow,
    line_number_width: usize,
    content_width: usize,
    output: &mut String,
) {
    let left_number_raw = match row.source_line_number {
        Some(n) => format!("{:>width$}", n, width = line_number_width),
        None => " ".repeat(line_number_width),
    };
    let right_number_raw = match row.destination_line_number {
        Some(n) => format!("{:>width$}", n, width = line_number_width),
        None => " ".repeat(line_number_width),
    };

    let left_text = row.source_text.as_deref().unwrap_or("");
    let right_text = row.destination_text.as_deref().unwrap_or("");

    // Pad left content to a fixed width so the centre separator aligns.
    let left_padded = format!("{:<width$}", left_text, width = content_width);
    let left_column: String = left_padded.chars().take(content_width).collect();
    // If char-count truncation made it shorter (shouldn't normally), re-pad.
    let left_column = format!("{:<width$}", left_column, width = content_width);

    let separator = "│".dimmed();

    let (colored_left_num, colored_right_num, colored_left, colored_right) = match row.kind {
        RowKind::Unchanged => {
            let (ln, rn) = if row.is_moved {
                (
                    left_number_raw.cyan().to_string(),
                    right_number_raw.cyan().to_string(),
                )
            } else {
                (
                    left_number_raw.dimmed().to_string(),
                    right_number_raw.dimmed().to_string(),
                )
            };
            (ln, rn, left_column, right_text.to_string())
        }
        RowKind::Deleted => (
            left_number_raw.red().to_string(),
            right_number_raw.to_string(),
            left_column.red().to_string(),
            String::new(),
        ),
        RowKind::Inserted => (
            left_number_raw.to_string(),
            right_number_raw.green().to_string(),
            left_column,
            right_text.green().to_string(),
        ),
        RowKind::Updated => {
            let (ln, rn) = if row.is_moved {
                (
                    left_number_raw.cyan().to_string(),
                    right_number_raw.cyan().to_string(),
                )
            } else {
                (
                    left_number_raw.dimmed().to_string(),
                    right_number_raw.dimmed().to_string(),
                )
            };
            (
                ln,
                rn,
                left_column.yellow().to_string(),
                right_text.yellow().to_string(),
            )
        }
    };

    writeln!(
        output,
        "{} {} {} {} {} {} {}",
        colored_left_num,
        separator,
        colored_left,
        separator,
        colored_right_num,
        separator,
        colored_right,
    )
    .unwrap();
}

/// Renders a horizontal separator between hunks.
fn render_separator(line_number_width: usize, content_width: usize, output: &mut String) {
    let number_bar = "─".repeat(line_number_width);
    let content_bar = "─".repeat(content_width);
    writeln!(
        output,
        "{}─┼─{}─┼─{}─┼─",
        number_bar, content_bar, number_bar
    )
    .unwrap();
}

/// Renders the full diff as a side-by-side colored string ready for terminal
/// output.
pub fn format_side_by_side(
    source_bytes: &[u8],
    destination_bytes: &[u8],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
    actions: &[Action],
) -> String {
    let source_lines = split_into_lines(source_bytes);
    let destination_lines = split_into_lines(destination_bytes);

    let line_mapping = build_line_mapping(
        &source_lines,
        &destination_lines,
        source_tree,
        destination_tree,
        mapping,
    );

    let moved_source_nodes = collect_moved_source_nodes(source_tree, actions);

    let rows = build_output_rows(
        &source_lines,
        &destination_lines,
        &line_mapping,
        &moved_source_nodes,
        source_tree,
    );

    let hunks = extract_hunks(&rows, 3);

    let max_line_number = source_lines.len().max(destination_lines.len());
    let line_number_width = format!("{}", max_line_number).len().max(3);
    let content_width = 50;

    let mut output = String::new();

    for (hunk_index, &(start, end)) in hunks.iter().enumerate() {
        if hunk_index > 0 {
            render_separator(line_number_width, content_width, &mut output);
        }
        for row in &rows[start..end] {
            render_row(row, line_number_width, content_width, &mut output);
        }
    }

    output
}
