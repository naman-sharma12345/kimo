//! v7 additions: mark_scene_cuts -- automates the existing two-call workflow (detect_cuts, then add_marker
//! once per cut by hand) into one tool that drops a timeline marker at every detected scene cut. Not a
//! duplicate of scene_split (which SPLITS a clip into separate timeline clips) or detect_cuts (which only
//! reports scores, never touches the project) -- this is the "I just want markers on my scene changes"
//! convenience neither of those gives you. Shares detect_cuts' detection options and scene_split's
//! fps-snap/range-filter technique (ext/scene_split.rs).
use super::kit::*;

fn fnum(a: &Value, k: &str) -> Result<Option<f64>, String> {
    match a.get(k).filter(|v| !v.is_null()) { None => Ok(None), Some(v) => { let f = v.as_f64().ok_or_else(|| format!("{k} must be a number"))?; if !f.is_finite() { return Err(format!("{k} must be finite")); } Ok(Some(f)) } }
}

fn mark_scene_cuts(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = a.get("source").and_then(|v| v.as_str()).unwrap_or("timeline").to_string();
    let sens = fnum(a, "sensitivity")?; if let Some(x) = sens { if !(0.0..=1.0).contains(&x) { return Err("sensitivity must be 0..1".into()); } }
    let thr = fnum(a, "threshold")?; if let Some(x) = thr { if !(0.0..=1.0).contains(&x) { return Err("threshold must be 0..1".into()); } }
    let dry = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let skip_existing = a.get("skip_existing").and_then(|v| v.as_bool()).unwrap_or(true);
    let prefix = a.get("name_prefix").and_then(|v| v.as_str()).unwrap_or("cut").to_string();
    let color = a.get("color").and_then(|v| v.as_str()).unwrap_or("red").to_string();
    let near_s = { let f = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 }; 2.0 / f }; // "already has a marker here" tolerance: 2 frames

    let mut q = json!({"source": source});
    if let Some(x) = sens { q["sensitivity"] = json!(x); } if let Some(x) = thr { q["threshold"] = json!(x); }
    for k in ["method", "fps", "min_gap_s"] { if let Some(v) = a.get(k).filter(|v| !v.is_null()) { q[k] = v.clone(); } }
    let det = crate::commands::dispatch_nested(ss, "detect_cuts", &q)?;
    let fps = if ss.project.fps > 0.0 { ss.project.fps } else { 30.0 };
    let snap = |t: f64| (t * fps).round() / fps;
    let mut times: Vec<f64> = det.get("cuts").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|c| c.get("time").and_then(|t| t.as_f64())).collect()).unwrap_or_default();
    times = times.into_iter().map(snap).collect(); times.sort_by(|x, y| x.total_cmp(y)); times.dedup_by(|b, a| (*b - *a).abs() < 1e-9);

    let existing: Vec<f64> = ss.project.markers.iter().map(|m| m.time).collect();
    let mut to_place = vec![]; let mut skipped = 0usize;
    for t in &times { if skip_existing && existing.iter().any(|e| (e - t).abs() <= near_s) { skipped += 1; } else { to_place.push(*t); } }

    if dry { return Ok(json!({"cuts": times, "would_place": to_place.len(), "would_skip_existing": skipped, "detected": det.get("count").cloned().unwrap_or(Value::Null), "dry_run": true, "no_history": true})); }
    if to_place.is_empty() { return Ok(json!({"cuts": times, "created": Value::Array(vec![]), "skipped_existing": skipped, "detected": det.get("count").cloned().unwrap_or(Value::Null), "no_history": true})); }

    let mut created = vec![];
    for (i, t) in to_place.iter().enumerate() {
        let r = crate::commands::dispatch_nested(ss, "add_marker", &json!({"time": t, "name": format!("{prefix} {}", i + 1), "color": color}))?;
        created.push(r.get("marker_id").cloned().unwrap_or(Value::Null));
    }
    Ok(json!({"cuts": times, "created": created, "skipped_existing": skipped, "detected": det.get("count").cloned().unwrap_or(Value::Null)}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "mark_scene_cuts", description: "Drop a timeline marker at every scene cut detect_cuts finds, in one call -- automates the detect_cuts-then-add_marker-per-cut workflow. Not scene_split (which splits a clip into separate timeline clips) and not a wrapper that edits clips in any way; only markers are added. source (default \"timeline\"): the composite, clip:<id> or file:<path>, exactly like detect_cuts -- times are the timeline seconds add_marker expects, so clip:<id>/timeline sources are the meaningful ones for markers. sensitivity, threshold, method, fps and min_gap_s pass straight through to detect_cuts. Markers are named \"<name_prefix> N\" (default prefix \"cut\") in detection order, with `color` (default red). skip_existing (default true) will not add a new marker within 2 frames of one that is already there, so calling this twice does not pile up duplicate markers; skipped_existing counts them. dry_run reports cuts/would_place/would_skip_existing without creating anything.", mutates: true,
        schema: obj(vec![("source", s("timeline (default), clip:<id> or file:<path>, as detect_cuts")), ("sensitivity", n("0..1 default .5 (detect_cuts)")), ("threshold", n("0..1 explicit cut threshold")), ("method", json!({"type":"string","enum":["histogram","luma","combined"]})), ("fps", n("analysis fps")), ("min_gap_s", n("minimum gap between detected cuts")), ("name_prefix", s("marker name prefix, default \"cut\"")), ("color", s("marker color name, default \"red\"")), ("skip_existing", json!({"type":"boolean","description":"default true: skip a cut within 2 frames of an existing marker"})), ("dry_run", json!({"type":"boolean","description":"report the cuts, create nothing"}))], &[]), run: mark_scene_cuts },
] }
