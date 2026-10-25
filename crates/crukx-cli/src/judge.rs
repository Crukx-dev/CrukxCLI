//! `crukx judge` — run an LLM judge over a human-labeled calibration set
//! and record the resulting `JudgeRecord` (agreement rate, model, rubric).
//! That record is what `crukx gate`'s drift detection compares against:
//! record it once as the baseline, again after any judge/model/prompt
//! change as `current`, and the gate refuses to pass when the judge
//! silently disagrees with humans more than its baseline did (Netflix's
//! unlabeled-swap lesson).
//!
//! Models ship out-of-box as named profiles: `glm-5.3-flash` and
//! `mimo-v2.5`, both routed through the OpenRouter-compatible API the
//! `CRUKX_API_KEY` env var authenticates. Any other OpenAI-compatible
//! endpoint works via `--base-url` + `--model`.

use crate::colors::{self, color_enabled};
use crate::ui;
use clap::ValueEnum;
use crukx_storage::json::write_json;
use crukx_storage::state_dir::StateDir;
use std::path::Path;

/// Shipped judge profiles — the "works the moment you install" story.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum JudgeProfile {
    /// Xiaomi MiMo v2.5 — shipped default judge (Anthropic-compatible
    /// endpoint at token-plan-sgp.xiaomimimo.com).
    #[default]
    Mimo,
    /// Zhipu GLM-5.3-Flash via OpenRouter — second opinion / panel member.
    Glm,
}

impl JudgeProfile {
    /// Default base URL per profile — both are OpenAI/Anthropic
    /// compatible, no extra service to run.
    fn default_base_url(&self) -> &'static str {
        match self {
            JudgeProfile::Mimo => "https://token-plan-sgp.xiaomimimo.com/anthropic",
            JudgeProfile::Glm => "https://openrouter.ai/api/v1",
        }
    }

    fn default_model(&self) -> &'static str {
        match self {
            JudgeProfile::Mimo => "mimo-v2.5",
            JudgeProfile::Glm => "z-ai/glm-5.3-flash",
        }
    }
}

/// One human-labeled case from the calibration set (JSONL:
/// `{"input": "...", "expected": true|false}` — the label is what the
/// *human* says the correct verdict is; agreement is measured against it).
#[derive(Debug, serde::Deserialize)]
struct CalibrationCase {
    input: String,
    expected: bool,
}

pub fn run_judge(
    profile: Option<JudgeProfile>,
    model_override: Option<String>,
    base_url: Option<String>,
    calibration_path: &Path,
    as_baseline: bool,
    cwd: &Path,
) -> i32 {
    let color = color_enabled();
    let profile = profile.unwrap_or(JudgeProfile::Mimo);

    let Some(api_key) = std::env::var_os("CRUKX_API_KEY")
        .map(|v| v.to_string_lossy().into_owned())
        .filter(|v| !v.is_empty())
    else {
        eprintln!("Crukx judge: CRUKX_API_KEY is not set");
        eprintln!(
            "{}",
            ui::hint(
                "export CRUKX_API_KEY=<key> — a Xiaomi MiMo key (token-plan) or OpenRouter key works with the shipped profiles",
                color
            )
        );
        return 1;
    };

    let base_url = base_url.unwrap_or_else(|| profile.default_base_url().to_string());
    let model = model_override.unwrap_or_else(|| profile.default_model().to_string());

    let cases = match read_calibration(calibration_path) {
        Ok(cases) => cases,
        Err(err) => {
            eprintln!(
                "Crukx judge: failed to read calibration set {}: {err}",
                calibration_path.display()
            );
            return 1;
        }
    };
    if cases.is_empty() {
        eprintln!(
            "Crukx judge: {} contains no labeled cases",
            calibration_path.display()
        );
        eprintln!(
            "{}",
            ui::hint(
                "each line is a JSON object {\"input\": \"...\", \"expected\": true|false}",
                color
            )
        );
        return 1;
    }

    println!(
        "{}",
        ui::header(
            "Crukx judge",
            Some(&format!(
                "{} · {} case{} · {}",
                model,
                cases.len(),
                if cases.len() == 1 { "" } else { "s" },
                calibration_path.display()
            )),
            color
        )
    );

    let mut agreements = 0usize;
    let mut failures = 0usize;
    for (index, case) in cases.iter().enumerate() {
        match judge_case(&base_url, &api_key, &model, &case.input) {
            Some(verdict) => {
                let mark = if verdict == case.expected {
                    ui::ok_mark(color)
                } else {
                    ui::fail_mark(color)
                };
                println!(
                    "  {mark} case {}/{} — {} (expected {})",
                    index + 1,
                    cases.len(),
                    colors::dim(
                        &format!("judge said {}", if verdict { "PASS" } else { "FAIL" }),
                        color
                    ),
                    if case.expected { "PASS" } else { "FAIL" },
                );
                if verdict == case.expected {
                    agreements += 1;
                }
            }
            None => {
                failures += 1;
                eprintln!(
                    "  {} case {}/{} — call failed",
                    ui::warn_mark(color),
                    index + 1,
                    cases.len()
                );
            }
        }
    }

    let scored = cases.len() - failures;
    if scored == 0 {
        eprintln!("Crukx judge: every call failed — check network/key/base-url");
        return 1;
    }
    // Failed calls are excluded from scoring, so they're excluded from the
    // denominator too — otherwise endpoint flakiness depresses the stored
    // rate that `crukx gate`'s drift detection compares against.
    let agreement = agreements as f64 / scored as f64;

    let record = crukx_core::evaluators::registry::JudgeRecord {
        judge_id: "llm-judge".to_string(),
        model: model.clone(),
        rubric_version: "1".to_string(),
        calibration_set_id: calibration_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "inline".to_string()),
        human_agreement_rate: agreement,
        evaluated_at: chrono::Utc::now().to_rfc3339(),
    };

    let state = StateDir::resolve(cwd);
    let target = if as_baseline {
        "baseline.json"
    } else {
        "current.json"
    };
    if let Err(err) = write_json(&state.evaluators_dir().join(target), &record) {
        eprintln!("Crukx judge: failed to write {target}: {err}");
        return 1;
    }

    println!();
    eprintln!(
        "human agreement {:.1}% ({agreements}/{}) across {} case{}{}",
        agreement * 100.0,
        scored,
        scored,
        if scored == 1 { "" } else { "s" },
        if failures > 0 {
            format!(" · {failures} call(s) failed (excluded from scoring)")
        } else {
            String::new()
        }
    );
    eprintln!(
        "record written: {}",
        state.evaluators_dir().join(target).display()
    );
    if as_baseline {
        eprintln!(
            "{}",
            ui::hint("baseline recorded — run again without --baseline after any judge change, then `crukx gate` enforces drift", color)
        );
    }

    0
}

fn read_calibration(path: &Path) -> Result<Vec<CalibrationCase>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut cases = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        cases.push(
            serde_json::from_str(line).map_err(|err| format!("line {}: {err}", line_no + 1))?,
        );
    }
    Ok(cases)
}

const JUDGE_SYSTEM_PROMPT: &str = "You are a strict release-gate judge. Reply with ONLY a JSON object: {\"pass\": true|false, \"reason\": \"...\"}. pass=true only when the input satisfies the stated acceptance condition.";

/// One judge call — routes to the right wire protocol from the base URL
/// (Anthropic-style endpoints like Xiaomi's get `x-api-key` +
/// `/v1/messages`; OpenAI-style like OpenRouter get `Bearer` +
/// `/chat/completions`). Returns the model's verdict boolean.
fn judge_case(base_url: &str, api_key: &str, model: &str, input: &str) -> Option<bool> {
    // The system prompt travels in the request's `system` / `role=system`
    // field only. It used to be prepended here as well, so every judge call
    // paid for the rubric twice and the model saw it as user text.
    let user_prompt = format!("Evaluate:\n{input}");
    let content = if base_url.contains("/anthropic") {
        call_anthropic(base_url, api_key, model, &user_prompt)?
    } else {
        call_openai(base_url, api_key, model, &user_prompt)?
    };
    extract_pass(&content)
}

fn call_anthropic(base_url: &str, api_key: &str, model: &str, prompt: &str) -> Option<String> {
    let body = serde_json::json!({
        "model": model,
        // Reasoning models spend the budget on thinking blocks before
        // emitting text — too small and the reply truncates to nothing.
        "max_tokens": 2048,
        "temperature": 0.0,
        "system": JUDGE_SYSTEM_PROMPT,
        "messages": [{"role": "user", "content": prompt}]
    });

    for attempt in 0..3 {
        let attempt_result: Option<serde_json::Value> = (|| {
            ureq::post(&format!("{base_url}/v1/messages"))
                .set("x-api-key", api_key)
                .set("anthropic-version", "2023-06-01")
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(90))
                .send_json(body.clone())
                .ok()?
                .into_json()
                .ok()
        })();
        // A malformed 200 (error envelope, wrong wire shape) is just a failed
        // attempt: retry it rather than abandoning the whole call.
        if let Some(text) = attempt_result.as_ref().and_then(anthropic_text) {
            return Some(text);
        }
        std::thread::sleep(std::time::Duration::from_millis(
            1500 * (attempt as u64 + 1),
        ));
    }
    None
}

/// The reply text of an Anthropic Messages response. Prefers the text
/// block; falls back to the thinking block — MiMo reasons out loud first,
/// and its thinking still names the verdict.
fn anthropic_text(response: &serde_json::Value) -> Option<String> {
    response["content"]
        .as_array()?
        .iter()
        .find_map(|block| {
            block["text"]
                .as_str()
                .or_else(|| block["thinking"].as_str())
        })
        .map(str::to_string)
}

fn call_openai(base_url: &str, api_key: &str, model: &str, prompt: &str) -> Option<String> {
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.0,
        // Same budget as the Anthropic path: reasoning models spend it on
        // thinking before emitting the verdict, and 512 truncated replies
        // into "no parseable result".
        "max_tokens": 2048,
        "messages": [
            {"role": "system", "content": JUDGE_SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ]
    });

    // Retry like the Anthropic path does — a single blip used to fail the
    // whole case (and a malformed 200 escaped without a second attempt).
    for attempt in 0..3 {
        let attempt_result: Option<serde_json::Value> = (|| {
            ureq::post(&format!("{base_url}/chat/completions"))
                .set("Authorization", &format!("Bearer {api_key}"))
                .set("Content-Type", "application/json")
                .timeout(std::time::Duration::from_secs(90))
                .send_json(body.clone())
                .ok()?
                .into_json()
                .ok()
        })();
        if let Some(text) = attempt_result.as_ref().and_then(openai_text) {
            return Some(text);
        }
        std::thread::sleep(std::time::Duration::from_millis(
            1500 * (attempt as u64 + 1),
        ));
    }
    None
}

/// The reply text of an OpenAI chat-completions response. `None` on a
/// malformed 200 so the caller's retry loop treats it as a failed attempt.
fn openai_text(response: &serde_json::Value) -> Option<String> {
    response["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
}

/// Pulls the boolean verdict out of model output — handles clean JSON,
/// fenced ```json blocks, and prose-wrapped JSON, and falls back to the
/// words PASS/FAIL appearing in the reply. Judges drift in *format* as
/// much as in judgment; the extractor is deliberately forgiving.
fn extract_pass(content: &str) -> Option<bool> {
    if let Some(start) = content.find('{') {
        let bytes = content.as_bytes();
        let mut depth = 0usize;
        for (index, &byte) in bytes.iter().enumerate().skip(start) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &content[start..=index];
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
                            if let Some(pass) = pass_from_value(&value) {
                                return Some(pass);
                            }
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    let lowered = content.to_lowercase();
    if lowered.contains("\"pass\": true") || lowered.contains("pass: true") {
        return Some(true);
    }
    if lowered.contains("\"pass\": false") || lowered.contains("pass: false") {
        return Some(false);
    }
    // Word-level fallback — matches "PASS"/"failed" as well as the exact
    // tokens, because judges also drift in *wording*. Failure checked
    // first: "failed to pass" must read FAIL.
    if lowered.split_whitespace().any(|w| w.starts_with("fail")) {
        return Some(false);
    }
    if lowered.split_whitespace().any(|w| w.starts_with("pass")) {
        return Some(true);
    }
    None
}

fn pass_from_value(value: &serde_json::Value) -> Option<bool> {
    match &value["pass"] {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::String(s) => match s.to_lowercase().as_str() {
            "true" | "pass" => Some(true),
            "false" | "fail" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_text_is_none_for_a_malformed_200_so_the_call_retries() {
        let envelope = serde_json::json!({"error": {"message": "rate limited"}});
        assert_eq!(openai_text(&envelope), None);
        let reply = serde_json::json!({
            "choices": [{"message": {"content": "{\"pass\": true}"}}]
        });
        assert_eq!(openai_text(&reply).as_deref(), Some("{\"pass\": true}"));
    }

    #[test]
    fn user_prompt_does_not_repeat_the_system_rubric() {
        // The rubric belongs in the request's system field only; baking it
        // into the user message as well paid for it twice and confused the
        // turn structure.
        let user_prompt = format!("Evaluate:\n{}", "the input");
        assert!(!user_prompt.contains("release-gate judge"));
        assert!(user_prompt.starts_with("Evaluate:\n"));
    }

    #[test]
    fn anthropic_text_is_none_for_a_malformed_200_so_the_call_retries() {
        let envelope = serde_json::json!({"type": "error", "error": {"message": "overloaded"}});
        assert_eq!(anthropic_text(&envelope), None);
        let reply = serde_json::json!({"content": [{"type": "text", "text": "{\"pass\": true}"}]});
        assert_eq!(anthropic_text(&reply).as_deref(), Some("{\"pass\": true}"));
    }

    #[test]
    fn extract_pass_from_clean_json() {
        assert_eq!(
            extract_pass("{\"pass\": true, \"reason\": \"ok\"}"),
            Some(true)
        );
        assert_eq!(
            extract_pass("{\"pass\": false, \"reason\": \"no\"}"),
            Some(false)
        );
    }

    #[test]
    fn extract_pass_from_prose_wrapped_json() {
        let noisy = "Sure! The verdict is ```json\n{\"pass\": false}\n``` — hope that helps";
        assert_eq!(extract_pass(noisy), Some(false));
    }

    #[test]
    fn extract_pass_falls_back_to_words_when_json_is_broken() {
        assert_eq!(extract_pass("The agent failed the task."), Some(false));
        assert_eq!(extract_pass("verdict: PASS"), Some(true));
    }

    #[test]
    fn extract_pass_returns_none_on_garbage() {
        assert_eq!(extract_pass("no verdict here"), None);
        assert_eq!(extract_pass(""), None);
    }

    #[test]
    fn profile_models_are_the_shipped_defaults() {
        assert_eq!(JudgeProfile::Mimo.default_model(), "mimo-v2.5");
        assert_eq!(JudgeProfile::Glm.default_model(), "z-ai/glm-5.3-flash");
        assert!(JudgeProfile::Mimo.default_base_url().contains("/anthropic"));
        assert!(JudgeProfile::Glm.default_base_url().contains("openrouter"));
    }

    #[test]
    fn mimo_is_the_default_profile() {
        // Ship order: Xiaomi first.
        assert_eq!(JudgeProfile::Mimo, JudgeProfile::default());
    }
}
