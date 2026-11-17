//! Google Gemini Cloud Assistant provider (`gemini-3.8-flash` primary + resilient Flash fallback).
//!
//! Uses the native Google Generative Language REST API (`generateContent`) with
//! `x-goog-api-key` authentication, optional JSON Schema enforcement, and
//! automatic fallback across active Flash models when `503 UNAVAILABLE`, `429`,
//! or transient upstream timeouts occur.

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "gemini-3-flash-preview";
pub const API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const FALLBACK_CHAIN: &[&str] = &[
    "gemini-3-flash-preview",
    "gemini-3.1-flash-lite",
    "gemini-3.5-flash-lite",
    "gemini-flash-lite-latest",
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

    let started = std::time::Instant::now();
    let total_budget = std::time::Duration::from_secs(32);

    for (idx, model) in chain.iter().enumerate() {
        let elapsed = started.elapsed();
        if elapsed >= total_budget {
            break;
        }
        let remaining = total_budget.saturating_sub(elapsed).as_secs().max(3);
        let url = format!("{API_BASE_URL}/{model}:generateContent");
        let timeout_secs = (if idx == 0 { 12 } else { 9 }).min(remaining);
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
    use base64::{engine::general_purpose::STANDARD, Engine};

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
    for model in DEFAULT_IMAGE_MODELS {
        let url = format!("{API_BASE_URL}/{model}:generateContent");
        let resp = match client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .timeout(std::time::Duration::from_secs(25))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = Some(format!("{model}: {e}"));
                continue;
            }
        };

        let status = resp.status();
        let raw = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            last_error = Some(format!("{model} HTTP {}", status.as_u16()));
            if matches!(status.as_u16(), 400 | 401 | 403) {
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
    let system = "You are a fine-art album cover illustrator creating standalone 1024x1024 SVG artwork.\n\
        Output ONLY a valid <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\" width=\"1024\" height=\"1024\">...</svg> document.\n\
        Rules:\n\
        - Rich painterly atmosphere using <defs> linearGradient/radialGradient, layered silhouettes, celestial/landscape/folk-instrument or abstract geometric forms.\n\
        - NO <text>, NO letters, NO words, NO logos, NO <script>, NO external URLs.\n\
        - Keep total SVG under 8000 characters, clean and visually striking at both 78px thumbnail and 1024px full view.";

    let user = format!(
        "Song title: {title}\nMusical style: {style}\nScene idea: {cover_prompt}\nCreate the 1024x1024 SVG album cover now."
    );

    let (_model, text) = generate_text(client, api_key, None, system, &user, Some("low"), None).await?;
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
}
