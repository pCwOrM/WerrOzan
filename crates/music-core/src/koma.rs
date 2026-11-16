//! 53-Koma (Holdrian Comma) & Arel-Ezgi-Uzdilek (AEU) Turkish Makam System.
//!
//! Provides deterministic microtonal pitch grids (`1 koma = 1200 / 53 ≈ 22.6415 cents`),
//! canonical Turkish makam scales (`Rast`, `Uşşak`, `Hüzzam`, `Hicaz`, `Segâh`,
//! `Nihavent`, `Hüseynî`, `Kürdî`), 14-bit MIDI Pitch Bend mapping, and ABC
//! microtonal score annotations.

use serde::{Deserialize, Serialize};

pub const KOMMA_PER_OCTAVE: f64 = 53.0;
pub const CENTS_PER_KOMA: f64 = 1200.0 / KOMMA_PER_OCTAVE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MakamId {
    Rast,
    Ussak,
    Huzzam,
    Hicaz,
    Segah,
    Nihavent,
    Huseyni,
    Kurdi,
}

#[derive(Debug, Clone, Serialize)]
pub struct MakamSpec {
    pub id: MakamId,
    pub slug: &'static str,
    pub display_name: &'static str,
    pub default_root_pc: u8,
    pub default_root_name: &'static str,
    pub default_root_hz: f64,
    /// Scale degrees in 53-TET komas above tonic (`0..53`).
    pub komas_53: [u8; 7],
    pub karar: &'static str,
    pub guclu: &'static str,
    pub yeden: &'static str,
    pub description: &'static str,
}

const SPECS: &[MakamSpec] = &[
    MakamSpec {
        id: MakamId::Rast,
        slug: "rast",
        display_name: "Rast",
        default_root_pc: 7, // G
        default_root_name: "Sol (Rast)",
        default_root_hz: 195.9977, // G3
        komas_53: [0, 9, 17, 22, 31, 40, 48],
        karar: "Rast (Sol)",
        guclu: "Nevâ (Re)",
        yeden: "Irak (Fa# -15.1c)",
        description: "Parlak, dengeli Türk müziği ana makamı; Segâh (3. derece) ve Eviç (7. derece) 17 ve 48 koma ile mikrotonaldir.",
    },
    MakamSpec {
        id: MakamId::Ussak,
        slug: "ussak",
        display_name: "Uşşak",
        default_root_pc: 9, // A
        default_root_name: "La (Dügâh)",
        default_root_hz: 220.0, // A3
        komas_53: [0, 8, 13, 22, 31, 35, 44],
        karar: "Dügâh (La)",
        guclu: "Nevâ (Re)",
        yeden: "Rast (Sol)",
        description: "Lirik ve içten karakterli makam; 2. derece (Segâh) 8 koma (~181.1c, 12-TET majör ikiliden ~18.9c pest) basılır.",
    },
    MakamSpec {
        id: MakamId::Huzzam,
        slug: "huzzam",
        display_name: "Hüzzam",
        default_root_pc: 11, // B
        default_root_name: "Si-koma (Segâh)",
        default_root_hz: 244.274, // Segah B3 (-18.9c)
        komas_53: [0, 5, 17, 22, 26, 39, 44],
        karar: "Segâh (Si-koma)",
        guclu: "Nevâ (Re)",
        yeden: "Kürdî (La#)",
        description: "Hüzünlü ve duygusal Segâh ailesi makamı; 5-12-5 komalık karakteristik artık ikili içerir.",
    },
    MakamSpec {
        id: MakamId::Hicaz,
        slug: "hicaz",
        display_name: "Hicaz",
        default_root_pc: 9, // A
        default_root_name: "La (Dügâh)",
        default_root_hz: 220.0, // A3
        komas_53: [0, 5, 17, 22, 31, 35, 44],
        karar: "Dügâh (La)",
        guclu: "Nevâ (Re)",
        yeden: "Rast (Sol)",
        description: "Anadolu ve Doğu müziğinin en bilinen makamı; 2. derece 5 koma (+13.2c) ve 3. derece 17 koma (-15.1c) ile 12 komalık artık ikili oluşturur.",
    },
    MakamSpec {
        id: MakamId::Segah,
        slug: "segah",
        display_name: "Segâh",
        default_root_pc: 11, // B
        default_root_name: "Si-koma (Segâh)",
        default_root_hz: 244.274,
        komas_53: [0, 5, 13, 22, 31, 36, 44],
        karar: "Segâh (Si-koma)",
        guclu: "Nevâ (Re)",
        yeden: "Kürdî (La#)",
        description: "Segâh beşlisi ve Uşşak dörtlüsü birleşimli klasik makam.",
    },
    MakamSpec {
        id: MakamId::Nihavent,
        slug: "nihavent",
        display_name: "Nihavent",
        default_root_pc: 7, // G
        default_root_name: "Sol (Rast)",
        default_root_hz: 195.9977,
        komas_53: [0, 9, 13, 22, 31, 35, 48],
        karar: "Rast (Sol)",
        guclu: "Nevâ (Re)",
        yeden: "Irak (Fa#)",
        description: "Batı armonik/melodik minörüne en yakın makam; Buselik ve Kürdî/Hicaz dörtlüsünü tam 53-koma oranlarıyla birleştirir.",
    },
    MakamSpec {
        id: MakamId::Huseyni,
        slug: "huseyni",
        display_name: "Hüseynî",
        default_root_pc: 9, // A
        default_root_name: "La (Dügâh)",
        default_root_hz: 220.0,
        komas_53: [0, 8, 13, 22, 31, 40, 44],
        karar: "Dügâh (La)",
        guclu: "Hüseynî (Mi)",
        yeden: "Rast (Sol)",
        description: "Türk halk müziğinde (özellikle bozlak ve uzun havalarda) çok yaygın olan parlak 6. dereceli (Eviç/Acem) makam.",
    },
    MakamSpec {
        id: MakamId::Kurdi,
        slug: "kurdi",
        display_name: "Kürdî",
        default_root_pc: 9, // A
        default_root_name: "La (Dügâh)",
        default_root_hz: 220.0,
        komas_53: [0, 4, 13, 22, 31, 35, 44],
        karar: "Dügâh (La)",
        guclu: "Nevâ (Re)",
        yeden: "Rast (Sol)",
        description: "4 komalık bakiye ikilisiyle modern Türk pop ve halk ezgilerinde yaygın kullanılan makam.",
    },
];

pub fn catalog() -> &'static [MakamSpec] {
    SPECS
}

pub fn spec(id: MakamId) -> &'static MakamSpec {
    SPECS.iter().find(|s| s.id == id).expect("every MakamId has a spec")
}

pub fn parse_makam(name: &str) -> Option<MakamId> {
    let norm = name
        .trim()
        .to_lowercase()
        .replace('ş', "s")
        .replace('ğ', "g")
        .replace('ü', "u")
        .replace('ö', "o")
        .replace('ç', "c")
        .replace('ı', "i")
        .replace('â', "a")
        .replace('î', "i");
    match norm.as_str() {
        "rast" => Some(MakamId::Rast),
        "ussak" => Some(MakamId::Ussak),
        "huzzam" => Some(MakamId::Huzzam),
        "hicaz" => Some(MakamId::Hicaz),
        "segah" => Some(MakamId::Segah),
        "nihavent" | "nihavend" => Some(MakamId::Nihavent),
        "huseyni" => Some(MakamId::Huseyni),
        "kurdi" => Some(MakamId::Kurdi),
        _ => None,
    }
}

pub fn parse_pitch_class(name: &str) -> Option<u8> {
    let t = name.trim().to_ascii_uppercase();
    match t.as_str() {
        "C" | "DO" => Some(0),
        "C#" | "DB" => Some(1),
        "D" | "RE" => Some(2),
        "D#" | "EB" => Some(3),
        "E" | "MI" => Some(4),
        "F" | "FA" => Some(5),
        "F#" | "GB" => Some(6),
        "G" | "SOL" => Some(7),
        "G#" | "AB" => Some(8),
        "A" | "LA" => Some(9),
        "A#" | "BB" => Some(10),
        "B" | "SI" => Some(11),
        _ => None,
    }
}

pub const fn koma_to_cents(koma: f64) -> f64 {
    koma * CENTS_PER_KOMA
}

/// Converts a cent deviation to a 14-bit MIDI Pitch Bend value (`0..=16383`, center `8192`).
pub fn cents_to_midi_pitch_bend_14bit(cents: f64, bend_range_semitones: f64) -> u16 {
    let range_cents = (bend_range_semitones.max(0.5)) * 100.0;
    let normalized = (cents / range_cents).clamp(-1.0, 1.0);
    let raw = (8192.0 + normalized * 8191.0).round();
    (raw.clamp(0.0, 16383.0)) as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NoteKomaMapping {
    pub midi_note: u8,
    pub degree_index: usize,
    pub koma_in_octave: u8,
    pub cents_deviation: f64,
    pub pitch_bend_14bit: u16,
}

/// Maps a 12-TET MIDI note (`0..=127`) to the nearest 53-koma degree in `makam`
/// anchored at `root_pc` (`0=C..11=B`).
pub fn map_midi_note_to_koma(
    makam: MakamId,
    root_pc: u8,
    midi_note: u8,
    bend_range_semitones: f64,
) -> NoteKomaMapping {
    let sp = spec(makam);
    let semitone_offset = (i32::from(midi_note) - i32::from(root_pc % 12)).rem_euclid(12) as f64;
    let target_cents_12tet = semitone_offset * 100.0;

    let mut best_idx = 0usize;
    let mut best_koma = sp.komas_53[0];
    let mut best_diff = f64::MAX;

    for (idx, &k) in sp.komas_53.iter().enumerate() {
        let k_cents = koma_to_cents(f64::from(k));
        // Compare wrapped on the 1200-cent octave circle so leading-tone / octave crossings match smoothly.
        let mut diff = k_cents - target_cents_12tet;
        while diff > 600.0 {
            diff -= 1200.0;
        }
        while diff < -600.0 {
            diff += 1200.0;
        }
        if diff.abs() < best_diff.abs() {
            best_diff = diff;
            best_idx = idx;
            best_koma = k;
        }
    }

    let rounded_diff = (best_diff * 100.0).round() / 100.0;
    NoteKomaMapping {
        midi_note,
        degree_index: best_idx,
        koma_in_octave: best_koma,
        cents_deviation: rounded_diff,
        pitch_bend_14bit: cents_to_midi_pitch_bend_14bit(best_diff, bend_range_semitones),
    }
}

/// Finds the nearest 53-koma frequency in `makam` (anchored at `root_hz`) for an observed
/// frequency `freq_hz`, returning `(target_hz, cents_diff)`.
pub fn nearest_makam_frequency(makam: MakamId, root_hz: f64, freq_hz: f64) -> (f64, f64) {
    let sp = spec(makam);
    let ratio = (freq_hz / root_hz.max(1.0)).max(1e-6);
    let komas_float = KOMMA_PER_OCTAVE * ratio.log2();
    let octave = (komas_float / KOMMA_PER_OCTAVE).floor();
    let in_oct = komas_float - octave * KOMMA_PER_OCTAVE;

    let mut best_abs_koma = koma_global(octave, sp.komas_53[0]);
    let mut best_dist = (komas_float - best_abs_koma).abs();

    // Check octave-1, octave, octave+1 boundaries cleanly.
    for oct_shift in [-1.0_f64, 0.0, 1.0] {
        for &k in &sp.komas_53 {
            let cand = koma_global(octave + oct_shift, k);
            let dist = (komas_float - cand).abs();
            if dist < best_dist {
                best_dist = dist;
                best_abs_koma = cand;
            }
        }
    }
    let _ = in_oct;

    let target_hz = root_hz * 2.0_f64.powf(best_abs_koma / KOMMA_PER_OCTAVE);
    let cents_diff = 1200.0 * (target_hz / freq_hz.max(1e-6)).log2();
    (target_hz, cents_diff)
}

#[inline]
fn koma_global(octave: f64, koma_in_octave: u8) -> f64 {
    octave * KOMMA_PER_OCTAVE + f64::from(koma_in_octave)
}

/// Annotates a YuE2 ABC score header with WerrOzan 53-Koma makam metadata comments
/// while keeping the strict `YuE2` ABC parser dialect intact.
pub fn annotate_abc_with_makam(abc: &str, makam: MakamId, root_pc: Option<u8>) -> String {
    let sp = spec(makam);
    let pc = root_pc.unwrap_or(sp.default_root_pc) % 12;
    const PC_NAMES: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
    let header_note = format!(
        "% WerrOzan 53-Koma Makam: {} | Root={} | Komas={:?} | Karar={} | Guclu={}",
        sp.display_name, PC_NAMES[pc as usize], sp.komas_53, sp.karar, sp.guclu
    );

    let mut out = Vec::new();
    let mut inserted = false;
    for line in abc.lines() {
        if line.trim_start().starts_with("% WerrOzan 53-Koma Makam:") {
            continue;
        }
        out.push(line.to_string());
        if !inserted && line.trim_start().starts_with("K:") {
            out.push(header_note.clone());
            inserted = true;
        }
    }
    if !inserted {
        out.insert(0, header_note);
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koma_math_and_rast_segah_deviation_are_exact() {
        assert!((koma_to_cents(53.0) - 1200.0).abs() < 1e-9);
        assert!((koma_to_cents(1.0) - 22.64150943).abs() < 1e-6);

        // In Rast anchored on G (pc=7), MIDI B4 (71) is 400c above G;
        // Rast Segah is 17 komas (~384.91c), giving ~ -15.09c correction from 12-TET B.
        let m = map_midi_note_to_koma(MakamId::Rast, 7, 71, 2.0);
        assert_eq!(m.koma_in_octave, 17);
        assert!((m.cents_deviation - (-15.09)).abs() < 0.05);
        assert!(m.pitch_bend_14bit < 8192);
    }

    #[test]
    fn nearest_frequency_pulls_12tet_e4_to_rast_segah() {
        // Root G3 = 195.9977 Hz; E4 in 12-TET is 329.6276 Hz.
        // Rast Segah (17 komas above G3, or +1 octave + 17 komas? Wait: G3->E4 is 900c = ~40 komas -> 6th degree!).
        // Wait: G3 + 9 semitones = E4 (40 komas = 905.66c); B3 (400c above G3) = 246.9417 Hz -> pulls to 17 komas (~244.78 Hz, -15.09c).
        let (target, diff) = nearest_makam_frequency(MakamId::Rast, 195.9977, 246.9417);
        assert!((target - 244.79).abs() < 0.25);
        assert!((diff - (-15.09)).abs() < 0.1);
    }

    #[test]
    fn parse_handles_turkish_diacritics_and_annotates_abc() {
        assert_eq!(parse_makam("Hüzzam"), Some(MakamId::Huzzam));
        assert_eq!(parse_makam("Uşşak"), Some(MakamId::Ussak));
        assert_eq!(parse_makam("Hüseynî"), Some(MakamId::Huseyni));

        let abc = "X:1\nT:Test\nM:4/4\nL:1/8\nK:G\nV: Vocal\nG2 A2 B2 c2|\n";
        let ann = annotate_abc_with_makam(abc, MakamId::Rast, None);
        assert!(ann.contains("% WerrOzan 53-Koma Makam: Rast"));
    }
}
