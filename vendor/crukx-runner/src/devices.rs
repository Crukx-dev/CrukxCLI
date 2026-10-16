use crukx_core::deploy::devices::{CpuInfo, DeviceProfile, GpuInfo, MemoryInfo, OsInfo};
use std::process::Command;

fn cmd_out(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() || text.len() > 4096 {
        None
    } else {
        Some(text)
    }
}

fn cpu_model() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                if let Some(model) = rest.split(':').nth(1) {
                    let model = model.trim().to_string();
                    if !model.is_empty() {
                        return Some(model);
                    }
                }
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        cmd_out("sysctl", &["-n", "machdep.cpu.brand_string"])
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

fn memory() -> MemoryInfo {
    #[cfg(target_os = "linux")]
    {
        let mut total_mb = None;
        let mut available_mb = None;
        if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                let key = parts.next().unwrap_or("");
                let value: Option<u64> = parts.next().and_then(|v| v.parse().ok());
                match (key, value) {
                    ("MemTotal:", Some(kb)) => total_mb = Some(kb / 1024),
                    ("MemAvailable:", Some(kb)) => available_mb = Some(kb / 1024),
                    _ => {}
                }
            }
        }
        MemoryInfo {
            total_mb,
            available_mb,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let total_mb = cmd_out("sysctl", &["-n", "hw.memsize"])
            .and_then(|v| v.parse::<u64>().ok())
            .map(|bytes| bytes / 1024 / 1024);
        MemoryInfo {
            total_mb,
            available_mb: None,
        }
    }
    #[cfg(target_os = "windows")]
    {
        windows_memory()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        MemoryInfo {
            total_mb: None,
            available_mb: None,
        }
    }
}

fn os_version() -> Option<String> {
    #[cfg(unix)]
    {
        cmd_out("uname", &["-r"])
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn nvidia_memory_mb() -> Option<u64> {
    let out = cmd_out(
        "nvidia-smi",
        &["--query-gpu=memory.total", "--format=csv,noheader,nounits"],
    )?;
    out.lines()
        .next()
        .and_then(|line| line.trim().parse::<u64>().ok())
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_gpus() -> Vec<GpuInfo> {
    let mut gpus = Vec::new();
    let text = match cmd_out("lspci", &["-mm"]) {
        Some(text) => text,
        None => return gpus,
    };
    for line in text.lines() {
        let lower = line.to_lowercase();
        if !lower.contains("vga") && !lower.contains("3d controller") {
            continue;
        }
        let (vendor, memory_mb) = if lower.contains("nvidia") {
            ("nvidia", nvidia_memory_mb())
        } else if lower.contains("amd") || lower.contains("advanced micro") {
            ("amd", None)
        } else if lower.contains("intel") {
            ("intel", None)
        } else {
            ("unknown", None)
        };
        gpus.push(GpuInfo {
            vendor: vendor.to_string(),
            model: None,
            memory_mb,
        });
    }
    gpus
}

fn gpus(cpu_model: Option<&str>) -> Vec<GpuInfo> {
    #[cfg(target_os = "linux")]
    {
        let _ = cpu_model;
        linux_gpus()
    }
    #[cfg(target_os = "macos")]
    {
        match cpu_model {
            Some(model) if model.contains("Apple") => vec![GpuInfo {
                vendor: "apple".to_string(),
                model: Some(model.to_string()),
                memory_mb: None,
            }],
            _ => Vec::new(),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = cpu_model;
        windows_gpus()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = cpu_model;
        Vec::new()
    }
}

fn has_nvidia_gpu(gpus: &[GpuInfo]) -> bool {
    gpus.iter().any(|g| g.vendor == "nvidia")
}

fn runtime_support_for(os: &str, gpus: &[GpuInfo]) -> Vec<String> {
    let mut runtimes = if os == "macos" {
        vec![
            "llama.cpp".to_string(),
            "onnx".to_string(),
            "coreml".to_string(),
        ]
    } else {
        vec!["llama.cpp".to_string(), "onnx".to_string()]
    };
    // Only advertise TensorRT when an NVIDIA adapter was actually detected —
    // never from the model name or OS alone (§11: claim support only after
    // validation; §6 guardrail: no capability inference from model name).
    if has_nvidia_gpu(gpus) && !runtimes.iter().any(|r| r == "tensorrt") {
        runtimes.push("tensorrt".to_string());
    }
    runtimes
}

#[cfg(target_os = "windows")]
fn windows_memory() -> MemoryInfo {
    // `wmic os get TotalVisibleMemorySize,FreePhysicalMemory /value` prints:
    //   FreePhysicalMemory=12345\r\nTotalVisibleMemorySize=16777216\r\n
    // Values are KiB. Pure parser below is unit-tested; this wrapper only
    // shells out so behavior on non-Windows hosts is unchanged.
    if let Some(text) = cmd_out(
        "wmic",
        &[
            "os",
            "get",
            "TotalVisibleMemorySize,FreePhysicalMemory",
            "/value",
        ],
    ) {
        if let Some(mem) = parse_wmic_mem(&text) {
            return mem;
        }
    }
    MemoryInfo {
        total_mb: None,
        available_mb: None,
    }
}

#[allow(dead_code)]
fn parse_wmic_mem(text: &str) -> Option<MemoryInfo> {
    let mut total_mb = None;
    let mut available_mb = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        let kb: u64 = value.trim().parse().ok()?;
        if kb == 0 || kb > (1 << 40) {
            return None;
        }
        match key.trim().to_lowercase().as_str() {
            "totalvisiblememorysize" => total_mb = Some(kb / 1024),
            "freephysicalmemory" => available_mb = Some(kb / 1024),
            _ => {}
        }
    }
    if total_mb.is_none() && available_mb.is_none() {
        return None;
    }
    Some(MemoryInfo {
        total_mb,
        available_mb,
    })
}

#[allow(dead_code)]
fn parse_wmic_gpus(text: &str, nvidia_default_mb: Option<u64>) -> Vec<GpuInfo> {
    // `wmic path win32_VideoController get Name /value` prints one
    // `Name=...` line per adapter.
    let mut gpus = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("Name=") else {
            continue;
        };
        let name = rest.trim();
        if name.is_empty() {
            continue;
        }
        let lower = name.to_lowercase();
        let (vendor, memory_mb) = if lower.contains("nvidia") {
            ("nvidia", nvidia_default_mb)
        } else if lower.contains("amd") || lower.contains("radeon") {
            ("amd", None)
        } else if lower.contains("intel") {
            ("intel", None)
        } else {
            ("unknown", None)
        };
        gpus.push(GpuInfo {
            vendor: vendor.to_string(),
            model: Some(name.to_string()),
            memory_mb,
        });
    }
    gpus
}

#[cfg(target_os = "windows")]
fn windows_gpus() -> Vec<GpuInfo> {
    // Query per-adapter VRAM once; wmic only yields adapter names.
    let mem = nvidia_memory_mb();
    match cmd_out(
        "wmic",
        &["path", "win32_VideoController", "get", "Name", "/value"],
    ) {
        Some(text) => parse_wmic_gpus(&text, mem),
        None => {
            // nvidia-smi present but wmic missing/failed: still report the
            // NVIDIA adapter so TensorRT is advertised honestly.
            match mem {
                Some(mb) => vec![GpuInfo {
                    vendor: "nvidia".to_string(),
                    model: None,
                    memory_mb: Some(mb),
                }],
                None => Vec::new(),
            }
        }
    }
}
fn precision_support() -> Vec<String> {
    let mut precisions = vec!["fp32".to_string()];
    #[cfg(target_arch = "aarch64")]
    {
        precisions.push("fp16".to_string());
    }
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("f16c") {
            precisions.push("fp16".to_string());
        }
        if std::arch::is_x86_feature_detected!("avx512bf16") {
            precisions.push("bf16".to_string());
        }
    }
    precisions.push("int8".to_string());
    precisions.push("int4".to_string());
    precisions
}

pub fn detect() -> DeviceProfile {
    let model = cpu_model();
    let memory = memory();
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let mut profile = DeviceProfile::new(
        CpuInfo {
            arch: std::env::consts::ARCH.to_string(),
            logical_cores: cores,
            model: model.clone(),
        },
        memory.clone(),
        OsInfo {
            name: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            version: os_version(),
        },
    );
    profile.gpus = gpus(model.as_deref());
    profile.runtime_support = runtime_support_for(std::env::consts::OS, &profile.gpus);
    profile.precision_support = precision_support();
    let budget_mb = memory.available_mb.or(memory.total_mb);
    profile.max_model_bytes = budget_mb.map(|mb| mb * 1024 * 1024 / 2);
    profile.refresh_id();
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_produces_stable_usable_profile() {
        let a = detect();
        let b = detect();
        assert_eq!(a.schema_version, 1);
        assert!(!a.cpu.arch.is_empty());
        assert!(a.cpu.logical_cores >= 1);
        assert_eq!(a.device_id, b.device_id);
        assert_eq!(a.device_id.len(), 64);
        assert!(a.supports_runtime("llama.cpp"));
        assert!(a.supports_precision("fp32"));
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains(&a.device_id));
    }

    #[test]
    fn wmic_memory_parses_kib_values() {
        let mem =
            parse_wmic_mem("FreePhysicalMemory=8388608\r\nTotalVisibleMemorySize=16777216\r\n")
                .unwrap();
        assert_eq!(mem.total_mb, Some(16384));
        assert_eq!(mem.available_mb, Some(8192));
    }

    #[test]
    fn wmic_memory_rejects_garbage() {
        assert!(parse_wmic_mem("").is_none());
        assert!(parse_wmic_mem("TotalVisibleMemorySize=notanumber\r\n").is_none());
        assert!(parse_wmic_mem("TotalVisibleMemorySize=0\r\n").is_none());
    }

    #[test]
    fn wmic_gpus_classify_vendors() {
        let gpus = parse_wmic_gpus(
            "Name=NVIDIA GeForce RTX 4090\r\nName=Intel(R) UHD Graphics 770\r\nName=\r\n",
            Some(24576),
        );
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].vendor, "nvidia");
        assert_eq!(gpus[0].model.as_deref(), Some("NVIDIA GeForce RTX 4090"));
        assert_eq!(gpus[0].memory_mb, Some(24576));
        assert_eq!(gpus[1].vendor, "intel");
        assert_eq!(gpus[1].memory_mb, None);
    }

    #[test]
    fn tensorrt_advertised_only_with_nvidia_present() {
        let nvidia = vec![GpuInfo {
            vendor: "nvidia".to_string(),
            model: Some("GeForce RTX 4090".to_string()),
            memory_mb: Some(24576),
        }];
        let with_nvidia = runtime_support_for("windows", &nvidia);
        assert!(with_nvidia.contains(&"tensorrt".to_string()));
        // Base runtimes stay first; tensorrt is appended, never replacing.
        assert_eq!(with_nvidia[0], "llama.cpp");

        let no_gpus = runtime_support_for("windows", &[]);
        assert!(!no_gpus.contains(&"tensorrt".to_string()));
        assert!(no_gpus.contains(&"llama.cpp".to_string()));

        // OS alone never implies TensorRT — Apple Silicon has no NVIDIA path.
        let apple = vec![GpuInfo {
            vendor: "apple".to_string(),
            model: Some("Apple M4".to_string()),
            memory_mb: None,
        }];
        let mac = runtime_support_for("macos", &apple);
        assert!(mac.contains(&"coreml".to_string()));
        assert!(!mac.contains(&"tensorrt".to_string()));
    }
}
