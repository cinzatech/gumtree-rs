/// Returns indices into `sequence` that form a longest strictly increasing
/// subsequence (patience-sort style, O(n log n)).
#[must_use]
pub fn longest_increasing_subsequence(sequence: &[usize]) -> Vec<usize> {
    let length = sequence.len();
    if length == 0 {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::new(); // tails[index] = index ending an LIS of length index+1
    let mut predecessors: Vec<Option<usize>> = vec![None; length];
    for index in 0..length {
        let value = sequence[index];
        let insert_position = tails
            .binary_search_by(|&tail_index| {
                if sequence[tail_index] < value {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_or_else(|position| position);
        if insert_position > 0 {
            predecessors[index] = Some(tails[insert_position - 1]);
        }
        if insert_position < tails.len() {
            tails[insert_position] = index;
        } else {
            tails.push(index);
        }
    }
    let mut result = Vec::new();
    let mut current = tails.last().copied();
    while let Some(index) = current {
        result.push(index);
        current = predecessors[index];
    }
    result.reverse();
    result
}
