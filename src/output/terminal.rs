//! Side-by-side colored terminal output.
//!
//! Colors are applied per AST span, not per line:
//!
//! * **Cyan** line numbers: the line belongs to a moved block.
//! * **Red** spans on the left: deleted tokens.
//! * **Green** spans on the right: inserted tokens.
//! * **Yellow** spans on both sides: updated (label-changed) tokens.
//! * Default: unchanged tokens and inter-token whitespace.
//! * `•`: filler for absent lines with no counterpart on the other side.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use colored::Colorize;

use crate::actions::Action;
use crate::mapping::Mapping;
use crate::tree::{NodeId, Tree};

use super::line_pairing::{build_line_pairing, split_into_lines, FileLine};
use super::{DiffFormatter, FormatInput};

/// The fill character used when a line has no counterpart on the other side.
const ABSENT_FILL: char = '•';

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
        let Some(&source_index) = line_mapping.get(&destination_index) else {
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
            continue;
        };

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

/// Builds a dimmed fill string of `•` characters to occupy `width` columns.
fn absent_fill(width: usize) -> String {
    let fill: String = std::iter::repeat_n(ABSENT_FILL, width).collect();
    fill.dimmed().to_string()
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

    // Left content: real spans or • fill when absent.
    let (left_padded, right_content) = if row.source_line_number.is_none() {
        // No source line, fill the left side with •.
        (
            absent_fill(content_width),
            render_spans(&row.destination_spans),
        )
    } else if row.destination_line_number.is_none() {
        // No destination line, fill the right side with •.
        let left_rendered = render_spans(&row.source_spans);
        let padding = content_width.saturating_sub(row.source_plain_len());
        (
            format!("{}{}", left_rendered, " ".repeat(padding)),
            absent_fill(content_width),
        )
    } else {
        // Both sides present.
        let left_rendered = render_spans(&row.source_spans);
        let padding = content_width.saturating_sub(row.source_plain_len());
        (
            format!("{}{}", left_rendered, " ".repeat(padding)),
            render_spans(&row.destination_spans),
        )
    };

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
        right_content,
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

pub struct TerminalFormatter;

impl DiffFormatter for TerminalFormatter {
    fn format(input: &FormatInput) -> String {
        format_side_by_side(
            input.source_bytes,
            input.destination_bytes,
            &input.result.src_tree,
            &input.result.dst_tree,
            &input.result.mapping,
            &input.result.actions,
        )
    }
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

    let pairing = build_line_pairing(
        &source_lines,
        &destination_lines,
        source_tree,
        destination_tree,
        mapping,
    );

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
        &pairing.dst_to_src,
        &pairing.moved_dst_lines,
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
