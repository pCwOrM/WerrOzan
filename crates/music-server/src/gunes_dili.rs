//! Güneş Dili Morpho-Semantic Diction, Syllable Meter & Vowel Harmony Analyzer.
//!
//! Implements the deterministic Turkish/agglutinative vowel-nucleus syllable counter,
//! palatal/labial vowel harmony checker (`gunes_dili.py` / `motor.js` parity),
//! acoustic formant centroid ($f_{\text{ac}} \in [100, 8000]\text{ Hz}$), and
//! YuE2 `LYRICS_RULES` / `DICTION_RULE` prosody validation.

use serde::Serialize;

const VOWELS: &[char] = &['a', 'e', 'ı', 'i', 'o', 'ö', 'u', 'ü', 'â', 'î', 'û'];
const BACK_VOWELS: &[char] = &['a', 'ı', 'o', 'u', 'â', 'û'];
const FRONT_VOWELS: &[char] = &['e', 'i', 'ö', 'ü', 'î'];
const VOICELESS_CONSONANTS: &[char] = &['ç', 'f', 'h', 'k', 'p', 's', 'ş', 't'];

pub const ALLOWED_SECTION_TAGS: &[&str] = &[
    "[Intro]",
    "[Verse]",
    "[Verse 1]",
    "[Verse 2]",
    "[Verse 3]",
    "[Verse 4]",
    "[Pre-Chorus]",
    "[Chorus]",
    "[Chorus 1]",
    "[Chorus 2]",
    "[Post-Chorus]",
    "[Bridge]",
    "[Interlude]",
    "[Solo]",
    "[Instrumental]",
    "[Instrumental Break]",
    "[Outro]",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineAnalysis {
    pub line_number: usize,
    pub text: String,
    pub syllables: usize,
    pub words: usize,
    pub harmony_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionAnalysis {
    pub tag: String,
    pub lines: Vec<LineAnalysis>,
    pub total_syllables: usize,
    pub min_syllables: usize,
    pub max_syllables: usize,
    pub balanced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GunesDiliReport {
    pub sections: Vec<SectionAnalysis>,
    pub total_syllables: usize,
    pub total_words: usize,
    pub vowel_harmony_ratio: f64,
    pub meter_stability: f64,
    pub formant_centroid_hz: f64,
    pub voiceless_ratio: f64,
    pub words_per_second: f64,
    pub warnings: Vec<String>,
    pub diction_passed: bool,
}

fn to_turkish_lower(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'İ' => out.push('i'),
            'I' => out.push('ı'),
            _ => {
                for lc in ch.to_lowercase() {
                    out.push(lc);
                }
            }
        }
    }
    out
}

/// Counts vocalic syllable nuclei in a Turkish/Latin phrase.
pub fn count_syllables(text: &str) -> usize {
    let lower = to_turkish_lower(text);
    let mut count = 0usize;
    let mut prev_vowel = false;
    for ch in lower.chars() {
        if VOWELS.contains(&ch) {
            // Standard Turkish has no diphthongs (each vowel = 1 syllable), while
            // non-Turkish Latin loanwords may double letters (`oo`, `ee`). Keep 1:1
            // per Turkish vowel and collapse identical repeated vowels.
            if !prev_vowel || matches!(ch, 'a' | 'e' | 'ı' | 'i' | 'o' | 'ö' | 'u' | 'ü') {
                count += 1;
            }
            prev_vowel = true;
        } else {
            prev_vowel = false;
        }
    }
    count
}

/// Checks palatal vowel harmony (Büyük Ünlü Uyumu) for a single word.
/// Words with 0 or 1 vowel trivially satisfy palatal harmony.
pub fn word_satisfies_palatal_harmony(word: &str) -> bool {
    let lower = to_turkish_lower(word);
    let mut has_back = false;
    let mut has_front = false;
    for ch in lower.chars() {
        if BACK_VOWELS.contains(&ch) {
            has_back = true;
        } else if FRONT_VOWELS.contains(&ch) {
            has_front = true;
        }
    }
    !(has_back && has_front)
}

fn acoustic_formant_hz(ch: char) -> Option<f64> {
    Some(match ch {
        'u' | 'o' => 420.0,
        'a' | 'ı' | 'â' | 'û' => 760.0,
        'ö' | 'ü' => 1150.0,
        'e' => 1650.0,
        'i' | 'î' => 2250.0,
        'm' | 'n' | 'l' | 'r' | 'y' => 1400.0,
        'b' | 'd' | 'g' | 'v' | 'z' | 'c' | 'j' => 1850.0,
        'ç' | 'f' | 'h' | 'k' | 'p' | 's' | 'ş' | 't' => 4200.0,
        _ => return None,
    })
}

fn is_section_tag(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('[') && t.ends_with(']') && t.len() >= 3
}

/// Analyzes lyrics for YuE2 section formatting, Turkish vowel harmony,
/// syllable meter consistency across neighbouring lines, and acoustic formant profile.
pub fn analyze_lyrics(lyrics: &str, target_duration_s: f64) -> GunesDiliReport {
    let mut sections: Vec<SectionAnalysis> = Vec::new();
    let mut current_tag = String::from("[Unlabeled]");
    let mut current_lines: Vec<LineAnalysis> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let mut total_words = 0usize;
    let mut harmony_words = 0usize;
    let mut total_syllables = 0usize;
    let mut formant_sum = 0.0_f64;
    let mut formant_count = 0usize;
    let mut voiceless_count = 0usize;
    let mut letter_count = 0usize;

    let flush_section = |tag: &str, lines: Vec<LineAnalysis>, out: &mut Vec<SectionAnalysis>, warns: &mut Vec<String>| {
        if tag == "[Unlabeled]" && lines.is_empty() {
            return;
        }
        let counts: Vec<usize> = lines.iter().map(|l| l.syllables).filter(|&s| s > 0).collect();
        let min_s = counts.iter().copied().min().unwrap_or(0);
        let max_s = counts.iter().copied().max().unwrap_or(0);
        let total_s: usize = counts.iter().sum();
        let balanced = max_s.saturating_sub(min_s) <= 4;
        if !balanced {
            warns.push(format!(
                "Section {tag} has uneven line lengths ({min_s}–{max_s} syllables); keep neighbouring lines closer so YuE2 vocals stay on beat."
            ));
        }
        out.push(SectionAnalysis {
            tag: tag.to_string(),
            lines,
            total_syllables: total_s,
            min_syllables: min_s,
            max_syllables: max_s,
            balanced,
        });
    };

    for (idx, raw) in lyrics.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_section_tag(trimmed) {
            flush_section(&current_tag, std::mem::take(&mut current_lines), &mut sections, &mut warnings);
            if !ALLOWED_SECTION_TAGS.iter().any(|t| t.eq_ignore_ascii_case(trimmed)) {
                warnings.push(format!("Non-standard section tag `{trimmed}` at line {}; YuE2 expects standard English bracket tags.", idx + 1));
            }
            current_tag = trimmed.to_string();
            continue;
        }
        if trimmed.contains('(') && trimmed.contains(')') {
            warnings.push(format!("Line {} contains parentheses (`{trimmed}`); avoid stage directions in sung lyrics.", idx + 1));
        }

        let syllables = count_syllables(trimmed);
        total_syllables += syllables;

        let mut line_words = 0usize;
        let mut line_harmony_ok = true;
        for token in trimmed.split_whitespace() {
            let clean: String = token.chars().filter(|c| c.is_alphabetic()).collect();
            if clean.is_empty() {
                continue;
            }
            line_words += 1;
            total_words += 1;
            if word_satisfies_palatal_harmony(&clean) {
                harmony_words += 1;
            } else {
                line_harmony_ok = false;
            }
            let lower = to_turkish_lower(&clean);
            for ch in lower.chars() {
                letter_count += 1;
                if VOICELESS_CONSONANTS.contains(&ch) {
                    voiceless_count += 1;
                }
                if let Some(hz) = acoustic_formant_hz(ch) {
                    formant_sum += hz;
                    formant_count += 1;
                }
            }
        }

        current_lines.push(LineAnalysis {
            line_number: idx + 1,
            text: trimmed.to_string(),
            syllables,
            words: line_words,
            harmony_ok: line_harmony_ok,
        });
    }
    flush_section(&current_tag, current_lines, &mut sections, &mut warnings);

    if sections.iter().any(|s| s.tag == "[Unlabeled]") {
        warnings.push("Lyrics contain lines outside any [Section] tag; start every section with [Verse 1], [Chorus], etc.".to_string());
    }

    let vowel_harmony_ratio = if total_words == 0 {
        1.0
    } else {
        (harmony_words as f64) / (total_words as f64)
    };
    let meter_stability = if sections.is_empty() {
        1.0
    } else {
        let balanced = sections.iter().filter(|s| s.balanced).count();
        (balanced as f64) / (sections.len() as f64)
    };
    let formant_centroid_hz = if formant_count == 0 {
        1200.0
    } else {
        formant_sum / (formant_count as f64)
    };
    let voiceless_ratio = if letter_count == 0 {
        0.0
    } else {
        (voiceless_count as f64) / (letter_count as f64)
    };
    let dur = target_duration_s.max(1.0);
    let words_per_second = (total_words as f64) / dur;
    if total_words > 0 && (words_per_second < 0.7 || words_per_second > 3.5) {
        warnings.push(format!(
            "Lyric density is {:.2} words/s for {:.0}s target (YuE2 sweet spot is ~1.5–3.0 sung words/s).",
            words_per_second, dur
        ));
    }

    let diction_passed = warnings.is_empty();
    GunesDiliReport {
        sections,
        total_syllables,
        total_words,
        vowel_harmony_ratio: (vowel_harmony_ratio * 1000.0).round() / 1000.0,
        meter_stability: (meter_stability * 1000.0).round() / 1000.0,
        formant_centroid_hz: (formant_centroid_hz * 10.0).round() / 10.0,
        voiceless_ratio: (voiceless_ratio * 1000.0).round() / 1000.0,
        words_per_second: (words_per_second * 100.0).round() / 100.0,
        warnings,
        diction_passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_turkish_syllables_and_vowel_harmony() {
        assert_eq!(count_syllables("kitaplarımızdan"), 6);
        assert_eq!(count_syllables("yıldızlarımızdan"), 6);
        assert_eq!(count_syllables("Güneş dili yankılanır"), 8);
        assert!(word_satisfies_palatal_harmony("yıldızlarımızdan"));
        assert!(word_satisfies_palatal_harmony("güneşlerden"));
        assert!(!word_satisfies_palatal_harmony("kitaplarımızdan"));
        assert!(!word_satisfies_palatal_harmony("elma"));
    }

    #[test]
    fn validates_balanced_sections_and_flags_imbalance() {
        let good = "[Verse 1]\nGece çöker sahile yavaşça\nDalgalar vurur kıyıya sessizce\n\n[Chorus]\nYükselir sesimiz göklere\nDöneriz yine aynı yerlere";
        let r_good = analyze_lyrics(good, 30.0);
        assert_eq!(r_good.sections.len(), 2);
        assert!(r_good.sections.iter().all(|s| s.balanced));

        let uneven = "[Verse 1]\nGel\nBu gece uzun yolların sonunda yıldızlara doğru yürüyoruz durmadan";
        let r_bad = analyze_lyrics(uneven, 20.0);
        assert!(!r_bad.sections[0].balanced);
        assert!(!r_bad.warnings.is_empty());
    }
}
