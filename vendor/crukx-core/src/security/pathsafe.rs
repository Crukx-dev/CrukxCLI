//! Guards the join points where Crukx builds a filesystem path out of a
//! string it didn't generate itself — a contract id from `crukx.yml` or a
//! `crukx replay <id>` CLI arg, a `contract.assertion_file` value read
//! back out of a contract JSON. `PathBuf::join` does not stop a segment
//! from containing `../` (walks out of the intended directory) or being
//! an absolute path (replaces the base directory entirely on Unix and
//! Windows both) — every caller that joins one of these onto a fixed base
//! directory must check `is_safe_path_segment` first, or a maliciously
//! crafted id/filename reads, writes, or (via `crukx replay`'s
//! `node <assertion_file>`) executes outside `.crukx/`.

/// Whether `s` is safe to use as a single path segment: non-empty, not
/// `.`/`..`, and free of path separators or a NUL byte. Deliberately
/// conservative — a real contract id or assertion filename never needs a
/// `/`, so refusing one is never a false-positive rejection of a
/// legitimate value, only ever a caught traversal attempt.
pub fn is_safe_path_segment(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_ids_and_filenames() {
        assert!(is_safe_path_segment("demo-1"));
        assert!(is_safe_path_segment("6f9299f1-e670-4001-ae21-30dc482af744"));
        assert!(is_safe_path_segment("assert.mjs"));
    }

    #[test]
    fn refuses_traversal_and_absolute_paths() {
        assert!(!is_safe_path_segment("../../etc/passwd"));
        assert!(!is_safe_path_segment("..\\..\\windows\\system32"));
        assert!(!is_safe_path_segment("/etc/passwd"));
        assert!(!is_safe_path_segment("a/../../b"));
        assert!(!is_safe_path_segment(".."));
        assert!(!is_safe_path_segment("."));
    }

    #[test]
    fn refuses_empty_and_nul_byte() {
        assert!(!is_safe_path_segment(""));
        assert!(!is_safe_path_segment("evil\0.json"));
    }
}
