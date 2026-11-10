//! RV-8: caption quality checks over plain cue data (no Session, no rasterizer; AC-1). Findings never quote cue text: they carry cue ids, a code, a fixed message and a fix hint.
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Profile { pub name: &'static str, pub max_chars: usize, pub max_lines: usize, pub max_cps: f64, pub min_dur: f64, pub max_dur: f64, pub gap_frames: f64 }
pub fn profile(name: &str) -> Option<Profile> { Some(match name {
    "web" => Profile { name: "web", max_chars: 42, max_lines: 2, max_cps: 17.0, min_dur: 1.0, max_dur: 6.0, gap_frames: 2.0 },
    "broadcast" => Profile { name: "broadcast", max_chars: 37, max_lines: 2, max_cps: 17.0, min_dur: 1.0, max_dur: 7.0, gap_frames: 2.0 },
    "social_9x16" => Profile { name: "social_9x16", max_chars: 28, max_lines: 2, max_cps: 20.0, min_dur: 0.7, max_dur: 4.0, gap_frames: 2.0 },
    "custom" => Profile { name: "custom", max_chars: 42, max_lines: 2, max_cps: 17.0, min_dur: 1.0, max_dur: 6.0, gap_frames: 2.0 }, _ => return None }) }
#[derive(Clone, Debug)]
pub struct WordIn { pub id: String, pub start: f64, pub end: f64, pub low_confidence: bool }
#[derive(Clone, Debug)]
pub struct CueIn { pub id: String, pub start: f64, pub end: f64, pub text: String, pub words: Vec<WordIn> }
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Finding { pub severity: &'static str, pub code: &'static str, pub cue_ids: Vec<String>, pub message: String, pub fix_hint: String }
fn f(sev: &'static str, code: &'static str, ids: &[&str], message: &str, hint: &str) -> Finding { Finding { severity: sev, code, cue_ids: ids.iter().map(|s| s.to_string()).collect(), message: message.into(), fix_hint: hint.into() } }

/// `cues` in timeline order. `script_tokens`: normalised script words when a script exists (enables SCRIPT_MISMATCH).
pub fn validate(cues: &[CueIn], p: &Profile, fps: f64, script_tokens: Option<&std::collections::BTreeSet<String>>) -> Vec<Finding> {
    let mut out = vec![]; let gap = p.gap_frames / fps.max(1.0);
    for (i, c) in cues.iter().enumerate() {
        let id = [c.id.as_str()]; let dur = c.end - c.start; let lines: Vec<&str> = c.text.split('\n').collect(); let chars = c.text.replace('\n', " ").chars().count() as f64;
        if dur < p.min_dur - 1e-9 { out.push(f("warning", "CUE_TOO_SHORT", &id, "cue is shorter than the profile minimum", "extend the cue end or merge it with a neighbour")); }
        if dur > p.max_dur + 1e-9 { out.push(f("warning", "CUE_TOO_LONG", &id, "cue is longer than the profile maximum", "split the cue at a sentence or clause break")); }
        if dur > 0.0 && chars / dur > p.max_cps + 1e-9 { out.push(f("error", "READING_SPEED", &id, "reading speed is above the profile limit", "extend the cue or regenerate with transcript_to_captions")); }
        if lines.iter().any(|l| l.chars().count() > p.max_chars) { out.push(f("error", "LINE_TOO_LONG", &id, "a line is longer than the profile limit", "re-wrap with a smaller max_chars_per_line")); }
        if lines.len() > p.max_lines { out.push(f("error", "TOO_MANY_LINES", &id, "cue has more lines than the profile allows", "regenerate with transcript_to_captions or split the cue")); }
        if c.text.split_whitespace().count() > 1 && lines.iter().any(|l| l.split_whitespace().count() == 1) { out.push(f("warning", "ORPHAN_WORD", &id, "a line holds a single word", "re-wrap so each line has at least two words")); }
        if c.words.iter().any(|w| w.low_confidence) { out.push(f("warning", "LOW_CONFIDENCE_WORD", &id, "cue contains words below the confidence threshold", "check the words with transcript_get and correct them with transcript_edit_word")); }
        if c.words.iter().any(|w| w.end < w.start - 1e-9) || c.words.windows(2).any(|w| w[1].start < w[0].start - 1e-9) { out.push(f("error", "WORD_TIMING_NON_MONOTONIC", &id, "bound word times are not in order", "re-import the transcript or fix the take order with transcript_set_take")); }
        if let Some(sc) = script_tokens { if c.text.split_whitespace().map(crate::transcript_coverage::norm).filter(|t| !t.is_empty()).any(|t| !sc.contains(&t)) { out.push(f("warning", "SCRIPT_MISMATCH", &id, "cue contains words that are not in the script", "run transcript_coverage to see extra and missing passages")); } }
        if let Some(n) = cues.get(i + 1) { let d = n.start - c.end; let ids = [c.id.as_str(), n.id.as_str()];
            if d < -1e-6 { out.push(f("error", "CAPTION_OVERLAP", &ids, "two cues overlap in time", "regenerate with transcript_to_captions or shorten the earlier cue")); }
            else if d < gap - 1e-6 { out.push(f("warning", "CAPTION_GAP_TOO_SMALL", &ids, "gap between cues is below the minimum frames", "end the earlier cue at least min_gap_frames before the next starts")); } }
    }
    out
}
pub const SKIPPED: [(&str, &str); 3] = [("OFF_SAFE_AREA", "text bounds are not available without the renderer"), ("CUE_BEFORE_AUDIO", "no voice-activity data for the audio"), ("CUE_AFTER_AUDIO", "no voice-activity data for the audio")];
