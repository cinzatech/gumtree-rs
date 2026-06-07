use gumtree_rs::line_tree::build_line_tree;
use gumtree_rs::matcher::line_diff::match_lines;

#[test]
fn identical_files_map_all_lines() {
    let source = build_line_tree(b"aaa\nbbb\nccc\n");
    let destination = build_line_tree(b"aaa\nbbb\nccc\n");
    let mapping = match_lines(&source, &destination);
    assert_eq!(mapping.len(), 4);
}

#[test]
fn completely_different_files_map_only_roots() {
    let source = build_line_tree(b"aaa\nbbb\n");
    let destination = build_line_tree(b"xxx\nyyy\n");
    let mapping = match_lines(&source, &destination);
    // Only roots; "aaa"↔"xxx" similarity is 0.0, below threshold.
    assert_eq!(mapping.len(), 1);
}

#[test]
fn added_lines_are_unmapped() {
    let source = build_line_tree(b"aaa\nccc\n");
    let destination = build_line_tree(b"aaa\nbbb\nccc\n");
    let mapping = match_lines(&source, &destination);
    assert_eq!(mapping.len(), 3);
}

#[test]
fn removed_lines_are_unmapped() {
    let source = build_line_tree(b"aaa\nbbb\nccc\n");
    let destination = build_line_tree(b"aaa\nccc\n");
    let mapping = match_lines(&source, &destination);
    assert_eq!(mapping.len(), 3);
}

#[test]
fn swapped_lines_are_both_matched() {
    let source = build_line_tree(b"Foo\nBar\nBaz\n");
    let destination = build_line_tree(b"Foo\nBaz\nBar\n");
    let mapping = match_lines(&source, &destination);
    // Root + all 3 lines matched (Foo stays, Bar and Baz swap).
    assert_eq!(mapping.len(), 4);
}

#[test]
fn similar_lines_are_matched() {
    let source = build_line_tree(b"Barbaz\n");
    let destination = build_line_tree(b"Bar baz\n");
    let mapping = match_lines(&source, &destination);
    // Root + the fuzzy-matched line.
    assert_eq!(mapping.len(), 2);
}

#[test]
fn dissimilar_lines_are_not_matched() {
    let source = build_line_tree(b"aaa\n");
    let destination = build_line_tree(b"zzz\n");
    let mapping = match_lines(&source, &destination);
    // Only root; similarity is 0.
    assert_eq!(mapping.len(), 1);
}

#[test]
fn duplicate_lines_are_matched_by_proximity() {
    let source = build_line_tree(b"x\nx\nx\n");
    let destination = build_line_tree(b"x\nx\n");
    let mapping = match_lines(&source, &destination);
    // Root + 2 matched "x" lines.
    assert_eq!(mapping.len(), 3);
}
