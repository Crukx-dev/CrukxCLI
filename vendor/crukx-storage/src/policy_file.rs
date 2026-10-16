use crate::json::StorageError;
use crukx_core::policy::PolicyFile;
use std::fs;
use std::path::Path;

/// Loads `crukx.yml` — the one file in Crukx's state that's YAML, not
/// JSON, because it's meant to be hand-edited.
pub fn load_policy(path: &Path) -> Result<PolicyFile, StorageError> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_policy_parses_contracts_and_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crukx.yml");
        let mut file = fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"
contracts:
  - id: demo-1
    mode: block
    candidate_label: ci
constraints:
  vtr_minimum: 0.98
  security_critical_delta_max: 0
"#
        )
        .unwrap();

        let policy = load_policy(&path).unwrap();
        assert_eq!(policy.contracts.len(), 1);
        assert_eq!(policy.contracts[0].id, "demo-1");
        assert_eq!(policy.constraints.unwrap().vtr_minimum, Some(0.98));
    }

    #[test]
    fn load_policy_surfaces_a_yaml_error_for_malformed_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yml");
        fs::write(&path, "contracts: [this is not valid: yaml: at all").unwrap();
        let result = load_policy(&path);
        assert!(matches!(result, Err(StorageError::Yaml(_))));
    }
}
