use crate::contracts::artifact::{
    ArtifactManifest, DeploymentRecord, OptimizationRecord, VerificationRecord,
};
use crate::deploy::benchmarks::RuntimeRecord;
use crate::deploy::devices::DeviceProfile;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub enum PackageError {
    MissingField(String),
    InvalidInput(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageError::MissingField(field) => write!(f, "missing required field: {}", field),
            PackageError::InvalidInput(reason) => write!(f, "invalid package input: {}", reason),
        }
    }
}

impl std::error::Error for PackageError {}

pub struct PackageInput {
    pub model: String,
    pub source_hash: String,
    pub workload_id: String,
    pub optimization: OptimizationRecord,
    pub verification: VerificationRecord,
    pub device: DeviceProfile,
    pub runtime: RuntimeRecord,
    pub peak_memory_kb: Option<u64>,
    pub throughput_tok_s: Option<f64>,
    pub crukx_version: String,
}

pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn target_label(device: &DeviceProfile) -> String {
    let memory_gb = device
        .memory
        .total_mb
        .map(|mb| format!("-{}gb", mb / 1024))
        .unwrap_or_default();
    format!("{}-{}{}", device.os.name, device.os.arch, memory_gb)
}

pub fn package(input: PackageInput) -> Result<ArtifactManifest, PackageError> {
    if input.model.is_empty() {
        return Err(PackageError::MissingField("model".to_string()));
    }
    if input.source_hash.is_empty() {
        return Err(PackageError::MissingField("source_hash".to_string()));
    }
    if input.workload_id.is_empty() {
        return Err(PackageError::MissingField("workload_id".to_string()));
    }
    if input.crukx_version.is_empty() {
        return Err(PackageError::MissingField("crukx_version".to_string()));
    }
    if input.optimization.transform.is_empty() {
        return Err(PackageError::InvalidInput(
            "optimization.transform must not be empty".to_string(),
        ));
    }
    if input.verification.suite.is_empty() {
        return Err(PackageError::InvalidInput(
            "verification.suite must not be empty".to_string(),
        ));
    }

    let mut manifest = ArtifactManifest::new(
        input.model,
        input.source_hash,
        input.workload_id,
        input.crukx_version,
    );
    manifest.optimization = Some(input.optimization);
    manifest.verification = Some(input.verification);
    manifest.deployment = Some(DeploymentRecord {
        target: target_label(&input.device),
        runtime: input.runtime.name.clone(),
        tok_s: input.throughput_tok_s,
        peak_memory_mb: input.peak_memory_kb.map(|kb| kb / 1024),
    });

    let canonical = serde_json::to_string(&manifest)
        .map_err(|e| PackageError::InvalidInput(format!("manifest serialization failed: {}", e)))?;
    manifest.artifact_hash = Some(sha256_hex(&canonical));
    Ok(manifest)
}

pub fn is_releasable(manifest: &ArtifactManifest) -> bool {
    manifest.is_verified()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PackageInput {
        PackageInput {
            model: "Qwen/Qwen3-4B".to_string(),
            source_hash: "sha256:abc".to_string(),
            workload_id: "support-agent-v4".to_string(),
            optimization: OptimizationRecord {
                transform: "gguf_quantize".to_string(),
                config: serde_json::json!({"type": "Q4_K_M"}),
            },
            verification: VerificationRecord {
                suite: "prod-golden-2026-09".to_string(),
                critical_pass: true,
                task_success_delta: Some(-0.0005),
                confidence: Some(0.95),
            },
            device: DeviceProfile::mock_metal(),
            runtime: RuntimeRecord {
                kind: crate::deploy::runtimes::RuntimeKind::LlamaCpp,
                name: "llama.cpp-metal".to_string(),
                version: Some("1234".to_string()),
            },
            peak_memory_kb: Some(4860 * 1024),
            throughput_tok_s: Some(51.2),
            crukx_version: "2.0.0-alpha".to_string(),
        }
    }

    #[test]
    fn package_happy_path() {
        let manifest = package(input()).unwrap();
        assert_eq!(manifest.model, "Qwen/Qwen3-4B");
        assert_eq!(manifest.deployment.as_ref().unwrap().tok_s, Some(51.2));
        assert_eq!(
            manifest.deployment.as_ref().unwrap().peak_memory_mb,
            Some(4860)
        );
        assert_eq!(
            manifest.deployment.as_ref().unwrap().target,
            "macos-aarch64-24gb"
        );
        assert!(is_releasable(&manifest));
        let hash = manifest.artifact_hash.clone().unwrap();
        let mut check = manifest.clone();
        check.artifact_hash = None;
        assert_eq!(sha256_hex(&serde_json::to_string(&check).unwrap()), hash);
    }

    #[test]
    fn rejection_is_preserved_never_flipped() {
        let mut rejected = input();
        rejected.verification.critical_pass = false;
        let manifest = package(rejected).unwrap();
        assert!(!manifest.verification.as_ref().unwrap().critical_pass);
        assert!(!is_releasable(&manifest));
    }

    #[test]
    fn missing_fields_error() {
        let mut bad = input();
        bad.model.clear();
        assert!(matches!(package(bad), Err(PackageError::MissingField(_))));
    }

    #[test]
    fn round_trips() {
        let manifest = package(input()).unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        let back: ArtifactManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, manifest);
    }
}
