//! v6 eyes: focus_series. Sharpness per sampled frame, raw numbers only. Every `step`-th project frame in [start, end) is
//! rendered at full resolution through the engine (the same render path as black_frames and frozen_frames) and its Rec.709
//! luma Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' is read on the 0..255 code scale. Two scores per frame, over interior pixels only
//! (the 1-pixel frame border has no full 3x3 neighbourhood and is skipped):
//! - `lap_var`: population variance of the 4-neighbour Laplacian Y'(x-1,y) + Y'(x+1,y) + Y'(x,y-1) + Y'(x,y+1) - 4 Y'(x,y),
//!   in (code units)^2;
//! - `tenengrad`: mean Sobel gradient magnitude sqrt(Gx^2 + Gy^2) (3x3 Sobel, 1-2-1 weights), in code units.
//! Both for the whole frame and for up to 16 `rects` [x, y, w, h] (frame pixels; a rect counts only its pixels that are interior
//! to the frame, and a rect with none reads null, never 0). A flat frame reads exactly 0 on both. No verdict: the only line is
//! the optional `soft_max` (a lap_var value); when given, runs of at least `min_run` sampled frames with lap_var <= soft_max are
//! listed. Definitions and predictions were committed before this file existed: tests/data/focus_series/PREREG.md.
use super::kit::*;
use crate::perception::analysis::{self, Image, Request};

const MAX_FRAMES: usize = 6000;
/// At most this many sampled frames per call (raise `step` or shorten start/end for more).
pub const MAX_SAMPLES: usize = 1200;
pub const MAX_RECTS: usize = 16;
pub const MAX_RUNS: usize = 200;
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a number in [{lo}, {hi}]")) }
}
fn int(a: &Value, k: &str, d: u64, lo: u64, hi: u64) -> Result<u64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_u64().filter(|x| *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a whole number {lo}..{hi}")) }
}

/// Sharpness of one region: (lap_var, tenengrad, interior pixel count); None when the region holds no interior pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score { pub lap_var: f64, pub tenengrad: f64, pub px: usize }

/// Pixel box [x0, x1) x [y0, y1) in frame pixels, before clipping to the interior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PxRect { pub x0: i64, pub y0: i64, pub x1: i64, pub y1: i64 }
impl PxRect {
    /// From [x, y, w, h]: x and y are floored, the far edge is floor(x + w) / floor(y + h) (exclusive).
    pub fn from_xywh(r: [f64; 4]) -> Self { PxRect { x0: r[0].floor() as i64, y0: r[1].floor() as i64, x1: (r[0] + r[2]).floor() as i64, y1: (r[1] + r[3]).floor() as i64 } }
}

/// Scores of a luma plane `y` (w x h, 0..255 codes) for the whole frame (None rect) or one rect.
pub fn score_plane(y: &[f64], w: usize, h: usize, rect: Option<PxRect>) -> Option<Score> {
    if w < 3 || h < 3 { return None; }
    let (mut x0, mut y0, mut x1, mut y1) = (1i64, 1i64, w as i64 - 1, h as i64 - 1);
    if let Some(r) = rect { x0 = x0.max(r.x0); y0 = y0.max(r.y0); x1 = x1.min(r.x1); y1 = y1.min(r.y1); }
    if x1 <= x0 || y1 <= y0 { return None; }
    let (mut sl, mut sl2, mut sg) = (0f64, 0f64, 0f64); let mut cnt = 0usize;
    for yy in y0 as usize..y1 as usize { for xx in x0 as usize..x1 as usize {
        let p = |dx: isize, dy: isize| y[((yy as isize + dy) as usize) * w + (xx as isize + dx) as usize];
        let c = p(0, 0);
        let l = p(-1, 0) + p(1, 0) + p(0, -1) + p(0, 1) - 4.0 * c;
        let gx = (p(1, -1) + 2.0 * p(1, 0) + p(1, 1)) - (p(-1, -1) + 2.0 * p(-1, 0) + p(-1, 1));
        let gy = (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1)) - (p(-1, -1) + 2.0 * p(0, -1) + p(1, -1));
        sl += l; sl2 += l * l; sg += (gx * gx + gy * gy).sqrt(); cnt += 1;
    } }
    let nn = cnt as f64; let mean = sl / nn;
    Some(Score { lap_var: (sl2 / nn - mean * mean).max(0.0), tenengrad: sg / nn, px: cnt })
}
/// The Rec.709 luma plane of a rendered frame on the 0..255 code scale.
pub fn luma_plane(im: &Image) -> Vec<f64> { (0..im.w * im.h).map(|i| im.luma(i) * 255.0).collect() }

/// Stretches (start, end) of at least `min_run` consecutive true flags.
fn stretches(flags: &[bool], min_run: usize) -> Vec<(usize, usize)> {
    let mut out = vec![]; let mut st: Option<usize> = None;
    for (i, f) in flags.iter().chain(std::iter::once(&false)).enumerate() { match (st, *f) { (None, true) => st = Some(i), (Some(a), false) => { if i - a >= min_run { out.push((a, i - 1)); } st = None; } _ => {} } }
    out
}
fn median(v: &mut Vec<f64>) -> Option<f64> { if v.is_empty() { return None; } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); let m = v.len() / 2; Some(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2.0 }) }
/// Index of the largest (or smallest) value; ties go to the earliest index. None values are skipped.
fn pick(v: &[Option<f64>], largest: bool) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, x) in v.iter().enumerate() { if let Some(x) = x { match best { None => best = Some((i, *x)), Some((_, b)) => if (largest && *x > b) || (!largest && *x < b) { best = Some((i, *x)) } } } }
    best.map(|b| b.0)
}
fn sc(o: Option<Score>) -> Value { match o { Some(s) => json!({"lap_var": r6(s.lap_var), "tenengrad": r6(s.tenengrad)}), None => json!({"lap_var": null, "tenengrad": null}) } }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = match a.get("source") { None | Some(Value::Null) => "timeline".to_string(), Some(v) => v.as_str().ok_or("invalid_source: source must be timeline or clip:<id>")?.to_string() };
    let (parsed, mut bounds) = if source == "timeline" { ("composite".to_string(), (0.0, ss.project.duration())) } else if let Some(id) = source.strip_prefix("clip:") {
        let c = ss.project.find_clip(id).ok_or("invalid_source: missing clip")?; (source.clone(), (c.start, c.start + c.duration)) } else { return Err("invalid_source: Valid: timeline, clip:<id>".into()) };
    let fps = ss.project.fps; if !(fps > 0.0 && fps <= 1000.0) { return Err("out_of_range: project fps must be in (0, 1000]".into()); }
    let st = num(a, "start", bounds.0, 0.0, 1e7)?; let en = num(a, "end", bounds.1, 0.0, 1e7)?;
    if st < bounds.0 - 1e-9 || en > bounds.1 + 1e-9 || en <= st { return Err(format!("out_of_range: start/end must lie inside the source span [{}, {}] with start < end", bounds.0, bounds.1)); }
    bounds = (st, en);
    let step = int(a, "step", 1, 1, MAX_FRAMES as u64)? as usize;
    let min_run = int(a, "min_run", 2, 1, MAX_SAMPLES as u64)? as usize;
    let soft_max = match a.get("soft_max") { None | Some(Value::Null) => None, Some(_) => Some(num(a, "soft_max", 0.0, 0.0, 1e12)?) };
    let mut rects: Vec<[f64; 4]> = vec![];
    match a.get("rects") { None | Some(Value::Null) => {}, Some(Value::Array(rs)) => {
        if rs.len() > MAX_RECTS { return Err(format!("out_of_range: at most {MAX_RECTS} rects")); }
        for r in rs { let v: Vec<f64> = r.as_array().map(|x| x.iter().filter_map(|e| e.as_f64()).filter(|e| e.is_finite()).collect()).unwrap_or_default();
            if v.len() != 4 || r.as_array().map_or(0, |x| x.len()) != 4 || v[2] <= 0.0 || v[3] <= 0.0 { return Err("invalid_rect: each rect is [x, y, w, h] in frame pixels with w > 0 and h > 0".into()); }
            rects.push([v[0], v[1], v[2], v[3]]); } }
        Some(_) => return Err("invalid_rect: rects must be a list of [x, y, w, h]".into()) }
    let pr: Vec<PxRect> = rects.iter().map(|r| PxRect::from_xywh(*r)).collect();
    let f0 = (bounds.0 * fps - 1e-9).ceil().max(0.0) as i64; let f1 = (bounds.1 * fps - 1e-9).ceil() as i64;
    let nf = (f1 - f0).max(0) as usize;
    if nf < 1 { return Err("out_of_range: the range holds no project frame".into()); }
    if nf > MAX_FRAMES { return Err(format!("out_of_range: the range holds {nf} frames; at most {MAX_FRAMES} (shorten start/end)")); }
    let idx: Vec<usize> = (0..nf).step_by(step).collect();
    if idx.len() > MAX_SAMPLES { return Err(format!("out_of_range: {} sampled frames; at most {MAX_SAMPLES} (raise step or shorten start/end)", idx.len())); }
    let mut dims = (0usize, 0usize);
    let mut whole: Vec<Option<Score>> = vec![]; let mut per: Vec<Vec<Option<Score>>> = vec![vec![]; pr.len()];
    for &i in &idx {
        crate::guard::check();
        let t = (f0 + i as i64) as f64 / fps;
        let im = analysis::render(ss, &Request { time: t, source: parsed.clone() })?;
        dims = (im.w, im.h); let y = luma_plane(&im);
        whole.push(score_plane(&y, im.w, im.h, None));
        for (k, r) in pr.iter().enumerate() { per[k].push(score_plane(&y, im.w, im.h, Some(*r))); }
    }
    let frame = |j: usize| f0 + idx[j] as i64; let tf = |j: usize| r6(frame(j) as f64 / fps);
    let series: Vec<Value> = (0..idx.len()).map(|j| { let mut v = sc(whole[j]); v["frame"] = json!(frame(j)); v["t"] = json!(tf(j));
        if !pr.is_empty() { v["rects"] = Value::Array(per.iter().map(|p| sc(p[j])).collect()); } v }).collect();
    let lv: Vec<Option<f64>> = whole.iter().map(|s| s.map(|s| s.lap_var)).collect();
    let tg: Vec<Option<f64>> = whole.iter().map(|s| s.map(|s| s.tenengrad)).collect();
    let at = |j: Option<usize>, w: &Vec<Option<Score>>| match j { Some(j) => { let mut v = sc(w[j]); v["frame"] = json!(frame(j)); v["t"] = json!(tf(j)); v } None => Value::Null };
    let stats = |v: &[Option<f64>]| { let mut x: Vec<f64> = v.iter().flatten().copied().collect(); if x.is_empty() { return Value::Null; }
        let (lo, hi) = x.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), y| (a.min(*y), b.max(*y))); json!({"min": r6(lo), "median": median(&mut x).map(r6), "max": r6(hi)}) };
    let rect_list: Vec<Value> = (0..pr.len()).map(|k| { let l: Vec<Option<f64>> = per[k].iter().map(|s| s.map(|s| s.lap_var)).collect();
        json!({"rect": rects[k], "interior_px": per[k].iter().flatten().next().map(|s| s.px).unwrap_or(0), "sharpest": at(pick(&l, true), &per[k]), "softest": at(pick(&l, false), &per[k]), "lap_var": stats(&l)}) }).collect();
    let (runs, run_count) = match soft_max { None => (Value::Null, Value::Null), Some(m) => {
        let flags: Vec<bool> = lv.iter().map(|x| x.map_or(false, |x| r6(x) <= m)).collect(); let found = stretches(&flags, min_run);
        let list: Vec<Value> = found.iter().take(MAX_RUNS).map(|(s0, e0)| json!({"start_frame": frame(*s0), "end_frame": frame(*e0), "samples": e0 - s0 + 1,
            "max_lap_var": r6((*s0..=*e0).filter_map(|j| lv[j]).fold(0.0, f64::max)), "start_s": tf(*s0), "end_s": r6((frame(*e0) + 1) as f64 / fps)})).collect();
        (Value::Array(list), json!(found.len())) } };
    Ok(json!({
        "source": source, "start": bounds.0, "end": bounds.1, "fps": fps, "first_frame": f0, "last_frame": f0 + *idx.last().unwrap() as i64,
        "frames": idx.len(), "measured": [dims.0, dims.1], "units": {"lap_var": "(0..255 code units)^2", "tenengrad": "0..255 code units"},
        "params": {"step": step, "soft_max": soft_max, "min_run": min_run, "rects": rects},
        "series": series, "sharpest": at(pick(&lv, true), &whole), "softest": at(pick(&lv, false), &whole),
        "lap_var": stats(&lv), "tenengrad": stats(&tg), "rects": rect_list,
        "soft_runs": runs, "soft_run_count": run_count,
    }))
}

pub fn commands() -> Vec<Command> { vec![Command { name: "focus_series", mutates: false,
    description: "Measure sharpness per frame: Laplacian variance and mean Sobel gradient of luma, for the whole frame and up to 16 rects. Every step-th project frame in start..end of source (timeline or clip:<id>) is rendered at full resolution; on its Rec.709 luma (0..255 code scale, interior pixels only) lap_var is the population variance of the 4-neighbour Laplacian, in code units squared, and tenengrad the mean 3x3 Sobel gradient magnitude, in code units. A flat frame reads exactly 0 on both; higher means more fine detail or harder edges, so compare frames of the same shot, not different shots. Returns the series (frame, t, both scores, and per rect when rects are given), the sharpest and softest sampled frame by lap_var (ties go to the earliest), and min, median and max of each score. Raw numbers, no blurry or sharp verdict: the only line is the optional soft_max (a lap_var value), which lists runs of at least min_run sampled frames at or under it. At most 1200 sampled frames per call.",
    schema: obj(vec![
        ("source", s("timeline (default) or clip:<id>")),
        ("start", n("range start, timeline seconds")),
        ("end", n("range end, timeline seconds")),
        ("step", json!({"type": "integer", "minimum": 1, "maximum": 6000, "description": "read every step-th project frame, default 1 (every frame)"})),
        ("rects", json!({"type": "array", "maxItems": 16, "items": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4}, "description": "up to 16 regions [x, y, w, h] in frame pixels, each scored on its own; a rect with no interior pixel reads null"})),
        ("soft_max", json!({"type": "number", "minimum": 0, "description": "optional lap_var line (code units squared): list runs of sampled frames at or under it; not given = no runs"})),
        ("min_run", json!({"type": "integer", "minimum": 1, "maximum": 1200, "description": "sampled frames in a listed soft run at least, default 2"})),
    ], &[]), run }] }
