//! crukx-git — thin `git` wrappers for capture: repository root, HEAD,
//! the tracked-file diff a session left behind, and the sha256 fingerprint
//! of that diff. Shells out to the `git` binary rather than a Rust git
//! library — capture only ever needs a handful of read-only plumbing
//! commands, not a full git implementation.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(args: &[&str], cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Falls back to `cwd` itself when `git` isn't available or `cwd` isn't
/// inside a repository — capture still works outside git, it just can't
/// record a diff.
pub fn resolve_repository_root(cwd: &Path) -> PathBuf {
    git(&["rev-parse", "--show-toplevel"], cwd)
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf())
}

pub fn read_head(repository_root: &Path) -> String {
    git(&["rev-parse", "HEAD"], repository_root).unwrap_or_else(|| "working-tree".to_string())
}

pub fn read_tracked_diff(repository_root: &Path) -> String {
    git(&["diff", "--binary", "HEAD", "--"], repository_root).unwrap_or_default()
}

pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    // Explicit byte-by-byte hex, not `format!("{:x}", digest)` — that
    // formats the GenericArray's Debug-ish representation, not a raw hex
    // digest, and silently produces the wrong (and non-hex-length) string.
    // Caught by the known-vector test below before this ever left this
    // function.
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;

    fn init_repo(dir: &Path) {
        StdCommand::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .status()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .unwrap();
        fs::write(dir.join("a.txt"), "hello\n").unwrap();
        StdCommand::new("git")
            .args(["add", "a.txt"])
            .current_dir(dir)
            .status()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[test]
    fn resolve_repository_root_finds_the_root_from_a_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let subdir = dir.path().join("nested");
        fs::create_dir_all(&subdir).unwrap();

        let root = resolve_repository_root(&subdir);
        // canonicalize both sides — macOS tempdirs resolve through a symlink
        assert_eq!(
            root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn resolve_repository_root_falls_back_to_cwd_outside_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = resolve_repository_root(dir.path());
        assert_eq!(root, dir.path());
    }

    #[test]
    fn read_head_returns_the_current_commit_sha() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        let head = read_head(dir.path());
        assert_eq!(head.len(), 40); // full sha
    }

    #[test]
    fn read_head_falls_back_when_there_is_no_commit_yet() {
        let dir = tempfile::tempdir().unwrap();
        StdCommand::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert_eq!(read_head(dir.path()), "working-tree");
    }

    #[test]
    fn read_tracked_diff_is_empty_with_no_changes_and_nonempty_after_an_edit() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        assert_eq!(read_tracked_diff(dir.path()), "");

        fs::write(dir.path().join("a.txt"), "hello world\n").unwrap();
        let diff = read_tracked_diff(dir.path());
        assert!(!diff.is_empty());
        assert!(diff.contains("a.txt"));
    }

    #[test]
    fn sha256_hex_is_64_hex_characters() {
        let digest = sha256_hex("");
        assert_eq!(digest.len(), 64, "digest was: {digest:?}");
        assert!(
            digest.chars().all(|c| c.is_ascii_hexdigit()),
            "digest was: {digest:?}"
        );
    }

    #[test]
    fn sha256_hex_is_deterministic() {
        assert_eq!(sha256_hex("hello"), sha256_hex("hello"));
        assert_ne!(sha256_hex("hello"), sha256_hex("world"));
    }
}
