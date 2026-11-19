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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Yue2LyricBudget {
    pub min_lines: usize,
    pub ideal_lines: usize,
    pub max_lines: usize,
    pub min_syllables: usize,
    pub ideal_syllables: usize,
    pub max_syllables: usize,
}

/// Realistic YuE2 sung-lyric budget accounting for ~35-50s of instrumental
/// [Intro]/[Outro] and Turkish agglutinative syllable density (~2.5 syl/word).
pub fn yue2_lyric_budget(target_duration_s: f64) -> Yue2LyricBudget {
    let dur = target_duration_s.clamp(20.0, 360.0);
    if dur <= 65.0 {
        Yue2LyricBudget {
            min_lines: 4,
            ideal_lines: 6,
            max_lines: 8,
            min_syllables: 28,
            ideal_syllables: 48,
            max_syllables: 68,
        }
    } else if dur <= 100.0 {
        Yue2LyricBudget {
            min_lines: 6,
            ideal_lines: 8,
            max_lines: 10,
            min_syllables: 44,
            ideal_syllables: 64,
            max_syllables: 90,
        }
    } else if dur <= 128.0 {
        Yue2LyricBudget {
            min_lines: 8,
            ideal_lines: 12,
            max_lines: 14,
            min_syllables: 64,
            ideal_syllables: 92,
            max_syllables: 125,
        }
    } else if dur <= 160.0 {
        Yue2LyricBudget {
            min_lines: 10,
            ideal_lines: 16,
            max_lines: 18,
            min_syllables: 85,
            ideal_syllables: 128,
            max_syllables: 162,
        }
    } else {
        let scale = (dur / 180.0).max(1.0);
        Yue2LyricBudget {
            min_lines: (12.0 * scale).round() as usize,
            ideal_lines: (18.0 * scale).round() as usize,
            max_lines: (24.0 * scale).round() as usize,
            min_syllables: (100.0 * scale).round() as usize,
            ideal_syllables: (150.0 * scale).round() as usize,
            max_syllables: (205.0 * scale).round() as usize,
        }
    }
}

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
    pub locale: String,
    pub prosody_mode: String,
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

/// Heuristic English syllable counter accounting for vowel digraphs, silent final `-e`,
/// syllabic `-le`, and `-ed` endings.
pub fn count_english_word_syllables(word: &str) -> usize {
    let clean: String = word
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if clean.is_empty() {
        return 0;
    }
    if clean.len() <= 3 {
        return 1;
    }
    let bytes = clean.as_bytes();
    let is_v = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');

    let mut groups = 0usize;
    let mut in_v = false;
    for &b in bytes {
        if is_v(b) {
            if !in_v {
                groups += 1;
                in_v = true;
            }
        } else {
            in_v = false;
        }
    }

    // Silent trailing 'e' (unless syllabic '-le' preceded by a consonant, e.g. "candle", "table")
    if clean.ends_with('e') && !clean.ends_with("ee") && !clean.ends_with("ye") && !clean.ends_with("oe") {
        let syllabic_le = clean.ends_with("le") && bytes.len() >= 3 && !is_v(bytes[bytes.len() - 3]);
        if !syllabic_le && groups > 1 {
            groups -= 1;
        }
    }
    // Silent '-ed' unless preceded by 't' or 'd'
    if clean.ends_with("ed") && bytes.len() >= 4 {
        let prev = bytes[bytes.len() - 3];
        if prev != b't' && prev != b'd' && !is_v(prev) && groups > 1 {
            groups -= 1;
        }
    }
    // Common hiatus pairs that add a syllable (ia, io, ua, eo in "radio", "piano", "quiet")
    for pair in ["ia", "io", "eo", "ua", "iu"] {
        if clean.contains(pair) && !clean.ends_with("tion") && !clean.ends_with("sion") {
            groups += 1;
        }
    }
    groups.max(1)
}

pub fn count_english_syllables(text: &str) -> usize {
    text.split_whitespace().map(count_english_word_syllables).sum()
}

/// Counts Russian vocalic syllable nuclei (а е ё и о у ы э ю я), falling back to Latin
/// syllable count for loanwords.
pub fn count_russian_syllables(text: &str) -> usize {
    let mut cyr = 0usize;
    let mut has_cyr = false;
    for ch in text.chars() {
        let lc = ch.to_lowercase().next().unwrap_or(ch);
        if ('\u{0400}'..='\u{04FF}').contains(&lc) {
            has_cyr = true;
            if matches!(lc, 'а' | 'е' | 'ё' | 'и' | 'о' | 'у' | 'ы' | 'э' | 'ю' | 'я') {
                cyr += 1;
            }
        }
    }
    if has_cyr {
        cyr.max(1)
    } else {
        count_english_syllables(text)
    }
}

/// Counts Japanese morae (Hiragana/Katakana minus small yoon glides, plus ~2 morae per Kanji).
pub fn count_japanese_morae(text: &str) -> usize {
    let mut morae = 0usize;
    let mut has_jp = false;
    for ch in text.chars() {
        if matches!(ch, 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ' | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ') {
            has_jp = true;
            continue;
        }
        if ('\u{3040}'..='\u{30FF}').contains(&ch) || ch == 'ー' || ch == 'っ' || ch == 'ッ' {
            has_jp = true;
            morae += 1;
        } else if ('\u{4E00}'..='\u{9FFF}').contains(&ch) {
            has_jp = true;
            morae += 2;
        }
    }
    if has_jp {
        morae.max(1)
    } else {
        count_english_syllables(text)
    }
}

/// Extracts a normalized final rhyme tail from an English lyric line for cadence checking.
fn english_line_rhyme_tail(line: &str) -> Option<String> {
    let last_word = line
        .split_whitespace()
        .rev()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphabetic())
                .map(|c| c.to_ascii_lowercase())
                .collect::<String>()
        })
        .find(|w| !w.is_empty())?;
    let bytes = last_word.as_bytes();
    let is_v = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    // Find the last vowel (skipping silent trailing 'e' when word len > 3)
    let search_end = if last_word.len() > 3 && last_word.ends_with('e') && !last_word.ends_with("ee") {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let mut last_v_idx = None;
    for i in (0..search_end).rev() {
        if is_v(bytes[i]) {
            last_v_idx = Some(i);
            break;
        }
    }
    let start = last_v_idx.unwrap_or_else(|| last_word.len().saturating_sub(2));
    Some(last_word[start..].to_string())
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

/// Analyzes lyrics using Turkish Güneş Dili prosody rules (default).
pub fn analyze_lyrics(lyrics: &str, target_duration_s: f64) -> GunesDiliReport {
    analyze_lyrics_for_locale(lyrics, target_duration_s, Some("tr"))
}

/// Analyzes lyrics with locale-aware prosody dynamics:
/// - `tr` (default): Turkish vowel-nucleus syllable counting, Büyük Ünlü Uyumu (palatal vowel harmony),
///   and Turkish agglutinative YuE2 duration budget.
/// - `en`: English syllable counting (digraphs, silent `-e`, `-le`, `-ed`), English meter/rhyme
///   cadence scoring (no Turkish vowel-harmony penalty), and English YuE2 duration budget.
pub fn analyze_lyrics_for_locale(lyrics: &str, target_duration_s: f64, locale: Option<&str>) -> GunesDiliReport {
    let norm_locale = locale
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tr".to_string());
    let is_english = norm_locale.starts_with("en");
    let is_russian = norm_locale.starts_with("ru");
    let is_japanese = norm_locale.starts_with("ja");
    let is_non_tr = is_english || is_russian || is_japanese;
    let prosody_mode = if is_english {
        "english_meter_rhyme".to_string()
    } else if is_russian {
        "russian_meter_flow".to_string()
    } else if is_japanese {
        "japanese_mora_rhythm".to_string()
    } else {
        "turkish_gunes_dili".to_string()
    };

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
        let max_spread = if is_japanese {
            6
        } else if is_english || is_russian {
            5
        } else {
            4
        };
        let balanced = max_s.saturating_sub(min_s) <= max_spread;
        if !balanced {
            if is_english {
                warns.push(format!(
                    "Section {tag} has uneven English meter ({min_s}–{max_s} syllables); keep lines within ~6–11 syllables so vocals lock to the beat."
                ));
            } else if is_russian {
                warns.push(format!(
                    "Section {tag} has uneven Russian meter ({min_s}–{max_s} syllables); keep neighbouring lines closer so vocals stay on beat."
                ));
            } else if is_japanese {
                warns.push(format!(
                    "Section {tag} has uneven Japanese mora rhythm ({min_s}–{max_s} morae); keep neighbouring lines balanced."
                ));
            } else {
                warns.push(format!(
                    "Section {tag} has uneven line lengths ({min_s}–{max_s} syllables); keep neighbouring lines closer so YuE2 vocals stay on beat."
                ));
            }
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

        let syllables = if is_english {
            count_english_syllables(trimmed)
        } else if is_russian {
            count_russian_syllables(trimmed)
        } else if is_japanese {
            count_japanese_morae(trimmed)
        } else {
            count_syllables(trimmed)
        };
        total_syllables += syllables;

        if is_english && syllables >= 16 {
            warnings.push(format!(
                "Line {} is very long for English sung phrasing ({syllables} syllables); aim for 6–11 syllables per line.",
                idx + 1
            ));
        } else if is_russian && syllables >= 16 {
            warnings.push(format!(
                "Line {} is very long for Russian sung phrasing ({syllables} syllables); aim for 6–11 syllables per line.",
                idx + 1
            ));
        } else if is_japanese && syllables >= 22 {
            warnings.push(format!(
                "Line {} is very long for Japanese mora phrasing ({syllables} morae); aim for 8–16 morae per line.",
                idx + 1
            ));
        }

        let mut line_words = 0usize;
        let mut line_harmony_ok = true;
        for token in trimmed.split_whitespace() {
            let clean: String = token.chars().filter(|c| c.is_alphabetic()).collect();
            if clean.is_empty() {
                continue;
            }
            line_words += 1;
            total_words += 1;
            if is_non_tr {
                let unit_len = if is_japanese {
                    count_japanese_morae(&clean)
                } else if is_russian {
                    count_russian_syllables(&clean)
                } else {
                    count_english_word_syllables(&clean)
                };
                if unit_len <= 5 {
                    harmony_words += 1;
                } else {
                    line_harmony_ok = false;
                }
            } else if word_satisfies_palatal_harmony(&clean) {
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

    let vowel_harmony_ratio = if is_non_tr {
        // For non-Turkish languages, compute combined rhythmic-flow & end-rhyme cadence score in [0.0, 1.0].
        let flow = if total_words == 0 {
            1.0
        } else {
            (harmony_words as f64) / (total_words as f64)
        };
        if is_english {
            let mut rhyme_checks = 0usize;
            let mut rhyme_hits = 0usize;
            for sec in &sections {
                let tails: Vec<Option<String>> = sec.lines.iter().map(|l| english_line_rhyme_tail(&l.text)).collect();
                for i in 0..tails.len() {
                    let Some(ref t) = tails[i] else { continue };
                    if tails.len() < 2 {
                        continue;
                    }
                    rhyme_checks += 1;
                    let matched = (0..tails.len()).any(|j| {
                        j != i
                            && tails[j].as_ref().is_some_and(|other| {
                                other == t
                                    || (other.len() >= 2 && t.len() >= 2 && other.ends_with(&t[t.len() - 2..]))
                            })
                    });
                    if matched {
                        rhyme_hits += 1;
                    }
                }
            }
            if rhyme_checks == 0 {
                flow
            } else {
                (0.65 * flow + 0.35 * ((rhyme_hits as f64) / (rhyme_checks as f64))).clamp(0.0, 1.0)
            }
        } else {
            flow
        }
    } else if total_words == 0 {
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
    let sung_lines: usize = sections.iter().map(|s| s.lines.len()).sum();
    let base_budget = yue2_lyric_budget(dur);
    let max_syllables = if is_japanese {
        ((base_budget.max_syllables as f64) * 1.35).round() as usize
    } else if is_english || is_russian {
        ((base_budget.max_syllables as f64) * 1.15).round() as usize
    } else {
        base_budget.max_syllables
    };
    let max_lines = base_budget.max_lines;

    for sec in &sections {
        let low_tag = sec.tag.to_ascii_lowercase();
        if (low_tag == "[intro]" || low_tag == "[outro]") && !sec.lines.is_empty() {
            warnings.push(format!(
                "Section {} has {} sung line(s); leave [Intro] and [Outro] empty so YuE2 does not cut off vocals.",
                sec.tag,
                sec.lines.len()
            ));
        }
        for line in &sec.lines {
            if line.text.split_whitespace().any(|w| w.matches('-').count() >= 2) {
                warnings.push(format!(
                    "Line {} contains hyphenated syllable splits ({}); write whole words.",
                    line.line_number, line.text
                ));
                break;
            }
        }
    }

    if total_words > 0 && (total_syllables > max_syllables || sung_lines > max_lines) {
        warnings.push(format!(
            "Cutoff risk for {:.0}s target: {} syllables / {} lines exceeds YuE2 budget (max ~{} syllables, ~{} lines with [Intro]/[Outro]). Use '✨ El Werr' to trim or increase Target Duration.",
            dur, total_syllables, sung_lines, max_syllables, max_lines
        ));
    } else if total_words > 0 && total_syllables < (base_budget.min_syllables / 2).max(16) {
        warnings.push(format!(
            "Very sparse lyrics ({} syllables for {:.0}s target); consider adding a few lines or lowering Target Duration.",
            total_syllables, dur
        ));
    }

    let diction_passed = warnings.is_empty();
    GunesDiliReport {
        locale: norm_locale,
        prosody_mode,
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

    #[test]
    fn enforces_yue2_cutoff_budget_and_empty_intro_outro() {
        let golden = "[Intro]\n\n[Verse 1]\nGece kül renkli perde\nSaklımız yok bu yerde\nSoygun vakti geliyor\nKader bizim ellerde\n\n[Chorus]\nAdalet bizim kılıcımız\nÖzgürlük tek inancımız\nAteşte yanar adımız\nBu gece bizim zaferimiz\n\n[Outro]";
        let ok = analyze_lyrics(golden, 90.0);
        assert!(ok.warnings.is_empty(), "expected no warnings for 8-line 90s golden lyrics, got: {:?}", ok.warnings);

        let too_long = format!(
            "[Intro]\n{}\n\n[Outro]",
            vec!["Karanlık gecede çalar bu ezgi"; 16].join("\n")
        );
        let bad = analyze_lyrics(&too_long, 90.0);
        assert!(bad.warnings.iter().any(|w| w.contains("Cutoff risk")));
        assert!(bad.warnings.iter().any(|w| w.contains("[Intro]")));
    }

    #[test]
    fn analyzes_english_lyrics_with_english_syllables_and_rhyme_dynamics() {
        assert_eq!(count_english_word_syllables("home"), 1);
        assert_eq!(count_english_word_syllables("night"), 1);
        assert_eq!(count_english_word_syllables("dream"), 1);
        assert_eq!(count_english_word_syllables("candle"), 2);
        assert_eq!(count_english_word_syllables("forever"), 3);

        let en_song = "[Intro]\n\n[Verse 1]\nDeep blue sky above\nWhispers of the wind\nGolden light descends\nSilent dreams begin\n\n[Chorus]\nFly away so high\nTouch the silver moon\nEchoes in the dark\nFading far too soon\n\n[Outro]";
        let rep = analyze_lyrics_for_locale(en_song, 90.0, Some("en"));
        assert_eq!(rep.locale, "en");
        assert_eq!(rep.prosody_mode, "english_meter_rhyme");
        assert!(rep.warnings.is_empty(), "unexpected warnings: {:?}", rep.warnings);
        assert!(rep.vowel_harmony_ratio > 0.7);

        let ru_song = "[Intro]\n\n[Verse 1]\nГород спит в огне огней\nТени пляшут на стене\nШепот ветра в тишине\nТает дым в моем окне\n\n[Outro]";
        let rep_ru = analyze_lyrics_for_locale(ru_song, 90.0, Some("ru"));
        assert_eq!(rep_ru.prosody_mode, "russian_meter_flow");
        assert!(rep_ru.total_syllables > 20);

        let ja_song = "[Intro]\n\n[Verse 1]\n青い空の彼方へ\n風と共に走り出す\n星が光る夜空に\n夢を乗せて歌うよ\n\n[Outro]";
        let rep_ja = analyze_lyrics_for_locale(ja_song, 90.0, Some("ja"));
        assert_eq!(rep_ja.prosody_mode, "japanese_mora_rhythm");
        assert!(rep_ja.total_syllables > 30);
    }
}
