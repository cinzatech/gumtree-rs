//! Output formatters for diff results.
//!
//! Each formatter implements [`DiffFormatter`] so the CLI can dispatch on `-f`
//! without knowing the details of any specific output format.

pub mod json;
pub mod line_pairing;
pub mod terminal;
pub mod text;

use crate::tree::Node;
use crate::DiffResult;

/// Everything a formatter needs to produce output.
pub struct FormatInput<'a> {
    pub source_bytes: &'a [u8],
    pub destination_bytes: &'a [u8],
    pub result: &'a DiffResult,
    /// Original path of the old file, shown on the left of the header.
    pub source_filename: Option<&'a str>,
    /// Original path of the new file, shown on the right of the header.
    pub destination_filename: Option<&'a str>,
    /// Detected language name, shown in the side-by-side header.
    pub language_name: Option<&'a str>,
}

/// Common interface implemented by every output format.
pub trait DiffFormatter {
    fn format(input: &FormatInput) -> String;
}

/// Removes ANSI escape sequences (`ESC [ ... m`) from a string.
///
/// Formatters always emit styled output; callers that need plain text
/// (non-terminal stdout, tests) strip the styling at the edge.
#[must_use]
pub fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_escape = false;
    for ch in input.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Returns the GumTree-style display string for a node.
#[must_use]
pub fn format_node(node: &Node) -> String {
    if node.label.is_empty() {
        format!("{} [{},{}]", node.kind, node.start_byte, node.end_byte)
    } else {
        format!(
            "{}: {} [{},{}]",
            node.kind, node.label, node.start_byte, node.end_byte
        )
    }
}
