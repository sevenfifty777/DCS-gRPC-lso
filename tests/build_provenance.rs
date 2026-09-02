#[path = "../build_support.rs"]
mod build_support;

#[test]
fn tracked_path_parser_handles_spaces_deletions_and_excludes_target() {
    let paths = build_support::tracked_paths(
        b"src/main.rs\0docs/file with spaces.md\0target/accidentally-tracked\0",
    );
    assert_eq!(paths, ["src/main.rs", "docs/file with spaces.md"]);
}

#[test]
fn dirty_parser_uses_tracked_status_output_only() {
    assert!(!build_support::tracked_status_is_dirty(b""));
    assert!(!build_support::tracked_status_is_dirty(b"\n"));
    assert!(build_support::tracked_status_is_dirty(b" M src/main.rs\n"));
    assert!(build_support::tracked_status_is_dirty(b" D docs/old.md\n"));
}
