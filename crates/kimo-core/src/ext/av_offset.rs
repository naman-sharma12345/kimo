//! v6 eyes: av_offset. How far the sound sits from the picture: audio onsets (positive jumps of short-time RMS, sample
//! accurate on a 1 ms grid) cross-correlated with the picture's frame-to-frame change (mean absolute luma difference of
//! consecutive frames, an event at the first frame that changed), both on a 1 ms grid. The curve is the Pearson
//! correlation of the two for every lag in +-max_lag_ms; the best lag is where it peaks. Positive lag = the sound comes
//! after the picture (audio late). Raw numbers, no verdict.
use super::kit::*;
use crate::perception::{analysis::{self, Request}, audio_targets};

const MAX_FRAMES: usize = 6000;
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a number in [{lo}, {hi}]")) }
}
fn int(a: &Value, k: &str, d: u64, lo: u64, hi: u64) -> Result<u64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_u64().filter(|x| *x >= lo && *x <= hi).ok_or(format!("out_of_range: {k} must be a whole number {lo}..{hi}")) }
}
/// Picture change per frame: mean |Y_f - Y_{f-1}| (Rec.709 weights on the output values 0..1); v[0] = 0 (no previous frame).
pub fn picture_change(frames: &[Vec<f64>]) -> Vec<f64> {
    let mut v = vec![0.0; frames.len()];
    for f in 1..frames.len() { let (a, b) = (&frames[f - 1], &frames[f]); v[f] = a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len().max(1) as f64; }
    v
}
/// Audio onset strength on a 1 ms grid: o[k] = max(0, RMS[k, k+w) - RMS[k-w, k)) at 48 kHz, k in ms from the first sample.
pub fn onset_envelope(mono: &[f32], window_ms: usize) -> Vec<f64> {
    let w = window_ms * 48; let n_ms = mono.len() / 48;
    let mut cs = vec![0f64; mono.len() + 1];
    for (i, x) in mono.iter().enumerate() { cs[i + 1] = cs[i] + (*x as f64) * (*x as f64); if i % 65536 == 0 { crate::guard::check(); } }
    let rms = |a: usize, b: usize| ((cs[b] - cs[a]) / (b - a) as f64).sqrt();
    (0..n_ms).map(|k| { let s = k * 48; if s < w || s + w > mono.len() { 0.0 } else { (rms(s, s + w) - rms(s - w, s)).max(0.0) } }).collect()
}
/// Pearson correlation on the 1 ms grid between the picture-change train (frame f's change v[f] placed at base_ms[f],
/// 0 at every other ms of [base_ms[1], base_ms[last]]) and the onset envelope shifted by `lag` (o[k + lag]), for every lag
/// in -max_lag..=max_lag; ms where o[k + lag] falls outside the envelope are left out. None where either side has no
/// variance. O(frames) per lag: the envelope side comes from prefix sums, the train is sparse.
pub fn corr_curve(v: &[f64], base_ms: &[i64], o: &[f64], max_lag: i64) -> Vec<(i64, Option<f64>)> {
    let mut p1 = vec![0f64; o.len() + 1]; let mut p2 = vec![0f64; o.len() + 1];
    for (i, x) in o.iter().enumerate() { p1[i + 1] = p1[i] + x; p2[i + 1] = p2[i] + x * x; }
    let (k0, k1) = if v.len() >= 2 { (base_ms[1], base_ms[v.len() - 1]) } else { (0, -1) };
    (-max_lag..=max_lag).map(|lag| {
        let lo = k0.max(-lag); let hi = k1.min(o.len() as i64 - 1 - lag);
        if hi - lo + 1 < 3 { return (lag, None); }
        let n = (hi - lo + 1) as f64;
        let (mut sx, mut sxx, mut sxy) = (0.0, 0.0, 0.0);
        for f in 1..v.len() { let k = base_ms[f]; if k < lo || k > hi { continue; } sx += v[f]; sxx += v[f] * v[f]; sxy += v[f] * o[(k + lag) as usize]; }
        let (a, b) = ((lo + lag) as usize, (hi + lag) as usize + 1);
        let (sy, syy) = (p1[b] - p1[a], p2[b] - p2[a]);
        let (vx, vy) = (sxx - sx * sx / n, syy - sy * sy / n);
        if !(vx > 1e-12 * sxx) || !(vy > 1e-12 * syy) { return (lag, None); }
        (lag, Some(((sxy - sx * sy / n) / (vx * vy).sqrt()).clamp(-1.0, 1.0)))
    }).collect()
}
/// One lag of corr_curve.
pub fn corr_at(v: &[f64], base_ms: &[i64], o: &[f64], lag: i64) -> Option<f64> {
    corr_curve(v, base_ms, o, lag.abs()).into_iter().find(|(l, _)| *l == lag).and_then(|(_, r)| r)
}
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = match a.get("source") { None | Some(Value::Null) => "timeline".to_string(), Some(v) => v.as_str().ok_or("invalid_source: source must be timeline or clip:<id>")?.to_string() };
    let (parsed, mut bounds) = if source == "timeline" { ("composite".to_string(), (0.0, ss.project.duration())) } else if let Some(id) = source.strip_prefix("clip:") {
        let c = ss.project.find_clip(id).ok_or("invalid_source: missing clip")?; (source.clone(), (c.start, c.start + c.duration)) } else { return Err("invalid_source: Valid: timeline, clip:<id>".into()) };
    let audio = match a.get("audio") { None | Some(Value::Null) => match source.strip_prefix("clip:") { Some(id) => json!(id), None => json!("mix") }, Some(v) => v.clone() };
    let fps = ss.project.fps; if !(fps > 0.0 && fps <= 1000.0) { return Err("out_of_range: project fps must be in (0, 1000] (one frame per ms of the grid at most)".into()); }
    let st = num(a, "start", bounds.0, 0.0, 1e7)?; let en = num(a, "end", bounds.1, 0.0, 1e7)?;
    if st < bounds.0 - 1e-9 || en > bounds.1 + 1e-9 || en <= st { return Err(format!("out_of_range: start/end must lie inside the source span [{}, {}] with start < end", bounds.0, bounds.1)); }
    bounds = (st, en);
    let max_lag = int(a, "max_lag_ms", 500, 1, 2000)? as i64;
    let curve_step = int(a, "curve_step_ms", 10, 1, 100)? as i64;
    let window_ms = int(a, "onset_window_ms", 10, 2, 100)? as usize;
    let scale = int(a, "scale", 160, 16, 1920)? as f64;
    // frames on the project grid inside [start, end)
    let f0 = (bounds.0 * fps - 1e-9).ceil().max(0.0) as i64; let f1 = (bounds.1 * fps - 1e-9).ceil() as i64;
    let nf = (f1 - f0).max(0) as usize;
    if nf < 4 { return Err("out_of_range: the range must hold at least 4 frames".into()); }
    if nf > MAX_FRAMES { return Err(format!("out_of_range: the range holds {nf} frames; at most {MAX_FRAMES} (shorten start/end)")); }
    let mut lumas = Vec::with_capacity(nf); let mut times = Vec::with_capacity(nf);
    for i in 0..nf {
        crate::guard::check();
        let t = (f0 + i as i64) as f64 / fps; times.push(t);
        let im = analysis::render(ss, &Request { time: t, source: parsed.clone() })?;
        let ratio = (scale / im.w.max(im.h) as f64).min(1.0);
        let (w, h) = (((im.w as f64 * ratio).round() as usize).max(1), ((im.h as f64 * ratio).round() as usize).max(1));
        let im = if w != im.w || h != im.h { im.downscale(w, h) } else { im };
        lumas.push((0..im.w * im.h).map(|p| 0.2126 * im.rgb[0][p] + 0.7152 * im.rgb[1][p] + 0.0722 * im.rgb[2][p]).collect::<Vec<f64>>());
    }
    let v = picture_change(&lumas); drop(lumas);
    let dec = audio_targets::decode(ss, &audio)?;
    if dec.pcm.is_empty() { return Err("no_audio: the audio target is empty".into()); }
    let mono: Vec<f32> = dec.pcm.chunks_exact(2).map(|x| (x[0] + x[1]) * 0.5).collect();
    let o = onset_envelope(&mono, window_ms);
    // base_ms[f]: frame f's time on the audio's 1 ms grid (audio sample 0 = dec.origin)
    let base: Vec<i64> = times.iter().map(|t| ((t - dec.origin) * 1000.0).round() as i64).collect();
    let all = corr_curve(&v, &base, &o, max_lag); let mut best: Option<(i64, f64)> = None;
    for (lag, r) in &all { if let Some(r) = *r { if best.map_or(true, |(bl, br)| r > br + 1e-12 || ((r - br).abs() <= 1e-12 && lag.abs() < bl.abs())) { best = Some((*lag, r)); } } }
    let frame_ms = 1000.0 / fps;
    let runner = best.and_then(|(bl, _)| all.iter().filter(|(l, r)| r.is_some() && ((l - bl).abs() as f64) > frame_ms).map(|(l, r)| (*l, r.unwrap())).fold(None, |acc: Option<(i64, f64)>, x| if acc.map_or(true, |a| x.1 > a.1 + 1e-12) { Some(x) } else { acc }));
    let r0 = all.iter().find(|(l, _)| *l == 0).and_then(|(_, r)| *r);
    let curve: Vec<Value> = all.iter().filter(|(l, _)| (l + max_lag) % curve_step == 0).map(|(l, r)| json!([l, r.map(r6)])).collect();
    let vmax = v.iter().cloned().fold(0.0, f64::max); let omax = o.iter().cloned().fold(0.0, f64::max);
    let mut top: Vec<usize> = (1..v.len()).collect(); top.sort_by(|p, q| v[*q].partial_cmp(&v[*p]).unwrap().then(p.cmp(q)));
    Ok(json!({
        "source": source, "audio": audio, "start": bounds.0, "end": bounds.1, "fps": fps, "frames_sampled": nf, "frame_ms": r6(frame_ms),
        "audio_origin": dec.origin, "audio_ms": o.len(), "max_lag_ms": max_lag, "onset_window_ms": window_ms, "scale": scale,
        "best_lag_ms": best.map(|b| b.0), "best_r": best.map(|b| r6(b.1)), "r_at_zero": r0.map(r6),
        "runner_up_lag_ms": runner.map(|b| b.0), "runner_up_r": runner.map(|b| r6(b.1)), "peak_margin": match (best, runner) { (Some(b), Some(u)) => json!(r6(b.1 - u.1)), _ => Value::Null },
        "sign": "positive lag = the sound comes after the picture (audio late)",
        "picture_change_max": r6(vmax), "onset_max": r6(omax),
        "top_picture_changes": top.iter().take(8).map(|f| json!({"t": r6(times[*f]), "change": r6(v[*f])})).collect::<Vec<_>>(),
        "curve_step_ms": curve_step, "curve": curve,
    }))
}

pub fn commands() -> Vec<Command> { vec![Command { name: "av_offset", mutates: false,
    description: "Measure how far the sound sits from the picture: audio onsets cross-correlated with picture change per frame, as a lag curve and best lag in ms. source = timeline (default) or clip:<id>; audio = mix (default for the timeline), a clip id (default for clip:<id>) or {clip_id|track_id}; start / end in timeline seconds (default the source span, 4..6000 frames). Picture change per frame = mean absolute Rec.709 luma difference from the previous frame on a copy scaled to scale px on the long side (default 160); audio onset = positive jump of RMS over onset_window_ms (default 10) on a 1 ms grid. For every lag in +-max_lag_ms (default 500, up to 2000) the Pearson correlation of the two on the 1 ms grid (each frame's change at its own ms); best_lag_ms / best_r at the peak (positive lag = the sound comes after the picture, audio late), r_at_zero, runner_up_lag_ms / runner_up_r (best peak more than a frame away) and peak_margin; curve every curve_step_ms (default 10). Raw numbers only, no verdict; bad arguments are errors, never clamped.",
    schema: obj(vec![
        ("source", s("timeline (default) or clip:<id>")),
        ("audio", json!({"description": "mix, a clip id, or {clip_id} / {track_id}; default mix (timeline) or the clip itself"})),
        ("start", n("range start, timeline seconds")),
        ("end", n("range end, timeline seconds")),
        ("max_lag_ms", json!({"type": "integer", "minimum": 1, "maximum": 2000, "description": "lags searched both ways, default 500"})),
        ("curve_step_ms", json!({"type": "integer", "minimum": 1, "maximum": 100, "description": "curve sample spacing, default 10"})),
        ("onset_window_ms", json!({"type": "integer", "minimum": 2, "maximum": 100, "description": "RMS window either side of an onset, default 10"})),
        ("scale", json!({"type": "integer", "minimum": 16, "maximum": 1920, "description": "picture analysis size, long side px, default 160"})),
    ], &[]), run }] }
