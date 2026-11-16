//! Symbolic 53-Koma Makam MIDI Pitch-Bend Exporter & ABC Annotator.
//!
//! Exports YuE2 two-voice ABC scores to Standard MIDI Files (Format 1) with
//! RPN Pitch Bend Sensitivity (`±2` semitones default) and exact 14-bit Pitch
//! Bend events (`0xE0..0xEF`) before every `Vocal` and `Ins` note according
//! to the selected Turkish makam's 53-koma grid.

use music_core::koma::{self, MakamId, NoteKomaMapping};
use serde::Serialize;

use super::{abc, export, sections, smf};

const CONDUCTOR: &str = "WerrOzan 53-Koma score";
const LINES: [(&str, usize, u8, u8, u8); 2] = [("Vocal", abc::VOCAL, 0, 53, 100), ("Ins", abc::INS, 1, 0, 90)];
const CHORDS: (&str, u8, u8, u8) = ("Chords", 2, 48, 64);

#[derive(Debug, Clone, Serialize)]
pub struct KomaScoreExport {
    pub makam: MakamId,
    pub root_pc: u8,
    pub bend_range_semitones: f64,
    pub annotated_abc: String,
    pub midi_bytes: Vec<u8>,
    pub note_mappings: Vec<NoteKomaMapping>,
}

/// Normalizes single-voice or minimal ABC snippets into the strict two-voice
/// (`V: Vocal` + `V: Ins`) YuE2 score format expected by `abc::parse`.
pub fn normalize_abc_for_yue2(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.contains("V: Vocal") && trimmed.contains("V: Ins") {
        if !trimmed.lines().any(|l| l.trim_start().starts_with("T:")) {
            let mut out = String::new();
            let mut inserted_t = false;
            for line in trimmed.lines() {
                out.push_str(line);
                out.push('\n');
                if !inserted_t && line.trim_start().starts_with("X:") {
                    out.push_str("T:\n");
                    inserted_t = true;
                }
            }
            return out;
        }
        return format!("{trimmed}\n");
    }

    let mut key = "Dm";
    let mut meter = "4/4";
    let mut unit = "1/16";
    let mut tempo = "1/4=90";
    let mut body_lines: Vec<&str> = Vec::new();

    for line in trimmed.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('%') {
            continue;
        }
        if t.starts_with("X:") || t.starts_with("T:") || t.starts_with("V:") {
            continue;
        } else if let Some(rest) = t.strip_prefix("M:") {
            meter = rest.trim();
        } else if let Some(rest) = t.strip_prefix("L:") {
            unit = rest.trim();
        } else if let Some(rest) = t.strip_prefix("Q:") {
            tempo = rest.trim();
        } else if let Some(rest) = t.strip_prefix("K:") {
            key = rest.trim();
        } else {
            body_lines.push(t);
        }
    }

    let vocal_body = if body_lines.is_empty() {
        "G4 A4 B4 c4|".to_string()
    } else {
        body_lines.join("\n")
    };
    let bar_count = vocal_body.matches('|').count().max(1);
    let ins_rests = if bar_count == 1 {
        "Z|".to_string()
    } else {
        format!("Z{bar_count}|")
    };

    format!(
        "X:1\nT:\nM:{meter}\nL:{unit}\nQ:{tempo}\n\
         V: Vocal clef=treble name=\"Vocal Melody\" snm=\"Vocal\"\n\
         V: Ins clef=treble name=\"Ins Melody\" snm=\"Inst.\"\n\
         K:{key}\n% verse\nV: Vocal\n{vocal_body}\nV: Ins\n{ins_rests}\n"
    )
}

/// Renders an ABC score with 53-koma pitch bend messages into a Format-1 MIDI file.
pub fn export_makam_midi(
    text: &str,
    makam: MakamId,
    root_pc: Option<u8>,
    bend_range_semitones: f64,
) -> Result<KomaScoreExport, String> {
    let sp = koma::spec(makam);
    let root_pc = root_pc.unwrap_or(sp.default_root_pc) % 12;
    let bend_range = bend_range_semitones.clamp(1.0, 12.0);
    let bend_semis_u8 = bend_range.round() as u8;

    let normalized = normalize_abc_for_yue2(text);
    let score = abc::parse(&normalized)?;
    let vocal = &score.voices[abc::VOCAL];
    let end = export::tick(vocal.time);

    let mut conductor = vec![
        (0, smf::text(smf::NAME, CONDUCTOR)),
        (0, smf::tempo((60e6 / f64::from(score.bpm)).round() as u32)),
    ];
    let mut meter = None;
    for bar in &vocal.bars {
        if Some(bar.meter) != meter {
            conductor.push((export::tick(bar.start), smf::meter(bar.meter.0, bar.meter.1)));
            meter = Some(bar.meter);
        }
    }
    let mut key: Option<&str> = None;
    for (start, name) in &vocal.keys {
        if Some(name.as_str()) != key {
            let sharps = abc::key_fifths(name)
                .ok_or_else(|| format!("the key {name} is not one the score format knows"))?;
            conductor.push((export::tick(*start), smf::key(sharps, name.ends_with('m'))));
            key = Some(name);
        }
    }
    for section in sections::sections(&normalized)? {
        if !section.label.is_empty() {
            conductor.push((export::tick(section.start), smf::text(smf::MARKER, &section.label)));
        }
    }

    let mut tracks = vec![conductor];
    let mut note_mappings = Vec::new();

    for (name, voice, channel, program, velocity) in LINES {
        let notes = &score.voices[voice].notes;
        if notes.is_empty() {
            continue;
        }
        let mut events = vec![
            (0, smf::text(smf::NAME, name)),
            (0, smf::program(channel, program)),
            // RPN 00 00: Pitch Bend Sensitivity in semitones
            (0, smf::control_change(channel, 101, 0)),
            (0, smf::control_change(channel, 100, 0)),
            (0, smf::control_change(channel, 6, bend_semis_u8)),
            (0, smf::control_change(channel, 38, 0)),
        ];
        for note in notes {
            let mapping = koma::map_midi_note_to_koma(makam, root_pc, note.pitch as u8, bend_range);
            let start_tick = export::tick(note.start);
            let end_tick = export::tick(note.start + note.duration);
            events.push((start_tick, smf::pitch_bend(channel, mapping.pitch_bend_14bit)));
            events.push((start_tick, smf::note_on(channel, note.pitch as u8, velocity)));
            events.push((end_tick, smf::note_off(channel, note.pitch as u8)));
            if voice == abc::VOCAL {
                note_mappings.push(mapping);
            }
        }
        tracks.push(events);
    }

    if !vocal.chords.is_empty() {
        let (name, channel, program, velocity) = CHORDS;
        let mut events = vec![(0, smf::text(smf::NAME, name)), (0, smf::program(channel, program))];
        for (index, (onset, symbol)) in vocal.chords.iter().enumerate() {
            let stop = vocal.chords.get(index + 1).map_or(vocal.time, |next| next.0);
            if stop <= *onset {
                continue;
            }
            for pitch in export::chord_pitches(symbol)? {
                events.push((export::tick(*onset), smf::note_on(channel, pitch as u8, velocity)));
                events.push((export::tick(stop), smf::note_off(channel, pitch as u8)));
            }
        }
        tracks.push(events);
    }

    let midi_bytes = smf::write(&tracks, smf::DIVISION, end)?;
    let annotated_abc = koma::annotate_abc_with_makam(&normalized, makam, Some(root_pc));

    Ok(KomaScoreExport {
        makam,
        root_pc,
        bend_range_semitones: bend_range,
        annotated_abc,
        midi_bytes,
        note_mappings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_rast_midi_with_pitch_bend_before_notes() {
        let abc = "X:1\nT:\nM:4/4\nL:1/32\nQ:1/4=90\nV: Vocal clef=treble name=\"Vocal Melody\" snm=\"Vocal\"\nV: Ins clef=treble name=\"Ins Melody\" snm=\"Inst.\"\nK:G\n% verse\nV: Vocal\nG8 A8 B8 c8|\nV: Ins\nG8 B8 d8 g8|\n";
        let res = export_makam_midi(abc, MakamId::Rast, Some(7), 2.0).unwrap();
        assert!(res.annotated_abc.contains("WerrOzan 53-Koma Makam: Rast"));
        assert!(!res.midi_bytes.is_empty());
        assert_eq!(res.note_mappings.len(), 4);
        // 3rd note is B4 (pitch 71): Rast Segah 17 komas (~ -15.09c)
        assert_eq!(res.note_mappings[2].koma_in_octave, 17);
        assert!(res.note_mappings[2].pitch_bend_14bit < 8192);
        // Verify MIDI parses cleanly via smf::read
        let song = smf::read(&res.midi_bytes).unwrap();
        assert_eq!(song.tracks.len(), 3);
        assert_eq!(song.tracks[1].notes.len(), 4);

        // Single-voice minimal ABC also normalizes and exports cleanly
        let minimal = "X:1\nM:4/4\nL:1/16\nQ:1/4=80\nK:Dm\nz4d2e2d3dc2B2|";
        let min_res = export_makam_midi(minimal, MakamId::Hicaz, Some(9), 2.0).unwrap();
        assert_eq!(min_res.note_mappings.len(), 6);
    }
}
