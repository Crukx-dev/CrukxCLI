use crate::runtimes::LlamaCppAdapter;
use crukx_core::deploy::devices::DeviceProfile;
use crukx_core::deploy::runtimes::{LlamaCppConfig, RuntimeAdapter, RuntimeError, RuntimeKind};
use crukx_core::deploy::selection::select_runtime;

pub fn adapter_with_binary(
    profile: &DeviceProfile,
    binary: &str,
) -> Result<LlamaCppAdapter, RuntimeError> {
    match select_runtime(profile) {
        None => Err(RuntimeError::NotSupported(
            "device lists no supported runtime".to_string(),
        )),
        Some(kind) if kind != RuntimeKind::LlamaCpp => Err(RuntimeError::NotSupported(format!(
            "{} adapter is not implemented",
            kind.as_str()
        ))),
        _ => {
            let adapter = LlamaCppAdapter::new(LlamaCppConfig {
                binary: binary.to_string(),
                ..LlamaCppConfig::from_profile(profile)
            });
            if adapter.is_available() {
                Ok(adapter)
            } else {
                Err(RuntimeError::NotFound(binary.to_string()))
            }
        }
    }
}

pub fn resolve_adapter(profile: &DeviceProfile) -> Result<LlamaCppAdapter, RuntimeError> {
    adapter_with_binary(profile, &LlamaCppConfig::from_profile(profile).binary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crukx_core::deploy::devices::DeviceProfile;

    #[test]
    fn unsupported_device_is_rejected() {
        let mut profile = DeviceProfile::mock_cpu_only();
        profile.runtime_support = vec!["tensorrt".to_string()];
        assert!(matches!(
            resolve_adapter(&profile),
            Err(RuntimeError::NotSupported(_))
        ));
    }

    #[test]
    fn empty_support_is_rejected() {
        let mut profile = DeviceProfile::mock_cpu_only();
        profile.runtime_support.clear();
        profile.refresh_id();
        assert!(matches!(
            resolve_adapter(&profile),
            Err(RuntimeError::NotSupported(_))
        ));
    }

    #[test]
    fn missing_binary_is_reported() {
        let profile = DeviceProfile::mock_cpu_only();
        let result = adapter_with_binary(&profile, "/nonexistent/llama-cli");
        assert!(matches!(result, Err(RuntimeError::NotFound(_))));
    }

    #[cfg(unix)]
    #[test]
    fn script_binary_resolves() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "crukx-select-test-{}-{}.sh",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        let adapter =
            adapter_with_binary(&DeviceProfile::mock_cpu_only(), &path.to_string_lossy()).unwrap();
        assert_eq!(adapter.kind(), RuntimeKind::LlamaCpp);
        std::fs::remove_file(&path).ok();
    }
}
