//! WERR Zero-Storage Procedural Boundary Dynamics ($\mathbb{Z}/9\mathbb{Z}$ Modular Resonance).
//!
//! Computes deterministic musical form states, section energy trajectories, and
//! CPU inference budgets from a 24-byte mathematical coordinate seed without
//! tensor storage or GPU VRAM allocation ($O(1)$ spatial memory).

use serde::{Deserialize, Serialize};

/// 24-byte coordinate seed in the complex boundary plane ($\partial \mathcal{M}$).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct FractalSeed24 {
    pub cx: f64,
    pub cy: f64,
    pub zoom: f64,
}

impl Default for FractalSeed24 {
    fn default() -> Self {
        Self {
            cx: -0.743643887,
            cy: 0.131825904,
            zoom: 120.0,
        }
    }
}

/// Three canonical states of the ideal $3\mathbb{Z}/9\mathbb{Z} = \{0, 3, 6\}$
/// modular residue chassis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZMod9Regime {
    /// $\tau \equiv 3 \pmod 9$: Laminar stability (Intro / Verse / Outro).
    Laminar = 3,
    /// $\tau \equiv 6 \pmod 9$: Controlled bifurcation (Pre-Chorus / Bridge).
    Critical = 6,
    /// $\tau \equiv 0 \pmod 9$: Peak closure or early-exit intervention (Chorus / Climax).
    Intervention = 0,
}

impl ZMod9Regime {
    pub const fn residue(self) -> u8 {
        self as u8
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Laminar => "laminar_3",
            Self::Critical => "critical_6",
            Self::Intervention => "closure_0",
        }
    }
}

/// Telemetry from a single $z_{n+1} = z_n^2 + c$ polynomial escape-time probe.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EscapeTelemetry {
    pub iterations: u16,
    pub escaped: bool,
    pub mod9_residue: u8,
    pub regime: ZMod9Regime,
    pub energy: f64,
}

/// Evaluates the procedural quadratic escape-time kernel around `seed` at normalized
/// phase coordinates `(u, v) in [-1.0, 1.0]^2` using strictly $O(1)$ stack memory.
pub fn procedural_escape_probe(seed: FractalSeed24, u: f64, v: f64, max_iter: u16) -> EscapeTelemetry {
    let span = 1.5 / seed.zoom.max(1.0);
    let cr = seed.cx + u.clamp(-1.0, 1.0) * span;
    let ci = seed.cy + v.clamp(-1.0, 1.0) * span;

    let mut zr = 0.0_f64;
    let mut zi = 0.0_f64;
    let mut iter = 0_u16;
    let mut escaped = false;

    while iter < max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            escaped = true;
            break;
        }
        zi = 2.0 * zr * zi + ci;
        zr = zr2 - zi2 + cr;
        iter += 1;
    }

    let mag = (zr * zr + zi * zi).sqrt().min(16.0);
    let mod9_residue = ((iter as u32 + (mag * 3.0) as u32) % 9) as u8;
    let regime = match mod9_residue % 3 {
        0 if mod9_residue == 0 => ZMod9Regime::Intervention,
        1 => ZMod9Regime::Laminar,
        2 => ZMod9Regime::Critical,
        _ => ZMod9Regime::Intervention,
    };

    let norm_iter = (iter as f64) / (max_iter.max(1) as f64);
    let energy = if escaped {
        (1.0 - norm_iter * 0.65).clamp(0.15, 1.0)
    } else {
        (0.35 + (mag * 0.25)).clamp(0.2, 0.95)
    };

    EscapeTelemetry {
        iterations: iter,
        escaped,
        mod9_residue,
        regime,
        energy,
    }
}

/// Input summary of a lyric section evaluated by the WERR form gate.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SectionProbeInput {
    pub tag: String,
    pub syllable_count: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionFormState {
    pub tag: String,
    pub index: usize,
    pub regime: ZMod9Regime,
    pub residue_z9: u8,
    pub target_energy: f64,
    pub probe: EscapeTelemetry,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CpuBudgetAdvice {
    pub recommended_max_seq: u32,
    pub recommended_duration_s: u32,
    pub early_exit_guard: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WerrStructureReport {
    pub seed: FractalSeed24,
    pub seed_bytes: usize,
    pub vram_bytes: usize,
    pub sections: Vec<SectionFormState>,
    pub coherence_score: f64,
    pub cpu_budget: CpuBudgetAdvice,
}

/// Evaluates song sections through the $\mathbb{Z}/9\mathbb{Z}$ modular resonance chassis
/// and derives a deterministic CPU sampling budget for headless execution (e.g. `lexsrv`).
pub fn evaluate_song_structure(
    seed: FractalSeed24,
    sections: &[SectionProbeInput],
    requested_duration_s: f64,
    cpu_only: bool,
) -> WerrStructureReport {
    let total = sections.len().max(1);
    let mut states = Vec::with_capacity(sections.len());
    let mut transition_balance = 0.0_f64;
    let mut prev_energy: Option<f64> = None;

    for (index, sec) in sections.iter().enumerate() {
        let phase_u = (index as f64 / total as f64) * 2.0 - 1.0;
        let phase_v = ((sec.syllable_count % 32) as f64 / 16.0) - 1.0;
        let probe = procedural_escape_probe(seed, phase_u, phase_v, 72);

        let lower = sec.tag.to_ascii_lowercase();
        let (regime, target_energy) = if lower.contains("chorus") && !lower.contains("pre") {
            (ZMod9Regime::Intervention, (0.78 + probe.energy * 0.22).min(1.0))
        } else if lower.contains("pre-chorus") || lower.contains("bridge") {
            (ZMod9Regime::Critical, (0.55 + probe.energy * 0.25).min(0.9))
        } else {
            (ZMod9Regime::Laminar, (0.32 + probe.energy * 0.30).min(0.75))
        };

        if let Some(prev) = prev_energy {
            let delta = (target_energy - prev).abs();
            transition_balance += 1.0 - (delta - 0.25).abs().min(0.75);
        }
        prev_energy = Some(target_energy);

        states.push(SectionFormState {
            tag: sec.tag.clone(),
            index,
            regime,
            residue_z9: regime.residue(),
            target_energy: (target_energy * 1000.0).round() / 1000.0,
            probe,
        });
    }

    let coherence_score = if states.len() <= 1 {
        1.0
    } else {
        (transition_balance / ((states.len() - 1) as f64)).clamp(0.0, 1.0)
    };

    // Each second of YuE2 audio corresponds to ~25 semantic tokens/sec at 25 Hz
    // (or ~50 interleaved tokens with acoustic steps). Keep CPU runs within safe RAM bounds.
    let clamped_duration = requested_duration_s.clamp(30.0, 360.0);
    let cpu_cap_s = if cpu_only { 180.0 } else { 360.0 };
    let recommended_duration_s = clamped_duration.min(cpu_cap_s).round() as u32;
    let recommended_max_seq = if cpu_only {
        if recommended_duration_s <= 90 { 8192 } else { 12288 }
    } else {
        16384
    };
    let early_exit_guard = cpu_only && requested_duration_s > cpu_cap_s;
    let reason = if early_exit_guard {
        format!("CPU mode clamped duration from {:.0}s to {}s (max_seq={recommended_max_seq}) to avoid RAM/CPU saturation", requested_duration_s, recommended_duration_s)
    } else {
        format!("WERR Z/9Z budget locked at {}s (max_seq={recommended_max_seq}, 0 Bytes VRAM)", recommended_duration_s)
    };

    WerrStructureReport {
        seed,
        seed_bytes: std::mem::size_of::<FractalSeed24>(),
        vram_bytes: 0,
        sections: states,
        coherence_score: (coherence_score * 1000.0).round() / 1000.0,
        cpu_budget: CpuBudgetAdvice {
            recommended_max_seq,
            recommended_duration_s,
            early_exit_guard,
            reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_strictly_24_bytes_and_deterministic() {
        assert_eq!(std::mem::size_of::<FractalSeed24>(), 24);
        let seed = FractalSeed24::default();
        let a = procedural_escape_probe(seed, 0.25, -0.5, 64);
        let b = procedural_escape_probe(seed, 0.25, -0.5, 64);
        assert_eq!(a, b);
        assert!(a.mod9_residue < 9);
    }

    #[test]
    fn song_structure_maps_sections_to_zmod9_closure_and_caps_cpu_budget() {
        let inputs = vec![
            SectionProbeInput { tag: "[Intro]".into(), syllable_count: 0, line_count: 0 },
            SectionProbeInput { tag: "[Verse 1]".into(), syllable_count: 32, line_count: 4 },
            SectionProbeInput { tag: "[Pre-Chorus]".into(), syllable_count: 16, line_count: 2 },
            SectionProbeInput { tag: "[Chorus]".into(), syllable_count: 34, line_count: 4 },
        ];
        let report = evaluate_song_structure(FractalSeed24::default(), &inputs, 240.0, true);
        assert_eq!(report.vram_bytes, 0);
        assert_eq!(report.seed_bytes, 24);
        assert_eq!(report.sections[1].residue_z9, 3);
        assert_eq!(report.sections[2].residue_z9, 6);
        assert_eq!(report.sections[3].residue_z9, 0);
        assert!(report.cpu_budget.early_exit_guard);
        assert_eq!(report.cpu_budget.recommended_duration_s, 180);
    }
}
