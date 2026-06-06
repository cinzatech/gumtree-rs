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

struct FileLine<'a> {
    text: &'a str,
    start_byte: usize,
    end_byte: usize,
}

fn split_into_lines<'a>(bytes: &'a [u8]) -> Vec<FileLine<'a>> {
    let text = std::str::from_utf8(bytes).expect("Input must be valid UTF-8");
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

fn line_index_at_byte(lines: &[FileLine], byte_offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| byte_offset >= line.start_byte && byte_offset <= line.end_byte)
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
                    let offset = destination_line - above_dst;
                    let candidate = above_src + offset;
                    (candidate < below_src && candidate < source_lines.len()).then_some(candidate)
                }
                (Some((above_dst, above_src)), _) => {
                    let offset = destination_line - above_dst;
                    let candidate = above_src + offset;
                    (candidate < source_lines.len() && offset <= 1).then_some(candidate)
                }
                (None, Some((below_dst, below_src))) => {
                    let offset = below_dst - destination_line;
                    (below_src >= offset && offset <= 1).then_some(below_src - offset)
                }
                _ => None,
            };

            let Some(source_line) = inferred_source else {
                continue;
            };
            if used_source_lines.contains(&source_line) {
                continue;
            }

            result.insert(destination_line, source_line);
            used_source_lines.insert(source_line);
            changed = true;
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

/// Builds a bijective destination-line → source-line mapping.
fn build_line_mapping<'a>(
    source_lines: &[FileLine<'a>],
    destination_lines: &[FileLine<'a>],
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> HashMap<usize, usize> {
    let candidates = phase1_vote(
        source_lines,
        destination_lines,
        source_tree,
        destination_tree,
        mapping,
    );
    let (result, used_source_lines) =
        phase2_interpolate(candidates, source_lines, destination_lines);
    phase3_blanks(result, used_source_lines, source_lines, destination_lines)
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

    source_tree
        .all_nodes()
        .filter(|n| n.children.is_empty())
        .map(|n| {
            let color = if !mapping.has_src(n.id) {
                SpanColor::Deleted
            } else if updated_nodes.contains(&n.id) {
                SpanColor::Updated
            } else {
                SpanColor::Unchanged
            };
            (n.id, color)
        })
        .collect()
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

    destination_tree
        .all_nodes()
        .filter(|n| n.children.is_empty())
        .map(|n| {
            let color = if !mapping.has_dst(n.id) {
                SpanColor::Inserted
            } else if mapping
                .get_src(n.id)
                .is_some_and(|src_id| updated_source_nodes.contains(&src_id))
            {
                SpanColor::Updated
            } else {
                SpanColor::Unchanged
            };
            (n.id, color)
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ColoredSpan<'a> {
    text: &'a str,
    color: SpanColor,
}

fn build_line_spans<'a>(
    line: &FileLine<'a>,
    tree: &Tree,
    leaf_colors: &HashMap<NodeId, SpanColor>,
) -> Vec<ColoredSpan<'a>> {
    let mut node_spans: Vec<(usize, usize, SpanColor)> = tree
        .all_nodes()
        .filter(|n| n.children.is_empty())
        .filter(|n| n.end_byte > line.start_byte && n.start_byte < line.end_byte)
        .filter_map(|n| leaf_colors.get(&n.id).map(|&c| (n, c)))
        .map(|(n, color)| {
            let start = n.start_byte.saturating_sub(line.start_byte);
            let end = (n.end_byte - line.start_byte).min(line.text.len());
            (start.min(end), end, color)
        })
        .collect();

    node_spans.sort_by_key(|s| s.0);

    let mut spans: Vec<ColoredSpan> = Vec::new();
    let mut position = 0;
    for (start, end, color) in &node_spans {
        if *start > position {
            spans.push(ColoredSpan {
                text: &line.text[position..*start],
                color: SpanColor::Unchanged,
            });
        }
        if *end > *start {
            spans.push(ColoredSpan {
                text: &line.text[*start..*end],
                color: *color,
            });
        }
        position = *end;
    }
    if position < line.text.len() {
        spans.push(ColoredSpan {
            text: &line.text[position..],
            color: SpanColor::Unchanged,
        });
    }
    spans
}

struct OutputRow<'a> {
    source_line_number: Option<usize>,
    source_spans: Vec<ColoredSpan<'a>>,
    destination_line_number: Option<usize>,
    destination_spans: Vec<ColoredSpan<'a>>,
    is_changed: bool,
    is_moved: bool,
}

impl OutputRow<'_> {
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

fn build_output_rows<'a>(
    source_lines: &[FileLine<'a>],
    destination_lines: &[FileLine<'a>],
    line_mapping: &HashMap<usize, usize>,
    moved_destination_lines: &HashSet<usize>,
    context: &DiffContext,
) -> Vec<OutputRow<'a>> {
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
                    .skip(gap_start)
                    .take(source_index - gap_start)
                {
                    if reverse_mapping.contains_key(&gap) || emitted_source_lines.contains(&gap) {
                        continue;
                    }
                    let spans =
                        build_line_spans(gap_line, context.source_tree, context.source_leaf_colors);
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
        if emitted_source_lines.contains(&source_index)
            || reverse_mapping.contains_key(&source_index)
        {
            continue;
        }
        let spans = build_line_spans(source_line, context.source_tree, context.source_leaf_colors);
        rows.push(OutputRow {
            source_line_number: Some(source_index + 1),
            source_spans: spans,
            destination_line_number: None,
            destination_spans: Vec::new(),
            is_changed: true,
            is_moved: false,
        });
    }

    rows
}

fn extract_hunks(rows: &[OutputRow], context: usize) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (i, _) in rows.iter().enumerate().filter(|(_, r)| r.is_changed) {
        let start = i.saturating_sub(context);
        let end = (i + context + 1).min(rows.len());
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
    spans
        .iter()
        .map(|span| match span.color {
            SpanColor::Unchanged => span.text.to_string(),
            SpanColor::Updated => span.text.yellow().to_string(),
            SpanColor::Deleted => span.text.red().to_string(),
            SpanColor::Inserted => span.text.green().to_string(),
        })
        .collect()
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

pub fn format_side_by_side<'a>(
    source_bytes: &'a [u8],
    destination_bytes: &'a [u8],
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
