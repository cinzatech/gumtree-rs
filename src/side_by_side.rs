//! Side-by-side colored diff output (tree-aware).
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

/// A single line of text with its byte range in the original file.
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

/// A top-level AST section with its line range.
struct Section {
    node_id: NodeId,
    start_line: usize,
    end_line: usize, // exclusive
}

fn find_sections(tree: &Tree, lines: &[FileLine]) -> Vec<Section> {
    let root = tree.root();
    tree.node(root)
        .children
        .iter()
        .filter_map(|&child_id| {
            let node = tree.node(child_id);
            let start = line_index_at_byte(lines, node.start_byte)?;
            let end = line_index_at_byte(lines, node.end_byte.saturating_sub(1))
                .map(|l| l + 1)
                .unwrap_or(start + 1);
            Some(Section {
                node_id: child_id,
                start_line: start,
                end_line: end,
            })
        })
        .collect()
}

/// Builds a bijective destination-line → source-line mapping by:
/// 1. Pairing top-level sections via the node mapping.
/// 2. Within each paired section, voting over leaf nodes (constrained).
/// 3. For gap lines (between sections), pairing by position.
fn build_tree_constrained_line_mapping(
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
    source_tree: &Tree,
    destination_tree: &Tree,
    source_sections: &[Section],
    destination_sections: &[Section],
    mapping: &Mapping,
) -> HashMap<usize, usize> {
    let mut result: HashMap<usize, usize> = HashMap::new();
    let mut used_source_lines: HashSet<usize> = HashSet::new();
    let mut covered_destination_lines: HashSet<usize> = HashSet::new();
    let mut covered_source_lines: HashSet<usize> = HashSet::new();

    // Phase 1: pair lines within matched sections.
    for destination_section in destination_sections {
        let source_section_index =
            mapping
                .get_src(destination_section.node_id)
                .and_then(|source_node_id| {
                    source_sections
                        .iter()
                        .position(|s| s.node_id == source_node_id)
                });

        let source_section = match source_section_index {
            Some(index) => &source_sections[index],
            None => continue,
        };

        let section_mapping = build_constrained_line_mapping(
            source_lines,
            destination_lines,
            source_section,
            destination_section,
            source_tree,
            destination_tree,
            mapping,
        );

        // Sort by vote confidence (already done inside build_constrained_line_mapping,
        // but we re-apply bijectivity across sections here).
        for (destination_line, source_line) in &section_mapping {
            if !used_source_lines.contains(source_line) {
                result.insert(*destination_line, *source_line);
                used_source_lines.insert(*source_line);
            }
        }

        for line in destination_section.start_line..destination_section.end_line {
            covered_destination_lines.insert(line);
        }
        for line in source_section.start_line..source_section.end_line {
            covered_source_lines.insert(line);
        }
    }

    // Phase 2: pair gap lines (lines not inside any section) by position.
    let source_gap_lines: Vec<usize> = (0..source_lines.len())
        .filter(|l| !covered_source_lines.contains(l) && !used_source_lines.contains(l))
        .collect();
    let destination_gap_lines: Vec<usize> = (0..destination_lines.len())
        .filter(|l| !covered_destination_lines.contains(l) && !result.contains_key(l))
        .collect();

    let pair_count = source_gap_lines.len().min(destination_gap_lines.len());
    for i in 0..pair_count {
        let source_line = source_gap_lines[i];
        let destination_line = destination_gap_lines[i];
        if !used_source_lines.contains(&source_line) {
            result.insert(destination_line, source_line);
            used_source_lines.insert(source_line);
        }
    }

    result
}

/// Builds a line mapping within a matched section pair, constrained to leaf
/// nodes whose byte ranges fall within both sections.
fn build_constrained_line_mapping(
    source_lines: &[FileLine],
    destination_lines: &[FileLine],
    source_section: &Section,
    destination_section: &Section,
    source_tree: &Tree,
    destination_tree: &Tree,
    mapping: &Mapping,
) -> HashMap<usize, usize> {
    let src_byte_start = source_lines[source_section.start_line].start_byte;
    let src_byte_end = source_lines[source_section.end_line - 1].end_byte;
    let dst_byte_start = destination_lines[destination_section.start_line].start_byte;
    let dst_byte_end = destination_lines[destination_section.end_line - 1].end_byte;

    let mut votes: HashMap<usize, HashMap<usize, usize>> = HashMap::new();

    for destination_node in destination_tree.all_nodes() {
        if !destination_node.children.is_empty() {
            continue;
        }
        if destination_node.start_byte < dst_byte_start
            || destination_node.start_byte > dst_byte_end
        {
            continue;
        }
        if let Some(source_node_id) = mapping.get_src(destination_node.id) {
            let source_node = source_tree.node(source_node_id);
            if source_node.start_byte < src_byte_start || source_node.start_byte > src_byte_end {
                continue;
            }
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

    let mut candidates: Vec<(usize, usize, usize)> = votes
        .into_iter()
        .filter_map(|(destination_line, line_votes)| {
            line_votes
                .into_iter()
                .max_by_key(|&(_, count)| count)
                .map(|(source_line, count)| (destination_line, source_line, count))
        })
        .collect();
    candidates.sort_by_key(|c| std::cmp::Reverse(c.2));

    let mut result: HashMap<usize, usize> = HashMap::new();
    let mut used: HashSet<usize> = HashSet::new();
    for (destination_line, source_line, _) in candidates {
        if !used.contains(&source_line) {
            result.insert(destination_line, source_line);
            used.insert(source_line);
        }
    }
    result
}

/// Determines which destination lines belong to moved sections.
fn detect_moved_destination_lines(
    source_sections: &[Section],
    destination_sections: &[Section],
    mapping: &Mapping,
) -> HashSet<usize> {
    // Build the sequence of source section indices in destination order.
    let mut matched_source_indices: Vec<usize> = Vec::new();
    let mut dst_to_src_section: HashMap<usize, usize> = HashMap::new();

    for (dst_index, destination_section) in destination_sections.iter().enumerate() {
        if let Some(source_node_id) = mapping.get_src(destination_section.node_id) {
            if let Some(src_index) = source_sections
                .iter()
                .position(|s| s.node_id == source_node_id)
            {
                matched_source_indices.push(src_index);
                dst_to_src_section.insert(dst_index, src_index);
            }
        }
    }

    let non_moved = longest_increasing_subsequence(&matched_source_indices);

    let mut moved_lines: HashSet<usize> = HashSet::new();
    for dst_index in dst_to_src_section.keys() {
        let src_idx = dst_to_src_section[dst_index];
        if !non_moved.contains(&src_idx) {
            let section = &destination_sections[*dst_index];
            for line in section.start_line..section.end_line {
                moved_lines.insert(line);
            }
        }
    }
    moved_lines
}

fn longest_increasing_subsequence(sequence: &[usize]) -> HashSet<usize> {
    if sequence.is_empty() {
        return HashSet::new();
    }
    let length = sequence.len();
    let mut tails: Vec<usize> = Vec::new();
    let mut parent: Vec<Option<usize>> = vec![None; length];
    let mut tail_indices: Vec<usize> = Vec::new();

    for i in 0..length {
        let value = sequence[i];
        let position = tails.partition_point(|&tail| tail < value);
        if position == tails.len() {
            tails.push(value);
            tail_indices.push(i);
        } else {
            tails[position] = value;
            tail_indices[position] = i;
        }
        if position > 0 {
            parent[i] = Some(tail_indices[position - 1]);
        }
    }

    let mut result = HashSet::new();
    let mut current = *tail_indices.last().unwrap();
    loop {
        result.insert(sequence[current]);
        match parent[current] {
            Some(predecessor) => current = predecessor,
            None => break,
        }
    }
    result
}

/// Per-leaf-node color classification.
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

/// A colored span within a line.
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

/// A row in the side-by-side output.
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

/// Bundles tree and color data needed for rendering.
struct DiffContext<'a> {
    source_tree: &'a Tree,
    destination_tree: &'a Tree,
    source_leaf_colors: &'a HashMap<NodeId, SpanColor>,
    destination_leaf_colors: &'a HashMap<NodeId, SpanColor>,
}

/// Builds output rows by walking destination lines and interleaving deletes.
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
            // Emit deleted source lines in the gap before this match.
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

    // Trailing deleted source lines.
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

/// Renders the full diff as a side-by-side colored string.
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

    let source_sections = find_sections(source_tree, &source_lines);
    let destination_sections = find_sections(destination_tree, &destination_lines);

    let line_mapping = build_tree_constrained_line_mapping(
        &source_lines,
        &destination_lines,
        source_tree,
        destination_tree,
        &source_sections,
        &destination_sections,
        mapping,
    );

    let moved_destination_lines =
        detect_moved_destination_lines(&source_sections, &destination_sections, mapping);

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
