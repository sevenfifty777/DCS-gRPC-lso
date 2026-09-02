#[path = "build_support.rs"]
mod build_support;

use std::process::Command;

fn git(args: &[&str]) -> Option<std::process::Output> {
    Command::new("git").args(args).output().ok()
}

fn main() {
    // `dirty` deliberately covers tracked files only. Every tracked path plus
    // Git's index/HEAD refs is watched, so modifications, deletions, staging and
    // commits invalidate the value. Untracked files do not participate and are
    // intentionally not watched; this also prevents target/ rebuild loops.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_support.rs");
    if let Some(output) = git(&["ls-files", "-z"]).filter(|output| output.status.success()) {
        for path in build_support::tracked_paths(&output.stdout) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    for git_path in ["HEAD", "index"] {
        if let Some(output) =
            git(&["rev-parse", "--git-path", git_path]).filter(|output| output.status.success())
        {
            if let Ok(path) = String::from_utf8(output.stdout) {
                println!("cargo:rerun-if-changed={}", path.trim());
            }
        }
    }
    if let Some(output) =
        git(&["symbolic-ref", "-q", "HEAD"]).filter(|output| output.status.success())
    {
        if let Ok(reference) = String::from_utf8(output.stdout) {
            if let Some(output) = git(&["rev-parse", "--git-path", reference.trim()])
                .filter(|output| output.status.success())
            {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    println!("cargo:rerun-if-changed={}", path.trim());
                }
            }
        }
    }

    let commit = git(&["rev-parse", "HEAD"])
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = git(&["status", "--porcelain=v1", "--untracked-files=no"])
        .filter(|output| output.status.success())
        .is_some_and(|output| build_support::tracked_status_is_dirty(&output.stdout));

    println!("cargo:rustc-env=GIT_COMMIT_HASH={commit}");
    println!("cargo:rustc-env=GIT_DIRTY={dirty}");
}
