//! Plain-text output format: one `Debug`-formatted action per line.

use std::fmt::Write;

use super::{DiffFormatter, FormatInput};

pub struct TextFormatter;

impl DiffFormatter for TextFormatter {
    fn format(input: &FormatInput) -> String {
        let mut output = String::new();
        for action in &input.result.actions {
            writeln!(output, "{:?}", action).unwrap();
        }
        output
    }
}
