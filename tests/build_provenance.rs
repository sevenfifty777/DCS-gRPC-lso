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
fn stubs_version_is_read_from_the_lockfile_package_block() {
    let lock = r#"
[[package]]
name = "dcs-grpc-lso-unrelated"
version = "9.9.9"

[[package]]
name = "dcs-grpc-stubs"
version = "0.10.0"
source = "git+https://github.com/sevenfifty777/rust-server.git?tag=v0.10.0#abc"
dependencies = [
 "prost",
]

[[package]]
name = "prost"
version = "0.13.5"
"#;
    assert_eq!(
        build_support::stubs_version_from_lock(lock).as_deref(),
        Some("0.10.0")
    );
    assert_eq!(build_support::stubs_version_from_lock(""), None);
    // The version line must belong to the stubs package, not to a neighbour.
    let wrong_block = "[[package]]\nname = \"dcs-grpc-stubs\"\n\n[[package]]\nname = \"prost\"\nversion = \"0.13.5\"\n";
    assert_eq!(build_support::stubs_version_from_lock(wrong_block), None);
}

#[test]
fn dirty_parser_uses_tracked_status_output_only() {
    assert!(!build_support::tracked_status_is_dirty(b""));
    assert!(!build_support::tracked_status_is_dirty(b"\n"));
    assert!(build_support::tracked_status_is_dirty(b" M src/main.rs\n"));
    assert!(build_support::tracked_status_is_dirty(b" D docs/old.md\n"));
}
