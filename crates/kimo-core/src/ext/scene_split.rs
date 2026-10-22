//! scene_split (Premiere "Scene Edit Detection"): split one clip at the content cuts detect_cuts finds inside it, so a clip that
//! holds several shots becomes one timeline clip per shot. Each cut is a frame-snapped split_clip, so every shot keeps the
//! clip's effects, grade, masks and transform; the first shot keeps the original id. min_shot_s drops cuts that would leave a
//! shot shorter than that; dry_run reports the cuts and changes nothing. Detection options pass through to detect_cuts.
use super::kit::*;

fn fnum(a: &Value, k: &str) -> Result<Option<f64>, String> {
    match a.get(k).filter(|v| !v.is_null()) { None => Ok(None), Some(v) => { let f = v.as_f64().ok_or_else(|| format!("{k} must be a number"))?; if !f.is_finite() { return Err(format!("{k} must be finite")); } Ok(Some(f)) } }
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "clip_id")?.to_string();
    let (start, dur) = { let c = ss.project.find_clip(&id).ok_or_else(|| format!("no clip '{id}'"))?; (c.start, c.duration) };
    let sens = fnum(a, "sensitivity")?; if let Some(x) = sens { if !(0.0..=1.0).contains(&x) { return Err("sensitivity must be 0..1".into()); } }
    let thr = fnum(a, "threshold")?; if let Some(x) = thr { if !(0.0..=1.0).contains(&x) { return Err("threshold must be 0..1".into()); } }
    let min_shot = fnum(a, "min_shot_s")?.unwrap_or(0.0); if min_shot < 0.0 { return Err("min_shot_s must be >= 0".into()); }
    let dry = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut q = json!({"source": format!("clip:{id}")});
    if let Some(x) = sens { q["sensitivity"] = json!(x); } if let Some(x) = thr { q["threshold"] = json!(x); }
    for k in ["method", "fps", "min_gap_s"] { if let Some(v) = a.get(k).filter(|v| !v.is_null()) { q[k] = v.clone(); } }
    let det = crate::commands::dispatch_nested(ss, "detect_cuts", &q)?;
    let fps = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 };
    let snap = |t: f64| (t * fps).round() / fps;
    let mut times: Vec<f64> = det.get("cuts").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|c| c.get("time").and_then(|t| t.as_f64())).collect()).unwrap_or_default();
    times = times.into_iter().map(snap).filter(|t| *t > start + 0.5 / fps && *t < start + dur - 0.5 / fps).collect();
    times.sort_by(|x, y| x.total_cmp(y)); times.dedup_by(|b, a| (*b - *a).abs() < 1e-9);
    // keep a cut only when both the shot before it and the shot after it are at least min_shot_s long
    let end = start + dur; let mut kept: Vec<f64> = vec![];
    for t in times { let prev = kept.last().copied().unwrap_or(start); if t - prev + 1e-9 >= min_shot { kept.push(t); } }
    while let Some(&l) = kept.last() { if end - l + 1e-9 < min_shot { kept.pop(); } else { break; } }
    if dry { return Ok(json!({"clip_id": id, "cuts": kept, "detected": det.get("count").cloned().unwrap_or(Value::Null), "dry_run": true})); }
    let mut clips = vec![json!({"clip_id": id, "start": start})]; let mut cur = id.clone();
    for t in &kept {
        let r = crate::commands::dispatch_nested(ss, "split_clip", &json!({"clip_id": cur, "at": t}))?;
        let nid = r.get("right_clip_id").and_then(|v| v.as_str()).map(String::from).ok_or_else(|| format!("split_clip returned no clip id: {r}"))?;
        clips.push(json!({"clip_id": nid, "start": t})); cur = nid;
    }
    for i in 0..clips.len() { let s0 = clips[i]["start"].as_f64().unwrap(); let s1 = clips.get(i + 1).and_then(|c| c["start"].as_f64()).unwrap_or(end); clips[i]["duration"] = json!(s1 - s0); }
    Ok(json!({"clip_id": id, "cuts": kept, "clips": clips, "detected": det.get("count").cloned().unwrap_or(Value::Null)}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "scene_split", description: "Split a clip into one clip per shot at the scene cuts detect_cuts finds in it (Premiere Scene Edit Detection).", mutates: true,
        schema: obj(vec![("clip_id", s("clip holding several shots")), ("sensitivity", n("0..1 default .5 (detect_cuts)")), ("threshold", n("0..1 explicit cut threshold")), ("method", json!({"type":"string","enum":["histogram","luma","combined"]})), ("fps", n("analysis fps")), ("min_gap_s", n("minimum gap between detected cuts")), ("min_shot_s", n("drop cuts that leave a shot shorter than this, default 0")), ("dry_run", json!({"type":"boolean","description":"report the cuts, change nothing"}))], &["clip_id"]),
        run },
] }
