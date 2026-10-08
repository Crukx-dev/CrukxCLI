use assert_cmd::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::cargo_bin("crukx")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn worker_help_names_poll_loop() {
    let output = run(&["worker", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--api-url"), "unexpected help:\n{stdout}");
}

#[test]
fn worker_without_key_exits_2() {
    let output = run(&["worker", "--api-url", "http://127.0.0.1:9"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing API key"), "unexpected stderr:\n{stderr}");
}

#[test]
fn worker_unreachable_plane_exits_1() {
    let output = run(&[
        "worker",
        "--api-url",
        "http://127.0.0.1:9",
        "--api-key",
        "test",
        "--once",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("claim failed"),
        "unexpected stderr:\n{stderr}"
    );
}
