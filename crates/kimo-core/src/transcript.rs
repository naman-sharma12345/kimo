//! Transcript data model (RV-8 step 1). Pure data and pure functions: no MCP, dispatch or Session dependency (AC-1).
//!
//! A transcript is an optional reference script plus takes and words. A word keeps what the recognizer heard (`recognized`, never overwritten) and the text
//! shown in captions (`display`). Word times on the timeline are DERIVED from the word's take by one formula (`Take::to_timeline`), never edited directly:
//!   timeline = take_start + (src_t - take.src_start) / take.speed
//! where take_start is `Take::timeline_start` when set, otherwise the end of the previous chosen take in `order` (see `Transcript::derive_times`).
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakeSource {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub clip_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub path_key: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakeStatus { Chosen, Alternate, Rejected }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Take {
    pub id: String,
    #[serde(default)] pub source: TakeSource,
    /// Source seconds covered by this take.
    pub src_start: f64,
    pub src_end: f64,
    /// Timeline second where src_start lands. None: placed right after the previous chosen take in `order`.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub timeline_start: Option<f64>,
    /// Playback speed of the take on the timeline (1.0 = normal).
    #[serde(default = "one")] pub speed: f64,
    /// Script order: lower is earlier. Independent of file or import order.
    pub order: u32,
    pub status: TakeStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub note: String,
}
fn one() -> f64 { 1.0 }
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WordFlag { LowConfidence, Repeated, Gap, Manual, Interpolated }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub id: String,
    /// Text used in captions; starts equal to `recognized`.
    pub display: String,
    /// What the recognizer produced. Never changed after import.
    pub recognized: String,
    /// Timeline seconds, derived from the take (`Transcript::derive_times`).
    pub start: f64,
    pub end: f64,
    /// Source seconds (the recognizer's times in the take's media).
    pub src_start: f64,
    pub src_end: f64,
    pub take_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub confidence: Option<f32>,
    #[serde(default)] pub corrected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub flags: Vec<WordFlag>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub id: String,
    pub name: String,
    #[serde(default)] pub language: String,
    /// User-supplied reference script. Never generated.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub script_text: Option<String>,
    #[serde(default)] pub takes: Vec<Take>,
    #[serde(default)] pub words: Vec<Word>,
    /// Bumped by every edit so previews can tell stale from current.
    #[serde(default)] pub version: u32,
}

impl Take {
    pub fn src_len(&self) -> f64 { (self.src_end - self.src_start).max(0.0) }
    pub fn timeline_len(&self) -> f64 { self.src_len() / self.speed }
    /// The single time formula. `start` is the take's effective timeline start.
    pub fn to_timeline(&self, start: f64, src_t: f64) -> f64 { start + (src_t - self.src_start) / self.speed }
}
impl Word {
    pub fn new(id: &str, take_id: &str, text: &str, src_start: f64, src_end: f64, confidence: Option<f32>) -> Word {
        Word { id: id.into(), display: text.into(), recognized: text.into(), start: src_start, end: src_end, src_start, src_end, take_id: take_id.into(), confidence, corrected: false, flags: vec![] }
    }
    pub fn has(&self, f: WordFlag) -> bool { self.flags.contains(&f) }
    pub fn add_flag(&mut self, f: WordFlag) { if !self.flags.contains(&f) { self.flags.push(f); self.flags.sort(); } }
}
impl Transcript {
    /// Takes that appear on the timeline, in script order (`order`, then id).
    pub fn chosen_takes(&self) -> Vec<&Take> { let mut t: Vec<&Take> = self.takes.iter().filter(|t| t.status == TakeStatus::Chosen).collect(); t.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id))); t }
    /// Effective timeline start of each chosen take: explicit `timeline_start`, else the end of the previous chosen take. (take id, start)
    pub fn take_starts(&self) -> Vec<(String, f64)> {
        let mut cursor = 0.0; let mut out = vec![];
        for t in self.chosen_takes() { let s = t.timeline_start.unwrap_or(cursor); out.push((t.id.clone(), s)); cursor = s + t.timeline_len(); } out
    }
    /// Recomputes `start`/`end` of every word from its take. Words of alternate/rejected takes keep their own timeline_start (0 when unset).
    pub fn derive_times(&mut self) {
        let starts: std::collections::BTreeMap<String, f64> = self.take_starts().into_iter().collect();
        let takes: std::collections::BTreeMap<&str, &Take> = self.takes.iter().map(|t| (t.id.as_str(), t)).collect();
        for w in &mut self.words { if let Some(t) = takes.get(w.take_id.as_str()) { let s = starts.get(&t.id).copied().or(t.timeline_start).unwrap_or(0.0); w.start = t.to_timeline(s, w.src_start); w.end = t.to_timeline(s, w.src_end); } }
    }
    /// Words of chosen takes in script order (take order, then source time).
    pub fn script_words(&self) -> Vec<&Word> {
        let mut out = vec![]; for t in self.chosen_takes() { let mut ws: Vec<&Word> = self.words.iter().filter(|w| w.take_id == t.id).collect(); ws.sort_by(|a, b| a.src_start.partial_cmp(&b.src_start).unwrap_or(std::cmp::Ordering::Equal)); out.extend(ws); } out
    }
    /// Structural invariants: unique ids, known takes, finite times, end >= start, src times inside the take, speed > 0.
    pub fn validate(&self) -> Result<(), String> {
        let mut tids = std::collections::BTreeSet::new();
        for t in &self.takes {
            if !tids.insert(t.id.as_str()) { return Err(format!("duplicate take id '{}'", t.id)); }
            if !(t.src_start.is_finite() && t.src_end.is_finite()) || t.src_end < t.src_start { return Err(format!("take '{}': bad source range {}..{}", t.id, t.src_start, t.src_end)); }
            if !(t.speed.is_finite() && t.speed > 0.0) { return Err(format!("take '{}': speed must be > 0", t.id)); }
            if t.timeline_start.map_or(false, |s| !s.is_finite()) { return Err(format!("take '{}': timeline_start is not finite", t.id)); }
        }
        let mut wids = std::collections::BTreeSet::new();
        for w in &self.words {
            if !wids.insert(w.id.as_str()) { return Err(format!("duplicate word id '{}'", w.id)); }
            if !tids.contains(w.take_id.as_str()) { return Err(format!("word '{}' belongs to unknown take '{}'", w.id, w.take_id)); }
            if !(w.src_start.is_finite() && w.src_end.is_finite()) || w.src_end < w.src_start { return Err(format!("word '{}': bad time {}..{}", w.id, w.src_start, w.src_end)); }
        }
        Ok(())
    }
}

/// The words (and where lines break) a caption cue was composed from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CueBinding { pub word_ids: Vec<String>, /// Indices into `word_ids` after which the text breaks to a new line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub breaks_after: Vec<usize> }
/// Cue text from word display texts: words joined by spaces, a newline after every index in `breaks_after`. The one composer used by caption generation AND word edits.
pub fn compose_cue_text(display: &[&str], breaks_after: &[usize]) -> String {
    let mut out = String::new(); for (i, w) in display.iter().enumerate() { out.push_str(w); if i + 1 < display.len() { out.push(if breaks_after.contains(&i) { '\n' } else { ' ' }); } } out
}
/// Next unused `<prefix><n>` id (one past the largest numeric suffix among `ids`).
pub fn next_id<'a>(prefix: &str, ids: impl Iterator<Item = &'a str>) -> String { let n = ids.filter_map(|i| i.strip_prefix(prefix).and_then(|r| r.parse::<u64>().ok())).max().unwrap_or(0); format!("{prefix}{}", n + 1) }
impl Transcript {
    /// Sets display text of the word `id`: keeps `recognized`, timing and take; `corrected` follows display != recognized; flag `manual` follows `corrected`.
    pub fn set_display(&mut self, id: &str, text: &str) -> Result<String, String> {
        let text = crate::transcript_parse::sanitize(text); if text.is_empty() { return Err("display text must not be empty".into()); }
        let w = self.words.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("no such word '{id}'"))?;
        let old = std::mem::replace(&mut w.display, text); w.corrected = w.display != w.recognized;
        if w.corrected { w.add_flag(WordFlag::Manual); } else { w.flags.retain(|f| *f != WordFlag::Manual); } self.version += 1; Ok(old)
    }
}
