//! add_lens_blur (After Effects Camera Lens Blur): an optical defocus on a clip. Bright points open into the iris shape
//! (disc or n-blade polygon) with highlight bloom, in linear light; an optional focus point keeps the subject exactly sharp.
//! It is a clip effect of kind lens_blur (lens.rs), so set_effect_param, set_keyframe, set_effect_mask and remove_effect work on it.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::*;

const ARGS: [&str; 12] = ["clip_id", "radius", "blades", "rotation", "roundness", "highlight_gain", "highlight_threshold", "focus_x", "focus_y", "focus_radius", "falloff", "remove"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_lens_blur", description: "Camera lens blur: optical defocus with iris-shaped bokeh and highlight bloom. Optional focus point keeps the subject sharp.", mutates: true,
        schema: obj(vec![("clip_id", s("clip to defocus")),
            ("radius", n("blur radius in px, 1..200, default 12")),
            ("blades", n("iris blades: 0 = round disc, 3..12 = polygon bokeh, default 6")),
            ("rotation", n("iris rotation in degrees, default 0")),
            ("roundness", n("0 = straight blades .. 1 = round, default 0")),
            ("highlight_gain", n("bokeh bloom on bright points, 0..20, default 2")),
            ("highlight_threshold", n("luma above which highlights bloom, 0..1, default 0.75")),
            ("focus_x", n("focus point x, 0..1; with focus_radius, the area around it stays sharp")),
            ("focus_y", n("focus point y, 0..1")),
            ("focus_radius", n("sharp area radius as a fraction of the short side, 0..1; 0 = blur everything (default)")),
            ("falloff", n("distance over which the blur ramps to full, fraction of the short side, default 0.1")),
            ("remove", json!({"type":"boolean","description":"true removes the clip's lens blur"}))], &["clip_id"]),
        run: |ss, a| {
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_lens_blur: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let cid = a.get("clip_id").and_then(|v| v.as_str()).ok_or("add_lens_blur needs clip_id")?.to_string();
            if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) {
                let c = ss.project.find_clip_mut(&cid).ok_or_else(|| format!("no such clip {cid}"))?; let n0 = c.effects.len(); c.effects.retain(|e| e.kind != "lens_blur");
                return Ok(json!({"removed": n0 - c.effects.len()})); }
            let num = |k: &str, d: f64, lo: f64, hi: f64| -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) if v.is_null() => Ok(d),
                Some(v) => { let x = v.as_f64().ok_or_else(|| format!("{k} must be a number ({lo}..{hi})"))?; if !(lo..=hi).contains(&x) { Err(format!("{k} must be between {lo} and {hi}, got {x}")) } else { Ok(x) } } } };
            let radius = num("radius", 12.0, 1.0, 200.0)?; let blades = num("blades", 6.0, 0.0, 12.0)?;
            if blades.fract() != 0.0 || (blades > 0.0 && blades < 3.0) { return Err(format!("blades must be 0 (round) or a whole number 3..12, got {blades}")); }
            let rotation = num("rotation", 0.0, -360.0, 360.0)?; let roundness = num("roundness", 0.0, 0.0, 1.0)?;
            let gain = num("highlight_gain", 2.0, 0.0, 20.0)?; let thr = num("highlight_threshold", 0.75, 0.0, 0.99)?;
            let fr = num("focus_radius", 0.0, 0.0, 1.0)?; let fx = num("focus_x", 0.5, 0.0, 1.0)?; let fy = num("focus_y", 0.5, 0.0, 1.0)?; let fall = num("falloff", 0.1, 0.005, 1.0)?;
            if fr == 0.0 && (a.get("focus_x").is_some() || a.get("focus_y").is_some()) { return Err("focus_x/focus_y need focus_radius > 0 (the size of the sharp area)".into()); }
            if ss.project.find_clip(&cid).is_none() { return Err(format!("no such clip {cid}")); }
            let vals = [("radius", radius), ("blades", blades), ("rotation", rotation), ("roundness", roundness), ("highlight_gain", gain), ("highlight_threshold", thr), ("cx", fx), ("cy", fy), ("focus_radius", fr), ("falloff", fall)];
            let params: std::collections::BTreeMap<String, Prop> = vals.iter().map(|(k, v)| (k.to_string(), Prop::new(*v))).collect();
            let eid = { let existing = ss.project.find_clip(&cid).unwrap().effects.iter().find(|e| e.kind == "lens_blur").map(|e| e.id.clone()); existing.unwrap_or_else(|| ss.project.gen_id("fx")) };
            let c = ss.project.find_clip_mut(&cid).unwrap();
            let replaced = if let Some(e) = c.effects.iter_mut().find(|e| e.id == eid) { e.params = params; true } else { c.effects.push(Effect { id: eid.clone(), kind: "lens_blur".into(), params, strings: Default::default() }); false };
            Ok(json!({"effect_id": eid, "updated": replaced, "radius": radius, "blades": blades, "focus": if fr > 0.0 { json!({"x": fx, "y": fy, "radius": fr}) } else { Value::Null }})) } },
] }
