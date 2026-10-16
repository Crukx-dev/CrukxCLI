use crukx_core::deploy::runtimes::{
    ExecutionRequest, ExecutionResult, LlamaCppConfig, RuntimeAdapter, RuntimeError, RuntimeKind,
};
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

pub struct LlamaCppAdapter {
    pub config: LlamaCppConfig,
}

impl LlamaCppAdapter {
    pub fn new(config: LlamaCppConfig) -> Self {
        Self { config }
    }

    fn resolve_binary(&self) -> Result<String, RuntimeError> {
        self.config.validate()?;
        if self.config.binary.contains('/') {
            return Ok(self.config.binary.clone());
        }
        if let Some(found) = LlamaCppConfig::find_binary() {
            if found
                .rsplit('/')
                .next()
                .is_some_and(|name| name == self.config.binary)
            {
                return Ok(found);
            }
        }
        Ok(self.config.binary.clone())
    }

    fn truncate_output(text: String) -> String {
        if text.len() <= MAX_OUTPUT_BYTES {
            return text;
        }
        let mut truncated = text;
        truncated.truncate(MAX_OUTPUT_BYTES);
        truncated.push_str("\n[truncated]");
        truncated
    }

    fn spawn_with_busy_retry(
        binary: &str,
        args: &[String],
    ) -> Result<std::process::Child, RuntimeError> {
        let mut attempts = 0;
        loop {
            match Command::new(binary)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => return Ok(child),
                Err(e) if Self::is_txt_busy(&e) && attempts < 5 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(if e.kind() == std::io::ErrorKind::NotFound {
                        RuntimeError::NotFound(binary.to_string())
                    } else {
                        RuntimeError::ExecutionFailed(e.to_string())
                    });
                }
            }
        }
    }

    #[cfg(unix)]
    fn is_txt_busy(e: &std::io::Error) -> bool {
        e.raw_os_error() == Some(26)
    }

    #[cfg(not(unix))]
    fn is_txt_busy(_e: &std::io::Error) -> bool {
        false
    }

    fn output_with_busy_retry(binary: &str, args: &[String]) -> Option<std::process::Output> {
        let mut attempts = 0;
        loop {
            match Command::new(binary).args(args).output() {
                Ok(output) => return Some(output),
                Err(e) if Self::is_txt_busy(&e) && attempts < 5 => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn peak_rss_kb(pid: u32) -> Option<u64> {
        let text = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                return rest.split_whitespace().next()?.parse().ok();
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    fn peak_rss_kb(_pid: u32) -> Option<u64> {
        None
    }
}

impl RuntimeAdapter for LlamaCppAdapter {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::LlamaCpp
    }

    fn is_available(&self) -> bool {
        if self.config.binary.contains('/') {
            return std::path::Path::new(&self.config.binary).is_file();
        }
        LlamaCppConfig::find_binary().is_some()
    }

    fn version(&self) -> Option<String> {
        let binary = self.resolve_binary().ok()?;
        let output = Self::output_with_busy_retry(&binary, &["--version".to_string()])?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        LlamaCppConfig::parse_version(&combined)
    }

    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult, RuntimeError> {
        request.validate()?;
        let binary = self.resolve_binary()?;
        let args = self.config.argv(request);
        let mut child = Self::spawn_with_busy_retry(&binary, &args)?;

        let started = Instant::now();
        let timeout = Duration::from_secs(request.timeout_secs);
        let poll = Duration::from_millis(10);
        let pid = child.id();
        let mut peak_memory_kb: Option<u64> = None;
        loop {
            if let Some(rss) = Self::peak_rss_kb(pid) {
                peak_memory_kb = Some(peak_memory_kb.map_or(rss, |peak| peak.max(rss)));
            }
            match child
                .try_wait()
                .map_err(|e| RuntimeError::ExecutionFailed(e.to_string()))?
            {
                Some(status) => {
                    let mut stdout = String::new();
                    let mut stderr = String::new();
                    if let Some(mut out) = child.stdout.take() {
                        let _ = out.read_to_string(&mut stdout);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        let _ = err.read_to_string(&mut stderr);
                    }
                    let combined_log = format!("{}{}", stdout, stderr);
                    return Ok(ExecutionResult {
                        exit_code: status.code().unwrap_or(1),
                        stdout: Self::truncate_output(stdout),
                        stderr: Self::truncate_output(stderr),
                        duration_ms: started.elapsed().as_millis() as u64,
                        timed_out: false,
                        peak_memory_kb,
                        tok_s: LlamaCppConfig::parse_tok_s(&combined_log),
                    });
                }
                None => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(RuntimeError::Timeout);
                    }
                    std::thread::sleep(poll);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn script_fixture(body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir();
        let name = format!(
            "crukx-llama-test-{}-{}-{}.sh",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst),
            body.len()
        );
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{}", body)).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn request() -> ExecutionRequest {
        ExecutionRequest {
            model_path: "model.gguf".to_string(),
            prompt: "hello".to_string(),
            max_tokens: 8,
            timeout_secs: 10,
        }
    }

    #[test]
    fn missing_binary_reports_not_found() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig {
            binary: "/nonexistent/llama-cli".to_string(),
            ..LlamaCppConfig::default()
        });
        assert!(!adapter.is_available());
        assert_eq!(adapter.version(), None);
        let result = adapter.execute(&request());
        assert!(matches!(result, Err(RuntimeError::NotFound(_))));
    }

    #[test]
    fn invalid_request_is_rejected_before_spawning() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig::default());
        let mut bad = request();
        bad.max_tokens = 0;
        assert!(matches!(
            adapter.execute(&bad),
            Err(RuntimeError::InvalidConfig(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn script_stdout_is_captured_with_measured_duration() {
        let path = script_fixture("echo model-output\nexit 0\n");
        let adapter = LlamaCppAdapter::new(LlamaCppConfig {
            binary: path.to_string_lossy().to_string(),
            ..LlamaCppConfig::default()
        });
        let result = adapter.execute(&request()).unwrap();
        assert!(result.succeeded());
        assert!(result.stdout.contains("model-output"));
        assert!(!result.timed_out);
        std::fs::remove_file(&path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn hanging_process_hits_timeout() {
        let path = script_fixture("sleep 30\nexit 0\n");
        let adapter = LlamaCppAdapter::new(LlamaCppConfig {
            binary: path.to_string_lossy().to_string(),
            ..LlamaCppConfig::default()
        });
        let mut req = request();
        req.timeout_secs = 1;
        let result = adapter.execute(&req);
        assert!(matches!(result, Err(RuntimeError::Timeout)));
        std::fs::remove_file(&path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn version_parses_from_script_output() {
        let path = script_fixture("echo 'version: 9999 (test)'\nexit 0\n");
        let adapter = LlamaCppAdapter::new(LlamaCppConfig {
            binary: path.to_string_lossy().to_string(),
            ..LlamaCppConfig::default()
        });
        assert_eq!(adapter.version(), Some("9999".to_string()));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn live_binary_version_check_when_installed() {
        let adapter = LlamaCppAdapter::new(LlamaCppConfig::default());
        if !adapter.is_available() {
            return;
        }
        assert!(adapter.version().is_some());
    }
}
