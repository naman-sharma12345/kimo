//! v6 eyes: black_frames. Runs of near-black frames over a range. Every project frame in [start, end) is rendered at full
//! resolution and its brightest pixel is measured: Rec.709 luma Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' on the full-range
//! 8-bit output codes / 255 (the same frame and formula lint_frame quality reads; limited-range video is expanded to full
//! range when the engine decodes it, so video black Y=16 reads as 0). A frame is near-black when the luma of EVERY pixel is at
//! or under `luma_max` (0..1, default 0.02 = 5.1/255, the fixture line; 0 is strict mode): the whole frame counts, so letterbox bars, a black corner, a dark
//! scene with one brighter object or a black frame with a logo are not near-black. A run starts at the first near-black frame
//! and ends at the last one (the first frame over the line is not in it); runs of `min_run` (default 2, so a lone black frame
//! is not reported) or more frames are reported with start frame, end frame, length and max_luma, the brightest pixel inside
//! the run (0 for pure black). No scale argument: a smaller copy averages pixels and would lower the max. Motion is not looked
//! at: a black run that is also still is reported by frozen_frames too, with the same frame numbers, and neither tool dedupes
//! the other. Limit: the tool reads the rendered picture and does not guess a range, so a limited-range file wrongly tagged
//! full range shows its black at 16/255 and is not reported at luma_max 0.02 (tests/black_frames.rs pins it). To make that
//! miss visible, the report carries each media clip's declared range tag (tv, pc or unknown) and lists stretches of frames that
//! sit flat at about 16/255 (darkest and brightest pixel both 15-17/255) outside any run, with a note.
//! Rules and numbers pre-registered in tests/data/black_frames/THRESHOLD_ADDENDUM.md and THRESHOLD_ADDENDUM_2.md. Raw numbers,
//! no verdict.
use super::kit::*;
use crate::perception::analysis::{self, Image, Request};

const MAX_FRAMES: usize = 6000;
/// The default line: 0.02 = 5.1/255, the same as the fixture line in tests/data/black_frames (THRESHOLD_ADDENDUM_2.md).
pub const DEFAULT_LUMA_MAX: f64 = 0.02;
/// A frame "sits flat at 16/255" when its darkest and brightest pixel luma both lie in this band (limited-range black read as full).
pub const FLAT16: (f64, f64) = (15.0 / 255.0, 17.0 / 255.0);
pub const MAX_RUNS: usize = 200;
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }
fn r2(x: f64) -> f64 { (x * 100.0).round() / 100.0 }
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a number in [{lo}, {hi}]")) }
}
fn int(a: &Value, k: &str, d: u64, lo: u64, hi: u64) -> Result<u64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_u64().filter(|x| *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a whole number {lo}..{hi}")) }
}

/// The brightest pixel of a frame: the largest Rec.709 luma (0..1) over every pixel.
pub fn max_luma(im: &Image) -> f64 { luma_range(im).1 }
/// The darkest and the brightest pixel's Rec.709 luma (0..1).
pub fn luma_range(im: &Image) -> (f64, f64) { (0..im.w * im.h).map(|i| im.luma(i)).fold((f64::INFINITY, 0.0f64), |(lo, hi), y| (lo.min(y), hi.max(y))) }
/// Stretches (start, end) of at least `min_run` consecutive true flags.
pub fn stretches(flags: &[bool], min_run: usize) -> Vec<(usize, usize)> {
    let mut out = vec![]; let mut s: Option<usize> = None;
    for (i, f) in flags.iter().chain(std::iter::once(&false)).enumerate() { match (s, *f) { (None, true) => s = Some(i), (Some(a), false) => { if i - a >= min_run { out.push((a, i - 1)); } s = None; } _ => {} } }
    out
}
/// The declared colour range of a media file's first video stream: "tv", "pc" or "unknown".
fn range_tag(path: &str) -> &'static str {
    match crate::color::delivery::inspect(path) { Ok(v) => match v["streams"].as_array().and_then(|s| s.iter().find(|x| x["codec_type"] == "video")).and_then(|x| x["color_range"].as_str()) { Some("tv") => "tv", Some("pc") => "pc", _ => "unknown" }, Err(_) => "unknown" }
}

/// One run: frame indices `start..=end` (into the sampled range) and the brightest pixel's luma in any of its frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub start: usize, pub end: usize, pub max_luma: f64 }
impl Run { pub fn len(&self) -> usize { self.end - self.start + 1 } }

/// Streams per-frame max luma in order and cuts runs: a frame at or under `luma_max` is near-black and extends (or starts)
/// the current run; a frame over it closes the run. Only runs of `min_run` frames or more are kept.
pub struct Runs { luma_max: f64, min_run: usize, cur: Option<Run>, n: usize, pub kept: Vec<Run> }
impl Runs {
    pub fn new(luma_max: f64, min_run: usize) -> Self { Runs { luma_max, min_run, cur: None, n: 0, kept: vec![] } }
    pub fn push(&mut self, frame_max: f64) {
        let i = self.n; self.n += 1;
        if frame_max <= self.luma_max {
            match &mut self.cur { Some(r) => { r.end = i; r.max_luma = r.max_luma.max(frame_max); } None => self.cur = Some(Run { start: i, end: i, max_luma: frame_max }) }
        } else { self.close(); }
    }
    fn close(&mut self) { if let Some(r) = self.cur.take() { if r.len() >= self.min_run { self.kept.push(r); } } }
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
    let luma_max = num(a, "luma_max", DEFAULT_LUMA_MAX, 0.0, 1.0)?;
    let min_run = int(a, "min_run", 2, 1, MAX_FRAMES as u64)? as usize;
    let f0 = (bounds.0 * fps - 1e-9).ceil().max(0.0) as i64; let f1 = (bounds.1 * fps - 1e-9).ceil() as i64;
    let nf = (f1 - f0).max(0) as usize;
    if nf < 1 { return Err("out_of_range: the range holds no project frame".into()); }
    if nf > MAX_FRAMES { return Err(format!("out_of_range: the range holds {nf} frames; at most {MAX_FRAMES} (shorten start/end)")); }
    let mut runs = Runs::new(luma_max, min_run); let mut dims = (0usize, 0usize); let mut flat = vec![false; nf];
    for i in 0..nf {
        crate::guard::check();
        let t = (f0 + i as i64) as f64 / fps;
        let im = analysis::render(ss, &Request { time: t, source: parsed.clone() })?;
        let (lo, hi) = luma_range(&im); flat[i] = lo >= FLAT16.0 && hi <= FLAT16.1;
        dims = (im.w, im.h); runs.push(hi);
    }
    let found = runs.finish();
    for r in &found { for f in r.start..=r.end { flat[f] = false; } }
    let suspect: Vec<Value> = stretches(&flat, min_run).iter().map(|(s0, e0)| json!({"start_frame": f0 + *s0 as i64, "end_frame": f0 + *e0 as i64, "length": e0 - s0 + 1})).collect();
    let note = if suspect.is_empty() { Value::Null } else { Value::String(format!("{} stretch(es) of frames sit flat at about 16/255 (darkest and brightest pixel both 15-17/255) and are not near-black at luma_max {luma_max}: likely limited-range black read as full range (see range_tags)", suspect.len())) };
    let only = source.strip_prefix("clip:");
    let mut tags: Vec<Value> = vec![]; let mut words: Vec<&str> = vec![];
    for tr in &ss.project.tracks { for cl in &tr.clips {
        if let crate::model::Source::Media { asset } = &cl.source {
            if only.map_or(false, |id| id != cl.id) || cl.start >= bounds.1 || cl.start + cl.duration <= bounds.0 { continue; }
            let w = ss.project.assets.get(asset).map(|a| range_tag(&ss.project.resolve(&a.path))).unwrap_or("unknown");
            tags.push(json!({"clip_id": cl.id, "color_range": w})); words.push(w);
        } } }
    let tag = if words.is_empty() { Value::Null } else if words.iter().all(|w| *w == words[0]) { json!(words[0]) } else { json!("mixed") };
    let tf = |f: usize| r6((f0 + f as i64) as f64 / fps);
    let list: Vec<Value> = found.iter().take(MAX_RUNS).map(|r| json!({"start_frame": f0 + r.start as i64, "end_frame": f0 + r.end as i64, "length": r.len(),
        "max_luma": r6(r.max_luma), "max_luma_255": r2(r.max_luma * 255.0), "start_s": tf(r.start), "end_s": tf(r.end + 1), "duration_s": r6(r.len() as f64 / fps)})).collect();
    let black: usize = found.iter().map(|r| r.len()).sum();
    Ok(json!({
        "source": source, "start": bounds.0, "end": bounds.1, "fps": fps, "first_frame": f0, "frames": nf, "measured": [dims.0, dims.1],
        "params": {"luma_max": luma_max, "min_run": min_run},
        "runs": list, "run_count": found.len(), "runs_truncated": found.len() > MAX_RUNS,
        "black_frames": black, "longest_run": found.iter().map(|r| r.len()).max(),
        "range_tag": tag, "range_tags": tags, "limited_range_suspect": suspect, "note": note,
    }))
}

pub fn commands() -> Vec<Command> { vec![Command { name: "black_frames", mutates: false,
    description: "Find runs of near-black frames: start frame, end frame, length and the brightest pixel inside each run. The whole frame must be near-black, judged on the frame's MAX luma, not its mean: a frame counts only when the Rec.709 luma of every pixel is at most luma_max, so one small bright element (a logo, a lit object, a few pixels) exempts the frame, and letterbox bars, a black corner or a dark scene with a brighter object are not runs. Units: luma_max is on a 0..1 scale (default 0.02 = 5.1/255; 0 is strict mode, exactly black); each run reports max_luma on the same 0..1 scale and max_luma_255 on the 0..255 code scale (4/255 reads as max_luma 0.015686 and max_luma_255 4), so luma_max is compared to max_luma, never to max_luma_255. Limit: luma is read from the rendered full-range picture; limited-range video is expanded at decode (video black 16 reads as 0), but a limited-range file wrongly tagged full range shows its black at 16/255 and is not found at the default; the report then lists those frames in limited_range_suspect with a note, beside each media clip's declared range tag (range_tags: tv, pc or unknown). Every project frame in start..end (timeline seconds, default the source span, 1..6000 frames) of source = timeline (default) or clip:<id> is rendered at full resolution; a run starts at the first near-black frame and ends before the first frame over luma_max, and runs of min_run (default 2, so a lone black frame is not reported) or more frames are reported (up to 200), each carrying max_luma (0..1 scale, the brightest pixel in the run; 0 for pure black) and max_luma_255 (the same value on the 0..255 scale). Motion is not looked at: a black run that is also still is reported by frozen_frames too, with the same frame numbers. Also returns run_count, black_frames (frames inside reported runs) and longest_run. Bad arguments are refused, never clamped. No verdict.",
    schema: obj(vec![
        ("source", s("timeline (default) or clip:<id>")),
        ("start", n("range start, timeline seconds")),
        ("end", n("range end, timeline seconds")),
        ("luma_max", json!({"type": "number", "minimum": 0, "maximum": 1, "description": "0..1 scale (not /255): a frame is near-black when its brightest pixel's Rec.709 luma is at most this (max over every pixel, not the mean), default 0.02 (= 5.1/255); 0 is strict mode (exactly black)"})),
        ("min_run", json!({"type": "integer", "minimum": 1, "maximum": 6000, "description": "frames in a reported run at least, default 2"})),
    ], &[]), run }] }
