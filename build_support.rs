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
