//! V1c presentation primitives. `add_dim_mask` is the primitive: a full-canvas black layer at `dim_factor` opacity with an inverted, feathered rect
//! mask hole, built from existing tools only (add_track, add_clip, set_property, add_mask, set_keyframe, set_meta) and applied as ONE undo step through
//! `batch`. `spotlight` is a pure wrapper: it expands to one `add_dim_mask` call with every parameter explicit and echoes it; `apply:false` (default) returns
//! the expansion and changes nothing, `apply:true` runs exactly that call.
use super::kit::*;

const MAX_FEATHER: f64 = 256.0;
fn finite(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(d),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("out_of_range: {k} must be a finite number (got {v}); fix_hint: pass a plain number")),
    }
}
fn range(a: &Value, k: &str, d: f64, lo: f64, hi: f64, hint: &str) -> Result<f64, String> {
    let v = finite(a, k, d)?;
    if v < lo || v > hi { return Err(format!("out_of_range: {k} must be between {lo} and {hi} (got {v}); fix_hint: {hint}")); }
    Ok(v)
}
/// Canvas-pixel box [x, y, w, h] from `target_box` or an axis-aligned shape clip.
fn target(ss: &Session, a: &Value) -> Result<[f64; 4], String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    if let Some(id) = a.get("shape_id").and_then(|v| v.as_str()) {
        if a.get("target_box").map_or(false, |v| !v.is_null()) { return Err("invalid_argument_combo: give target_box or shape_id, not both; fix_hint: drop one of them".into()); }
        let id = super::names::resolve_ref(&ss.project, "clip", id)?;
        let c = ss.project.find_clip(&id).ok_or_else(|| format!("no clip '{id}'"))?;
        let crate::model::Source::Shape { width, height, .. } = &c.source else { return Err(format!("invalid_source: '{id}' is not a shape clip; fix_hint: pass target_box [x,y,w,h] in canvas pixels instead")) };
        let q = crate::render::resolve_xform(&ss.project, c, 0.0);
        if q.rotation.abs() > 1e-9 { return Err(format!("invalid_source: shape '{id}' is rotated, its box is not axis aligned; fix_hint: pass target_box [x,y,w,h] in canvas pixels")); }
        let (w, h) = (width * q.scale, height * q.scale);
        return Ok([cw / 2.0 + q.x - w / 2.0, ch / 2.0 + q.y - h / 2.0, w, h]);
    }
    let arr = a.get("target_box").and_then(|v| v.as_array()).ok_or("missing array 'target_box'; fix_hint: pass target_box [x, y, w, h] in canvas pixels or shape_id")?;
    if arr.len() != 4 { return Err(format!("out_of_range: target_box must have 4 numbers [x, y, w, h] (got {}); fix_hint: pass [x, y, w, h]", arr.len())); }
    let mut b = [0.0; 4];
    for (i, v) in arr.iter().enumerate() { b[i] = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("out_of_range: target_box[{i}] must be a finite number (got {v}); fix_hint: pass plain numbers in canvas pixels"))?; }
    if b[2] <= 0.0 || b[3] <= 0.0 { return Err(format!("out_of_range: target_box w and h must be > 0 (got {} x {}); fix_hint: pass a box with positive size, e.g. [{}, {}, 64, 64]", b[2], b[3], b[0], b[1])); }
    Ok(b)
}
/// Every parameter resolved and echoed, plus the batch op list that builds the layer.
fn plan(ss: &Session, a: &Value) -> Result<(Value, Vec<Value>), String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let tb = target(ss, a)?;
    let (x0, y0, x1, y1) = (tb[0].max(0.0), tb[1].max(0.0), (tb[0] + tb[2]).min(cw), (tb[1] + tb[3]).min(ch));
    if x1 <= x0 || y1 <= y0 { return Err(format!("out_of_range: target_box [{}, {}, {}, {}] lies outside the {cw} x {ch} canvas; fix_hint: move the box onto the canvas", tb[0], tb[1], tb[2], tb[3])); }
    let dim = range(a, "dim_factor", 0.6, 0.0, 1.0, "use 0 (no dim) to 1 (black)")?;
    let feather = range(a, "feather_px", 0.0, 0.0, MAX_FEATHER, "feather_px is capped at 256; lower it")?;
    let (bw, bh) = (x1 - x0, y1 - y0);
    let radius = range(a, "corner_radius", 0.0, 0.0, bw.min(bh) / 2.0, "corner_radius cannot exceed half the shorter side of the clipped box")?;
    let start = range(a, "start", 0.0, 0.0, 1e7, "start is timeline seconds, >= 0")?;
    let dur = range(a, "duration", 5.0, 1e-3, 1e7, "duration is seconds, > 0")?;
    let ein = range(a, "ease_in", 0.0, 0.0, dur, "ease_in is seconds, 0..duration")?;
    let eout = range(a, "ease_out", 0.0, 0.0, dur, "ease_out is seconds, 0..duration")?;
    if ein + eout > dur { return Err(format!("out_of_range: ease_in + ease_out must be <= duration (got {} > {dur}); fix_hint: shorten the eases", ein + eout)); }
    let track = match a.get("track_id") { None | Some(Value::Null) => None, Some(v) => Some(super::names::resolve_ref(&ss.project, "track", v.as_str().ok_or("argument 'track_id' must be a string")?)?) };
    let params = json!({"target_box":[tb[0], tb[1], tb[2], tb[3]], "clipped_box":[x0, y0, bw, bh], "dim_factor":dim, "feather_px":feather, "corner_radius":radius, "start":start, "duration":dur, "ease_in":ein, "ease_out":eout, "track_id":track,
        "layer":"solid black, opacity = dim_factor, rect mask invert=true (hole), feather in px, mode add", "role":"dim_mask"});
    let mut ops = vec![];
    let tid = match &track { Some(t) => json!(t), None => { ops.push(json!({"id":"track","tool":"add_track","args":{}})); json!("$track.track_id") } };
    ops.push(json!({"id":"dim","tool":"add_clip","args":{"track_id":tid,"source_type":"solid","color":[0,0,0],"start":start,"duration":dur}}));
    ops.push(json!({"id":"role","tool":"set_meta","args":{"id":"$dim.clip_id","role":"dim_mask"}}));
    ops.push(json!({"id":"op","tool":"set_property","args":{"clip_id":"$dim.clip_id","property":"opacity","value":dim}}));
    let round = if bw.min(bh) > 0.0 { radius / (bw.min(bh) / 2.0) } else { 0.0 };
    ops.push(json!({"id":"mask","tool":"add_mask","args":{"clip_id":"$dim.clip_id","kind":"rect","mode":"add","invert":true,"feather":feather,"params":{"cx":(x0 + bw / 2.0) / cw,"cy":(y0 + bh / 2.0) / ch,"w":bw / cw,"h":bh / ch,"round":round}}}));
    if ein > 0.0 || eout > 0.0 {
        let key = |t: f64, v: f64| json!({"tool":"set_keyframe","args":{"clip_id":"$dim.clip_id","property":"opacity","time":t,"value":v,"ease":"linear"}});
        if ein > 0.0 { ops.push(key(0.0, 0.0)); ops.push(key(ein, dim)); } else { ops.push(key(0.0, dim)); }
        if eout > 0.0 { ops.push(key(dur - eout, dim)); ops.push(key(dur, 0.0)); } else { ops.push(key(dur, dim)); }
    }
    Ok((params, ops))
}
fn apply_ops(ss: &mut Session, ops: &[Value]) -> Result<(String, String, Option<String>), String> {
    let r = crate::commands::dispatch_nested(ss, "batch", &json!({"ops":ops,"atomic":true,"return":"full"}))?;
    if r["ok"] != true { return Err(format!("batch_failed: {}", r["failed"])); }
    let res = r["results"].as_array().ok_or("batch returned no results")?;
    let find = |op: &str, k: &str| res.iter().find(|x| x["id"] == op).and_then(|x| x["result"][k].as_str()).map(String::from);
    Ok((find("dim", "clip_id").ok_or("dim layer not created")?, find("mask", "mask_id").ok_or("mask not created")?, find("track", "track_id")))
}
fn add_dim_mask(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (params, ops) = plan(ss, a)?;
    let (clip, mask, track) = apply_ops(ss, &ops)?;
    Ok(json!({"clip_id":clip,"mask_id":mask,"track_id":track,"params":params,"expansion":ops,"note":"Animate the hole with set_mask_prop_key on mask_id (cx, cy, w, h, feather, round are clip-local, normalized 0..1 except feather px)."}))
}
fn spotlight(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let apply = match a.get("apply") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("argument 'apply' must be a boolean")? };
    let (params, _) = plan(ss, a)?;
    let args = json!({"target_box":params["clipped_box"],"dim_factor":params["dim_factor"],"feather_px":params["feather_px"],"corner_radius":params["corner_radius"],"start":params["start"],"duration":params["duration"],"ease_in":params["ease_in"],"ease_out":params["ease_out"],"track_id":params["track_id"]});
    let expansion = json!([{"tool":"add_dim_mask","args":args}]);
    if !apply { return Ok(json!({"applied":false,"no_history":true,"params":params,"expansion":expansion})); }
    let r = add_dim_mask(ss, &args)?;
    Ok(json!({"applied":true,"clip_id":r["clip_id"],"mask_id":r["mask_id"],"track_id":r["track_id"],"params":params,"expansion":expansion}))
}
pub fn commands() -> Vec<Command> {
    let ease = |d: &str| n(d);
    vec![
        Command{name:"add_dim_mask",mutates:true,description:"Dim everything except a box: adds a full-canvas black layer at dim_factor opacity with a feathered inverted rect hole over target_box [x,y,w,h] canvas px or an axis-aligned shape_id (box clipped to the canvas). Built from add_track/add_clip/set_property/add_mask/set_keyframe/set_meta and applied as one undo step; returns clip_id, mask_id and the exact op list. feather_px max 256. Animate the hole with set_mask_prop_key.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip to spotlight")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("0..256, default 0")),("corner_radius",n("px, default 0")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("fade-in seconds, default 0")),("ease_out",ease("fade-out seconds, default 0")),("track_id",s("existing track; default a new top track"))],&[]),run:add_dim_mask},
        Command{name:"spotlight",mutates:true,description:"Wrapper over add_dim_mask: same parameters, returns the expansion (one add_dim_mask call with every parameter explicit) and params. apply:false (default) changes nothing; apply:true runs that call as one undo step.",
            schema:obj(vec![("target_box",json!({"type":"array","items":{"type":"number"},"description":"[x,y,w,h] canvas pixels"})),("shape_id",s("axis-aligned shape clip")),("dim_factor",n("0..1, default 0.6")),("feather_px",n("0..256, default 0")),("corner_radius",n("px, default 0")),("start",n("seconds, default 0")),("duration",n("seconds, default 5")),("ease_in",ease("seconds, default 0")),("ease_out",ease("seconds, default 0")),("track_id",s("existing track")),("apply",json!({"type":"boolean","description":"default false: return the expansion only"}))],&[]),run:spotlight},
    ]
}
