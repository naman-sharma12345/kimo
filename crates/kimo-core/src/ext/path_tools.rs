//! Bezier path shapes over MCP: add_path_shape, set_path, path_info. Path = SVG path syntax or points (optionally smoothed).
use super::kit::*;
use crate::model::{Effect, Source};
use crate::paths::{smooth_d, Path};
use std::collections::BTreeMap;

fn resolve_d(a: &Value) -> Result<String, String> {
    if let Some(d) = a.get("d").and_then(|v| v.as_str()) { if a.get("points").is_some() { return Err("give either 'd' or 'points', not both".into()); } return Ok(d.to_string()); }
    let pts = a.get("points").and_then(|v| v.as_array()).ok_or("give 'd' (SVG path) or 'points' ([[x,y],...] pixels from canvas center)")?;
    let pts: Vec<[f64; 2]> = pts.iter().map(|p| { let p = p.as_array().filter(|p| p.len() == 2)?; let (x, y) = (p[0].as_f64()?, p[1].as_f64()?); if x.is_finite() && y.is_finite() { Some([x, y]) } else { None } }).collect::<Option<_>>().ok_or("points must be [[x,y],...] finite numbers")?;
    if pts.len() < 2 { return Err("need at least 2 points".into()); } if pts.len() > 500 { return Err("max 500 points".into()); }
    let closed = a.get("closed").and_then(|v| v.as_bool()).unwrap_or(false);
    if a.get("smooth").and_then(|v| v.as_bool()).unwrap_or(false) { let t = opt_num(a, "tension", 0.5); if !(0.0..=1.0).contains(&t) { return Err("tension must be 0..1".into()); } Ok(smooth_d(&pts, closed, t)) }
    else { let mut s = format!("M{} {}", pts[0][0], pts[0][1]); for p in &pts[1..] { s += &format!(" L{} {}", p[0], p[1]); } if closed { s += " Z"; } Ok(s) }
}
fn rule(a: &Value) -> Result<bool, String> { match a.get("fill_rule").and_then(|v| v.as_str()).unwrap_or("nonzero") { "nonzero" => Ok(false), "evenodd" => Ok(true), o => Err(format!("fill_rule must be nonzero|evenodd, got '{o}'")) } }
fn info(p: &Path, d: &str) -> Value { json!({"d": d, "length_px": p.total, "segments": p.segs.len(), "subpaths": p.subpaths, "bbox": {"x0": p.bbox[0], "y0": p.bbox[1], "x1": p.bbox[2], "y1": p.bbox[3]}, "fill_rule": if p.evenodd { "evenodd" } else { "nonzero" }}) }
fn keys_of(c: &crate::model::Clip) -> Vec<(f64, String, String)> { c.effects.iter().find(|e| e.kind == "shape_path").and_then(|e| e.strings.get("keys")).and_then(|k| serde_json::from_str(k).ok()).unwrap_or_default() }
fn store(c: &mut crate::model::Clip, d: &str, evenodd: bool) {
    let mut strings = BTreeMap::new(); strings.insert("d".into(), d.to_string()); strings.insert("fill_rule".into(), if evenodd { "evenodd" } else { "nonzero" }.into());
    if let Some(e) = c.effects.iter_mut().find(|e| e.kind == "shape_path") { e.strings = strings; } else { c.effects.insert(0, Effect { id: "shape_path".into(), kind: "shape_path".into(), params: BTreeMap::new(), strings }); }
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_path_shape", description: "Add a Bezier path shape layer. Path = `d` (SVG path syntax: M L H V C S Q T Z, absolute/relative; no arcs) or `points` [[x,y],...] with closed/smooth (Catmull-Rom to Beziers, tension 0..1). Coordinates are pixels relative to the canvas center. Supports fill (fill_rule nonzero|evenodd), stroke, and every shape modifier: trim (exact arc length), offset, merge (also with another path via strings.shape=path + strings.d), repeater, gradient_stroke. Returns length/bbox.", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("start", n("s")), ("duration", n("s")), ("d", s("SVG path data")), ("points", json!({"type":"array","items":{"type":"array","items":{"type":"number"}}})), ("closed", json!({"type":"boolean"})), ("smooth", json!({"type":"boolean"})), ("tension", n("0..1")), ("fill_rule", s("nonzero|evenodd")), ("fill", json!({"type":"array","items":{"type":"number"},"description":"[r,g,b]; negative first value = no fill"})), ("stroke_width", n("px")), ("stroke_color", json!({"type":"array","items":{"type":"number"}}))], &["track_id","start","duration"]),
        run: |ss, a| { let d = resolve_d(a)?; let eo = rule(a)?; let p = Path::parse(&d, eo)?;
            let mut args = a.as_object().cloned().unwrap_or_default(); for k in ["d","points","closed","smooth","tension","fill_rule"] { args.remove(k); } args.insert("shape".into(), json!("rect"));
            let r = crate::commands::dispatch(ss, "add_shape", &Value::Object(args))?; let id = r.get("clip_id").and_then(|v| v.as_str()).map(String::from).or_else(|| r.get("clip_ids").and_then(|v| v.get(0)).and_then(|v| v.as_str()).map(String::from)).ok_or("add_shape returned no clip id")?;
            let c = ss.project.find_clip_mut(&id).ok_or("clip vanished")?;
            if let Source::Shape { shape, .. } = &mut c.source { *shape = "path".into(); } store(c, &d, eo);
            let mut o = info(&p, &d); o["clip_id"] = json!(id); Ok(o) } },
    Command { name: "set_path", description: "Replace the Bezier path of a path shape (clears path keys) (same inputs as add_path_shape: d or points[+closed/smooth/tension], fill_rule). Modifiers and styles on the clip are kept. Validates before changing anything.", mutates: true,
        schema: obj(vec![("clip_id", s("path shape clip")), ("d", s("SVG path data")), ("points", json!({"type":"array","items":{"type":"array","items":{"type":"number"}}})), ("closed", json!({"type":"boolean"})), ("smooth", json!({"type":"boolean"})), ("tension", n("0..1")), ("fill_rule", s("nonzero|evenodd"))], &["clip_id"]),
        run: |ss, a| { let d = resolve_d(a)?; let eo = rule(a)?; let p = Path::parse(&d, eo)?; let cid = str_arg(a, "clip_id")?.to_string();
            let c = ss.project.find_clip_mut(&cid).ok_or(format!("no such clip '{cid}'"))?;
            if !matches!(&c.source, Source::Shape { shape, .. } if shape == "path") { return Err("clip is not a path shape (create with add_path_shape)".into()); } store(c, &d, eo); Ok(info(&p, &d)) } },
    Command { name: "path_info", description: "Report a path shape's SVG data, arc length in px, flattened segment count, subpath count, bbox and fill rule. Animate draw-on with add_shape_modifier kind=trim and keyframes on its `end` param.", mutates: false,
        schema: obj(vec![("clip_id", s("path shape clip"))], &["clip_id"]),
        run: |ss, a| { let cid = str_arg(a, "clip_id")?.to_string(); let c = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == cid).ok_or(format!("no such clip '{cid}'"))?;
            let e = c.effects.iter().find(|e| e.kind == "shape_path").ok_or("clip has no path")?; let d = e.strings.get("d").cloned().unwrap_or_default(); let p = Path::parse(&d, e.strings.get("fill_rule").map_or(false, |r| r == "evenodd"))?; let mut o = info(&p, &d); o["path_keys"] = json!(keys_of(c).len()); Ok(o) } },
    Command { name: "add_path_key", description: "Keyframe the path itself (per-point animation / shape morph): at clip-local `time` the path becomes `d` or `points` (same inputs as add_path_shape). Every key must have the SAME structure as the base path (same commands and argument counts; for points use the same count and smooth/closed). Control points, endpoints and arc radii interpolate linearly, then `ease` (linear|in|out|in_out|hold) shapes each segment; arc flags snap at 50%. The base path is used before the first key. Trim/offset/merge/gradient modifiers keep working on the morphing path. Replaces a key at the same time.", mutates: true,
        schema: obj(vec![("clip_id", s("path shape clip")), ("time", n("clip-local seconds")), ("d", s("SVG path data")), ("points", json!({"type":"array","items":{"type":"array","items":{"type":"number"}}})), ("closed", json!({"type":"boolean"})), ("smooth", json!({"type":"boolean"})), ("tension", n("0..1")), ("ease", s("linear|in|out|in_out|hold, for the segment leaving this key"))], &["clip_id","time"]),
        run: |ss, a| { let d = resolve_d(a)?; Path::parse(&d, false)?; let t = num_arg(a, "time")?; if !t.is_finite() || t < 0.0 { return Err("time must be >= 0".into()); }
            let ez = a.get("ease").and_then(|v| v.as_str()).unwrap_or("linear").to_string(); if !crate::keyframe::EASE_NAMES.contains(&ez.as_str()) { return Err(format!("ease must be {}, got '{ez}'", crate::keyframe::EASE_NAMES.join("|"))); }
            let cid = str_arg(a, "clip_id")?.to_string(); let c = ss.project.find_clip_mut(&cid).ok_or(format!("no such clip '{cid}'"))?;
            let e = c.effects.iter().find(|e| e.kind == "shape_path").ok_or("clip has no path (create with add_path_shape)")?; let base = e.strings.get("d").cloned().unwrap_or_default();
            crate::paths::same_structure(&base, &d).map_err(|m| format!("{m}; base path is: {base}"))?;
            let mut keys = keys_of(c); if keys.len() >= 256 { return Err("max 256 path keys".into()); } if keys.is_empty() && t > 1e-9 { keys.push((0.0, base.clone(), "linear".into())); }
            keys.retain(|k| (k.0 - t).abs() > 1e-6); keys.push((t, d, ez)); keys.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let e = c.effects.iter_mut().find(|e| e.kind == "shape_path").unwrap(); e.strings.insert("keys".into(), serde_json::to_string(&keys).unwrap()); Ok(json!({"keys": keys.iter().map(|k| json!({"time": k.0, "ease": k.2})).collect::<Vec<_>>()})) } },
    Command { name: "clear_path_keys", description: "Remove all path keyframes (the base path stays).", mutates: true, schema: obj(vec![("clip_id", s("path shape clip"))], &["clip_id"]),
        run: |ss, a| { let cid = str_arg(a, "clip_id")?.to_string(); let c = ss.project.find_clip_mut(&cid).ok_or(format!("no such clip '{cid}'"))?; let e = c.effects.iter_mut().find(|e| e.kind == "shape_path").ok_or("clip has no path")?; Ok(json!({"removed": e.strings.remove("keys").is_some()})) } },
] }
