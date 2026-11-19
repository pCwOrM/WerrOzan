//! Google Gemini Cloud Assistant provider (`gemini-3.8-flash` primary + resilient Flash fallback).
//!
//! Uses the native Google Generative Language REST API (`generateContent`) with
//! `x-goog-api-key` authentication, optional JSON Schema enforcement, and
//! automatic fallback across active Flash models when `503 UNAVAILABLE`, `429`,
//! or transient upstream timeouts occur.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "gemini-3-flash-preview";
pub const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const FALLBACK_CHAIN: &[&str] = &[
    "gemini-3-flash-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.1-flash-lite",
    "gemini-3.8-flash",
    "gemini-3.5-flash",
    "gemini-flash-lite-latest",
    "gemini-flash-latest",
];

/// Cooldown duration after a (key, model) pair hits `429` (quota/rate limit).
const KEY_QUOTA_COOLDOWN: Duration = Duration::from_secs(180);
/// Cooldown duration after a key hits `401`/`403` (invalid/revoked).
const KEY_AUTH_COOLDOWN: Duration = Duration::from_secs(600);

fn key_cooldowns() -> &'static Mutex<HashMap<String, Instant>> {
    static MAP: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Non-sensitive short fingerprint for logging and cooldown tracking (never logs full key).
fn key_fingerprint(key: &str) -> String {
    let trimmed = key.trim();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}

fn cooldown_slot(key: &str, model: Option<&str>) -> String {
    let fp = key_fingerprint(key);
    match model {
        Some(m) => format!("{fp}::{m}"),
        None => fp,
    }
}

fn mark_key_cooldown(key: &str, duration: Duration, reason: &str) {
    let slot = cooldown_slot(key, None);
    if let Ok(mut guard) = key_cooldowns().lock() {
        guard.insert(slot.clone(), Instant::now() + duration);
    }
    eprintln!(
        "[WARN] Gemini key {slot} placed on {}s cooldown ({reason})",
        duration.as_secs()
    );
}

fn mark_key_model_cooldown(key: &str, model: &str, duration: Duration, reason: &str) {
    let slot = cooldown_slot(key, Some(model));
    if let Ok(mut guard) = key_cooldowns().lock() {
        guard.insert(slot.clone(), Instant::now() + duration);
    }
    eprintln!(
        "[WARN] Gemini slot {slot} placed on {}s cooldown ({reason})",
        duration.as_secs()
    );
}

fn is_slot_in_cooldown(slot: &str) -> bool {
    let Ok(mut guard) = key_cooldowns().lock() else {
        return false;
    };
    let now = Instant::now();
    if let Some(&until) = guard.get(slot) {
        if until > now {
            return true;
        }
        guard.remove(slot);
    }
    false
}

fn is_key_in_cooldown(key: &str) -> bool {
    is_slot_in_cooldown(&cooldown_slot(key, None))
}

fn is_key_model_in_cooldown(key: &str, model: &str) -> bool {
    is_key_in_cooldown(key) || is_slot_in_cooldown(&cooldown_slot(key, Some(model)))
}

/// Orders keys so healthy (non-cooled-down) keys are tried first, preserving relative priority.
/// If all keys are in cooldown, returns them in original order as a last-resort retry.
fn ordered_keys(api_keys: &[String]) -> Vec<String> {
    ordered_keys_for_model(api_keys, None)
}

fn ordered_keys_for_model(api_keys: &[String], model: Option<&str>) -> Vec<String> {
    let mut healthy = Vec::new();
    let mut cooled = Vec::new();
    for key in api_keys {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let in_cooldown = match model {
            Some(m) => is_key_model_in_cooldown(trimmed, m),
            None => is_key_in_cooldown(trimmed),
        };
        if in_cooldown {
            cooled.push(trimmed.to_string());
        } else {
            healthy.push(trimmed.to_string());
        }
    }
    healthy.extend(cooled);
    healthy
}

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
        "off" | "none" | "minimal" | "low" => Some("low"),
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

/// Calls Gemini `generateContent` with automatic multi-key + multi-model failover on
/// `429`/`503`/timeout. Returns `(model_used, text)`.
pub async fn generate_text(
    client: &reqwest::Client,
    api_key: &str,
    configured_model: Option<&str>,
    system: &str,
    user: &str,
    effort: Option<&str>,
    schema: Option<Value>,
) -> Result<(String, String)> {
    generate_text_with_keys(
        client,
        &[api_key.to_string()],
        configured_model,
        system,
        user,
        effort,
        schema,
    )
    .await
}

/// Calls Gemini `generateContent` across an ordered pool of API keys with
/// model-first healthy-key rotation, per-(key, model) `429` cooldown, and fast
/// Flash fallback on `503`/timeout. Returns `(model_used, text)`.
pub async fn generate_text_with_keys(
    client: &reqwest::Client,
    api_keys: &[String],
    configured_model: Option<&str>,
    system: &str,
    user: &str,
    effort: Option<&str>,
    schema: Option<Value>,
) -> Result<(String, String)> {
    let base_keys: Vec<String> = api_keys
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();
    if base_keys.is_empty() {
        bail!("no Gemini API key configured");
    }

    let chain = model_chain(configured_model);
    let body = build_generate_body(system, user, effort, schema);
    let mut last_error: Option<String> = None;

    let started = Instant::now();
    // Stay well below Cloudflare's 100s gateway timeout & frontend's 95s abort timeout
    // so callers always get a crisp JSON response or error instead of a browser timeout.
    let total_budget = Duration::from_secs(56);

    for (model_idx, model) in chain.iter().enumerate() {
        if started.elapsed() >= total_budget {
            break;
        }
        let ordered = ordered_keys_for_model(&base_keys, Some(model));
        let has_healthy = ordered.iter().any(|k| !is_key_model_in_cooldown(k, model));
        let mut model_503_or_timeout_count: usize = 0;

        for (key_idx, api_key) in ordered.iter().enumerate() {
            let elapsed = started.elapsed();
            if elapsed >= total_budget {
                break;
            }
            // If we already tried all healthy keys for this model, don't waste time
            // retrying cooled-down keys unless we're on the very last fallback model.
            if has_healthy
                && is_key_model_in_cooldown(api_key, model)
                && model_idx + 1 < chain.len()
            {
                continue;
            }

            let fp = key_fingerprint(api_key);
            let remaining = total_budget.saturating_sub(elapsed).as_secs().max(3);
            let url = format!("{API_BASE_URL}/{model}:generateContent");
            // Give the top two verified-fast models 18s (full Turkish song JSON needs ~4-12s),
            // and fallback tiers 11s so we fail over briskly if a tier stalls.
            let timeout_secs = (if model_idx < 2 { 18 } else { 11 }).min(remaining);
            let req = client
                .post(&url)
                .header("x-goog-api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(Duration::from_secs(timeout_secs));

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let raw = resp.text().await.unwrap_or_default();
                    if status.is_success() {
                        let parsed: Value = serde_json::from_str(&raw)
                            .with_context(|| format!("invalid JSON from Gemini ({model})"))?;
                        let text = extract_text(&parsed)?;
                        if key_idx > 0 || model_idx > 0 {
                            eprintln!("[INFO] Gemini succeeded on key {fp} ({model})");
                        }
                        return Ok((model.clone(), text));
                    }
                    let code = status.as_u16();
                    let snippet: String = raw.chars().take(200).collect();
                    let msg = format!("{model} (key {fp}) returned HTTP {code}: {snippet}");
                    last_error = Some(msg.clone());

                    if matches!(code, 401 | 403) {
                        mark_key_cooldown(api_key, KEY_AUTH_COOLDOWN, &format!("HTTP {code}"));
                        continue;
                    }
                    if code == 429 {
                        mark_key_model_cooldown(api_key, model, KEY_QUOTA_COOLDOWN, "HTTP 429 quota");
                        continue;
                    }
                    if code == 404 {
                        // Model does not exist on this API version; skip to next model immediately.
                        eprintln!("[WARN] Gemini model unavailable: {msg}");
                        break;
                    }
                    let retryable = matches!(code, 500 | 502 | 503 | 504);
                    if !retryable {
                        return Err(anyhow!(msg));
                    }
                    model_503_or_timeout_count += 1;
                    eprintln!("[WARN] Gemini fallback triggered: {msg}");
                    // If 2 keys see 503/5xx on the same model, the model itself is overloaded globally;
                    // jump to the next model instead of retrying every remaining key on it.
                    if model_503_or_timeout_count >= 2 {
                        break;
                    }
                }
                Err(err) => {
                    let msg = format!("{model} (key {fp}) transport error: {err}");
                    last_error = Some(msg.clone());
                    model_503_or_timeout_count += 1;
                    eprintln!("[WARN] Gemini fallback triggered: {msg}");
                    if model_503_or_timeout_count >= 2 {
                        break;
                    }
                }
            }
        }
    }

    Err(anyhow!(
        "all Gemini keys/models failed ({})",
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

pub const DEFAULT_IMAGE_MODELS: &[&str] = &[
    "gemini-2.5-flash-image",
    "gemini-3.1-flash-lite-image",
    "gemini-3.1-flash-image-preview",
];

/// Attempts native raster image generation via Gemini image models.
/// Returns `(image_bytes, mime_type)` if the key has image quota enabled (billing active).
pub async fn generate_cover_image(
    client: &reqwest::Client,
    api_key: &str,
    prompt: &str,
) -> Result<(Vec<u8>, String)> {
    generate_cover_image_with_keys(client, &[api_key.to_string()], prompt).await
}

/// Multi-key failover wrapper for native raster album cover generation.
pub async fn generate_cover_image_with_keys(
    client: &reqwest::Client,
    api_keys: &[String],
    prompt: &str,
) -> Result<(Vec<u8>, String)> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let keys = ordered_keys(api_keys);
    if keys.is_empty() {
        bail!("no Gemini API key configured");
    }

    let body = json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": format!("Square album cover artwork, atmospheric, painterly, no text, no letters, no watermark: {prompt}") }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE", "TEXT"]
        }
    });

    let mut last_error: Option<String> = None;
    for api_key in &keys {
        let fp = key_fingerprint(api_key);
        for model in DEFAULT_IMAGE_MODELS {
            let url = format!("{API_BASE_URL}/{model}:generateContent");
            let resp = match client
                .post(&url)
                .header("x-goog-api-key", api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .timeout(Duration::from_secs(18))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(format!("{model} ({fp}): {e}"));
                    continue;
                }
            };

            let status = resp.status();
            let raw = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                let code = status.as_u16();
                last_error = Some(format!("{model} ({fp}) HTTP {code}"));
                if matches!(code, 400 | 401 | 403 | 429) {
                    break;
                }
                continue;
            }

            let parsed: Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(e) => {
                    last_error = Some(format!("{model} JSON error: {e}"));
                    continue;
                }
            };

            if let Some(parts) = parsed.pointer("/candidates/0/content/parts").and_then(Value::as_array) {
                for part in parts {
                    if let Some(inline) = part.get("inlineData") {
                        let b64 = inline.get("data").and_then(Value::as_str).unwrap_or("");
                        let mime = inline
                            .get("mimeType")
                            .and_then(Value::as_str)
                            .unwrap_or("image/png")
                            .to_string();
                        if let Ok(bytes) = STANDARD.decode(b64.trim()) {
                            if !bytes.is_empty() {
                                return Ok((bytes, mime));
                            }
                        }
                    }
                }
            }
            last_error = Some(format!("{model} returned no inlineData image"));
        }
    }

    Err(anyhow!(
        "Gemini image models unavailable ({})",
        last_error.unwrap_or_else(|| "quota or unsupported".into())
    ))
}

/// Extracts and sanitizes a standalone `<svg ...>...</svg>` document from model output.
pub fn sanitize_svg_document(raw: &str) -> Result<Vec<u8>> {
    let trimmed = raw
        .trim()
        .trim_start_matches("```svg")
        .trim_start_matches("```xml")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let start = trimmed.find("<svg").context("response did not contain <svg")?;
    let end = trimmed.rfind("</svg>").context("response did not contain </svg>")?;
    if end <= start {
        bail!("malformed svg bounds");
    }
    let mut svg = trimmed[start..end + 6].to_string();
    // Ensure XML namespace so browsers render standalone .svg files reliably.
    if !svg.contains("xmlns=\"http://www.w3.org/2000/svg\"") && !svg.contains("xmlns='http://www.w3.org/2000/svg'") {
        svg = svg.replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1);
    }
    let lower = svg.to_ascii_lowercase();
    if lower.contains("<script") || lower.contains("<foreignobject") || lower.contains("javascript:") {
        bail!("svg contained disallowed active content");
    }
    Ok(svg.into_bytes())
}

/// Uses the free-tier Gemini text model chain to compose a bespoke 1024x1024 artistic SVG album cover.
pub async fn generate_cover_svg(
    client: &reqwest::Client,
    api_key: &str,
    title: &str,
    style: &str,
    cover_prompt: &str,
) -> Result<Vec<u8>> {
    generate_cover_svg_with_keys(client, &[api_key.to_string()], title, style, cover_prompt).await
}

/// Multi-key failover wrapper for bespoke SVG album cover generation.
pub async fn generate_cover_svg_with_keys(
    client: &reqwest::Client,
    api_keys: &[String],
    title: &str,
    style: &str,
    cover_prompt: &str,
) -> Result<Vec<u8>> {
    let system = "You are a fine-art album cover illustrator creating standalone 1024x1024 SVG artwork.\n\
        Output ONLY a valid <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\" width=\"1024\" height=\"1024\">...</svg> document.\n\
        Rules:\n\
        - Rich painterly atmosphere using <defs> linearGradient/radialGradient, layered silhouettes, celestial/landscape/folk-instrument or abstract geometric forms.\n\
        - NO <text>, NO letters, NO words, NO logos, NO <script>, NO external URLs.\n\
        - Keep total SVG under 8000 characters, clean and visually striking at both 78px thumbnail and 1024px full view.";

    let user = format!(
        "Song title: {title}\nMusical style: {style}\nScene idea: {cover_prompt}\nCreate the 1024x1024 SVG album cover now."
    );

    let (_model, text) = generate_text_with_keys(client, api_keys, None, system, &user, Some("low"), None).await?;
    sanitize_svg_document(&text)
}

/// Deterministic zero-dependency SVG cover used only if Gemini is completely unreachable.
pub fn procedural_fallback_svg(title: &str, style: &str) -> Vec<u8> {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in title.bytes().chain(style.bytes()) {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let hue1 = (hash % 360) as u16;
    let hue2 = ((hash >> 9) % 360) as u16;
    let hue3 = ((hue1 + 42) % 360) as u16;
    let sun_x = 240 + ((hash >> 16) % 544);
    let sun_y = 220 + ((hash >> 24) % 240);
    let r_outer = 110 + ((hash >> 32) % 90);
    let r_inner = (r_outer * 62) / 100;
    let p1 = 560 + ((hash >> 40) % 90);
    let p2 = 650 + ((hash >> 48) % 80);

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0.3" y2="1">
      <stop offset="0%" stop-color="hsl({hue1}, 58%, 14%)"/>
      <stop offset="55%" stop-color="hsl({hue2}, 52%, 24%)"/>
      <stop offset="100%" stop-color="hsl({hue3}, 65%, 18%)"/>
    </linearGradient>
    <radialGradient id="glow" cx="50%" cy="50%" r="50%">
      <stop offset="0%" stop-color="hsl({hue3}, 90%, 72%)" stop-opacity="0.95"/>
      <stop offset="55%" stop-color="hsl({hue3}, 80%, 52%)" stop-opacity="0.35"/>
      <stop offset="100%" stop-color="hsl({hue3}, 80%, 40%)" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1024" height="1024" fill="url(#sky)"/>
  <circle cx="{sun_x}" cy="{sun_y}" r="{r_outer}" fill="url(#glow)"/>
  <circle cx="{sun_x}" cy="{sun_y}" r="{r_inner}" fill="hsl({hue3}, 88%, 74%)" opacity="0.88"/>
  <path d="M0 {p1} Q256 {p1_up} 512 {p1} T1024 {p1} L1024 1024 L0 1024 Z" fill="hsl({hue1}, 46%, 12%)" opacity="0.85"/>
  <path d="M0 {p2} Q320 {p2_up} 640 {p2} T1024 {p2_end} L1024 1024 L0 1024 Z" fill="hsl({hue2}, 42%, 8%)"/>
  <circle cx="512" cy="512" r="390" fill="none" stroke="hsl({hue3}, 75%, 70%)" stroke-width="2" opacity="0.22"/>
</svg>"##,
        p1_up = p1.saturating_sub(95),
        p2_up = p2.saturating_sub(70),
        p2_end = p2 + 25,
    );
    svg.into_bytes()
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

    #[test]
    fn ordered_keys_deprioritizes_cooled_down_key() {
        let k1 = "test-key-alpha-111111".to_string();
        let k2 = "test-key-beta-222222".to_string();
        mark_key_cooldown(&k1, Duration::from_secs(60), "unit test 429");
        let ordered = ordered_keys(&[k1.clone(), k2.clone()]);
        assert_eq!(ordered, vec![k2, k1]);
    }
}
