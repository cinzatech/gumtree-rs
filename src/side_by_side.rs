//! Side-by-side colored diff output.
//!
//! Colors are applied per AST span, not per line:
//!
//! * **Cyan** line numbers — the line belongs to a moved block.
//! * **Red** spans on the left — deleted tokens.
//! * **Green** spans on the right — inserted tokens.
//! * **Yellow** spans on both sides — updated (label-changed) tokens.
//! * Default — unchanged tokens and inter-token whitespace.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use colored::Colorize;

use crate::actions::Action;
use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

struct FileLine {
    text: String,
    start_byte: usize,
    end_byte: usize,
}

fn split_into_lines(bytes: &[u8]) -> Vec<FileLine> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split('\n') {
        let end = offset + line.len();
        lines.push(FileLine {
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

fn line_index_at_byte(lines: &[FileLine], byte_offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| byte_offset >= line.start_byte && byte_offset <= line.end_byte)
}

/// Builds a bijective destination-line → source-line mapping.
///
/// Phase 1: vote using identifier nodes weighted by inverse label frequency
/// in the destination file. Unique names (function names) dominate; common
/// names (`print`, `a`) are downweighted.
///
/// Phase 2: fill gaps by proximity — if the lines above and below an unmatched
/// destination line map to a contiguous source range, interpolate.
///
/// Phase 3: pair remaining unmatched blank lines positionally.
fn build_line_mapping(
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> HashMap<usize, usize> {
    // Count how often each identifier label appears in the destination tree.
    let mut destination_label_frequency: HashMap<String, usize> = HashMap::new();
    for node in destination_tree.all_nodes() {
        if node.children.is_empty() && node.kind == "identifier" {
            *destination_label_frequency
                .entry(node.label.clone())
                .or_insert(0) += 1;
        }
    }

    // Phase 1: vote using only identifiers whose label appears exactly once
    // in the destination file. These are reliable anchors (function/class names).
    let mut weighted_votes: HashMap<usize, HashMap<usize, f64>> = HashMap::new();

    for destination_node in destination_tree.all_nodes() {
        if !destination_node.children.is_empty() {
            continue;
        }
        if destination_node.kind != "identifier" {
            continue;
        }
        let frequency = destination_label_frequency
            .get(&destination_node.label)
            .copied()
            .unwrap_or(1);
        if frequency != 1 {
            continue;
        }
        let weight = 1.0;

        if let Some(source_node_id) = mapping.get_src(destination_node.id) {
            let source_node = source_tree.node(source_node_id);
            if let (Some(destination_line), Some(source_line)) = (
                line_index_at_byte(destination_lines, destination_node.start_byte),
                line_index_at_byte(source_lines, source_node.start_byte),
            ) {
                *weighted_votes
                    .entry(destination_line)
                    .or_default()
                    .entry(source_line)
                    .or_insert(0.0) += weight;
            }
        }
    }

    // Pick best source line per destination line.
    let mut candidates: Vec<(usize, usize, f64)> = weighted_votes
        .into_iter()
        .filter_map(|(destination_line, line_votes)| {
            line_votes
                .into_iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(source_line, weight)| (destination_line, source_line, weight))
        })
        .collect();
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

    // Greedy bijective assignment.
    let mut result: HashMap<usize, usize> = HashMap::new();
    let mut used_source_lines: HashSet<usize> = HashSet::new();
    for (destination_line, source_line, _) in &candidates {
        if !used_source_lines.contains(source_line) {
            result.insert(*destination_line, *source_line);
            used_source_lines.insert(*source_line);
        }
    }

    // Phase 2: fill gaps by proximity interpolation.
    // Extend from matched anchor lines into unmatched gaps. Only extend when
    // source lines are contiguous (same section).
    let mut changed = true;
    while changed {
        changed = false;
        for destination_line in 0..destination_lines.len() {
            if result.contains_key(&destination_line) {
                continue;
            }
            let above = (0..destination_line)
                .rev()
                .find_map(|d| result.get(&d).map(|&s| (d, s)));
            let below = ((destination_line + 1)..destination_lines.len())
                .find_map(|d| result.get(&d).map(|&s| (d, s)));

            let inferred_source = match (above, below) {
                (Some((above_dst, above_src)), Some((_below_dst, below_src)))
                    if above_src < below_src =>
                {
                    // Contiguous source range — safe to interpolate.
                    let offset = destination_line - above_dst;
                    let candidate = above_src + offset;
                    if candidate < below_src && candidate < source_lines.len() {
                        Some(candidate)
                    } else {
                        None
                    }
                }
                (Some((above_dst, above_src)), _) => {
                    // Different sections or no below — extend from above only,
                    // limited to one line at a time.
                    let offset = destination_line - above_dst;
                    let candidate = above_src + offset;
                    if candidate < source_lines.len() && offset <= 1 {
                        Some(candidate)
                    } else {
                        None
                    }
                }
                (None, Some((below_dst, below_src))) => {
                    let offset = below_dst - destination_line;
                    if below_src >= offset && offset <= 1 {
                        Some(below_src - offset)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(source_line) = inferred_source {
                if !used_source_lines.contains(&source_line) {
                    result.insert(destination_line, source_line);
                    used_source_lines.insert(source_line);
                    changed = true;
                }
            }
        }
    }

    // Phase 3: pair remaining blank lines positionally.
    let mut unmatched_source_blanks: Vec<usize> = (0..source_lines.len())
        .filter(|i| !used_source_lines.contains(i) && source_lines[*i].text.trim().is_empty())
        .collect();
    let unmatched_destination_blanks: Vec<usize> = (0..destination_lines.len())
        .filter(|i| !result.contains_key(i) && destination_lines[*i].text.trim().is_empty())
        .collect();
    for destination_blank in unmatched_destination_blanks {
        if let Some(source_blank) = unmatched_source_blanks.first().copied() {
            result.insert(destination_blank, source_blank);
            used_source_lines.insert(source_blank);
            unmatched_source_blanks.remove(0);
        }
    }

    result
}

/// Detects which destination lines belong to moved blocks by finding inversions
/// in the source-line sequence.
fn detect_moved_destination_lines(
    line_mapping: &HashMap<usize, usize>,
    destination_line_count: usize,
) -> HashSet<usize> {
    // Build contiguous blocks of consecutive mapped lines.
    let mut blocks: Vec<(Vec<usize>, usize)> = Vec::new();

    for destination_line in 0..destination_line_count {
        if let Some(&source_line) = line_mapping.get(&destination_line) {
            let extends_previous = blocks.last().is_some_and(|(dst_lines, first_src)| {
                let expected_src = first_src + dst_lines.len();
                source_line == expected_src
            });
            if extends_previous {
                blocks.last_mut().unwrap().0.push(destination_line);
            } else {
                blocks.push((vec![destination_line], source_line));
            }
        }
    }

    if blocks.len() <= 1 {
        return HashSet::new();
    }

    // Mark blocks participating in inversions.
    let source_representatives: Vec<usize> = blocks.iter().map(|(_, src)| *src).collect();
    let mut moved_block_indices: HashSet<usize> = HashSet::new();

    for i in 0..source_representatives.len() {
        for j in (i + 1)..source_representatives.len() {
            if source_representatives[i] > source_representatives[j] {
                moved_block_indices.insert(i);
                moved_block_indices.insert(j);
            }
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum SpanColor {
    Unchanged,
    Updated,
    Deleted,
    Inserted,
}

fn classify_source_leaves(
    source_tree: &Tree,
    mapping: &Mapping,
    actions: &[Action],
) -> HashMap<NodeId, SpanColor> {
    let updated_nodes: HashSet<NodeId> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Update { node, .. } => Some(*node),
            _ => None,
        })
        .collect();

    let mut colors = HashMap::new();
    for node in source_tree.all_nodes() {
        if !node.children.is_empty() {
            continue;
        }
        let color = if !mapping.has_src(node.id) {
            SpanColor::Deleted
        } else if updated_nodes.contains(&node.id) {
            SpanColor::Updated
        } else {
            SpanColor::Unchanged
        };
        colors.insert(node.id, color);
    }
    colors
}

fn classify_destination_leaves(
    destination_tree: &Tree,
    mapping: &Mapping,
    actions: &[Action],
) -> HashMap<NodeId, SpanColor> {
    let updated_source_nodes: HashSet<NodeId> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Update { node, .. } => Some(*node),
            _ => None,
        })
        .collect();

    let mut colors = HashMap::new();
    for node in destination_tree.all_nodes() {
        if !node.children.is_empty() {
            continue;
        }
        let color = if !mapping.has_dst(node.id) {
            SpanColor::Inserted
        } else if mapping
            .get_src(node.id)
            .is_some_and(|src_id| updated_source_nodes.contains(&src_id))
        {
            SpanColor::Updated
        } else {
            SpanColor::Unchanged
        };
        colors.insert(node.id, color);
    }
    colors
}

#[derive(Debug, Clone)]
struct ColoredSpan {
    text: String,
    color: SpanColor,
}

fn build_line_spans(
    line: &FileLine,
    tree: &Tree,
    leaf_colors: &HashMap<NodeId, SpanColor>,
) -> Vec<ColoredSpan> {
    let mut node_spans: Vec<(usize, usize, SpanColor)> = Vec::new();
    for node in tree.all_nodes() {
        if !node.children.is_empty() {
            continue;
        }
        if node.end_byte <= line.start_byte || node.start_byte >= line.end_byte {
            continue;
        }
        if let Some(&color) = leaf_colors.get(&node.id) {
            let start = node.start_byte.saturating_sub(line.start_byte);
            let end = (node.end_byte - line.start_byte).min(line.text.len());
            let start = start.min(end);
            node_spans.push((start, end, color));
        }
    }
    node_spans.sort_by_key(|s| s.0);

    let mut spans: Vec<ColoredSpan> = Vec::new();
    let mut position = 0;
    for (start, end, color) in &node_spans {
        if *start > position {
            spans.push(ColoredSpan {
                text: line.text[position..*start].to_string(),
                color: SpanColor::Unchanged,
            });
        }
        if *end > *start {
            spans.push(ColoredSpan {
                text: line.text[*start..*end].to_string(),
                color: *color,
            });
        }
        position = *end;
    }
    if position < line.text.len() {
        spans.push(ColoredSpan {
            text: line.text[position..].to_string(),
            color: SpanColor::Unchanged,
        });
    }
    spans
}

struct OutputRow {
    source_line_number: Option<usize>,
    source_spans: Vec<ColoredSpan>,
    destination_line_number: Option<usize>,
    destination_spans: Vec<ColoredSpan>,
    is_changed: bool,
    is_moved: bool,
}

impl OutputRow {
    fn source_plain_len(&self) -> usize {
        self.source_spans.iter().map(|s| s.text.len()).sum()
    }
}

struct DiffContext<'a> {
    source_tree: &'a Tree,
    destination_tree: &'a Tree,
    source_leaf_colors: &'a HashMap<NodeId, SpanColor>,
    destination_leaf_colors: &'a HashMap<NodeId, SpanColor>,
}

fn build_output_rows(
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
    line_mapping: &HashMap<usize, usize>,
    moved_destination_lines: &HashSet<usize>,
    context: &DiffContext,
) -> Vec<OutputRow> {
    let reverse_mapping: HashMap<usize, usize> =
        line_mapping.iter().map(|(&d, &s)| (s, d)).collect();

    let mut rows: Vec<OutputRow> = Vec::new();
    let mut emitted_source_lines: HashSet<usize> = HashSet::new();
    let mut last_source_line: Option<usize> = None;

    for (destination_index, destination_line) in destination_lines.iter().enumerate() {
        if let Some(&source_index) = line_mapping.get(&destination_index) {
            let gap_start = last_source_line.map_or(0, |l| l + 1);
            if source_index >= gap_start {
                for (gap, gap_line) in source_lines
                    .iter()
                    .enumerate()
                    .take(source_index)
                    .skip(gap_start)
                {
                    if !reverse_mapping.contains_key(&gap) && !emitted_source_lines.contains(&gap) {
                        let spans = build_line_spans(
                            gap_line,
                            context.source_tree,
                            context.source_leaf_colors,
                        );
                        rows.push(OutputRow {
                            source_line_number: Some(gap + 1),
                            source_spans: spans,
                            destination_line_number: None,
                            destination_spans: Vec::new(),
                            is_changed: true,
                            is_moved: false,
                        });
                        emitted_source_lines.insert(gap);
                    }
                }
            }

            let is_changed = source_lines[source_index].text != destination_line.text;
            let is_moved = moved_destination_lines.contains(&destination_index);
            let source_spans = build_line_spans(
                &source_lines[source_index],
                context.source_tree,
                context.source_leaf_colors,
            );
            let destination_spans = build_line_spans(
                destination_line,
                context.destination_tree,
                context.destination_leaf_colors,
            );

            rows.push(OutputRow {
                source_line_number: Some(source_index + 1),
                source_spans,
                destination_line_number: Some(destination_index + 1),
                destination_spans,
                is_changed,
                is_moved,
            });
            emitted_source_lines.insert(source_index);
            last_source_line = Some(source_index);
        } else {
            let destination_spans = build_line_spans(
                destination_line,
                context.destination_tree,
                context.destination_leaf_colors,
            );
            rows.push(OutputRow {
                source_line_number: None,
                source_spans: Vec::new(),
                destination_line_number: Some(destination_index + 1),
                destination_spans,
                is_changed: true,
                is_moved: false,
            });
        }
    }

    for (source_index, source_line) in source_lines.iter().enumerate() {
        if !emitted_source_lines.contains(&source_index)
            && !reverse_mapping.contains_key(&source_index)
        {
            let spans =
                build_line_spans(source_line, context.source_tree, context.source_leaf_colors);
            rows.push(OutputRow {
                source_line_number: Some(source_index + 1),
                source_spans: spans,
                destination_line_number: None,
                destination_spans: Vec::new(),
                is_changed: true,
                is_moved: false,
            });
        }
    }

    rows
}

fn extract_hunks(rows: &[OutputRow], context: usize) -> Vec<(usize, usize)> {
    let changed: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.is_changed)
        .map(|(i, _)| i)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &index in &changed {
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

fn render_spans(spans: &[ColoredSpan]) -> String {
    let mut result = String::new();
    for span in spans {
        let colored = match span.color {
            SpanColor::Unchanged => span.text.to_string(),
            SpanColor::Updated => span.text.yellow().to_string(),
            SpanColor::Deleted => span.text.red().to_string(),
            SpanColor::Inserted => span.text.green().to_string(),
        };
        result.push_str(&colored);
    }
    result
}

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

    let colored_left_number = if row.is_moved && row.source_line_number.is_some() {
        left_number_raw.cyan().to_string()
    } else {
        left_number_raw.dimmed().to_string()
    };
    let colored_right_number = if row.is_moved && row.destination_line_number.is_some() {
        right_number_raw.cyan().to_string()
    } else {
        right_number_raw.dimmed().to_string()
    };

    let left_rendered = render_spans(&row.source_spans);
    let padding = content_width.saturating_sub(row.source_plain_len());
    let left_padded = format!("{}{}", left_rendered, " ".repeat(padding));
    let right_rendered = render_spans(&row.destination_spans);
    let separator = "│".dimmed();

    writeln!(
        output,
        "{} {} {} {} {} {} {}",
        colored_left_number,
        separator,
        left_padded,
        separator,
        colored_right_number,
        separator,
        right_rendered,
    )
    .unwrap();
}

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

    let moved_destination_lines =
        detect_moved_destination_lines(&line_mapping, destination_lines.len());

    let source_leaf_colors = classify_source_leaves(source_tree, mapping, actions);
    let destination_leaf_colors = classify_destination_leaves(destination_tree, mapping, actions);

    let context = DiffContext {
        source_tree,
        destination_tree,
        source_leaf_colors: &source_leaf_colors,
        destination_leaf_colors: &destination_leaf_colors,
    };

    let rows = build_output_rows(
        &source_lines,
        &destination_lines,
        &line_mapping,
        &moved_destination_lines,
        &context,
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
