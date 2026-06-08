/// Returns a similarity score in `[0.0, 1.0]` based on Levenshtein distance,
/// normalised by the length of the longer string. Two empty strings score 1.0.
#[must_use]
pub fn normalised_similarity(left: &str, right: &str) -> f64 {
    let max_length = left.len().max(right.len());
    if max_length == 0 {
        return 1.0;
    }
    let distance = levenshtein_distance(left, right);
    1.0 - (distance as f64 / max_length as f64)
}

/// Standard O(n·m) Levenshtein distance on bytes.
#[must_use]
pub fn levenshtein_distance(left: &str, right: &str) -> usize {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let right_length = right_bytes.len();

    // Single-row DP; only the previous row is needed.
    let mut previous_row: Vec<usize> = (0..=right_length).collect();
    let mut current_row = vec![0; right_length + 1];

    for (left_index, &left_byte) in left_bytes.iter().enumerate() {
        current_row[0] = left_index + 1;
        for (right_index, &right_byte) in right_bytes.iter().enumerate() {
            let substitution_cost = usize::from(left_byte != right_byte);
            current_row[right_index + 1] = (previous_row[right_index] + substitution_cost)
                .min(previous_row[right_index + 1] + 1)
                .min(current_row[right_index] + 1);
        }
        std::mem::swap(&mut previous_row, &mut current_row);
    }

    previous_row[right_length]
}
