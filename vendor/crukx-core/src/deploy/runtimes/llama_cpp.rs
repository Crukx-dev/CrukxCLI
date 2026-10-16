use super::{ExecutionRequest, RuntimeError};
use crate::deploy::devices::DeviceProfile;
use serde::{Deserialize, Serialize};

pub const KNOWN_BINARIES: &[&str] = &["llama-cli", "main"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct LlamaCppConfig {
    pub binary: String,
    pub n_ctx: u32,
    pub n_threads: u32,
    #[serde(default)]
    pub extra_args: Vec<String>,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            binary: "llama-cli".to_string(),
            n_ctx: 4096,
            n_threads: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(4),
            extra_args: Vec::new(),
        }
    }
}

impl LlamaCppConfig {
    pub fn from_profile(profile: &DeviceProfile) -> Self {
        Self {
            n_threads: profile.cpu.logical_cores.max(1),
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.binary.is_empty() {
            return Err(RuntimeError::InvalidConfig(
                "binary must not be empty".to_string(),
            ));
        }
        if self.n_ctx == 0 {
            return Err(RuntimeError::InvalidConfig(
                "n_ctx must be greater than zero".to_string(),
            ));
        }
        if self.n_threads == 0 {
            return Err(RuntimeError::InvalidConfig(
                "n_threads must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }

    pub fn argv(&self, request: &ExecutionRequest) -> Vec<String> {
        let mut args = vec![
            "-m".to_string(),
            request.model_path.clone(),
            "-p".to_string(),
            request.prompt.clone(),
            "-n".to_string(),
            request.max_tokens.to_string(),
            "-c".to_string(),
            self.n_ctx.to_string(),
            "-t".to_string(),
            self.n_threads.to_string(),
        ];
        args.extend(self.extra_args.iter().cloned());
        args
    }

    pub fn parse_tok_s(output: &str) -> Option<f64> {
        let mut last = None;
        for line in output.lines() {
            if !line.contains("tokens per second") {
                continue;
            }
            let before = line.split("tokens per second").next()?;
            let token = before
                .rsplit(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .find(|s| !s.is_empty())?;
            if let Ok(value) = token.parse::<f64>() {
                if value.is_finite() && value > 0.0 {
                    last = Some(value);
                }
            }
        }
        last
    }

    pub fn parse_version(output: &str) -> Option<String> {
        for line in output.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("version:") {
                let token = rest.split_whitespace().next()?;
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
            if line.starts_with("llama") && line.contains("version") {
                let token = line.split_whitespace().last()?;
                if !token.is_empty() {
                    return Some(token.trim_matches(|c| c == '(' || c == ')').to_string());
                }
            }
        }
        None
    }

    pub fn find_binary() -> Option<String> {
        let path = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path) {
            for name in KNOWN_BINARIES {
                let candidate = dir.join(name);
                if is_executable(&candidate) {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
        None
    }
}

fn is_executable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ExecutionRequest {
        ExecutionRequest {
            model_path: "model.gguf".to_string(),
            prompt: "hello".to_string(),
            max_tokens: 32,
            timeout_secs: 60,
        }
    }

    #[test]
    fn request_validation() {
        assert!(request().validate().is_ok());
        let mut bad = request();
        bad.max_tokens = 0;
        assert!(matches!(
            bad.validate(),
            Err(RuntimeError::InvalidConfig(_))
        ));
        let mut bad = request();
        bad.model_path.clear();
        assert!(matches!(
            bad.validate(),
            Err(RuntimeError::InvalidConfig(_))
        ));
    }

    #[test]
    fn config_validation() {
        let good = LlamaCppConfig::default();
        assert!(good.validate().is_ok());
        let bad = LlamaCppConfig {
            n_ctx: 0,
            ..LlamaCppConfig::default()
        };
        assert!(matches!(
            bad.validate(),
            Err(RuntimeError::InvalidConfig(_))
        ));
    }

    #[test]
    fn argv_shape() {
        let config = LlamaCppConfig::default();
        let args = config.argv(&request());
        assert_eq!(
            &args[0..9],
            &[
                "-m",
                "model.gguf",
                "-p",
                "hello",
                "-n",
                "32",
                "-c",
                "4096",
                "-t"
            ]
        );
        assert_eq!(args[9], config.n_threads.to_string());
    }

    #[test]
    fn tok_s_parsing_prefers_last_eval_line() {
        let log = "prompt eval time =     100.00 ms /     8 runs (   12.50 ms per token,    80.00 tokens per second)\neval time =     200.00 ms /    16 runs (   12.50 ms per token,    51.20 tokens per second)\n";
        assert_eq!(LlamaCppConfig::parse_tok_s(log), Some(51.2));
    }

    #[test]
    fn tok_s_absent_without_perf_lines() {
        assert_eq!(LlamaCppConfig::parse_tok_s("hello world\n"), None);
        assert_eq!(LlamaCppConfig::parse_tok_s("tokens per second: lots"), None);
    }

    #[test]
    fn version_parsing() {
        assert_eq!(
            LlamaCppConfig::parse_version("version: 1234 (build 5678 with Apple clang)"),
            Some("1234".to_string())
        );
        assert_eq!(
            LlamaCppConfig::parse_version("  version: b6070  "),
            Some("b6070".to_string())
        );
        assert_eq!(
            LlamaCppConfig::parse_version("garbage\nno version here"),
            None
        );
    }

    #[test]
    fn from_profile_uses_cpu_cores() {
        let profile = DeviceProfile::mock_metal();
        let config = LlamaCppConfig::from_profile(&profile);
        assert_eq!(config.n_threads, 10);
        assert!(config.validate().is_ok());
    }
}
