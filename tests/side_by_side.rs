//! Regression tests for side-by-side rendering.
//!
//! Each test exercises a specific rendering defect found during review.
//! They all use real Python source through `diff_sources` so the full
//! pipeline (tree-sitter → matcher → actions → renderer) is exercised.

use std::collections::HashSet;

use diffame::languages;
use diffame::side_by_side::{format_side_by_side, SideBySideInput};
use diffame::{diff_sources, DiffOptions};

/// Helper: diff two Python snippets and return the side-by-side output with
/// ANSI colors stripped.
fn side_by_side_plain(old: &str, new: &str) -> String {
    colored::control::set_override(false);
    let output = side_by_side_raw(old, new);
    colored::control::unset_override();
    output
}

/// Helper: diff two Python snippets and return the side-by-side output with
/// ANSI color codes included.
fn side_by_side_colored(old: &str, new: &str) -> String {
    colored::control::set_override(true);
    let output = side_by_side_raw(old, new);
    colored::control::unset_override();
    output
}

fn side_by_side_raw(old: &str, new: &str) -> String {
    let profile = languages::profile_for_ext("py").expect("python profile");
    let result = diff_sources(
        old.as_bytes(),
        new.as_bytes(),
        profile,
        &DiffOptions::default(),
    )
    .expect("diff failed");
    format_side_by_side(&SideBySideInput {
        source_bytes: old.as_bytes(),
        destination_bytes: new.as_bytes(),
        source_tree: &result.src_tree,
        destination_tree: &result.dst_tree,
        mapping: &result.mapping,
        actions: &result.actions,
        filename: None,
        language_name: None,
    })
}

/// Parse plain-text rows into (left_num, left_text, right_num, right_text).
fn parse_rows(output: &str) -> Vec<(Option<usize>, String, Option<usize>, String)> {
    output
        .lines()
        .filter(|line| line.contains('│'))
        .map(|line| {
            let clean = strip_ansi(line);
            let parts: Vec<&str> = clean.split('│').collect();
            if parts.len() < 4 {
                return (None, String::new(), None, String::new());
            }
            let left_num = parts[0].trim().parse::<usize>().ok();
            let left_text = parts[1].trim().to_string();
            let right_num = parts[2].trim().parse::<usize>().ok();
            let right_text = parts[3].trim().to_string();
            (left_num, left_text, right_num, right_text)
        })
        .collect()
}

/// Strip ANSI escape codes from a string.
fn strip_ansi(input: &str) -> String {
    let mut result = String::new();
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

const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_CYAN: &str = "\x1b[36m";

const FUNC_OLD: &str = "\
def greet(name):
    print(\"Hello, \" + name)

def add(a, b):
    return a + b

def unused():
    pass
";

const FUNC_NEW: &str = "\
def add(a, b):
    return a + b

def greet(person):
    print(\"Hi, \" + person)

def multiply(a, b):
    return a * b

def helper():
    x = 1
    return x
";

/// Issue 1: A blank line that exists in both files at the boundary between
/// functions should not appear as a deleted row on the left with no right
/// counterpart.
#[test]
fn blank_lines_preserved_in_both_files_are_not_shown_as_deleted() {
    let output = side_by_side_plain(FUNC_OLD, FUNC_NEW);
    let rows = parse_rows(&output);

    let phantom_blank_deletes: Vec<_> = rows
        .iter()
        .filter(|(left_num, left_text, right_num, _)| {
            left_num.is_some() && left_text.trim().is_empty() && right_num.is_none()
        })
        .collect();

    assert!(
        phantom_blank_deletes.is_empty(),
        "Blank lines that exist in both files should not appear as deleted rows.\n\
         Found {} phantom blank-line delete(s): {:?}",
        phantom_blank_deletes.len(),
        phantom_blank_deletes,
    );
}

/// Issue 2: When only a single token on a line changes (e.g. `name` → `person`),
/// the unchanged portions of the line (like `def greet(`) must NOT be colored
/// yellow. Only the changed token should be yellow.
#[test]
fn updated_line_colors_only_changed_tokens() {
    let old = "def greet(name):\n    pass\n";
    let new = "def greet(person):\n    pass\n";
    let output = side_by_side_colored(old, new);

    // Find the line containing the greet definition.
    let greet_line = output
        .lines()
        .find(|line| strip_ansi(line).contains("def greet("))
        .expect("should have a line containing 'def greet('");

    // The unchanged prefix `def greet(` must NOT be wrapped in yellow.
    // If the entire line is yellow, `def greet(` will be preceded by the
    // yellow ANSI code. Check that `def greet(` does NOT appear immediately
    // after a yellow escape.
    //
    // Split by the separator │ to isolate the left content column.
    let left_content = greet_line
        .split('│')
        .nth(1)
        .expect("should have left content column");

    // If `def` is inside a yellow span, the yellow escape code will appear
    // before it with no reset in between. The whole left column being yellow
    // means ANSI_YELLOW appears and then `def` follows without a reset.
    assert!(
        !left_content.contains(&format!("{}def greet(", ANSI_YELLOW)),
        "The unchanged prefix 'def greet(' should not be colored yellow.\n\
         Full left column: {:?}",
        left_content,
    );

    // But the changed token `name` SHOULD be yellow on the left.
    assert!(
        left_content.contains(ANSI_YELLOW),
        "The changed token 'name' should be colored yellow.\n\
         Full left column: {:?}",
        left_content,
    );
}

/// Issue 3: A function that kept its relative position among its peers should
/// NOT have cyan (moved) line numbers. `unused` stayed after `add` in both
/// files, it was not reordered.
#[test]
fn non_reordered_function_is_not_marked_as_moved() {
    let output = side_by_side_colored(FUNC_OLD, FUNC_NEW);

    // Find the line where source shows "def unused".
    let unused_line = output
        .lines()
        .find(|line| strip_ansi(line).contains("def unused"))
        .expect("should have a line for def unused");

    // The source line number (left column) for this row should NOT be cyan.
    let left_number_column = unused_line
        .split('│')
        .next()
        .expect("should have left number column");

    assert!(
        !left_number_column.contains(ANSI_CYAN),
        "Line number for 'def unused' should not be cyan (it was not moved).\n\
         Left number column: {:?}",
        left_number_column,
    );
}

/// The user's actual test case: FUNC_OLD + think appended, FUNC_NEW + think
/// appended. The output should pair greet↔greet, add↔add, think↔think because
/// the leaf-level identifiers map correctly (greet→greet, think→think).
///
/// The bug: parent-level function_definition mappings can disagree with leaf
/// mappings (greet's function_definition maps to think's function_definition),
/// and the section-constrained approach used the WRONG level to constrain,
/// garbling the output.
#[test]
fn extended_test_case_pairs_functions_by_leaf_identity() {
    let old = "\
def greet(name):
    print(\"Hello, \" + name)

def add(a, b):
    return a + b

def unused():
    pass

def think(about):
    print(about)
";
    let new = "\
def add(a, b):
    return a + b

def greet(person):
    print(\"Hi, \" + person)

def multiply(a, b):
    return a * b

def helper():
    x = 1
    return x

def think(thought):
    print(\"Thinking about \" + thought)
";
    let output = side_by_side_plain(old, new);
    let rows = parse_rows(&output);

    // No source line number may appear more than once.
    let source_line_numbers: Vec<usize> = rows
        .iter()
        .filter_map(|(left_num, _, _, _)| *left_num)
        .collect();
    let unique_source: HashSet<usize> = source_line_numbers.iter().copied().collect();
    assert_eq!(
        source_line_numbers.len(),
        unique_source.len(),
        "Source line numbers must be unique. Duplicates found in: {:?}",
        source_line_numbers,
    );

    // greet must pair with greet.
    let greet_row = rows
        .iter()
        .find(|(_, _, _, right)| right.contains("def greet"))
        .expect("should have a row with 'def greet' on the right");
    assert!(
        greet_row.1.contains("def greet"),
        "right 'def greet' should pair with left 'def greet', got left={:?}",
        greet_row.1,
    );

    // think must pair with think.
    let think_row = rows
        .iter()
        .find(|(_, _, _, right)| right.contains("def think"))
        .expect("should have a row with 'def think' on the right");
    assert!(
        think_row.1.contains("def think"),
        "right 'def think' should pair with left 'def think', got left={:?}",
        think_row.1,
    );

    // add must pair with add.
    let add_row = rows
        .iter()
        .find(|(_, _, _, right)| right.contains("def add"))
        .expect("should have a row with 'def add' on the right");
    assert!(
        add_row.1.contains("def add"),
        "right 'def add' should pair with left 'def add', got left={:?}",
        add_row.1,
    );

    // think's print(about) must pair with think's print(... thought).
    let think_print_row = rows
        .iter()
        .find(|(_, _, _, right)| right.contains("Thinking about"))
        .expect("should have a row for think's print");
    assert!(
        think_print_row.1.contains("print(about)"),
        "think's print should pair with 'print(about)', got left={:?}",
        think_print_row.1,
    );
}

/// Structural invariants that must hold for any diff output.
#[test]
fn cross_function_leaf_mapping_does_not_garble_lines() {
    let old = "\
def format_name(first, last):
    full = first + \" \" + last
    return full.upper()

def add(a, b):
    return a + b

def process(items):
    for item in items:
        print(item)
    return len(items)
";
    let new = "\
def format_name(given, family):
    full = given + \" \" + family
    return full.upper()

def add(x, y):
    return x + y

def process(entries):
    for entry in entries:
        print(entry)
    return len(entries)
";
    let output = side_by_side_plain(old, new);
    let rows = parse_rows(&output);

    // No source line number appears more than once.
    let source_line_numbers: Vec<usize> = rows
        .iter()
        .filter_map(|(left_num, _, _, _)| *left_num)
        .collect();
    let unique_source: HashSet<usize> = source_line_numbers.iter().copied().collect();
    assert_eq!(
        source_line_numbers.len(),
        unique_source.len(),
        "Source line numbers must be unique. Duplicates found in: {:?}",
        source_line_numbers,
    );

    // No destination line number appears more than once.
    let destination_line_numbers: Vec<usize> = rows
        .iter()
        .filter_map(|(_, _, right_num, _)| *right_num)
        .collect();
    let unique_destination: HashSet<usize> = destination_line_numbers.iter().copied().collect();
    assert_eq!(
        destination_line_numbers.len(),
        unique_destination.len(),
        "Destination line numbers must be unique. Duplicates found in: {:?}",
        destination_line_numbers,
    );
}
