//! `crukx review` launches an interactive TUI that takes over the
//! terminal — not something `assert_cmd` can drive. The one path that
//! runs and exits before the TUI takes over (no session found) is what's
//! covered here; `crukx-tui`'s actual rendering/navigation logic has its
//! own unit + TestBackend snapshot tests in crates/crukx-tui/src.

use assert_cmd::Command;

#[test]
fn review_reports_a_clear_error_and_exits_when_no_sessions_exist() {
    let dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("crukx").unwrap();
    let output = cmd
        .current_dir(dir.path())
        .args(["review", "latest"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no Crukx sessions found"), "{stderr}");
}
