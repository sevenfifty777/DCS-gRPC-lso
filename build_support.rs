pub fn tracked_paths(output: &[u8]) -> Vec<&str> {
    output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .filter_map(|path| std::str::from_utf8(path).ok())
        .filter(|path| !path.replace('\\', "/").starts_with("target/"))
        .collect()
}

pub fn tracked_status_is_dirty(output: &[u8]) -> bool {
    output.iter().any(|byte| !byte.is_ascii_whitespace())
}

/// The version Cargo resolved for the `dcs-grpc-stubs` dependency, read from `Cargo.lock`. The
/// lockfile is the only place that records the exact version behind a Git `tag =` pin, and
/// `--locked` guarantees it is the one actually compiled in, so a report can never claim a stub
/// version other than the one it was built with.
pub fn stubs_version_from_lock(lock: &str) -> Option<String> {
    let mut in_stubs_package = false;
    for line in lock.lines().map(str::trim) {
        if line == "[[package]]" {
            in_stubs_package = false;
            continue;
        }
        match line.strip_prefix("name = ") {
            Some(name) => in_stubs_package = name.trim_matches('"') == "dcs-grpc-stubs",
            None => {
                if in_stubs_package {
                    if let Some(version) = line.strip_prefix("version = ") {
                        return Some(version.trim_matches('"').to_string());
                    }
                }
            }
        }
    }
    None
}
