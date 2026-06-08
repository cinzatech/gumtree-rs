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
    /// Original file path, shown in the side-by-side header.
    pub filename: Option<&'a str>,
    /// Detected language name, shown in the side-by-side header.
    pub language_name: Option<&'a str>,
}

/// Common interface implemented by every output format.
pub trait DiffFormatter {
    fn format(input: &FormatInput) -> String;
}

/// Returns the GumTree-style display string for a node.
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
