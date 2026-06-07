use gumtree_rs::languages::{profile_for_ext, profile_for_filename, supported_extensions};

#[test]
fn every_supported_extension_resolves_to_a_profile() {
    for ext in supported_extensions() {
        assert!(
            profile_for_ext(ext).is_some(),
            "supported_extensions() lists {:?} but profile_for_ext returns None",
            ext,
        );
    }
}

#[test]
fn profile_for_filename_handles_dockerfile_and_makefile() {
    assert!(profile_for_filename("Dockerfile").is_some());
    assert!(profile_for_filename("Makefile").is_some());
    assert!(profile_for_filename("GNUmakefile").is_some());
    assert!(profile_for_filename("Containerfile").is_some());
    assert!(profile_for_filename("random.txt").is_none());
}

#[test]
fn ambiguous_extensions_are_not_in_default_map() {
    assert!(profile_for_ext("conf").is_none(), ".conf is ambiguous");
    assert!(
        profile_for_ext("m").is_none(),
        ".m is ambiguous (MATLAB vs Objective-C)"
    );
}
