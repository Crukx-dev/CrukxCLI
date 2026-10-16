use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEVICE_PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CpuInfo {
    pub arch: String,
    pub logical_cores: u32,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct GpuInfo {
    pub vendor: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NpuInfo {
    pub vendor: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MemoryInfo {
    #[serde(default)]
    pub total_mb: Option<u64>,
    #[serde(default)]
    pub available_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OsInfo {
    pub name: String,
    pub arch: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeviceProfile {
    pub schema_version: u32,
    pub device_id: String,
    pub cpu: CpuInfo,
    #[serde(default)]
    pub gpus: Vec<GpuInfo>,
    #[serde(default)]
    pub npu: Option<NpuInfo>,
    pub memory: MemoryInfo,
    pub os: OsInfo,
    #[serde(default)]
    pub runtime_support: Vec<String>,
    #[serde(default)]
    pub precision_support: Vec<String>,
    #[serde(default)]
    pub max_model_bytes: Option<u64>,
}

impl DeviceProfile {
    pub fn new(cpu: CpuInfo, memory: MemoryInfo, os: OsInfo) -> Self {
        let mut profile = Self {
            schema_version: DEVICE_PROFILE_SCHEMA_VERSION,
            device_id: String::new(),
            cpu,
            gpus: Vec::new(),
            npu: None,
            memory,
            os,
            runtime_support: Vec::new(),
            precision_support: Vec::new(),
            max_model_bytes: None,
        };
        profile.refresh_id();
        profile
    }

    pub fn refresh_id(&mut self) {
        self.device_id = self.compute_id();
    }

    pub fn compute_id(&self) -> String {
        let total_gb = self
            .memory
            .total_mb
            .map(|mb| (mb / 1024).to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let mut gpu_part: Vec<String> = self
            .gpus
            .iter()
            .map(|g| format!("{}:{}", g.vendor, g.model.as_deref().unwrap_or("unknown")))
            .collect();
        gpu_part.sort();
        let input = format!(
            "v1|{}|{}|{}|{}|{}|{}",
            self.cpu.arch,
            self.cpu.model.as_deref().unwrap_or("unknown"),
            self.cpu.logical_cores,
            total_gb,
            self.os.name,
            gpu_part.join(","),
        );
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn supports_runtime(&self, runtime: &str) -> bool {
        self.runtime_support.iter().any(|r| r == runtime)
    }

    pub fn supports_precision(&self, precision: &str) -> bool {
        self.precision_support.iter().any(|p| p == precision)
    }

    pub fn mock_cpu_only() -> Self {
        let mut profile = Self::new(
            CpuInfo {
                arch: "x86_64".to_string(),
                logical_cores: 8,
                model: Some("Mock CPU".to_string()),
            },
            MemoryInfo {
                total_mb: Some(16384),
                available_mb: Some(8192),
            },
            OsInfo {
                name: "linux".to_string(),
                arch: "x86_64".to_string(),
                version: None,
            },
        );
        profile.runtime_support = vec!["llama.cpp".to_string(), "onnx".to_string()];
        profile.precision_support = vec![
            "fp32".to_string(),
            "fp16".to_string(),
            "int8".to_string(),
            "int4".to_string(),
        ];
        profile.max_model_bytes = Some(4 * 1024 * 1024 * 1024);
        profile.refresh_id();
        profile
    }

    pub fn mock_metal() -> Self {
        let mut profile = Self::new(
            CpuInfo {
                arch: "aarch64".to_string(),
                logical_cores: 10,
                model: Some("Apple M4".to_string()),
            },
            MemoryInfo {
                total_mb: Some(24576),
                available_mb: Some(16384),
            },
            OsInfo {
                name: "macos".to_string(),
                arch: "aarch64".to_string(),
                version: None,
            },
        );
        profile.gpus = vec![GpuInfo {
            vendor: "apple".to_string(),
            model: Some("Apple M4".to_string()),
            memory_mb: None,
        }];
        profile.runtime_support = vec![
            "llama.cpp".to_string(),
            "onnx".to_string(),
            "coreml".to_string(),
        ];
        profile.precision_support = vec![
            "fp32".to_string(),
            "fp16".to_string(),
            "int8".to_string(),
            "int4".to_string(),
        ];
        profile.max_model_bytes = Some(8 * 1024 * 1024 * 1024);
        profile.refresh_id();
        profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_round_trips() {
        let profile = DeviceProfile::mock_metal();
        let json = serde_json::to_string(&profile).unwrap();
        let back: DeviceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, profile);
    }

    #[test]
    fn device_id_is_stable_hex() {
        let a = DeviceProfile::mock_cpu_only();
        let b = DeviceProfile::mock_cpu_only();
        assert_eq!(a.device_id, b.device_id);
        assert_eq!(a.device_id.len(), 64);
    }

    #[test]
    fn device_id_differs_across_devices() {
        let a = DeviceProfile::mock_cpu_only();
        let b = DeviceProfile::mock_metal();
        assert_ne!(a.device_id, b.device_id);
    }

    #[test]
    fn available_memory_does_not_change_id() {
        let mut a = DeviceProfile::mock_cpu_only();
        let mut b = DeviceProfile::mock_cpu_only();
        b.memory.available_mb = Some(1024);
        b.refresh_id();
        assert_eq!(a.device_id, b.device_id);
        a.gpus.push(GpuInfo {
            vendor: "nvidia".to_string(),
            model: None,
            memory_mb: None,
        });
        a.refresh_id();
        assert_ne!(a.device_id, b.device_id);
    }

    #[test]
    fn capability_helpers() {
        let profile = DeviceProfile::mock_metal();
        assert!(profile.supports_runtime("coreml"));
        assert!(!profile.supports_runtime("tensorrt"));
        assert!(profile.supports_precision("int4"));
        assert!(!profile.supports_precision("fp64"));
    }
}
