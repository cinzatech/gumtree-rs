use diffame::lis::longest_increasing_subsequence;

#[test]
fn finds_increasing_indices() {
    let lis = longest_increasing_subsequence(&[3, 1, 4, 1, 5, 9, 2, 6]);
    let seq = [3usize, 1, 4, 1, 5, 9, 2, 6];
    for window in lis.windows(2) {
        assert!(seq[window[0]] < seq[window[1]]);
    }
    assert_eq!(lis.len(), 4);
}

#[test]
fn empty_is_empty() {
    assert!(longest_increasing_subsequence(&[]).is_empty());
}

#[test]
fn sorted_is_full_length() {
    let lis = longest_increasing_subsequence(&[1, 2, 3, 4]);
    assert_eq!(lis.len(), 4);
}

#[test]
fn reverse_is_length_one() {
    let lis = longest_increasing_subsequence(&[4, 3, 2, 1]);
    assert_eq!(lis.len(), 1);
}
