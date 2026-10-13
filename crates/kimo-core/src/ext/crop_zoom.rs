//! V1c-B crop_zoom_keyframes: scale + position keyframes on one clip so the visible window moves from `from_box` to `to_box` (canvas px).
//! The maths is pure (ext/curves.rs); this tool only samples it once per frame and writes the keys through the existing `set_keyframes` tool as ONE batch (one undo step).
use super::kit::*;
use crate::model::Source;
use super::curves::{clamp_window, ease_at, fit_window, lerp_box, window_to_xform, xform_to_window, Box4};

const MAX_KEYS: usize = 600;
fn box_arg(a: &Value, k: &str) -> Result<Box4, String> {
    let v = a.get(k).and_then(|v| v.as_array()).filter(|v| v.len() == 4).ok_or_else(|| format!("invalid_box: {k} must be [x, y, w, h] in canvas px"))?;
    let b = [0, 1, 2, 3].map(|i| v[i].as_f64().unwrap_or(f64::NAN));
    if b.iter().any(|x| !x.is_finite()) || b[2] <= 0. || b[3] <= 0. { return Err(format!("invalid_box: {k} must be four finite numbers with w and h > 0")); }
    Ok(b)
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "clip_id")?.to_string(); let (from, to) = (box_arg(a, "from_box")?, box_arg(a, "to_box")?);
    let dur = num_arg(a, "duration")?; if !dur.is_finite() || dur <= 0. { return Err("out_of_range: duration must be > 0 seconds".into()); }
    let start = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.); if !start.is_finite() || start < 0. { return Err("out_of_range: start must be >= 0 (clip-local seconds)".into()); }
    let ease = a.get("easing").cloned().unwrap_or(json!("in_out")); ease_at(&ease, 0.5)?;
    let clamp = a.get("clamp").and_then(|v| v.as_bool()).unwrap_or(true); let apply = a.get("apply").and_then(|v| v.as_bool()).unwrap_or(true);
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64); let fps = ss.project.fps;
    let n = (dur * fps).ceil().max(1.) as usize; if n + 1 > MAX_KEYS { return Err(format!("limit_exceeded: {} keys per property exceeds {MAX_KEYS}; shorten duration or split the move", n + 1)); }
    let (c, lt0) = { let c = ss.project.find_clip(&id).ok_or_else(|| format!("not_found: no clip {id}"))?; (c.clone(), start) };
    let (sw, sh) = super::lint_frame::source_size(&ss.project, &c, lt0).ok_or("invalid_source: the clip has no drawn bounds to zoom into")?;
    let src: Box4 = [cw / 2. - sw / 2., ch / 2. - sh / 2., sw, sh];
    let (wa, pad_a) = fit_window(from, cw, ch); let (wb, pad_b) = fit_window(to, cw, ch);
    let (mut ks, mut kx, mut ky, mut clamped) = (vec![], vec![], vec![], false); let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut wins = vec![];
    for i in 0..=n {
        let u = i as f64 / n as f64; let t = start + dur * u; let e = ease_at(&ease, u)?;
        let mut w = lerp_box(wa, wb, e); if clamp { let (cw_, ch_) = clamp_window(w, src); w = cw_; clamped |= ch_; }
        let (s, x, y) = window_to_xform(w, cw, ch); smin = smin.min(s); smax = smax.max(s);
        ks.push(json!({"t":t,"v":s})); kx.push(json!({"t":t,"v":x})); ky.push(json!({"t":t,"v":y})); wins.push((t, xform_to_window(s, x, y, cw, ch)));
    }
    let ops = json!([{"tool":"set_keyframes","args":{"clip_id":id,"property":"scale","keys":ks}},{"tool":"set_keyframes","args":{"clip_id":id,"property":"x","keys":kx}},{"tool":"set_keyframes","args":{"clip_id":id,"property":"y","keys":ky}}]);
    let eff: Vec<Value> = [0, n / 2, n].iter().map(|&i| json!({"t":wins[i].0,"box":wins[i].1})).collect();
    let mut out = json!({"clip_id":id,"applied":apply,"ops":ops,"keys_per_property":n+1,"scale_range":[smin,smax],"clamped":clamped,
        "effective_box_at":eff,
        "convention":"window = canvas rectangle of the unzoomed frame that fills the canvas; keys are one per frame, linear between, eased progress sampled with the keyframe evaluator; box size and centre interpolate linearly in px"});
    let mut hints = vec![];
    if pad_a || pad_b { let (ca, which) = (cw / ch, if pad_b { "to_box" } else { "from_box" }); let ba = if pad_b { to[2] / to[3] } else { from[2] / from[3] };
        hints.push(crate::perception::fix_hint::none(&format!("{which} aspect {ba:.3} differs from canvas {ca:.3}; the window was grown to the canvas aspect around the box (pad: extra source shows, nothing is cropped). To crop instead, pass a box with the canvas aspect"))); }
    if let Source::Media { asset } = &c.source { if let Some(a) = ss.project.assets.get(asset) { if a.width > 0 && sw > 0. {
        let ppc = a.width as f64 / sw / smax; out["source_px_per_canvas_px"] = json!(ppc);
        if 1. / ppc > 1.5 { hints.push(crate::perception::fix_hint::hint("set_param", "set_keyframes", json!({"clip_id":id,"property":"scale","keys":[{"t":start+dur,"v":smax * 1.5 * ppc}]}), format!("the end window draws the source at {:.2}x its pixels (lint limit 1.5); a smaller final scale stays sharp but shows more of the frame", 1. / ppc), "estimate")); } } } }
    if !hints.is_empty() { out["fix_hint"] = hints[0].clone(); out["fix_hints"] = json!(hints); }
    if apply { let r = crate::commands::dispatch_nested(ss, "batch", &json!({"ops":ops,"label":"crop_zoom_keyframes","atomic":true}))?; out["batch"] = json!({"applied":r["applied"],"history_label":r["history_label"]}); }
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"crop_zoom_keyframes",mutates:true,description:"Keyframe scale, x and y on a clip so the visible window moves from from_box to to_box (canvas px [x,y,w,h]) over duration. Aspect-preserving: a box with another aspect than the canvas is grown to the canvas aspect around its centre (pad, never crop; fix_hint reports it). clamp (default true) keeps the window inside the clip source. One key per frame via set_keyframes in one batch, easing name or [x1,y1,x2,y2]. apply:false returns the ops only. Reports effective_box_at, scale_range, clamped and a low-resolution hint. No motion blur.",schema:obj(vec![("clip_id",s("clip to zoom")),("from_box",json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[x,y,w,h] canvas px at start"})),("to_box",json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"[x,y,w,h] canvas px at end"})),("duration",n("seconds")),("start",n("clip-local start seconds, default 0")),("easing",json!({"description":"name or [x1,y1,x2,y2]; default in_out"})),("clamp",json!({"type":"boolean"})),("apply",json!({"type":"boolean","description":"default true; false returns ops without changing the project"}))],&["clip_id","from_box","to_box","duration"]),run}]
}
