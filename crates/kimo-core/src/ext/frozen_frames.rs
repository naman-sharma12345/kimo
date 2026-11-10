//! v6 eyes: frozen_frames. Where the picture stands still over a range, as runs of project frames. Every project frame in
//! [start, end) is rendered (full resolution unless `scale` asks for a smaller copy) and compared with the first frame of
//! the run it may belong to: a frame whose largest absolute R, G or B difference from that first frame is at or under
//! `tolerance` (0..1, default 0 = identical) extends the run; a run of `min_run` (default 2) or more frames is reported
//! with its start frame, end frame, length and the largest per-pixel difference inside it. Comparing with the run's first
//! frame (not the frame before) keeps a slow drift from chaining into a hold. Black frames get no special case: they are a
//! hold only when they repeat within the tolerance like any other frame. Limit: the tolerance applies to the total drift from
//! the run's first frame, so a very slow pan reads as a hold until its drift crosses it, and nothing under it is detected
//! (tests/frozen_frames.rs pins an edge-free pan at about 1.1 counts per frame that holds for frames 0-5 at 6.5/255).
//! Raw numbers, no verdict.
use super::kit::*;
use crate::perception::analysis::{self, Request};

const MAX_FRAMES: usize = 6000;
pub const MAX_RUNS: usize = 200;
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a number in [{lo}, {hi}]")) }
}
fn int(a: &Value, k: &str, d: u64, lo: u64, hi: u64) -> Result<u64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_u64().filter(|x| *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a whole number {lo}..{hi}")) }
}

/// The largest |R, G or B| difference (0..1) between two RGB planes of the same size; a size change counts as 1.
pub fn max_diff(a: &[Vec<f64>; 3], b: &[Vec<f64>; 3]) -> f64 {
    if a[0].len() != b[0].len() { return 1.0; }
    let mut m = 0.0f64; for c in 0..3 { for (x, y) in a[c].iter().zip(&b[c]) { m = m.max((x - y).abs()); } } m
}

/// One run: frame indices `start..=end` (into the sampled range) and the largest difference of any of its frames from `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub start: usize, pub end: usize, pub max_diff: f64 }
impl Run { pub fn len(&self) -> usize { self.end - self.start + 1 } }

/// Streams frames in order and cuts them into runs. Each frame is compared with the current run's first frame: at or under
/// `tolerance` it joins the run, otherwise it starts the next one. Only runs of `min_run` frames or more are kept.
pub struct Runs { tolerance: f64, min_run: usize, anchor: Option<[Vec<f64>; 3]>, cur: Run, n: usize, pub kept: Vec<Run> }
impl Runs {
    pub fn new(tolerance: f64, min_run: usize) -> Self { Runs { tolerance, min_run, anchor: None, cur: Run { start: 0, end: 0, max_diff: 0.0 }, n: 0, kept: vec![] } }
    pub fn push(&mut self, frame: [Vec<f64>; 3]) {
        let i = self.n; self.n += 1;
        match &self.anchor {
            Some(a) => { let d = max_diff(a, &frame);
                if d <= self.tolerance { self.cur.end = i; self.cur.max_diff = self.cur.max_diff.max(d); }
                else { self.close(); self.cur = Run { start: i, end: i, max_diff: 0.0 }; self.anchor = Some(frame); } }
            None => { self.cur = Run { start: i, end: i, max_diff: 0.0 }; self.anchor = Some(frame); }
        }
    }
    fn close(&mut self) { if self.anchor.is_some() && self.cur.len() >= self.min_run { self.kept.push(self.cur.clone()); } }
    pub fn finish(mut self) -> Vec<Run> { self.close(); self.kept }
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = match a.get("source") { None | Some(Value::Null) => "timeline".to_string(), Some(v) => v.as_str().ok_or("invalid_source: source must be timeline or clip:<id>")?.to_string() };
    let (parsed, mut bounds) = if source == "timeline" { ("composite".to_string(), (0.0, ss.project.duration())) } else if let Some(id) = source.strip_prefix("clip:") {
        let c = ss.project.find_clip(id).ok_or("invalid_source: missing clip")?; (source.clone(), (c.start, c.start + c.duration)) } else { return Err("invalid_source: Valid: timeline, clip:<id>".into()) };
    let fps = ss.project.fps; if !(fps > 0.0 && fps <= 1000.0) { return Err("out_of_range: project fps must be in (0, 1000]".into()); }
    let st = num(a, "start", bounds.0, 0.0, 1e7)?; let en = num(a, "end", bounds.1, 0.0, 1e7)?;
    if st < bounds.0 - 1e-9 || en > bounds.1 + 1e-9 || en <= st { return Err(format!("out_of_range: start/end must lie inside the source span [{}, {}] with start < end", bounds.0, bounds.1)); }
    bounds = (st, en);
    let tolerance = num(a, "tolerance", 0.0, 0.0, 1.0)?;
    let min_run = int(a, "min_run", 2, 2, MAX_FRAMES as u64)? as usize;
    let scale = match a.get("scale") { None | Some(Value::Null) => None, Some(_) => Some(int(a, "scale", 0, 16, 7680)? as f64) };
    let f0 = (bounds.0 * fps - 1e-9).ceil().max(0.0) as i64; let f1 = (bounds.1 * fps - 1e-9).ceil() as i64;
    let nf = (f1 - f0).max(0) as usize;
    if nf < 1 { return Err("out_of_range: the range holds no project frame".into()); }
    if nf > MAX_FRAMES { return Err(format!("out_of_range: the range holds {nf} frames; at most {MAX_FRAMES} (shorten start/end)")); }
    let mut runs = Runs::new(tolerance, min_run); let mut dims = (0usize, 0usize);
    for i in 0..nf {
        crate::guard::check();
        let t = (f0 + i as i64) as f64 / fps;
        let im = analysis::render(ss, &Request { time: t, source: parsed.clone() })?;
        let im = match scale { Some(sc) => { let ratio = (sc / im.w.max(im.h) as f64).min(1.0);
            let (w, h) = (((im.w as f64 * ratio).round() as usize).max(1), ((im.h as f64 * ratio).round() as usize).max(1));
            if w != im.w || h != im.h { im.downscale(w, h) } else { im } } None => im };
        dims = (im.w, im.h); runs.push(im.rgb);
    }
    let found = runs.finish();
    let tf = |f: usize| r6((f0 + f as i64) as f64 / fps);
    let list: Vec<Value> = found.iter().take(MAX_RUNS).map(|r| json!({"start_frame": f0 + r.start as i64, "end_frame": f0 + r.end as i64, "length": r.len(),
        "max_diff": r6(r.max_diff), "max_diff_255": (r.max_diff * 255.0).round() as u64, "start_s": tf(r.start), "end_s": tf(r.end + 1), "duration_s": r6(r.len() as f64 / fps)})).collect();
    let held: usize = found.iter().map(|r| r.len() - 1).sum();
    Ok(json!({
        "source": source, "start": bounds.0, "end": bounds.1, "fps": fps, "first_frame": f0, "frames": nf, "compared": [dims.0, dims.1],
        "params": {"tolerance": tolerance, "min_run": min_run, "scale": scale},
        "runs": list, "run_count": found.len(), "runs_truncated": found.len() > MAX_RUNS,
        "held_frames": held, "longest_run": found.iter().map(|r| r.len()).max(),
    }))
}

pub fn commands() -> Vec<Command> { vec![Command { name: "frozen_frames", mutates: false,
    description: "Find runs of frames where the picture stands still: start frame, end frame, length and the largest pixel difference inside each run. Limit: each frame is compared with its run's first frame, so a total drift from that frame within tolerance still reads as a hold (at 6.5/255 = 0.0255, any drift under 6.5/255): a very slow pan reads as a hold until its total drift crosses the line, and nothing under the line is detected. Every project frame in start..end (timeline seconds, default the source span, 1..6000 frames) of source = timeline (default) or clip:<id> is rendered at full resolution (or on a copy scaled to scale px on the long side) and compared with the first frame of its run: a frame whose largest |R, G or B difference| (0..1) from that first frame is at most tolerance (default 0 = identical) extends the run, and runs of min_run (default 2) or more frames are reported (up to 200), each carrying max_diff (0..1) and max_diff_255. Comparing with the run's first frame keeps a slow drift from chaining into a hold. Black frames are not special: they are a hold only when they repeat within the tolerance. Also returns run_count, held_frames (frames that repeat their run's first frame) and longest_run. Bad arguments are refused, never clamped. No verdict.",
    schema: obj(vec![
        ("source", s("timeline (default) or clip:<id>")),
        ("start", n("range start, timeline seconds")),
        ("end", n("range end, timeline seconds")),
        ("tolerance", json!({"type": "number", "minimum": 0, "maximum": 1, "description": "largest R,G,B difference (0..1) from the run's first frame that still counts as the same picture, default 0 (identical)"})),
        ("min_run", json!({"type": "integer", "minimum": 2, "maximum": 6000, "description": "frames in a reported run at least, default 2"})),
        ("scale", json!({"type": "integer", "minimum": 16, "maximum": 7680, "description": "compare on a copy this many px on the long side; default full resolution"})),
    ], &[]), run }] }
