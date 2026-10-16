use crukx_core::contracts::artifact::ArtifactManifest;
use crukx_core::deploy::exporters::sha256_hex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum ExportError {
    Io(String),
    InvalidInput(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Io(reason) => write!(f, "artifact export I/O failed: {}", reason),
            ExportError::InvalidInput(reason) => write!(f, "invalid export input: {}", reason),
        }
    }
}

impl std::error::Error for ExportError {}

#[derive(Debug, Clone)]
pub struct ExportedArtifact {
    pub dir: PathBuf,
    pub manifest_path: PathBuf,
    pub evidence_path: PathBuf,
    pub manifest_sha256: String,
}

pub fn write_artifact_package(
    manifest_json: &str,
    evidence_json: &str,
    out_dir: &Path,
) -> Result<ExportedArtifact, ExportError> {
    if manifest_json.is_empty() {
        return Err(ExportError::InvalidInput(
            "manifest_json must not be empty".to_string(),
        ));
    }
    if evidence_json.is_empty() {
        return Err(ExportError::InvalidInput(
            "evidence_json must not be empty".to_string(),
        ));
    }
    serde_json::from_str::<serde_json::Value>(manifest_json).map_err(|e| {
        ExportError::InvalidInput(format!("manifest_json is not valid JSON: {}", e))
    })?;
    serde_json::from_str::<serde_json::Value>(evidence_json).map_err(|e| {
        ExportError::InvalidInput(format!("evidence_json is not valid JSON: {}", e))
    })?;

    std::fs::create_dir_all(out_dir)
        .map_err(|e| ExportError::Io(format!("cannot create {}: {}", out_dir.display(), e)))?;
    let manifest_path = out_dir.join("manifest.json");
    let evidence_path = out_dir.join("evidence.json");
    std::fs::write(&manifest_path, manifest_json)
        .map_err(|e| ExportError::Io(format!("cannot write manifest: {}", e)))?;
    std::fs::write(&evidence_path, evidence_json)
        .map_err(|e| ExportError::Io(format!("cannot write evidence: {}", e)))?;

    Ok(ExportedArtifact {
        dir: out_dir.to_path_buf(),
        manifest_path,
        evidence_path,
        manifest_sha256: sha256_hex(manifest_json),
    })
}

pub fn verify_package(dir: &Path) -> Result<ArtifactManifest, ExportError> {
    let manifest_path = dir.join("manifest.json");
    let evidence_path = dir.join("evidence.json");
    if !evidence_path.is_file() {
        return Err(ExportError::InvalidInput(
            "evidence.json is missing from package".to_string(),
        ));
    }
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| ExportError::Io(format!("cannot read manifest: {}", e)))?;
    let mut manifest: ArtifactManifest = serde_json::from_str(&text).map_err(|e| {
        ExportError::InvalidInput(format!("manifest.json is not a manifest: {}", e))
    })?;
    let expected = manifest
        .artifact_hash
        .clone()
        .ok_or_else(|| ExportError::InvalidInput("manifest has no artifact_hash".to_string()))?;
    manifest.artifact_hash = None;
    let canonical = serde_json::to_string(&manifest).map_err(|e| {
        ExportError::InvalidInput(format!("manifest re-serialization failed: {}", e))
    })?;
    if sha256_hex(&canonical) != expected {
        return Err(ExportError::InvalidInput(
            "artifact_hash does not match manifest contents".to_string(),
        ));
    }
    manifest.artifact_hash = Some(expected);
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_writes_both_files() {
        let dir = std::env::temp_dir().join(format!("crukx-export-test-{}", std::process::id()));
        let exported = write_artifact_package(
            r#"{"model":"Qwen/Qwen3-4B"}"#,
            r#"{"protocol_version":1}"#,
            &dir,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&exported.manifest_path).unwrap(),
            r#"{"model":"Qwen/Qwen3-4B"}"#
        );
        assert_eq!(
            std::fs::read_to_string(&exported.evidence_path).unwrap(),
            r#"{"protocol_version":1}"#
        );
        assert_eq!(
            exported.manifest_sha256,
            sha256_hex(r#"{"model":"Qwen/Qwen3-4B"}"#)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_json_is_rejected() {
        let dir =
            std::env::temp_dir().join(format!("crukx-export-test-bad-{}", std::process::id()));
        let result = write_artifact_package("not json", "{}", &dir);
        assert!(matches!(result, Err(ExportError::InvalidInput(_))));
        assert!(!dir.exists());
    }

    #[test]
    fn written_package_verifies() {
        let dir =
            std::env::temp_dir().join(format!("crukx-export-test-vfy-{}", std::process::id()));
        let manifest =
            crukx_core::deploy::exporters::package(crukx_core::deploy::exporters::PackageInput {
                model: "M".to_string(),
                source_hash: "h".to_string(),
                workload_id: "w".to_string(),
                optimization: crukx_core::contracts::artifact::OptimizationRecord {
                    transform: "t".to_string(),
                    config: serde_json::json!({}),
                },
                verification: crukx_core::contracts::artifact::VerificationRecord {
                    suite: "s".to_string(),
                    critical_pass: true,
                    task_success_delta: None,
                    confidence: None,
                },
                device: crukx_core::deploy::devices::DeviceProfile::mock_cpu_only(),
                runtime: crukx_core::deploy::benchmarks::RuntimeRecord {
                    kind: crukx_core::deploy::runtimes::RuntimeKind::LlamaCpp,
                    name: "llama.cpp".to_string(),
                    version: None,
                },
                peak_memory_kb: None,
                throughput_tok_s: None,
                crukx_version: "2.0.0-alpha".to_string(),
            })
            .unwrap();
        let manifest_json = serde_json::to_string(&manifest).unwrap();
        write_artifact_package(&manifest_json, r#"{"v":1}"#, &dir).unwrap();
        let reopened = verify_package(&dir).unwrap();
        assert_eq!(reopened.artifact_hash, manifest.artifact_hash);
        std::fs::write(dir.join("manifest.json"), r#"{"tampered":true}"#).unwrap();
        assert!(matches!(
            verify_package(&dir),
            Err(ExportError::InvalidInput(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }
}
