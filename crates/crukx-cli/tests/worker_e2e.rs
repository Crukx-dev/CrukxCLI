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

/// Minimal HTTP server for claim/patch behaviour — binds an ephemeral port
/// and replays canned `(status, body)` responses, one per request.
fn spawn_mock(responses: Vec<(u16, String)>) -> String {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Drain the full request (headers + Content-Length body) before
            // answering: replying while request bytes are still in flight
            // turns the close into an abortive reset on Windows, and the
            // client then reports "connection forcibly closed" instead of
            // ever seeing our response.
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            let body_start = loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break request.len();
                }
                request.extend_from_slice(&chunk[..n]);
                if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let content_length = String::from_utf8_lossy(&request[..body_start])
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .next()
                .unwrap_or(0);
            while request.len() < body_start + content_length {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }

            let response = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            // Clean FIN, not an RST: the client must be able to read the
            // response before the socket goes away.
            let _ = stream.shutdown(Shutdown::Write);
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
