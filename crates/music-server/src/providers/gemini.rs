//! Google Gemini Cloud Assistant provider (`gemini-3.8-flash` primary + resilient Flash fallback).
//!
//! Uses the native Google Generative Language REST API (`generateContent`) with
//! `x-goog-api-key` authentication, optional JSON Schema enforcement, and
//! automatic fallback across active Flash models when `503 UNAVAILABLE`, `429`,
//! or transient upstream timeouts occur.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "gemini-3.8-flash";
pub const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const FALLBACK_CHAIN: &[&str] = &[
    "gemini-3.8-flash",
    "gemini-3.7-flash",
    "gemini-3.5-flash",
    "gemini-flash-latest",
];

/// Resolves the primary model (`config` -> `GEMINI_MODEL` env -> `gemini-3.8-flash`)
/// and returns the ordered deduplicated candidate chain.
pub fn model_chain(configured: Option<&str>) -> Vec<String> {
    let env_model = std::env::var("GEMINI_MODEL").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let primary = configured
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or(env_model)
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let mut chain = vec![primary];
    for &fallback in FALLBACK_CHAIN {
        if !chain.iter().any(|m| m == fallback) {
            chain.push(fallback.to_string());
        }
    }
    chain
}

/// Strips JSON Schema fields rejected by Gemini's `responseJsonSchema` validator
/// (`additionalProperties` at root or nested objects is not supported on all Flash tiers).
fn sanitize_schema_for_gemini(schema: Value) -> Value {
    match schema {
        Value::Object(mut map) => {
            map.remove("additionalProperties");
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                if let Some(v) = map.remove(&k) {
                    map.insert(k, sanitize_schema_for_gemini(v));
                }
            }
            Value::Object(map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sanitize_schema_for_gemini).collect()),
        other => other,
    }
}

fn map_thinking_level(effort: Option<&str>) -> Option<&'static str> {
    match effort.map(str::trim).unwrap_or("low").to_ascii_lowercase().as_str() {
        "off" | "none" => Some("minimal"),
        "minimal" | "low" => Some("low"),
        "medium" => Some("medium"),
        "high" | "xhigh" | "max" => Some("high"),
        _ => Some("low"),
    }
}

/// Builds a native Gemini `generateContent` request payload.
pub fn build_generate_body(
    system: &str,
    user: &str,
    effort: Option<&str>,
    schema: Option<Value>,
) -> Value {
    let mut gen_cfg = json!({
        "temperature": 0.8,
        "maxOutputTokens": 4096,
    });
    if let Some(level) = map_thinking_level(effort) {
        gen_cfg["thinkingConfig"] = json!({ "thinkingLevel": level });
    }
    if let Some(schema) = schema {
        gen_cfg["responseMimeType"] = json!("application/json");
        gen_cfg["responseJsonSchema"] = sanitize_schema_for_gemini(schema);
    }

    json!({
        "systemInstruction": {
            "parts": [{ "text": system }]
        },
        "contents": [{
            "role": "user",
            "parts": [{ "text": user }]
        }],
        "generationConfig": gen_cfg,
    })
}

/// Extracts visible output text from Gemini `generateContent` JSON, skipping thinking parts.
pub fn extract_text(response: &Value) -> Result<String> {
    let parts = response
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array)
        .context("Gemini response contained no candidate parts")?;

    let mut out = String::new();
    for part in parts {
        if part.get("thought").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
    }
    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        bail!("Gemini candidate contained empty text");
    }
    Ok(trimmed)
}

/// Calls Gemini `generateContent` with automatic fallback across Flash models on
/// `503`/`429`/timeout. Returns `(model_used, text)`.
pub async fn generate_text(
    client: &reqwest::Client,
    api_key: &str,
    configured_model: Option<&str>,
    system: &str,
    user: &str,
    effort: Option<&str>,
    schema: Option<Value>,
) -> Result<(String, String)> {
    let chain = model_chain(configured_model);
    let body = build_generate_body(system, user, effort, schema);
    let mut last_error: Option<String> = None;

    for (idx, model) in chain.iter().enumerate() {
        let url = format!("{API_BASE_URL}/{model}:generateContent");
        let timeout_secs = if idx == 0 { 18 } else { 25 };
        let req = client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(timeout_secs));

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let raw = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    let parsed: Value = serde_json::from_str(&raw)
                        .with_context(|| format!("invalid JSON from Gemini ({model})"))?;
                    let text = extract_text(&parsed)?;
                    return Ok((model.clone(), text));
                }
                let code = status.as_u16();
                let retryable = matches!(code, 404 | 429 | 500 | 502 | 503 | 504);
                let msg = format!("{model} returned HTTP {code}: {}", raw.chars().take(240).collect::<String>());
                last_error = Some(msg.clone());
                if !retryable {
                    return Err(anyhow!(msg));
                }
                eprintln!("[WARN] Gemini fallback triggered: {msg}");
            }
            Err(err) => {
                let msg = format!("{model} transport error: {err}");
                last_error = Some(msg.clone());
                eprintln!("[WARN] Gemini fallback triggered: {msg}");
            }
        }
    }

    Err(anyhow!(
        "all Gemini models failed ({})",
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_chain_starts_with_configured_or_38_flash_and_deduplicates() {
        let c = model_chain(Some("gemini-3.8-flash"));
        assert_eq!(c[0], "gemini-3.8-flash");
        assert!(c.contains(&"gemini-3.5-flash".to_string()));
        assert_eq!(c.iter().filter(|m| m.as_str() == "gemini-3.8-flash").count(), 1);
    }

    #[test]
    fn sanitize_removes_additional_properties_and_extracts_non_thought_text() {
        let schema = json!({
            "type": "object",
            "properties": { "lyrics": { "type": "string" } },
            "additionalProperties": false
        });
        let cleaned = sanitize_schema_for_gemini(schema);
        assert!(cleaned.get("additionalProperties").is_none());

        let resp = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        { "text": "internal thinking", "thought": true },
                        { "text": "{\"lyrics\":\"ok\"}" }
                    ]
                }
            }]
        });
        assert_eq!(extract_text(&resp).unwrap(), "{\"lyrics\":\"ok\"}");
    }
}
