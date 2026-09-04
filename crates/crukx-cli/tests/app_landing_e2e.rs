//! E2E for the "open the app" surfaces: bare `crukx` in a project with
//! state is a dashboard, bare `crukx` in a fresh directory is onboarding
//! (exit 2), `crukx doctor` is a pass/fail checklist, and completions
//! generate. All must stay script-safe: no panics, sane exit codes,
//! plain text without a tty.

use assert_cmd::Command;
use std::fs;

#[test]
fn bare_crukx_in_project_with_state_shows_dashboard_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".crukx/contracts")).unwrap();
    fs::write(dir.path().join(".crukx/contracts/demo.json"), "{}").unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "dashboard exit 0: {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("◆ Crukx ·"), "{stdout}");
    assert!(stdout.contains("1 contract"), "{stdout}");
    assert!(stdout.contains("crukx doctor"), "{stdout}");
}

#[test]
fn bare_crukx_in_fresh_directory_is_onboarding_exit_two() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "fresh dir keeps exit 2 contract");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Release gate for AI-written software"), "{stdout}");
    assert!(stdout.contains("Commands:"), "help still prints: {stdout}");
}

#[test]
fn doctor_reports_checks_and_exits_zero_on_healthy_machine() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("crukx")
        .unwrap()
        .current_dir(dir.path())
        .args(["doctor"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✓ crukx binary"), "{stdout}");
    assert!(stdout.contains("state dir"), "{stdout}");
    assert!(
        stdout.contains("Crukx doctor: all critical checks passed"),
        "{stdout}"
    );
    assert!(output.status.success(), "{stdout}");
}

#[test]
fn completion_emits_scripts_for_supported_shells() {
    for shell in ["bash", "zsh", "fish"] {
        let output = Command::cargo_bin("crukx")
            .unwrap()
            .args(["completion", "--shell", shell])
            .output()
            .unwrap();
        assert!(output.status.success(), "{shell}: {output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("crukx"), "{shell} script: {stdout}");
    }
}
