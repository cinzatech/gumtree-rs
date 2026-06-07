use diffame::string_distance::{levenshtein_distance, normalised_similarity};

#[test]
fn levenshtein_identical_strings() {
    assert_eq!(levenshtein_distance("hello", "hello"), 0);
}

#[test]
fn levenshtein_completely_different() {
    assert_eq!(levenshtein_distance("abc", "xyz"), 3);
}

#[test]
fn levenshtein_empty_strings() {
    assert_eq!(levenshtein_distance("", ""), 0);
    assert_eq!(levenshtein_distance("abc", ""), 3);
    assert_eq!(levenshtein_distance("", "abc"), 3);
}

#[test]
fn levenshtein_single_edit() {
    assert_eq!(levenshtein_distance("kitten", "sitten"), 1);
    assert_eq!(levenshtein_distance("abc", "abcd"), 1);
}

#[test]
fn similarity_identical() {
    assert!((normalised_similarity("hello", "hello") - 1.0).abs() < f64::EPSILON);
}

#[test]
fn similarity_empty_strings() {
    assert!((normalised_similarity("", "") - 1.0).abs() < f64::EPSILON);
}

#[test]
fn similarity_completely_different() {
    assert!((normalised_similarity("abc", "xyz")).abs() < f64::EPSILON);
}

#[test]
fn similarity_partial_match() {
    // "Barbaz" vs "Bar baz": distance 1, length 7 → similarity ≈ 0.857
    let score = normalised_similarity("Barbaz", "Bar baz");
    assert!(score > 0.8);
    assert!(score < 0.9);
}
