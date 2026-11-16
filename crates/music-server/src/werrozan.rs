//! WerrOzan Phase 2 Orchestration Endpoints:
//! - Deterministic Güneş Dili morpho-semantic prosody & syllable meter analysis
//! - WERR $\mathbb{Z}/9\mathbb{Z}$ procedural boundary structure & CPU budget gating
//! - Google Gemini (`gemini-3.8-flash` + Flash fallback) credential settings

use axum::{http::StatusCode, Json};
use music_core::werr::{self, FractalSeed24, SectionProbeInput, WerrStructureReport};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{api_error, credentials, gunes_dili::{self, GunesDiliReport}, providers, remote, ApiError};

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    #[serde(default)]
    pub lyrics: String,
    #[serde(default = "default_duration_s")]
    pub duration_seconds: f64,
    #[serde(default)]
    pub seed: Option<FractalSeed24>,
    #[serde(default = "default_cpu_only")]
    pub cpu_only: bool,
}

fn default_duration_s() -> f64 {
    120.0
}

fn default_cpu_only() -> bool {
    true
}

#[derive(Debug, serde::Serialize)]
pub struct WerrOzanAnalysis {
    pub gunes_dili: GunesDiliReport,
    pub werr: WerrStructureReport,
}

pub fn analyze_text(lyrics: &str, duration_seconds: f64, seed: Option<FractalSeed24>, cpu_only: bool) -> WerrOzanAnalysis {
    let gunes_dili = gunes_dili::analyze_lyrics(lyrics, duration_seconds);
    let section_inputs: Vec<SectionProbeInput> = gunes_dili
        .sections
        .iter()
        .map(|sec| SectionProbeInput {
            tag: sec.tag.clone(),
            syllable_count: sec.total_syllables,
            line_count: sec.lines.len(),
        })
        .collect();
    let werr = werr::evaluate_song_structure(
        seed.unwrap_or_default(),
        &section_inputs,
        duration_seconds,
        cpu_only,
    );
    WerrOzanAnalysis { gunes_dili, werr }
}

pub async fn analyze(Json(req): Json<AnalyzeRequest>) -> Json<WerrOzanAnalysis> {
    Json(analyze_text(&req.lyrics, req.duration_seconds, req.seed, req.cpu_only))
}

pub async fn status() -> Json<Value> {
    let chain = providers::gemini::model_chain(None);
    Json(json!({
        "name": "WerrOzan",
        "phase": 2,
        "gemini_ready": credentials::gemini_source().is_some(),
        "gemini_source": credentials::gemini_source(),
        "gemini_model": chain.first().cloned().unwrap_or_else(|| providers::gemini::DEFAULT_MODEL.to_string()),
        "gemini_fallback_chain": chain,
        "gunes_dili": true,
        "werr_zmod9": true,
        "koma_engine": "phase3-planned",
        "allowed_origins": remote::allowed_origins(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct GeminiSettingsRequest {
    pub api_key: Option<String>,
}

pub async fn update_gemini_settings(
    Json(req): Json<GeminiSettingsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let source = credentials::store_gemini_api_key(req.api_key.as_deref())
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, format!("{error:#}")))?;
    Ok(Json(json!({
        "gemini_api_key_set": source.is_some(),
        "source": source,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_text_combines_gunes_dili_and_werr_zero_vram() {
        let lyrics = "[Verse 1]\nYıldızlar parlar gece göğünde\nRüzgar eser sessiz kıyılarda\n\n[Chorus]\nYükselir sesimiz dağlara doğru\nAçarız kalbimizi yarına";
        let out = analyze_text(lyrics, 120.0, None, true);
        assert_eq!(out.werr.vram_bytes, 0);
        assert_eq!(out.werr.seed_bytes, 24);
        assert_eq!(out.gunes_dili.sections.len(), 2);
        assert_eq!(out.werr.sections[0].residue_z9, 3);
        assert_eq!(out.werr.sections[1].residue_z9, 0);
    }
}
