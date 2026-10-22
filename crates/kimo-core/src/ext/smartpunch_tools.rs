//! add_smart_punch: the automatic subject-follow punch-in. Samples `samples` times evenly across a clip (first to last frame),
//! renders the composited output frame at each, and finds where the eye goes with saliency_map's spectral residual saliency
//! (Hou & Zhang 2007, same grid size and blur as saliency_map's defaults), optionally mixed with the luma change to the next
//! project frame (`motion`, as saliency_map does). The mass centroid of each map is the focus. Every frame is measured
//! before anything is keyed, so all samples see the original, unzoomed picture (flat frame: canvas centre).
//! Then scale / x / y are keyed at the sample times: scale ramps 1 -> zoom over the clip, and the position zooms about that
//! sample's focus (x -> k x + (1 - k)(focus - centre)), so the focus stays put while the picture pushes in and never shows an
//! edge. The clip's own scale / x / y keys are folded in (sampled at the same times, then replaced); expressions stay on top.
use super::kit::*;
use crate::ext::punchin_tools::focus_offset;
use crate::ext::saliency_map::{area_down, blur, luma_of, spectral_residual, SParams};
use crate::keyframe::{Ease, Prop};

const ARGS: &[&str] = &["clip_id", "zoom", "samples", "motion"];
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(x) } } } }
fn r1(x: f64) -> f64 { (x * 10.0).round() / 10.0 }
fn scale_max(v: &mut [f64]) { let mx = v.iter().cloned().fold(0.0f64, f64::max); if mx > 0.0 { for x in v.iter_mut() { *x /= mx; } } }

/// Saliency mass centroid (canvas px) of the output frame at timeline time t; the canvas centre for a flat frame.
fn focus_at(ss: &mut Session, t: f64, motion: f64) -> Result<(f64, f64), String> {
    let (w, h) = (ss.project.width as usize, ss.project.height as usize); let q = SParams::default();
    let long = w.max(h).max(1); let (ww, wh) = (((w * q.work_px) as f64 / long as f64).round().max(1.0) as usize, ((h * q.work_px) as f64 / long as f64).round().max(1.0) as usize);
    crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t))?;   // media decoded into the session cache first (an empty cache draws media black)
    let l0 = luma_of(&crate::render::render_frame(&ss.project, t, &ss.cache)?);
    let spec = spectral_residual(&area_down(&l0, w, h, ww, wh), ww, wh, q.blur_px); let mut s = spec.clone();
    if motion > 0.0 {
        let t1 = t + 1.0 / ss.project.fps.max(1e-6);
        crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t1))?; let l1 = luma_of(&crate::render::render_frame(&ss.project, t1, &ss.cache)?);
        let d: Vec<f64> = l0.iter().zip(&l1).map(|(a, b)| (a - b).abs()).collect(); let mut m = blur(&area_down(&d, w, h, ww, wh), ww, wh, q.blur_px); scale_max(&mut m);
        for i in 0..s.len() { s[i] = (1.0 - motion) * spec[i] + motion * m[i]; } scale_max(&mut s);
    }
    let (cw, ch) = (w as f64 / ww as f64, h as f64 / wh as f64); let total: f64 = s.iter().sum();
    if !(total > 1e-9) { return Ok((w as f64 / 2.0, h as f64 / 2.0)); }
    Ok((s.iter().enumerate().map(|(i, v)| v * ((i % ww) as f64 + 0.5) * cw).sum::<f64>() / total, s.iter().enumerate().map(|(i, v)| v * ((i / ww) as f64 + 0.5) * ch).sum::<f64>() / total))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_smart_punch", description: "Automatic subject-follow punch-in: a continuous push-in zoom on a clip, centred where its own frames' saliency says the eye goes. Samples the composited output across the clip (spectral residual saliency, as saliency_map; motion mixes in frame-to-frame change), then keys scale 1 -> zoom and x / y so each sample's focus point stays put; no manual tracking or focus point needed.", mutates: true,
        schema: obj(vec![("clip_id", s("clip to animate")), ("zoom", n("target scale at clip end, default 1.3, 1.05..3.0")),
            ("samples", json!({"type":"integer","description":"analysis points across the clip, default 6, 2..20"})),
            ("motion", n("weight of the frame-to-frame luma change in the saliency (favours a moving subject), default 0.2, 0..1"))], &["clip_id"]),
        run: |ss, a| { let o = a.as_object().ok_or("add_smart_punch: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_smart_punch: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let id = str_arg(a, "clip_id")?.to_string();
            let zoom = num(a, "zoom", 1.3, 1.05, 3.0)?; let motion = num(a, "motion", 0.2, 0.0, 1.0)?;
            let n = match a.get("samples") { None => 6, Some(v) => v.as_u64().filter(|x| (2..=20).contains(x)).ok_or("samples must be a whole number within 2..20")? as usize };
            let c = ss.project.find_clip(&id).ok_or_else(|| format!("no such clip {id}"))?.clone();
            if !(c.duration > 0.0) { return Err(format!("clip {id} has no duration")); }
            let (w, h) = (ss.project.width as f64, ss.project.height as f64); let fr = 1.0 / ss.project.fps.max(1e-6);
            // key times span the whole clip; the last frame shown is one frame before its end, so that is where the end is measured
            let lts: Vec<f64> = (0..n).map(|i| c.duration * i as f64 / (n - 1) as f64).collect();
            let mut foci = Vec::with_capacity(n);
            for lt in &lts { foci.push(focus_at(ss, c.start + lt.min((c.duration - fr).max(0.0)), motion)?); }
            let scales: Vec<f64> = (0..n).map(|i| 1.0 + (zoom - 1.0) * i as f64 / (n - 1) as f64).collect();
            let cm = ss.project.find_clip_mut(&id).unwrap();
            for (key, rest) in [("scale", 1.0), ("x", 0.0), ("y", 0.0)] {
                let q = cm.props.entry(key.into()).or_insert_with(|| Prop::new(rest)); let own: Vec<f64> = lts.iter().map(|lt| q.raw(*lt)).collect(); q.keys.clear();
                for i in 0..n { let k = scales[i]; let (ox, oy) = focus_offset(foci[i].0, foci[i].1, w / 2.0, h / 2.0, k);
                    let v = match key { "scale" => own[i] * k, "x" => k * own[i] + ox, _ => k * own[i] + oy }; q.set_key(lts[i], v, Ease::InOut); } }
            Ok(json!({"clip_id": id, "samples": n, "zoom": zoom, "motion": motion, "times": lts.iter().map(|lt| c.start + lt).collect::<Vec<_>>(),
                "centroids": foci.iter().map(|(x, y)| json!([r1(*x), r1(*y)])).collect::<Vec<_>>(), "scales": scales})) } },
] }
