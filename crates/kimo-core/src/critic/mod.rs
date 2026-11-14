//! Critics: domain checks that turn what the engine knows and measures into verdicts an agent can act on. Every check
//! returns `Finding`s with a severity, where (time, range, clip), what was measured against what threshold, a plain
//! message and, when there is one, the exact tool call that fixes it.
//!
//! Two kinds of check:
//! * timeline critics (`editing`, `transitions`, `text`, `motion`) read the project only, so they are cheap and run on
//!   every edit (limited to the spans the edit touched) and over the whole timeline in `review`;
//! * picture critics (`picture`) read rendered frames (luma, clipping, crush, an edit that turned a frame black).
//!
//! Thresholds are conservative on purpose: a warning should mean "a viewer would notice", not "a number moved".
use crate::model::Project;
use serde::Serialize;
use serde_json::{json, Value};

pub mod editing;
pub mod motion;
pub mod picture;
pub mod text;
pub mod transitions;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Info, Warning, Error }

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> { match s { "info" => Some(Severity::Info), "warning" | "warn" => Some(Severity::Warning), "error" => Some(Severity::Error), _ => None } }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: Severity,
    /// editing | transitions | text | motion | color | picture | audio | effects
    pub domain: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")] pub at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub range: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")] pub clip_id: Option<String>,
    #[serde(skip_serializing_if = "Value::is_null")] pub measured: Value,
    #[serde(skip_serializing_if = "Value::is_null")] pub threshold: Value,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")] pub fix: Value,
}

impl Finding {
    pub fn new(code: &'static str, severity: Severity, domain: &'static str, message: impl Into<String>) -> Finding {
        Finding { code, severity, domain, at: None, range: None, clip_id: None, measured: Value::Null, threshold: Value::Null, message: message.into(), fix: Value::Null }
    }
    pub fn at(mut self, t: f64) -> Self { self.at = Some(r3(t)); self }
    pub fn range(mut self, a: f64, b: f64) -> Self { self.range = Some([r3(a), r3(b)]); self }
    pub fn clip(mut self, id: &str) -> Self { self.clip_id = Some(id.to_string()); self }
    pub fn measured(mut self, v: Value) -> Self { self.measured = v; self }
    pub fn threshold(mut self, v: Value) -> Self { self.threshold = v; self }
    pub fn fix(mut self, tool: &str, args: Value) -> Self { self.fix = json!({"tool": tool, "args": args}); self }
    /// Does this finding touch [a, b]?
    pub fn touches(&self, a: f64, b: f64) -> bool {
        if let Some([s, e]) = self.range { return s <= b && e >= a; }
        if let Some(t) = self.at { return t >= a && t <= b; }
        true
    }
}

pub fn r3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

/// One frame at the project rate.
pub fn frame(p: &Project) -> f64 { 1.0 / if p.fps > 0.0 { p.fps } else { 30.0 } }

/// Every timeline critic over the whole project.
pub fn timeline(p: &Project) -> Vec<Finding> {
    let mut v = editing::check(p);
    v.extend(transitions::check(p));
    v.extend(text::check(p));
    v.extend(motion::check(p));
    v
}

/// Timeline critics limited to findings that touch any of `spans` (an edit only reports what it is responsible for).
pub fn timeline_in(p: &Project, spans: &[[f64; 2]]) -> Vec<Finding> {
    timeline(p).into_iter().filter(|f| spans.iter().any(|s| f.touches(s[0], s[1]))).collect()
}

/// Most severe first, then by time.
pub fn sort(v: &mut [Finding]) {
    v.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.at.or(a.range.map(|r| r[0])).unwrap_or(0.0).partial_cmp(&b.at.or(b.range.map(|r| r[0])).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)));
}

pub fn counts(v: &[Finding]) -> Value {
    json!({"error": v.iter().filter(|f| f.severity == Severity::Error).count(), "warning": v.iter().filter(|f| f.severity == Severity::Warning).count(), "info": v.iter().filter(|f| f.severity == Severity::Info).count()})
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Real sessions built through the engine's own tools (no media decoded: solid colour and text clips), so the
    //! critics are tested on projects exactly as agents make them.
    use crate::commands::{dispatch, Session};
    use serde_json::{json, Value};
    pub fn session() -> Session {
        let mut ss = Session::new();
        dispatch(&mut ss, "project_set", &json!({"width": 1920, "height": 1080, "fps": 30})).unwrap();
        ss
    }
    pub fn call(ss: &mut Session, name: &str, args: Value) -> Value { dispatch(ss, name, &args).unwrap_or_else(|e| panic!("{name}: {e}")) }
    pub fn track(ss: &mut Session) -> String { call(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }
    pub fn solid(ss: &mut Session, track: &str, start: f64, dur: f64) -> String {
        call(ss, "add_clip", json!({"track_id": track, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
    }
    pub fn text(ss: &mut Session, track: &str, t: &str, start: f64, dur: f64) -> String {
        call(ss, "add_text", json!({"track_id": track, "text": t, "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
    }
    pub fn codes(v: &[super::Finding]) -> Vec<&'static str> { v.iter().map(|f| f.code).collect() }
}
