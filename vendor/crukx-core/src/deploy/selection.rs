use crate::deploy::devices::DeviceProfile;
use crate::deploy::runtimes::RuntimeKind;

pub const PREFERENCE_ORDER: &[RuntimeKind] = &[
    RuntimeKind::LlamaCpp,
    RuntimeKind::Onnx,
    RuntimeKind::CoreMl,
    RuntimeKind::Executorch,
    RuntimeKind::TensorRt,
];

pub fn select_runtime(profile: &DeviceProfile) -> Option<RuntimeKind> {
    PREFERENCE_ORDER
        .iter()
        .find(|kind| profile.supports_runtime(kind.as_str()))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_llama_cpp_when_listed() {
        assert_eq!(
            select_runtime(&DeviceProfile::mock_metal()),
            Some(RuntimeKind::LlamaCpp)
        );
    }

    #[test]
    fn falls_back_to_next_supported() {
        let mut profile = DeviceProfile::mock_cpu_only();
        profile.runtime_support = vec!["tensorrt".to_string()];
        assert_eq!(select_runtime(&profile), Some(RuntimeKind::TensorRt));
    }

    #[test]
    fn empty_support_selects_nothing() {
        let mut profile = DeviceProfile::mock_cpu_only();
        profile.runtime_support.clear();
        profile.refresh_id();
        assert_eq!(select_runtime(&profile), None);
    }
}
