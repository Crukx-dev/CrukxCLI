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
    assert!(
        stderr.contains("missing API key"),
        "unexpected stderr:\n{stderr}"
    );
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

/// Minimal one-shot-per-response HTTP server for claim/patch behaviour —
/// binds an ephemeral port and replays canned `(status, body)` responses.
fn spawn_mock(responses: Vec<(u16, String)>) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

#[test]
fn worker_rejects_a_non_job_claim_body() {
    // A 200 whose body is not a job (error envelope, proxy page) must not be
    // processed as claimed work.
    let url = spawn_mock(vec![(200, r#"{"error":"not a job"}"#.to_string())]);
    let output = run(&["worker", "--api-url", &url, "--api-key", "test", "--once"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("claim failed"),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn worker_rejects_an_unexpected_status_as_a_job() {
    let job = r#"{"job_id":"job-1","payload":{"model":"m.gguf"}}"#;
    let url = spawn_mock(vec![(201, job.to_string())]);
    let output = run(&["worker", "--api-url", &url, "--api-key", "test", "--once"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected http status 201"),
        "unexpected stderr:\n{stderr}"
    );
}

#[test]
fn worker_claims_runs_and_reports_a_real_job() {
    let job = r#"{"job_id":"job-1","kind":"benchmark","state":"running","payload":{"model":"/nonexistent/model.gguf"},"result":null,"artifact_id":null,"created_at":"2026-10-21T00:00:00Z","updated_at":"2026-10-21T00:00:00Z"}"#;
    let url = spawn_mock(vec![(200, job.to_string()), (200, "{}".to_string())]);
    let output = run(&["worker", "--api-url", &url, "--api-key", "test", "--once"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("job job-1 -> failed"),
        "unexpected stdout:\n{stdout}"
    );
}
