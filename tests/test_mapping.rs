use gumtree_rs::mapping::Mapping;

#[test]
fn new_mapping_is_empty() {
    let mapping = Mapping::new();
    assert!(mapping.is_empty());
    assert_eq!(mapping.len(), 0);
    assert!(mapping.pairs().is_empty());
}

#[test]
fn link_creates_bidirectional_lookup() {
    let mut mapping = Mapping::new();
    assert!(mapping.link(3, 7));
    assert_eq!(mapping.get_dst(3), Some(7));
    assert_eq!(mapping.get_src(7), Some(3));
    assert!(mapping.has_src(3));
    assert!(mapping.has_dst(7));
    assert_eq!(mapping.len(), 1);
}

#[test]
fn link_rejects_when_src_already_mapped() {
    let mut mapping = Mapping::new();
    assert!(mapping.link(3, 7));
    assert!(!mapping.link(3, 9));
    assert_eq!(mapping.get_dst(3), Some(7));
    assert!(!mapping.has_dst(9));
}

#[test]
fn link_rejects_when_dst_already_mapped() {
    let mut mapping = Mapping::new();
    assert!(mapping.link(3, 7));
    assert!(!mapping.link(5, 7));
    assert_eq!(mapping.get_src(7), Some(3));
    assert!(!mapping.has_src(5));
}

#[test]
fn link_rejects_duplicate_pair() {
    let mut mapping = Mapping::new();
    assert!(mapping.link(3, 7));
    assert!(!mapping.link(3, 7));
    assert_eq!(mapping.len(), 1);
}

#[test]
fn lookups_on_unmapped_return_none() {
    let mut mapping = Mapping::new();
    mapping.link(1, 2);
    assert_eq!(mapping.get_dst(99), None);
    assert_eq!(mapping.get_src(99), None);
    assert!(!mapping.has_src(99));
    assert!(!mapping.has_dst(99));
}

#[test]
fn pairs_yields_every_link() {
    let mut mapping = Mapping::new();
    mapping.link(1, 10);
    mapping.link(2, 20);
    mapping.link(3, 30);
    let mut pairs = mapping.pairs();
    pairs.sort();
    assert_eq!(pairs, vec![(1, 10), (2, 20), (3, 30)]);
}

#[test]
fn supports_zero_node_ids() {
    let mut mapping = Mapping::new();
    assert!(mapping.link(0, 0));
    assert_eq!(mapping.get_dst(0), Some(0));
    assert_eq!(mapping.get_src(0), Some(0));
}

#[test]
fn clone_is_independent() {
    let mut mapping = Mapping::new();
    mapping.link(1, 10);
    let cloned = mapping.clone();
    mapping.link(2, 20);
    assert_eq!(cloned.len(), 1);
    assert_eq!(mapping.len(), 2);
}
