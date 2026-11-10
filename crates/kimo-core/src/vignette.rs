//! Shaped + tracked vignette (the `vignette` effect's extended params). With none of NEW_PARAMS set and no strings the effect keeps its
//! original formula bit-exactly (effects.rs). Otherwise, for a pixel (x, y) in canvas px and centre C = (cx, cy) px:
//!   frame-shaped half axes  (w/2*sqrt2, h/2*sqrt2)  (the original: r = 1 at the frame corner), circle half axes (D, D), D = sqrt((w/2)^2 + (h/2)^2),
//!   (ax, ay) = lerp(frame, circle, roundness) * size, ax *= aspect;  u = (x - cx)/ax, v = (y - cy)/ay,
//!   r = (|u|^q + |v|^q)^(1/q), q = 2 + 6*squareness (2 ellipse .. 8 rounded rectangle),
//!   k = amount * min(1, (max(0, r - (1 - feather)) / feather)^2), out = rgb*(1 - k) + color*k*alpha (color 0,0,0 = the original darkening).
//! Centre, first match: strings.source_clip = a clip moved by track_point (or keyed): C = (center_x*w + X(lt) - X.base, center_y*h + Y(lt) - Y.base);
//! strings.source_mask = a mask layer id (e.g. one track_mask keyed): C = its centre at the owner clip's local time (rect/ellipse/radial: cx, cy;
//! path: the mean of its points), in canvas px; else (center_x*w, center_y*h). The renderer injects C as _src_x/_src_y (render::resolve_flares).
use crate::effects::{p, Frame, Params};

pub const NEW_PARAMS: &[&str] = &["center_x", "center_y", "roundness", "squareness", "size", "aspect", "feather", "color_r", "color_g", "color_b"];

/// true when the call uses any extended param or a centre source, so the original formula is left untouched otherwise.
pub fn extended(params: &Params) -> bool { params.keys().any(|k| NEW_PARAMS.contains(&k.as_str()) || k == "_src_x") }

pub fn needs_source(strings: &std::collections::BTreeMap<String, String>) -> bool { strings.contains_key("source_clip") || strings.contains_key("source_mask") }

/// Centre in canvas px at timeline time t (lt = the effect owner's local time). Err names the missing clip / mask.
pub fn source_px(pj: &crate::model::Project, pr: &Params, strings: &std::collections::BTreeMap<String, String>, t: f64, lt: f64) -> Result<(f64, f64), String> {
    let (w, h) = (pj.width as f64, pj.height as f64);
    let (x, y) = (pr.get("center_x").map(|q| q.at(lt)).unwrap_or(0.5), pr.get("center_y").map(|q| q.at(lt)).unwrap_or(0.5));
    if let Some(id) = strings.get("source_clip") {
        let c = pj.tracks.iter().flat_map(|tr| tr.clips.iter()).find(|c| &c.id == id).ok_or_else(|| format!("vignette: no clip '{id}' for source_clip"))?; let ct = t - c.start;
        let d = |k: &str| c.props.get(k).map(|q| q.at(ct) - q.base).unwrap_or(0.0); return Ok((x * w + d("x"), y * h + d("y"))); }
    if let Some(id) = strings.get("source_mask") {
        for c in pj.tracks.iter().flat_map(|tr| tr.clips.iter()) { let Some(m) = &c.mask else { continue }; let Some(l) = m.layers.iter().find(|l| &l.id == id) else { continue };
            let ct = t - c.start; let g = |k: &str, d: f64| l.props.get(k).map(|q| q.at(ct)).unwrap_or(d);
            let (mx, my) = match l.kind.as_str() {
                "path" => { let pts = crate::masks::path_at(l, ct); if pts.is_empty() { return Err(format!("vignette: mask '{id}' has no points")); } let n = pts.len() as f64; (pts.iter().map(|q| q.x).sum::<f64>() / n, pts.iter().map(|q| q.y).sum::<f64>() / n) }
                "rect" | "ellipse" | "radial" => (g("cx", 0.5), g("cy", 0.5)),
                k => return Err(format!("vignette: mask '{id}' is a {k} mask (no centre); use a rect, ellipse, radial or path mask")) };
            return Ok((mx * w, my * h)); }
        return Err(format!("vignette: no mask layer '{id}' for source_mask")); }
    Ok((x * w, y * h))
}

pub fn apply(params: &Params, f: &mut Frame, t: f64) {
    let a = p(params, "amount", 0.5, t);
    let fe = if params.contains_key("feather") { p(params, "feather", 0.5, t) } else { p(params, "softness", 0.5, t) }.clamp(0.05, 1.0);
    let (w, h) = (f.w as f32, f.h as f32);
    let cx = params.get("_src_x").map(|q| q.at(t) as f32).unwrap_or(p(params, "center_x", 0.5, t) * w);
    let cy = params.get("_src_y").map(|q| q.at(t) as f32).unwrap_or(p(params, "center_y", 0.5, t) * h);
    let ro = p(params, "roundness", 0.0, t).clamp(0.0, 1.0); let sq = p(params, "squareness", 0.0, t).clamp(0.0, 1.0);
    let size = p(params, "size", 1.0, t).max(0.01); let asp = p(params, "aspect", 1.0, t).clamp(0.05, 20.0);
    let col = [p(params, "color_r", 0.0, t), p(params, "color_g", 0.0, t), p(params, "color_b", 0.0, t)];
    let d = ((w * 0.5).powi(2) + (h * 0.5).powi(2)).sqrt(); let s2 = std::f32::consts::SQRT_2;
    let ax = ((w * 0.5 * s2) * (1.0 - ro) + d * ro) * size * asp; let ay = ((h * 0.5 * s2) * (1.0 - ro) + d * ro) * size;
    let q = 2.0 + 6.0 * sq;
    for y in 0..f.h { for x in 0..f.w {
        let u = ((x as f32 - cx) / ax).abs(); let v = ((y as f32 - cy) / ay).abs();
        let r = if sq == 0.0 { (u * u + v * v).sqrt() } else { (u.powf(q) + v.powf(q)).powf(1.0 / q) };
        let k = a * ((r - (1.0 - fe)).max(0.0) / fe).powi(2).min(1.0);
        let i = (y * f.w + x) * 4; let al = f.d[i + 3]; for j in 0..3 { f.d[i + j] = f.d[i + j] * (1.0 - k) + col[j] * k * al; } } }
}
