use serde::{Deserialize, Serialize};

pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OptimizationRecord {
    pub transform: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct VerificationRecord {
    pub suite: String,
    pub critical_pass: bool,
    #[serde(default)]
    pub task_success_delta: Option<f64>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DeploymentRecord {
    pub target: String,
    pub runtime: String,
    #[serde(default)]
    pub tok_s: Option<f64>,
    #[serde(default)]
    pub peak_memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub model: String,
    pub source_hash: String,
    pub workload_id: String,
    #[serde(default)]
    pub optimization: Option<OptimizationRecord>,
    #[serde(default)]
    pub verification: Option<VerificationRecord>,
    #[serde(default)]
    pub deployment: Option<DeploymentRecord>,
    #[serde(default)]
    pub artifact_hash: Option<String>,
    pub crukx_version: String,
}

impl ArtifactManifest {
    pub fn new(
        model: String,
        source_hash: String,
        workload_id: String,
        crukx_version: String,
    ) -> Self {
        Self {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            model,
            source_hash,
            workload_id,
            optimization: None,
            verification: None,
            deployment: None,
            artifact_hash: None,
            crukx_version,
        }
    }

    pub fn is_verified(&self) -> bool {
        self.verification.as_ref().is_some_and(|v| v.critical_pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let mut m = ArtifactManifest::new(
            "Qwen/Qwen3-4B".to_string(),
            "sha256:abc".to_string(),
            "support-agent-v4".to_string(),
            "2.0.0-alpha".to_string(),
        );
        m.deployment = Some(DeploymentRecord {
            target: "macbook-m4-24gb".to_string(),
            runtime: "llama.cpp-metal".to_string(),
            tok_s: Some(51.2),
            peak_memory_mb: Some(4860),
        });
        let json = serde_json::to_string(&m).unwrap();
        let back: ArtifactManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "Qwen/Qwen3-4B");
        assert_eq!(back.deployment.unwrap().tok_s, Some(51.2));
    }

    #[test]
    fn unverified_manifest_reports_false() {
        let m = ArtifactManifest::new(
            "m".to_string(),
            "h".to_string(),
            "w".to_string(),
            "2.0.0-alpha".to_string(),
        );
        assert!(!m.is_verified());
    }
}
