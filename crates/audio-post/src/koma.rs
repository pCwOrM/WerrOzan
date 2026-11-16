//! STFT Phase-Vocoder Acoustic Koma Quantizer for 53-TET Turkish Makams.
//!
//! Analyzes stereo tracks with a 4096-point Hann STFT (`hop = 1024`), detects
//! tonal spectral peaks via parabolic / instantaneous-frequency estimation,
//! shifts active peak neighborhoods using Laroche-Dolson peak-locked phase
//! rotation toward the nearest 53-koma degree of the selected makam, and
//! preserves inter-channel stereo phase differences and unvoiced consonants.

use anyhow::{bail, Result};
use music_core::koma::{self, MakamId};
use realfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::stft::{mid_magnitudes, Spectrogram, Stft};
use crate::Stereo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KomaSettings {
    /// Makam name (`rast`, `ussak`, `huzzam`, `hicaz`, `segah`, `nihavent`, `huseyni`, `kurdi`).
    pub makam: String,
    /// Optional tonic frequency in Hz (defaults to the makam's canonical tonic).
    pub root_hz: Option<f64>,
    /// Pull strength in `0.0..=1.0` (`0.0` = bypass, `1.0` = full snap to 53-koma grid).
    pub strength: f32,
    /// Lower analysis boundary in Hz.
    pub min_hz: f32,
    /// Upper analysis boundary in Hz.
    pub max_hz: f32,
}

impl Default for KomaSettings {
    fn default() -> Self {
        Self {
            makam: "rast".to_string(),
            root_hz: None,
            strength: 0.75,
            min_hz: 90.0,
            max_hz: 2400.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct KomaQuantizeReport {
    pub makam: MakamId,
    pub root_hz: f64,
    pub strength: f32,
    pub frames_processed: usize,
    pub peaks_shifted: usize,
    pub mean_cent_correction: f64,
}

#[inline]
fn wrap_pi(mut x: f64) -> f64 {
    const TWO_PI: f64 = std::f64::consts::TAU;
    x = (x + std::f64::consts::PI).rem_euclid(TWO_PI) - std::f64::consts::PI;
    x
}

/// Applies 53-koma makam quantization on stereo audio without returning telemetry.
pub fn quantize(audio: &Stereo, settings: &KomaSettings) -> Result<Stereo> {
    let (out, _) = quantize_with_report(audio, settings)?;
    Ok(out)
}

/// Applies 53-koma makam quantization on stereo audio and returns shift telemetry.
pub fn quantize_with_report(audio: &Stereo, settings: &KomaSettings) -> Result<(Stereo, KomaQuantizeReport)> {
    let makam = koma::parse_makam(&settings.makam)
        .ok_or_else(|| anyhow::anyhow!("unknown makam `{}`", settings.makam))?;
    let sp = koma::spec(makam);
    let root_hz = settings.root_hz.filter(|hz| *hz > 20.0).unwrap_or(sp.default_root_hz);
    let strength = settings.strength.clamp(0.0, 1.0);

    if strength <= 0.0 || audio.frames() == 0 {
        return Ok((
            audio.clone(),
            KomaQuantizeReport {
                makam,
                root_hz,
                strength,
                frames_processed: 0,
                peaks_shifted: 0,
                mean_cent_correction: 0.0,
            },
        ));
    }

    if audio.rate == 0 {
        bail!("audio sample rate must be positive");
    }

    const FFT_SIZE: usize = 4096;
    const HOP: usize = 1024;
    const LOBE_RADIUS: usize = 3;

    let stft = Stft::new(FFT_SIZE, HOP);
    let left_spec = stft.analyze(&audio.left);
    let right_spec = stft.analyze(&audio.right);
    let mags = mid_magnitudes(&left_spec, &right_spec);

    let bins = left_spec.bins();
    let bin_hz = f64::from(audio.rate) / (FFT_SIZE as f64);
    let min_bin = ((f64::from(settings.min_hz.max(40.0)) / bin_hz).floor() as usize).clamp(LOBE_RADIUS + 1, bins.saturating_sub(LOBE_RADIUS + 2));
    let max_bin = ((f64::from(settings.max_hz.max(settings.min_hz + 100.0)) / bin_hz).ceil() as usize).clamp(min_bin + 1, bins.saturating_sub(LOBE_RADIUS + 2));

    let expected_step = std::f64::consts::TAU * (HOP as f64) / (FFT_SIZE as f64);
    let mut prev_ana_phase = vec![0.0_f64; bins];
    let mut peak_rot = vec![0.0_f64; bins];

    let mut out_left_frames = left_spec.frames.clone();
    let mut out_right_frames = right_spec.frames.clone();

    let mut peaks_shifted = 0usize;
    let mut abs_cent_sum = 0.0_f64;
    let frame_count = mags.len();

    for f in 0..frame_count {
        let mag = &mags[f];
        let l_in = &left_spec.frames[f];
        let r_in = &right_spec.frames[f];

        let mut ana_phase = vec![0.0_f64; bins];
        for k in 0..bins {
            let mid = (l_in[k] + r_in[k]) * 0.5;
            ana_phase[k] = f64::from(mid.im).atan2(f64::from(mid.re));
        }

        let band_len = (max_bin - min_bin + 1) as f64;
        let band_mean = mag[min_bin..=max_bin].iter().map(|&v| f64::from(v)).sum::<f64>() / band_len;
        let peak_floor = band_mean * 1.8;

        if band_mean < 1e-6 {
            prev_ana_phase = ana_phase;
            continue;
        }

        let mut new_l = l_in.clone();
        let mut new_r = r_in.clone();
        let mut cleared = vec![false; bins];
        let mut next_rot = vec![0.0_f64; bins];

        // Collect active tonal peaks in [min_bin..=max_bin]
        let mut shifts: Vec<(usize, f64, f64)> = Vec::new();
        for k in min_bin..=max_bin {
            if mag[k] > mag[k - 1] && mag[k] >= mag[k + 1] && f64::from(mag[k]) > peak_floor {
                let m1 = f64::from(mag[k - 1]);
                let m2 = f64::from(mag[k]);
                let m3 = f64::from(mag[k + 1]);
                let denom = m1 - 2.0 * m2 + m3;
                let para_delta = if denom.abs() > 1e-12 {
                    (0.5 * (m1 - m3) / denom).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                let est_bin = if f == 0 {
                    k as f64 + para_delta
                } else {
                    let dp = wrap_pi(ana_phase[k] - prev_ana_phase[k] - (k as f64) * expected_step);
                    let pv_bin = (k as f64) + dp / expected_step;
                    if (pv_bin - (k as f64)).abs() <= 1.2 { pv_bin } else { k as f64 + para_delta }
                };

                let obs_hz = est_bin * bin_hz;
                let (target_hz, cents_diff) = koma::nearest_makam_frequency(makam, root_hz, obs_hz);
                if cents_diff.abs() >= 3.0 && cents_diff.abs() <= 45.0 {
                    let pulled_hz = obs_hz + (target_hz - obs_hz) * f64::from(strength);
                    let target_bin = (pulled_hz / bin_hz).clamp(2.0, (bins - 3) as f64);
                    shifts.push((k, target_bin, est_bin));
                    peaks_shifted += 1;
                    abs_cent_sum += cents_diff.abs() * f64::from(strength);
                }
            }
        }

        if !shifts.is_empty() {
            for &(pk, _, _) in &shifts {
                let lo = pk.saturating_sub(LOBE_RADIUS);
                let hi = (pk + LOBE_RADIUS).min(bins - 1);
                for b in lo..=hi {
                    if !cleared[b] {
                        new_l[b] = Complex32::new(0.0, 0.0);
                        new_r[b] = Complex32::new(0.0, 0.0);
                        cleared[b] = true;
                    }
                }
            }

            for &(pk, target_bin, est_bin) in &shifts {
                let delta_bin = target_bin - est_bin;
                // Identity phase locking: carry peak rotation continuously across frames,
                // inheriting from pk-1..=pk+1 if the peak crossed a bin boundary.
                let prev_rot = [peak_rot[pk], peak_rot[pk.saturating_sub(1)], peak_rot[(pk + 1).min(bins - 1)]]
                    .into_iter()
                    .find(|&r| r != 0.0)
                    .unwrap_or(0.0);
                let rot = if f == 0 {
                    0.0
                } else {
                    wrap_pi(prev_rot + delta_bin * expected_step)
                };
                next_rot[pk] = rot;

                let phasor = Complex32::from_polar(1.0, rot as f32);
                let lo = pk.saturating_sub(LOBE_RADIUS);
                let hi = (pk + LOBE_RADIUS).min(bins - 1);
                for b in lo..=hi {
                    let dst_f = (b as f64 + delta_bin).clamp(1.0, (bins - 2) as f64);
                    let dst0 = dst_f.floor() as usize;
                    let frac = (dst_f - dst0 as f64) as f32;

                    let rot_l = l_in[b] * phasor;
                    let rot_r = r_in[b] * phasor;

                    new_l[dst0] += rot_l * (1.0 - frac);
                    new_l[dst0 + 1] += rot_l * frac;
                    new_r[dst0] += rot_r * (1.0 - frac);
                    new_r[dst0 + 1] += rot_r * frac;
                }
            }
        }

        peak_rot = next_rot;
        prev_ana_phase = ana_phase;
        out_left_frames[f] = new_l;
        out_right_frames[f] = new_r;
    }

    let left_out_spec = Spectrogram {
        frames: out_left_frames,
        len: audio.left.len(),
    };
    let right_out_spec = Spectrogram {
        frames: out_right_frames,
        len: audio.right.len(),
    };
    let left = stft.synthesize(&left_out_spec);
    let right = stft.synthesize(&right_out_spec);

    let mut out = Stereo::new(left, right, audio.rate);
    out.keep_below(0.99);

    let mean_cent_correction = if peaks_shifted == 0 {
        0.0
    } else {
        ((abs_cent_sum / (peaks_shifted as f64)) * 100.0).round() / 100.0
    };

    Ok((
        out,
        KomaQuantizeReport {
            makam,
            root_hz,
            strength,
            frames_processed: frame_count,
            peaks_shifted,
            mean_cent_correction,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use realfft::RealFftPlanner;

    fn dominant_hz(signal: &[f32], rate: u32) -> f64 {
        let n = 16384.min(signal.len()).next_power_of_two() / 2;
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let mut input: Vec<f32> = signal[signal.len() - n..].to_vec();
        for (i, x) in input.iter_mut().enumerate() {
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
            *x *= w;
        }
        let mut spec = fft.make_output_vec();
        fft.process(&mut input, &mut spec).unwrap();
        let (k, _) = spec.iter().enumerate().skip(1).max_by(|a, b| a.1.norm().partial_cmp(&b.1.norm()).unwrap()).unwrap();
        let m1 = f64::from(spec[k - 1].norm());
        let m2 = f64::from(spec[k].norm());
        let m3 = f64::from(spec[k + 1].norm());
        let delta = 0.5 * (m1 - m3) / (m1 - 2.0 * m2 + m3);
        (k as f64 + delta) * f64::from(rate) / (n as f64)
    }

    #[test]
    fn strength_zero_is_bit_exact_original() {
        let x: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.04).sin() * 0.4).collect();
        let s = Stereo::new(x.clone(), x, 44100);
        let (out, rep) = quantize_with_report(&s, &KomaSettings { makam: "rast".into(), strength: 0.0, ..Default::default() }).unwrap();
        assert_eq!(out.left, s.left);
        assert_eq!(rep.peaks_shifted, 0);
    }

    #[test]
    fn rast_quantizer_pulls_12tet_b3_down_toward_segah_17_komas() {
        let rate = 44100;
        let len = rate / 2;
        let f_12tet_b3 = 246.9417_f64;
        let x: Vec<f32> = (0..len)
            .map(|i| (std::f64::consts::TAU * f_12tet_b3 * (i as f64) / f64::from(rate)).sin() as f32 * 0.5)
            .collect();
        let stereo = Stereo::new(x.clone(), x, rate);
        let (out, rep) = quantize_with_report(
            &stereo,
            &KomaSettings {
                makam: "rast".into(),
                root_hz: Some(195.9977),
                strength: 1.0,
                min_hz: 90.0,
                max_hz: 1200.0,
            },
        )
        .unwrap();
        assert!(rep.peaks_shifted > 0);
        let out_hz = dominant_hz(&out.left, rate);
        // Expected 17-koma Rast Segah above 195.9977 Hz ≈ 244.78 Hz (down from 246.94 Hz)
        assert!(out_hz < 245.5 && out_hz > 244.0, "expected ~244.78 Hz, got {out_hz:.2} Hz");
    }
}
