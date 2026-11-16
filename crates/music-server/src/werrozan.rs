//! WerrOzan Phase 2 & 3 Orchestration Endpoints:
//! - Deterministic Güneş Dili morpho-semantic prosody & syllable meter analysis
//! - WERR $\mathbb{Z}/9\mathbb{Z}$ procedural boundary structure & CPU budget gating
//! - Google Gemini (`gemini-3.8-flash` + Flash fallback) credential settings
//! - 53-Koma / Turkish Makam symbolic MIDI/ABC and STFT acoustic quantization

use axum::{http::StatusCode, Json};
use base64::Engine;
use music_core::koma::{self as core_koma};
use music_core::werr::{self, FractalSeed24, SectionProbeInput, WerrStructureReport};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{api_error, audio_pcm, credentials, gunes_dili::{self, GunesDiliReport}, providers, remote, score, ApiError};

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
    let mailer_sum = crate::mailer::config_summary();
    Json(json!({
        "name": "WerrOzan",
        "phase": 4,
        "membership": true,
        "mailer": {
            "configured": mailer_sum.configured,
            "dkim_ready": mailer_sum.dkim_ready,
        },
        "gemini_ready": credentials::gemini_source().is_some(),
        "gemini_source": credentials::gemini_source(),
        "gemini_model": chain.first().cloned().unwrap_or_else(|| providers::gemini::DEFAULT_MODEL.to_string()),
        "gemini_fallback_chain": chain,
        "gunes_dili": true,
        "werr_zmod9": true,
        "koma_engine": "53-tet-aeu-active",
        "makam_count": core_koma::catalog().len(),
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

pub async fn makams() -> Json<Value> {
    Json(json!({
        "system": "53-TET Holdrian Comma / Arel-Ezgi-Uzdilek",
        "cents_per_koma": core_koma::CENTS_PER_KOMA,
        "makams": core_koma::catalog(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct KomaScoreRequest {
    pub abc: String,
    #[serde(default = "default_makam")]
    pub makam: String,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default = "default_bend_range")]
    pub bend_range_semitones: f64,
}

fn default_makam() -> String {
    "rast".to_string()
}

fn default_bend_range() -> f64 {
    2.0
}

pub async fn koma_score(
    Json(req): Json<KomaScoreRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let makam = core_koma::parse_makam(&req.makam)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, format!("unknown makam `{}`", req.makam)))?;
    let root_pc = match req.root.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => Some(core_koma::parse_pitch_class(name).ok_or_else(|| {
            api_error(StatusCode::BAD_REQUEST, format!("unknown root note `{name}`"))
        })?),
        None => None,
    };
    let exported = score::koma_midi::export_makam_midi(&req.abc, makam, root_pc, req.bend_range_semitones)
        .map_err(|err| api_error(StatusCode::BAD_REQUEST, err))?;
    let midi_base64 = base64::engine::general_purpose::STANDARD.encode(&exported.midi_bytes);
    Ok(Json(json!({
        "ok": true,
        "makam": exported.makam,
        "root_pc": exported.root_pc,
        "bend_range_semitones": exported.bend_range_semitones,
        "annotated_abc": exported.annotated_abc,
        "midi_base64": midi_base64,
        "note_mappings": exported.note_mappings,
    })))
}

#[derive(Debug, Deserialize)]
pub struct KomaQuantizeRequest {
    #[serde(default)]
    pub song_id: Option<String>,
    #[serde(default)]
    pub audio_base64: Option<String>,
    #[serde(default = "default_format")]
    pub input_format: String,
    #[serde(default = "default_out_format")]
    pub output_format: String,
    #[serde(default = "default_makam")]
    pub makam: String,
    #[serde(default)]
    pub root_hz: Option<f64>,
    #[serde(default = "default_strength")]
    pub strength: f32,
}

fn default_format() -> String {
    "wav".to_string()
}

fn default_out_format() -> String {
    "flac".to_string()
}

fn default_strength() -> f32 {
    0.75
}

const MAX_QUANTIZE_BYTES: usize = 32 * 1024 * 1024;

pub async fn koma_quantize(
    principal: Option<axum::Extension<crate::remote::AuthPrincipal>>,
    Json(req): Json<KomaQuantizeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let owner_info = principal.as_ref().map(|axum::Extension(p)| crate::auth::principal_owner_info(p));
    let mut linked_song: Option<crate::auth::UserSongRecord> = None;

    let (raw, ext) = if let Some(b64) = req.audio_base64.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if let (Some(sid), Some((oid, _, _)), Some(store)) = (
            req.song_id.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            owner_info.as_ref(),
            crate::auth::global(),
        ) {
            linked_song = store
                .get_user_song_for_owner(sid, oid)
                .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| api_error(StatusCode::BAD_REQUEST, "audio_base64 is not valid base64".into()))?;
        let ext = req.input_format.trim().trim_start_matches('.').to_ascii_lowercase();
        (bytes, ext)
    } else if let Some(sid) = req.song_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let store = crate::auth::global()
            .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "Üyelik deposu hazır değil.".into()))?;
        let (oid, _, _) = owner_info
            .as_ref()
            .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "Kimlik doğrulama gerekli.".into()))?;
        let song = store
            .get_user_song_for_owner(sid, oid)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Şarkı bulunamadı veya size ait değil.".into()))?;
        let bytes = tokio::fs::read(&song.file_path)
            .await
            .map_err(|e| api_error(StatusCode::NOT_FOUND, format!("Şarkı dosyası okunamadı: {e}")))?;
        let ext = song.format.trim().trim_start_matches('.').to_ascii_lowercase();
        linked_song = Some(song);
        (bytes, ext)
    } else {
        return Err(api_error(StatusCode::BAD_REQUEST, "audio_base64 veya song_id sağlanmalıdır.".into()));
    };

    if raw.len() > MAX_QUANTIZE_BYTES {
        return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "audio payload exceeds 32 MB limit".into()));
    }
    let out_is_wav = req.output_format.trim().trim_start_matches('.').eq_ignore_ascii_case("wav");
    let makam_id = req.makam.trim().to_ascii_lowercase();
    let makam_name = match makam_id.as_str() {
        "hicaz" => "Hicaz".to_string(),
        "ussak" => "Uşşak".to_string(),
        "huseyni" => "Hüseynî".to_string(),
        "kurdi" => "Kürdî".to_string(),
        "rast" => "Rast".to_string(),
        "segah" => "Segâh".to_string(),
        "nihavend" => "Nihavend".to_string(),
        "saba" => "Sabâ".to_string(),
        "huzzam" => "Hüzzam".to_string(),
        "mahur" => "Mahur".to_string(),
        "acemasiran" => "Acemaşîran".to_string(),
        "nikriz" => "Nikriz".to_string(),
        _ => req.makam.clone(),
    };
    let strength_val = f64::from(req.strength);
    let settings = audio_post::koma::KomaSettings {
        makam: req.makam,
        root_hz: req.root_hz,
        strength: req.strength,
        ..Default::default()
    };

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<(Vec<u8>, audio_post::koma::KomaQuantizeReport)> {
        let stereo = audio_pcm::decode_stereo_bytes(raw, &ext)?;
        let (quantized, report) = audio_post::koma::quantize_with_report(&stereo, &settings)?;
        let encoded = if out_is_wav {
            audio_pcm::wav24_bytes(&quantized)?
        } else {
            audio_post::encode::flac(&quantized)?
        };
        Ok((encoded, report))
    })
    .await
    .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "quantizer task panicked".into()))?
    .map_err(|err| api_error(StatusCode::BAD_REQUEST, format!("{err:#}")))?;

    let (encoded, report) = result;
    let mut saved_variant: Option<crate::auth::UserKomaVariantRecord> = None;

    if let (Some(song), Some((oid, oemail, _)), Some(store)) = (linked_song, owner_info, crate::auth::global()) {
        let dir = crate::auth::user_media_dir(&oid);
        if tokio::fs::create_dir_all(&dir).await.is_ok() {
            let out_ext = if out_is_wav { "wav" } else { "flac" };
            let dest = dir.join(format!("{}-{makam_id}.{out_ext}", song.id));
            if tokio::fs::write(&dest, &encoded).await.is_ok() {
                if let Ok(variant) = store.upsert_koma_variant(crate::auth::NewKomaVariant {
                    song_id: song.id.clone(),
                    owner_id: oid,
                    makam_id: makam_id.clone(),
                    makam_name: makam_name.clone(),
                    strength: strength_val,
                    file_path: dest.to_string_lossy().into_owned(),
                    peaks_shifted: report.peaks_shifted as i64,
                    mean_cent_correction: report.mean_cent_correction,
                }) {
                    if let Some(email) = oemail.or(song.owner_email) {
                        let song_title = song.title.clone();
                        let share_url = variant.share_url.clone();
                        let mname = makam_name.clone();
                        let pct = (strength_val * 100.0).round() as u32;
                        let mean_c = report.mean_cent_correction;
                        tokio::spawn(async move {
                            let _ = crate::mailer::send_koma_completed_email(
                                &email,
                                &song_title,
                                &mname,
                                pct,
                                mean_c,
                                &share_url,
                            )
                            .await;
                        });
                    }
                    saved_variant = Some(variant);
                }
            }
        }
    }

    Ok(Json(json!({
        "ok": true,
        "format": if out_is_wav { "wav" } else { "flac" },
        "audio_base64": base64::engine::general_purpose::STANDARD.encode(&encoded),
        "report": report,
        "variant": saved_variant,
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

    #[test]
    fn koma_quantizer_end_to_end_roundtrip_works_on_wav_bytes() {
        let rate = 44100;
        let samples: Vec<f32> = (0..22050)
            .map(|i| (std::f64::consts::TAU * 246.9417 * (i as f64) / f64::from(rate)).sin() as f32 * 0.4)
            .collect();
        let stereo = audio_post::Stereo::new(samples.clone(), samples, rate);
        let wav = audio_pcm::wav24_bytes(&stereo).unwrap();
        let decoded = audio_pcm::decode_stereo_bytes(wav, "wav").unwrap();
        let (out, rep) = audio_post::koma::quantize_with_report(
            &decoded,
            &audio_post::koma::KomaSettings {
                makam: "rast".into(),
                root_hz: Some(195.9977),
                strength: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(rep.peaks_shifted > 0);
        assert!(!audio_post::encode::flac(&out).unwrap().is_empty());
    }
}
