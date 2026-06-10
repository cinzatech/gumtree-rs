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
use unicode_width::UnicodeWidthChar;

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

/// Source nodes targeted by an `Update` action.
fn updated_nodes(actions: &[Action]) -> HashSet<NodeId> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Update { node, .. } => Some(*node),
            _ => None,
        })
        .collect()
}

/// Colors every leaf of `tree`: `unmapped_color` when `is_unmapped`,
/// `Updated` when `is_updated`, `Unchanged` otherwise. The two closures
/// encode the only source/destination asymmetry (which side of the mapping
/// to consult).
fn classify_leaves(
    tree: &Tree,
    is_unmapped: impl Fn(NodeId) -> bool,
    is_updated: impl Fn(NodeId) -> bool,
    unmapped_color: SpanColor,
) -> HashMap<NodeId, SpanColor> {
    tree.all_nodes()
        .filter(|n| n.children.is_empty())
        .map(|n| {
            let color = if is_unmapped(n.id) {
                unmapped_color
            } else if is_updated(n.id) {
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

impl<'a> OutputRow<'a> {
    /// Unpaired source line: entire line is deleted.
    fn deleted(line_index: usize, text: &'a str) -> Self {
        Self {
            source_line_number: Some(line_index + 1),
            source_spans: vec![ColoredSpan {
                text,
                color: SpanColor::Deleted,
            }],
            destination_line_number: None,
            destination_spans: Vec::new(),
            is_changed: true,
            is_moved: false,
        }
    }

    /// Unpaired destination line: entire line is inserted.
    fn inserted(line_index: usize, text: &'a str) -> Self {
        Self {
            source_line_number: None,
            source_spans: Vec::new(),
            destination_line_number: Some(line_index + 1),
            destination_spans: vec![ColoredSpan {
                text,
                color: SpanColor::Inserted,
            }],
            is_changed: true,
            is_moved: false,
        }
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
            rows.push(OutputRow::inserted(
                destination_index,
                destination_line.text,
            ));
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
                rows.push(OutputRow::deleted(gap, gap_line.text));
                emitted_source_lines.insert(gap);
            }
        }

        let is_changed = source_lines[source_index].text != destination_line.text;
        let is_moved = moved_destination_lines.contains(&destination_index);

        // Paired identical lines: suppress all coloring so that AST
        // mapping noise (e.g. on moved code) doesn't produce false diffs.
        let (source_spans, destination_spans) = if is_changed {
            (
                build_line_spans(
                    &source_lines[source_index],
                    context.source_tree,
                    context.source_leaf_colors,
                ),
                build_line_spans(
                    destination_line,
                    context.destination_tree,
                    context.destination_leaf_colors,
                ),
            )
        } else {
            (
                vec![ColoredSpan {
                    text: source_lines[source_index].text,
                    color: SpanColor::Unchanged,
                }],
                vec![ColoredSpan {
                    text: destination_line.text,
                    color: SpanColor::Unchanged,
                }],
            )
        };

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
        rows.push(OutputRow::deleted(source_index, source_line.text));
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

/// Split a list of colored spans into visual lines of at most `width` columns.
/// Each returned element is (spans, `visual_length`).
///
/// Uses Unicode display width so that multi-byte characters like `─` (3 bytes,
/// 1 column) and CJK characters (3–4 bytes, 2 columns) are measured correctly.
fn wrap_spans<'a>(spans: &[ColoredSpan<'a>], width: usize) -> Vec<(Vec<ColoredSpan<'a>>, usize)> {
    if width == 0 {
        let total: usize = spans.iter().map(|s| display_width(s.text)).sum();
        return vec![(spans.to_vec(), total)];
    }
    let mut lines: Vec<(Vec<ColoredSpan>, usize)> = Vec::new();
    let mut current_spans: Vec<ColoredSpan> = Vec::new();
    let mut current_len: usize = 0; // display columns

    for span in spans {
        let mut remaining = span.text;
        while !remaining.is_empty() {
            let avail = width.saturating_sub(current_len);
            if avail == 0 {
                lines.push((std::mem::take(&mut current_spans), current_len));
                current_len = 0;
                continue;
            }
            let (take_bytes, take_cols) = bytes_fitting_columns(remaining, avail);
            if take_bytes == 0 {
                // Character wider than available space; wrap to new line.
                if !current_spans.is_empty() {
                    lines.push((std::mem::take(&mut current_spans), current_len));
                    current_len = 0;
                }
                let (take_bytes, take_cols) = bytes_fitting_columns(remaining, width);
                let take_bytes = take_bytes.max(remaining.chars().next().map_or(1, char::len_utf8));
                current_spans.push(ColoredSpan {
                    text: &remaining[..take_bytes],
                    color: span.color,
                });
                current_len += take_cols.max(1);
                remaining = &remaining[take_bytes..];
            } else {
                current_spans.push(ColoredSpan {
                    text: &remaining[..take_bytes],
                    color: span.color,
                });
                current_len += take_cols;
                remaining = &remaining[take_bytes..];
            }
        }
    }
    if !current_spans.is_empty() || lines.is_empty() {
        lines.push((current_spans, current_len));
    }
    lines
}

/// Returns (byte_count, column_count) for the longest prefix of `text`
/// that fits within `max_cols` display columns.
fn bytes_fitting_columns(text: &str, max_cols: usize) -> (usize, usize) {
    let mut cols = 0;
    let mut bytes = 0;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if cols + w > max_cols {
            break;
        }
        cols += w;
        bytes += ch.len_utf8();
    }
    (bytes, cols)
}

/// Display width of a string in terminal columns.
fn display_width(text: &str) -> usize {
    text.chars().map(|ch| ch.width().unwrap_or(0)).sum()
}

/// Formats one line-number cell: right-aligned and dimmed (or cyan when the
/// row is moved) on the first visual line, blank on wrapped continuation
/// lines or when the side has no line.
fn number_cell(number: Option<usize>, visual_index: usize, is_moved: bool, width: usize) -> String {
    match number {
        Some(n) if visual_index == 0 => {
            let cell = format!("{n:>width$}");
            if is_moved {
                cell.cyan().to_string()
            } else {
                cell.dimmed().to_string()
            }
        }
        _ => " ".repeat(width),
    }
}

fn render_row(
    row: &OutputRow,
    line_number_width: usize,
    content_width: usize,
    output: &mut String,
) {
    let left_visual_lines = if row.source_line_number.is_none() {
        // No source line: fill with • for each visual line on the right.
        vec![]
    } else {
        wrap_spans(&row.source_spans, content_width)
    };

    let right_visual_lines = if row.destination_line_number.is_none() {
        vec![]
    } else {
        wrap_spans(&row.destination_spans, content_width)
    };

    let num_visual = left_visual_lines.len().max(right_visual_lines.len()).max(1);
    let separator = "│".dimmed();

    for v in 0..num_visual {
        let colored_left_number =
            number_cell(row.source_line_number, v, row.is_moved, line_number_width);
        let colored_right_number = number_cell(
            row.destination_line_number,
            v,
            row.is_moved,
            line_number_width,
        );

        // Left content.
        let left_padded = if row.source_line_number.is_none() {
            absent_fill(content_width)
        } else if let Some((ref spans, vis_len)) = left_visual_lines.get(v) {
            let rendered = render_spans(spans);
            let padding = content_width.saturating_sub(*vis_len);
            format!("{}{}", rendered, " ".repeat(padding))
        } else {
            " ".repeat(content_width)
        };

        // Right content.
        let right_content = if row.destination_line_number.is_none() {
            absent_fill(content_width).clone()
        } else if let Some((ref spans, _)) = right_visual_lines.get(v) {
            render_spans(spans)
        } else {
            String::new()
        };

        writeln!(
            output,
            "{colored_left_number} {separator} {left_padded} {separator} {colored_right_number} {separator} {right_content}",
        )
        .unwrap();
    }
}

fn render_separator(line_number_width: usize, content_width: usize, output: &mut String) {
    let number_bar = "─".repeat(line_number_width);
    let content_bar = "─".repeat(content_width);
    let raw = format!("{number_bar}─┼─{content_bar}─┼─{number_bar}─┼─{content_bar}");
    writeln!(output, "{}", raw.dimmed()).unwrap();
}

pub struct TerminalFormatter;

impl DiffFormatter for TerminalFormatter {
    fn format(input: &FormatInput) -> String {
        format_side_by_side(&SideBySideInput {
            source_bytes: input.source_bytes,
            destination_bytes: input.destination_bytes,
            source_tree: &input.result.src_tree,
            destination_tree: &input.result.dst_tree,
            mapping: &input.result.mapping,
            actions: &input.result.actions,
            filename: input.filename,
            language_name: input.language_name,
        })
    }
}

/// Render a file-info header line (filename and detected language).
fn render_file_header(
    filename: Option<&str>,
    language_name: Option<&str>,
    line_number_width: usize,
    content_width: usize,
    output: &mut String,
) {
    let label = match (filename, language_name) {
        (Some(f), Some(l)) => format!("{f} [{l}]"),
        (Some(f), None) => f.to_string(),
        (None, Some(l)) => format!("[{l}]"),
        (None, None) => return,
    };
    // Total width: line_num + " │ " + content + " │ " + line_num + " │ " + content
    let total_width =
        line_number_width + 3 + content_width + 3 + line_number_width + 3 + content_width;
    let padded = if label.len() < total_width {
        format!(" {}{}", label, " ".repeat(total_width - label.len() - 1))
    } else {
        format!(" {}", &label[..total_width - 1])
    };
    writeln!(output, "{}", padded.bold().dimmed()).unwrap();
}

/// All inputs needed to produce a side-by-side diff.
pub struct SideBySideInput<'a> {
    pub source_bytes: &'a [u8],
    pub destination_bytes: &'a [u8],
    pub source_tree: &'a Tree,
    pub destination_tree: &'a Tree,
    pub mapping: &'a Mapping,
    pub actions: &'a [Action],
    pub filename: Option<&'a str>,
    pub language_name: Option<&'a str>,
}

#[must_use]
pub fn format_side_by_side(input: &SideBySideInput) -> String {
    let SideBySideInput {
        source_tree,
        destination_tree,
        mapping,
        actions,
        ..
    } = *input;

    let source_lines = split_into_lines(input.source_bytes);
    let destination_lines = split_into_lines(input.destination_bytes);

    let pairing = build_line_pairing(
        &source_lines,
        &destination_lines,
        source_tree,
        destination_tree,
        mapping,
    );

    let updated = updated_nodes(actions);
    let source_leaf_colors = classify_leaves(
        source_tree,
        |id| !mapping.has_src(id),
        |id| updated.contains(&id),
        SpanColor::Deleted,
    );
    let destination_leaf_colors = classify_leaves(
        destination_tree,
        |id| !mapping.has_dst(id),
        |id| {
            mapping
                .get_src(id)
                .is_some_and(|src| updated.contains(&src))
        },
        SpanColor::Inserted,
    );

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
    let line_number_width = format!("{max_line_number}").len().max(3);
    let content_width = {
        // Layout: {line_num} │ {content} │ {line_num} │ {content}
        // Total  = 2*line_number_width + 2*content_width + 9  (three " │ " separators)
        let default = 50;
        terminal_size::terminal_size()
            .map(|(terminal_size::Width(w), _)| {
                let chrome = 2 * line_number_width + 9;
                let w = w as usize;
                if w > chrome + 2 {
                    (w - chrome) / 2
                } else {
                    default
                }
            })
            .unwrap_or(default)
    };

    let mut output = String::new();
    if input.filename.is_some() || input.language_name.is_some() {
        render_file_header(
            input.filename,
            input.language_name,
            line_number_width,
            content_width,
            &mut output,
        );
        render_separator(line_number_width, content_width, &mut output);
    }
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
