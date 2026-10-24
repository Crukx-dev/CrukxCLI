use crukx_core::deploy::benchmarks::{BenchmarkConfig, BenchmarkReport};
use crukx_runner::benchmarks::benchmark;
use crukx_runner::devices::detect;
use crukx_runner::selection::resolve_adapter;
use std::path::PathBuf;

pub struct WorkerArgs {
    pub api_url: String,
    pub api_key: Option<String>,
    pub poll_secs: u64,
    pub once: bool,
}

fn api_key(args: &WorkerArgs) -> Result<String, i32> {
    match &args.api_key {
        Some(key) if !key.is_empty() => Ok(key.clone()),
        _ => match std::env::var("CRUKX_API_KEY") {
            Ok(key) if !key.is_empty() => Ok(key),
            _ => {
                eprintln!("worker: missing API key (pass --api-key or set CRUKX_API_KEY)");
                Err(2)
            }
        },
    }
}

fn claim_job(api_url: &str, key: &str) -> Result<Option<serde_json::Value>, String> {
    let response = ureq::post(&format!("{}/api/jobs/claim", api_url))
        .set("Authorization", &format!("Bearer {}", key))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .send_json(serde_json::json!({"kind": "benchmark"}))
        .map_err(|e| e.to_string())?;
    if response.status() == 204 {
        return Ok(None);
    }
    // 200 + a job body is the only other valid answer. Anything else (an
    // error envelope, a proxy page, a redirect body) must not be mistaken
    // for claimed work and turned into a phantom job.
    if response.status() != 200 {
        return Err(format!(
            "unexpected http status {} from claim",
            response.status()
        ));
    }
    let job: serde_json::Value = response.into_json().map_err(|e| e.to_string())?;
    if job.get("job_id").and_then(|v| v.as_str()).is_none() {
        return Err(format!("claim response is not a job: {job}"));
    }
    Ok(Some(job))
}

fn patch_job(
    api_url: &str,
    key: &str,
    job_id: &str,
    state: &str,
    result: serde_json::Value,
) -> Result<(), String> {
    let response = ureq::patch(&format!("{}/api/jobs/{}", api_url, job_id))
        .set("Authorization", &format!("Bearer {}", key))
        .set("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .send_json(serde_json::json!({"state": state, "result": result}))
        .map_err(|e| e.to_string())?;
    // Be explicit rather than relying on the client's error policy: losing a
    // job report must never look like success to the caller.
    if response.status() != 200 {
        return Err(format!(
            "unexpected http status {} reporting job",
            response.status()
        ));
    }
    Ok(())
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// `default` when the key is absent; an error when it's present but doesn't
/// fit in u32 (`v as u32` used to wrap, so 4294967297 runs became 1).
fn payload_u32(payload: &serde_json::Value, key: &str, default: u32) -> Result<u32, String> {
    match payload.get(key).and_then(|v| v.as_u64()) {
        None => Ok(default),
        Some(v) => u32::try_from(v).map_err(|_| format!("{key} = {v} is out of range")),
    }
}

fn payload_u64(payload: &serde_json::Value, key: &str, default: u64) -> u64 {
    payload.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

fn run_claimed_job(job: &serde_json::Value) -> (String, serde_json::Value) {
    let job_id = job
        .get("job_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let payload = job
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let model = match payload_string(&payload, "model") {
        Some(model) => model,
        None => {
            return (
                job_id,
                serde_json::json!({"error": "job has no benchmark payload"}),
            );
        }
    };
    if !PathBuf::from(&model).is_file() {
        return (
            job_id,
            serde_json::json!({"error": format!("model file not found: {}", model)}),
        );
    }
    let profile = detect();
    let adapter = match resolve_adapter(&profile) {
        Ok(adapter) => adapter,
        Err(e) => {
            return (
                job_id,
                serde_json::json!({"error": format!("no usable runtime: {}", e)}),
            );
        }
    };
    let config = (|| -> Result<BenchmarkConfig, String> {
        let min_successes = match payload.get("min_successes") {
            Some(_) => Some(payload_u32(&payload, "min_successes", 0)?),
            None => None,
        };
        Ok(BenchmarkConfig {
            warmup_runs: payload_u32(&payload, "warmup", 1)?,
            measured_runs: payload_u32(&payload, "runs", 5)?.max(1),
            prompt: payload_string(&payload, "prompt").unwrap_or_else(|| "benchmark".to_string()),
            max_tokens: payload_u32(&payload, "max_tokens", 32)?,
            timeout_secs: payload_u64(&payload, "timeout_secs", 120),
            min_successes,
        })
    })();
    let config = match config {
        Ok(config) => config,
        Err(e) => {
            return (
                job_id,
                serde_json::json!({"error": format!("invalid job payload: {e}")}),
            );
        }
    };
    match benchmark(&adapter, &profile, &model, &config) {
        Ok(report) => {
            let report_json: serde_json::Value =
                serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
            (job_id, report_json)
        }
        Err(e) => (job_id, serde_json::json!({"error": e.to_string()})),
    }
}

fn report_is_usable(result: &serde_json::Value) -> bool {
    result.get("error").is_none() && result.get("summary").is_some_and(|s| !s.is_null()) && {
        serde_json::from_value::<BenchmarkReport>(result.clone()).is_ok()
    }
}

pub fn run_worker(args: &WorkerArgs) -> i32 {
    let key = match api_key(args) {
        Ok(key) => key,
        Err(code) => return code,
    };
    loop {
        match claim_job(&args.api_url, &key) {
            Err(e) => {
                eprintln!("worker: claim failed: {}", e);
                return 1;
            }
            Ok(None) => {
                if args.once {
                    println!("worker: no jobs queued");
                    return 0;
                }
            }
            Ok(Some(job)) => {
                let (job_id, result) = run_claimed_job(&job);
                let state = if report_is_usable(&result) {
                    "succeeded"
                } else {
                    "failed"
                };
                println!("worker: job {} -> {}", job_id, state);
                if let Err(e) = patch_job(&args.api_url, &key, &job_id, state, result) {
                    eprintln!("worker: cannot report job {}: {}", job_id, e);
                    return 1;
                }
                if args.once {
                    return 0;
                }
            }
        }
        if !args.once {
            std::thread::sleep(std::time::Duration::from_secs(args.poll_secs.max(1)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_u32_rejects_values_that_would_wrap() {
        let p = serde_json::json!({"runs": 4294967297u64, "warmup": 3});
        assert!(payload_u32(&p, "runs", 5).is_err());
        assert_eq!(payload_u32(&p, "warmup", 1), Ok(3));
        assert_eq!(payload_u32(&p, "max_tokens", 32), Ok(32));
    }
}
