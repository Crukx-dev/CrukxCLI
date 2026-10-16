use serde::{Deserialize, Serialize};
use std::fmt;

pub mod llama_cpp;
pub use llama_cpp::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    LlamaCpp,
    Onnx,
    Executorch,
    TensorRt,
    CoreMl,
}

impl RuntimeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeKind::LlamaCpp => "llama.cpp",
            RuntimeKind::Onnx => "onnx",
            RuntimeKind::Executorch => "executorch",
            RuntimeKind::TensorRt => "tensorrt",
            RuntimeKind::CoreMl => "coreml",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeInfo {
    pub kind: RuntimeKind,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionRequest {
    pub model_path: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl ExecutionRequest {
    pub fn validate(&self) -> Result<(), RuntimeError> {
        if self.model_path.is_empty() {
            return Err(RuntimeError::InvalidConfig(
                "model_path must not be empty".to_string(),
            ));
        }
        if self.prompt.is_empty() {
            return Err(RuntimeError::InvalidConfig(
                "prompt must not be empty".to_string(),
            ));
        }
        if self.max_tokens == 0 {
            return Err(RuntimeError::InvalidConfig(
                "max_tokens must be greater than zero".to_string(),
            ));
        }
        if self.timeout_secs == 0 {
            return Err(RuntimeError::InvalidConfig(
                "timeout_secs must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
    #[serde(default)]
    pub peak_memory_kb: Option<u64>,
    #[serde(default)]
    pub tok_s: Option<f64>,
}

impl ExecutionResult {
    pub fn succeeded(&self) -> bool {
        !self.timed_out && self.exit_code == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    NotFound(String),
    NotSupported(String),
    ExecutionFailed(String),
    Timeout,
    InvalidConfig(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::NotFound(binary) => write!(f, "runtime binary not found: {}", binary),
            RuntimeError::NotSupported(reason) => write!(f, "runtime not supported: {}", reason),
            RuntimeError::ExecutionFailed(reason) => {
                write!(f, "runtime execution failed: {}", reason)
            }
            RuntimeError::Timeout => write!(f, "runtime execution timed out"),
            RuntimeError::InvalidConfig(reason) => write!(f, "invalid runtime config: {}", reason),
        }
    }
}

impl std::error::Error for RuntimeError {}

pub trait RuntimeAdapter {
    fn kind(&self) -> RuntimeKind;
    fn is_available(&self) -> bool;
    fn version(&self) -> Option<String>;
    fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult, RuntimeError>;
}
