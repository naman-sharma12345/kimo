//! RV-27 lane A: curves. `set_keyframes` (A1) writes many keys on one property in one atomic call. Values are never clamped: any finite number is stored.
use super::kit::*;
use crate::keyframe::{Ease, Handle, Key, Prop};

const CLIP_PROPS: &[&str] = &["opacity", "x", "y", "scale", "rotation", "rot_x", "rot_y", "pos_z", "backface", "fill_alpha", "stroke_alpha", "gradient_angle", "flow_blur", "flow_quality", "pin_gain", "reveal", "path"];
fn finite(a: &Value, k: &str, what: &str, i: usize) -> Result<f64, String> { let x = a.get(k).and_then(|v| v.as_f64()).ok_or_else(|| format!("keys[{i}].{what} must be a number"))?; if !x.is_finite() { return Err(format!("keys[{i}].{what} must be finite")); } Ok(x) }
fn parse_keys(a: &Value) -> Result<Vec<Key>, String> {
    let ks = a.get("keys").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "keys", "array"))?; if ks.is_empty() { return Err("keys must have at least one key".into()); }
    let mut out = vec![];
    for (i, k) in ks.iter().enumerate() {
        if !k.is_object() { return Err(format!("keys[{i}] must be an object {{t, v, ease?, bez?, hold?}}")); }
        let t = finite(k, "t", "t", i)?; let v = finite(k, "v", "v", i)?;
        let mut ease: Ease = match k.get("ease") { None | Some(Value::Null) => Ease::Linear, Some(e) => serde_json::from_value(e.clone()).map_err(|_| format!("keys[{i}].ease is not allowed. Valid: linear, in, out, in_out, hold, out_quart, out_expo, out_back, in_out_quint, in_expo"))? };
        match k.get("hold") { None | Some(Value::Null) => {} Some(Value::Bool(b)) => { if *b { ease = Ease::Hold; } } _ => return Err(format!("keys[{i}].hold must be true or false")) }
        let bez = match k.get("bez") { None | Some(Value::Null) => None, Some(b) => { let arr = b.as_array().filter(|x| x.len() == 4).ok_or_else(|| format!("keys[{i}].bez must be [x1,y1,x2,y2]"))?; let mut o = [0.0; 4]; for (j, x) in arr.iter().enumerate() { o[j] = x.as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("keys[{i}].bez[{j}] must be a finite number"))?; } Some(o) } };
        let hd = |name: &str| -> Result<Option<Handle>, String> { match k.get(name) { None | Some(Value::Null) => Ok(None), Some(h) => { let dt = h.get("dt").and_then(|x| x.as_f64()).filter(|f| f.is_finite()).ok_or_else(|| format!("keys[{i}].{name} must be {{dt, dv}} with finite numbers"))?; let dv = h.get("dv").and_then(|x| x.as_f64()).filter(|f| f.is_finite()).ok_or_else(|| format!("keys[{i}].{name} must be {{dt, dv}} with finite numbers"))?; if dt < 0.0 { return Err(format!("keys[{i}].{name}.dt must be >= 0 (a handle never points backwards in time)")); } Ok(Some(Handle { dt, dv })) } } };
        out.push(Key { t, v, ease, bez, out: hd("out")?, inn: hd("in")? });
    }
    Ok(out)
}
/// Same semantics as one `set_keyframe` call per key, in order (a key within 1e-6 s of an existing one replaces it); keeps `bez` when given.
fn put(p: &mut Prop, k: &Key) { p.set_key(k.t, k.v, k.ease); if let Some(x) = p.keys.iter_mut().find(|x| (x.t - k.t).abs() <= 1e-6) { if k.bez.is_some() { x.bez = k.bez; } x.out = k.out; x.inn = k.inn; } }
fn set_keyframes(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let prop = str_arg(a, "property")?.to_string(); let keys = parse_keys(a)?;
    let mode = a.get("mode").and_then(|v| v.as_str()).unwrap_or("replace_all"); if !["replace_all", "merge", "replace_range"].contains(&mode) { return Err("mode is not allowed. Valid: replace_all, merge, replace_range".into()); }
    let eid = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
    if eid.is_none() && !CLIP_PROPS.contains(&prop.as_str()) { return Err(format!("unknown clip property '{prop}': nothing reads it. Valid: {}. To keyframe an effect param pass effect_id.", CLIP_PROPS.join(", "))); }
    let cid = str_arg(a, "clip_id")?; let c = ss.project.find_clip_mut(cid).ok_or(format!("no clip '{cid}'"))?;
    let first = keys[0].v;
    let p = match eid { Some(id) => c.effects.iter_mut().find(|x| x.id == id).ok_or("no such effect")?.params.entry(prop).or_insert(Prop::new(first)), None => c.props.entry(prop).or_insert(Prop::new(first)) };
    let old = p.keys.clone();
    match mode { "replace_all" => p.keys.clear(), "replace_range" => { let (lo, hi) = keys.iter().fold((f64::MAX, f64::MIN), |(l, h), k| (l.min(k.t), h.max(k.t))); p.keys.retain(|x| x.t < lo - 1e-6 || x.t > hi + 1e-6); } _ => {} }
    for k in &keys { put(p, k); }
    let replaced = old.iter().filter(|o| !p.keys.iter().any(|n| (n.t - o.t).abs() <= 1e-6 && n.v == o.v && n.ease == o.ease && n.bez == o.bez && n.out == o.out && n.inn == o.inn)).count();
    Ok(json!({"keys": p.keys.len(), "written": keys.len(), "replaced": replaced, "mode": mode}))
}

/// What the renderer reads for a standard clip property that was never keyed. There is no single renderer default table, so each default is pinned to the line that reads it:
/// opacity/x/y/scale/rotation/reveal come from model::default_props (reveal 1); fill_alpha/stroke_alpha 1 (render.rs prop_or, line ~255); pin_gain 1 (planar.rs unwrap_or(1.0), line ~233);
/// rot_x, rot_y, pos_z, backface, flow_blur, flow_quality, path read 0 through render.rs prop() (flow_quality 0 is not "no quality": the renderer clamps it to 3..6 at render.rs ~305).
pub(crate) fn static_default(k: &str) -> Option<Prop> { if let Some(p) = crate::model::default_props().remove(k) { return Some(p); } if !CLIP_PROPS.contains(&k) { return None; } Some(Prop::new(if matches!(k, "fill_alpha" | "stroke_alpha" | "pin_gain") { 1.0 } else { 0.0 })) }
const MAX_SAMPLES: usize = 100_000;
/// Local clip time exactly as the renderer derives it for transform props (render.rs: lt0): posterized when the clip has posterize_fps, never negative.
fn local_time(c: &crate::model::Clip, t: f64) -> f64 { crate::render::clip_local_time(c, t) }
fn sample_times(a: &Value) -> Result<Vec<f64>, String> {
    match a.get("times") {
        Some(Value::Array(v)) => { if v.is_empty() || v.len() > MAX_SAMPLES { return Err(format!("times needs 1 to {MAX_SAMPLES} numbers")); } v.iter().enumerate().map(|(i, x)| x.as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("times[{i}] must be a finite number"))).collect() }
        Some(Value::Object(o)) => { let g = |k: &str| o.get(k).and_then(|v| v.as_f64()).filter(|f| f.is_finite()).ok_or_else(|| format!("times.{k} must be a finite number")); let (s0, e0) = (g("start")?, g("end")?); let n = o.get("n").and_then(|v| v.as_u64()).ok_or("times.n must be a positive integer")? as usize; if n == 0 || n > MAX_SAMPLES { return Err(format!("times.n must be 1 to {MAX_SAMPLES}")); } Ok((0..n).map(|i| if n == 1 { s0 } else { s0 + (e0 - s0) * i as f64 / (n - 1) as f64 }).collect()) }
        _ => Err(crate::errors::arg_err(a, "times", "array of seconds or {start,end,n}")),
    }
}
fn sample_property(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let times = sample_times(a)?; let space = a.get("space").and_then(|v| v.as_str()).unwrap_or("timeline"); if !["timeline", "clip"].contains(&space) { return Err("space is not allowed. Valid: timeline, clip".into()); }
    let prop = str_arg(a, "property")?;
    if let Some(cam) = a.get("camera").and_then(|v| v.as_bool()).filter(|b| *b) { let _ = cam; let p = ss.project.camera.get(prop).ok_or_else(|| format!("no camera field '{prop}'. Valid: {}", ss.project.camera.keys().cloned().collect::<Vec<_>>().join(", ")))?; let mut vals = Vec::with_capacity(times.len()); for t in &times { vals.push(p.at(*t)); } return Ok(json!({"values": vals, "times": times, "space": "timeline", "target": "camera"})); }
    let cid = str_arg(a, "clip_id")?; let c = ss.project.find_clip(cid).ok_or(format!("no clip '{cid}'"))?;
    let eff = a.get("effect_id").and_then(|v| v.as_str()); let dflt = if eff.is_none() && !c.props.contains_key(prop) { static_default(prop) } else { None };
    let p = match eff { Some(id) => c.effects.iter().find(|x| x.id == id).ok_or("no such effect")?.params.get(prop), None => c.props.get(prop).or(dflt.as_ref()) };
    let p = p.ok_or_else(|| format!("clip has no property '{prop}'. Properties on this clip: {}", c.props.keys().cloned().collect::<Vec<_>>().join(", ")))?;
    let mut vals = Vec::with_capacity(times.len()); let mut active = Vec::with_capacity(times.len());
    for t in &times { let lt = if space == "clip" { *t } else { local_time(c, *t) }; vals.push(p.at(lt)); active.push(if space == "clip" { *t >= 0.0 && *t < c.duration } else { *t >= c.start && *t < c.start + c.duration }); }
    Ok(json!({"values": vals, "times": times, "active": active, "space": space, "keys": p.keys.len(), "expressions": p.expr.len()}))
}
/// A4: bake any property (keys + easing + expressions) into dense raw keys, then drop the expression ops. Samples come from the single evaluator `Prop::at` (clip-local time); keys are linear. max_error is measured at segment midpoints against the original evaluator.
fn bake_grid(a: &Value) -> Result<Vec<f64>, String> {
    let g = |k: &str| a.get(k).and_then(|v| v.as_f64()).filter(|f| f.is_finite()).ok_or_else(|| format!("{k} must be a finite number of seconds"));
    let (s0, e0) = (g("start")?, g("end")?); if e0 <= s0 { return Err("end must be greater than start".into()); }
    let n = match (a.get("fps").and_then(|v| v.as_f64()), a.get("n").and_then(|v| v.as_u64())) {
        (Some(f), None) => { if !f.is_finite() || f <= 0.0 { return Err("fps must be a positive number".into()); } ((e0 - s0) * f + 1e-9).floor() as usize + 1 }
        (None, Some(n)) => n as usize, (None, None) => return Err("give fps (keys per second) or n (key count)".into()), _ => return Err("give fps or n, not both".into()) };
    if n < 2 || n > MAX_SAMPLES { return Err(format!("bake needs 2 to {MAX_SAMPLES} keys, got {n}")); }
    Ok((0..n).map(|i| if a.get("fps").is_some() { s0 + i as f64 / a["fps"].as_f64().unwrap() } else { s0 + (e0 - s0) * i as f64 / (n - 1) as f64 }).collect())
}
fn bake_property(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let prop = str_arg(a, "property")?.to_string(); let ts = bake_grid(a)?;
    if a.get("drop_expression").and_then(|v| v.as_bool()) == Some(false) { return Err("drop_expression:false would apply the expression twice (the baked keys already contain it). Use sample_property to read without changing the property".into()); }
    let cam = a.get("camera").and_then(|v| v.as_bool()).unwrap_or(false); let eff = a.get("effect_id").and_then(|v| v.as_str()).map(String::from);
    // 1. validate everything on read-only state: an error changes nothing (no default entry is inserted yet)
    let proj_dur = ss.project.duration();
    let (old, span, needs_default): (Prop, f64, bool) = if cam { let keys = ss.project.camera.keys().cloned().collect::<Vec<_>>().join(", "); (ss.project.camera.get(&prop).ok_or_else(|| format!("no camera field '{prop}'. Valid: {keys}"))?.clone(), proj_dur, false) } else {
        let cid = str_arg(a, "clip_id")?; let c = ss.project.find_clip(cid).ok_or(format!("no clip '{cid}'"))?; let have = c.props.keys().cloned().collect::<Vec<_>>().join(", ");
        let (p, nd) = match &eff { Some(id) => (c.effects.iter().find(|x| x.id == *id).ok_or("no such effect")?.params.get(&prop).cloned(), false), None => match c.props.get(&prop) { Some(p) => (Some(p.clone()), false), None => (static_default(&prop), true) } };
        (p.ok_or_else(|| format!("clip has no property '{prop}'. Properties on this clip: {have}"))?, c.duration, nd) };
    let keys: Vec<Key> = ts.iter().map(|t| Key { t: *t, v: old.at(*t), ease: Ease::Linear, bez: None, out: None, inn: None }).collect();
    if keys.iter().any(|k| !k.v.is_finite()) { return Err("the property evaluates to a non-finite value inside the range; nothing was baked".into()); }
    // 2. commit
    let p: &mut Prop = if cam { ss.project.camera.get_mut(&prop).unwrap() } else { let c = ss.project.find_clip_mut(str_arg(a, "clip_id")?).unwrap();
        match &eff { Some(id) => c.effects.iter_mut().find(|x| x.id == *id).unwrap().params.get_mut(&prop).unwrap(), None => { if needs_default { c.props.insert(prop.clone(), old.clone()); } c.props.get_mut(&prop).unwrap() } } };
    let dropped = old.expr.len(); let (lo, hi) = (ts[0], *ts.last().unwrap());
    p.keys.retain(|x| x.t < lo - 1e-6 || x.t > hi + 1e-6); for k in &keys { put(p, k); } p.expr.clear();
    let mut max_err = 0.0f64; let mut at = lo;
    for w in ts.windows(2) { let m = (w[0] + w[1]) / 2.0; let e = (old.at(m) - p.at(m)).abs(); if e > max_err { max_err = e; at = m; } }
    let outside = dropped > 0 && (lo > 1e-6 || hi < span - 1e-6);
    let mut out = json!({"keys": keys.len(), "total_keys": p.keys.len(), "expressions_dropped": dropped, "max_error": max_err, "max_error_at": at, "range": [lo, hi], "outside_range_changed": outside, "span": [0.0, span]});
    if let Some(e) = a.get("end").and_then(|v| v.as_f64()) { if (e - hi).abs() > 1e-9 { out["end_requested"] = json!(e); } }
    if outside { out["note"] = json!(format!("the expression ops were dropped for the whole property, but only [{lo}, {hi}] was baked: the property now holds its edge key outside that range (full span is [0, {span}])")); }
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_keyframes", description: "Write many keyframes on ONE property in a single atomic call (any order, any finite value, no clamping). mode: replace_all (default, drops existing keys), merge (overwrite same-time keys, keep others), replace_range (drop existing keys inside the new keys' time span). Each key: t clip-local seconds, v value, optional ease, hold:true, bez [x1,y1,x2,y2], and tangent handles out {dt,dv} (leaving this key: control point t+dt, v+dv) and in {dt,dv} (arriving: control point t-dt, v-dv; mirrored handles have equal in/out values, giving a smooth C1 key). dt is seconds >= 0 and clamped to the segment; dv is unclamped so curves can overshoot. A missing handle on a segment defaults to a straight third. Handles win over bez on the same segment. A bad key changes nothing.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("property", s("opacity|x|y|scale|rotation... or effect param name when effect_id is given")), ("effect_id", s("optional: target an effect param")), ("keys", json!({"type":"array","items":{"type":"object","properties":{"t":{"type":"number"},"v":{"type":"number"},"ease":{"type":"string","enum":crate::keyframe::EASE_NAMES},"hold":{"type":"boolean"},"bez":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4},"out":{"type":"object","properties":{"dt":{"type":"number"},"dv":{"type":"number"}},"required":["dt","dv"]},"in":{"type":"object","properties":{"dt":{"type":"number"},"dv":{"type":"number"}},"required":["dt","dv"]}},"required":["t","v"]},"description":"keys to write"})), ("mode", json!({"type":"string","enum":["replace_all","merge","replace_range"]}))], &["clip_id", "property", "keys"]), run: set_keyframes },
    Command { name: "sample_property", description: "Read a property's evaluated value at any times, exactly as the renderer evaluates it (keys, easing, expressions, posterize), without baking. times: [seconds] or {start,end,n}. space: timeline (default; clip start applied) or clip (clip-local seconds). Target a clip property (clip_id + property, effect_id for an effect param) or a camera field (camera:true + property). Returns values and, for clips, whether the clip is active at each time. Unkeyed standard properties return the renderer default; flow_quality 0 means unset (the renderer clamps it to 3..6).", mutates: false,
        schema: obj(vec![("clip_id", s("clip (omit for camera)")), ("property", s("property name, effect param, or camera field")), ("effect_id", s("optional: effect whose param is read")), ("camera", json!({"type":"boolean","description":"read a camera field instead of a clip property"})), ("times", json!({"description":"array of seconds, or {start,end,n} evenly spaced inclusive"})), ("space", json!({"type":"string","enum":["timeline","clip"]}))], &["property", "times"]), run: sample_property },
    Command { name: "bake_property", description: "Turn ANY animated property (keys, easing, expressions) into dense raw linear keys you can edit, then drop its expression ops. start/end seconds in clip time, with fps (keys per second) or n (key count, at least 2). Existing keys inside the range are replaced. Returns max_error: an estimate of the worst gap to the original curve, sampled at segment midpoints (not a bound; raise fps/n to shrink it). The expression ops are dropped for the WHOLE property: when the baked range does not cover the clip span (camera: project span) outside_range_changed is true. end_requested appears when fps made the last key fall short of end. Target a clip property (clip_id + property, effect_id for an effect param) or a camera field (camera:true + property). Undoable.", mutates: true,
        schema: obj(vec![("clip_id", s("clip (omit for camera)")), ("property", s("property name, effect param, or camera field")), ("effect_id", s("optional: effect whose param is baked")), ("camera", json!({"type":"boolean"})), ("start", json!({"type":"number"})), ("end", json!({"type":"number"})), ("fps", json!({"type":"number","description":"keys per second"})), ("n", json!({"type":"integer","description":"key count (instead of fps)"})), ("drop_expression", json!({"type":"boolean","description":"default true; false is rejected because baked keys already include the expression"}))], &["property", "start", "end"]), run: bake_property },
] }
