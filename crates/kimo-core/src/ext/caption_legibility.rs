//! v6 eyes: caption legibility numbers for validate_captions (optional `legibility: true`). Per cue, at its middle frame:
//! the capital-H ink height of its style (as lint_frame's min_text_size measures it, times the clip's scale) as canvas px and
//! as a fraction of the canvas height, the ink box from compute_text_bounds, characters (newlines left out) per second of
//! on-screen time, and the WCAG contrast of the caption colour (at its opacity) against the frame rendered WITHOUT that cue,
//! pixel by pixel inside the ink box (min, p10, mean, worst pixel), as lint_frame's text_contrast samples it. Raw numbers only.
use super::contrast::{ratio, rel_lum};
use super::kit::*;
use crate::model::Source;
use crate::perception::analysis::Image;

pub const MAX_CUES: usize = 500;
fn r4(x: f64) -> f64 { (x * 1e4).round() / 1e4 }

/// Legibility rows for the given cue clip ids (already sorted by start), plus the group's extremes.
pub fn legibility(ss: &mut Session, cue_ids: &[String]) -> Result<Value, String> {
    if cue_ids.len() > MAX_CUES { return Err(format!("too_many_cues: {} cues; legibility measures at most {MAX_CUES} per group", cue_ids.len())); }
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let mut rows = vec![];
    let (mut min_cap, mut max_cps, mut min_con): (Option<(f64, String)>, Option<(f64, String)>, Option<(f64, String)>) = (None, None, None);
    for id in cue_ids {
        crate::guard::check();
        let Some(c) = ss.project.find_clip(id).cloned() else { continue };
        let Source::Text { text, color, .. } = &c.source else { continue };
        let (start, end) = (c.start, c.start + c.duration);
        let t = start + c.duration / 2.0;
        let chars = text.chars().filter(|ch| *ch != '\n' && *ch != '\r').count();
        let lines = text.lines().count().max(1);
        let cps = if c.duration > 0.0 { Some(chars as f64 / c.duration) } else { None };
        let lt = crate::render::clip_local_time(&c, t);
        let xf = crate::render::resolve_xform(&ss.project, &c, lt);
        let cap = super::lint_frame::text_cap(&c, w, h)? * xf.scale.abs();
        let b = crate::commands::dispatch(ss, "compute_text_bounds", &json!({"clip_id": c.id, "t": t}))?;
        let rect: Option<[f64; 4]> = b["canvas_rect"].as_array().filter(|v| v.len() == 4).map(|v| [0, 1, 2, 3].map(|k| v[k].as_f64().unwrap_or(0.0)));
        // contrast: the caption colour at its opacity over the frame without this cue, every pixel of the ink box
        let mut con = Value::Null;
        if let Some(r) = rect {
            let p = super::view::view_of(&ss.project, None, Some(&[c.id.clone()]));
            crate::render::prepare_at(&p, &mut ss.cache, Some(t))?;   // media frames decoded into the session cache first (an empty cache draws media black)
            let bg = crate::render::render_frame(&p, t, &ss.cache)?;
            let img = Image::from_frame(&bg);
            let (x0, y0) = (r[0].floor().max(0.0) as usize, r[1].floor().max(0.0) as usize);
            let (x1, y1) = (((r[0] + r[2]).ceil().max(0.0) as usize).min(img.w), ((r[1] + r[3]).ceil().max(0.0) as usize).min(img.h));
            let op = xf.opacity.clamp(0.0, 1.0);
            let mut rs = vec![]; let mut worst = [0usize; 2]; let mut mn = f64::INFINITY;
            for y in y0.min(img.h)..y1 { for x in x0.min(img.w)..x1 {
                let i = y * img.w + x; let bpx = [img.rgb[0][i], img.rgb[1][i], img.rgb[2][i]];
                let fg = [0, 1, 2].map(|k| op * color[k] + (1.0 - op) * bpx[k]);
                let q = ratio(rel_lum(fg, None), rel_lum(bpx, None)); if q < mn { mn = q; worst = [x, y]; } rs.push(q); } }
            if !rs.is_empty() {
                rs.sort_by(f64::total_cmp); let mean = rs.iter().sum::<f64>() / rs.len() as f64;
                con = json!({"min_ratio": r4(mn), "p10_ratio": r4(rs[(rs.len() - 1) / 10]), "mean_ratio": r4(mean), "worst_px": worst, "pixels": rs.len()});
                if min_con.as_ref().map_or(true, |m| mn < m.0) { min_con = Some((mn, id.clone())); }
            }
        }
        if cap > 0.0 && min_cap.as_ref().map_or(true, |m| cap / h < m.0) { min_cap = Some((cap / h, id.clone())); }
        if let Some(v) = cps { if max_cps.as_ref().map_or(true, |m| v > m.0) { max_cps = Some((v, id.clone())); } }
        rows.push(json!({"cue_id": id, "start": start, "end": end, "duration_s": r4(c.duration), "t_measured": r4(t), "chars": chars, "lines": lines,
            "cps": cps.map(r4), "cap_px": r4(cap), "cap_frac": r4(cap / h), "ink_rect": rect.map(|r| r.map(r4)), "ink_h_frac": rect.map(|r| r4(r[3] / h)),
            "opacity": r4(xf.opacity), "contrast": con}));
    }
    let ext = |m: Option<(f64, String)>| m.map_or(Value::Null, |(v, id)| json!({"value": r4(v), "cue_id": id}));
    Ok(json!({"canvas": [w, h], "measured_at": "each cue's middle (start + duration / 2)", "cues": rows,
        "min_cap_frac": ext(min_cap), "max_cps": ext(max_cps), "min_contrast": ext(min_con)}))
}

/// Top-level summary over every group's legibility block, flat scalars only (the spectator stream keeps two levels of
/// nesting, so the GUI reads these): the smallest cap_frac, the lowest min contrast and the highest cps, each with its cue.
pub fn summary(groups: &[Value]) -> Value {
    let mut out = json!({"canvas_h": Value::Null, "min_cap_frac": Value::Null, "min_cap_frac_cue": Value::Null, "min_cap_px": Value::Null,
        "min_contrast": Value::Null, "min_contrast_cue": Value::Null, "max_cps": Value::Null, "max_cps_cue": Value::Null});
    for g in groups {
        let l = &g["legibility"]; if !l.is_object() { continue; }
        out["canvas_h"] = l["canvas"][1].clone();
        let better = |cur: &Value, v: f64, lower: bool| cur.as_f64().map_or(true, |c| if lower { v < c } else { v > c });
        if let Some(v) = l["min_cap_frac"]["value"].as_f64() { if better(&out["min_cap_frac"], v, true) { out["min_cap_frac"] = json!(v); out["min_cap_frac_cue"] = l["min_cap_frac"]["cue_id"].clone();
            out["min_cap_px"] = l["cues"].as_array().and_then(|c| c.iter().find(|r| r["cue_id"] == l["min_cap_frac"]["cue_id"])).map_or(Value::Null, |r| r["cap_px"].clone()); } }
        if let Some(v) = l["min_contrast"]["value"].as_f64() { if better(&out["min_contrast"], v, true) { out["min_contrast"] = json!(v); out["min_contrast_cue"] = l["min_contrast"]["cue_id"].clone(); } }
        if let Some(v) = l["max_cps"]["value"].as_f64() { if better(&out["max_cps"], v, false) { out["max_cps"] = json!(v); out["max_cps_cue"] = l["max_cps"]["cue_id"].clone(); } }
    }
    out
}

pub fn commands() -> Vec<Command> { vec![] }
