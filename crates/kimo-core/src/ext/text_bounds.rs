//! RV-28 C2: compute_text_bounds - the renderer's own text layout as read-only numbers.
use super::kit::*;
use crate::commands::Session;
use crate::model::{Source, TextFx};
use crate::text::{measure_text, TextMetrics};

fn metrics_json(m: &TextMetrics, grow: f64) -> Value {
    json!({"width": m.width, "height": m.height, "line_count": m.lines.len(),
        "lines": m.lines.iter().map(|l| json!({"text": l.text, "x": l.x, "y": l.y, "w": l.w, "h": l.h, "baseline": l.baseline})).collect::<Vec<_>>(),
        "ink": m.ink.map(|i| json!([i[0] - grow, i[1] - grow, i[2] + grow, i[3] + grow])),
        "font": {"file": m.font_file, "substituted": m.substituted}})
}
fn opt(a: &Value, k: &str) -> Option<f64> { a.get(k).and_then(|v| v.as_f64()) }
fn compute_text_bounds(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let mut warnings: Vec<String> = vec![];
    if let Some(cid) = a.get("clip_id").and_then(|v| v.as_str()) {
        let cid = super::names::resolve_ref(&ss.project, "clip", cid)?;
        let (m, grow, font) = { let c = ss.project.find_clip(&cid).ok_or(format!("no clip '{cid}'"))?;
            let Source::Text { text, font, size, letter_spacing, stroke_width, align, .. } = &c.source else { return Err("not_text_clip: compute_text_bounds needs a text clip".into()) };
            let fx = c.text_fx.clone().unwrap_or_default(); let weight = fx.weight;
            (measure_text(text, font, *size, *letter_spacing, weight, align, cw, ch, Some(&fx))?, stroke_width.max(0.0).ceil(), font.clone()) };
        let mut out = metrics_json(&m, grow);
        let mut targ = json!({"clip_id": cid, "space": "canvas_px"}); if let Some(t) = a.get("t") { targ["t"] = t.clone(); }
        let tr = super::xform_tools::get_transform(ss, &targ)?;
        if let (Some(i), Some(mx)) = (m.ink, tr.get("matrix").and_then(|v| v.as_array())) {
            let g = |r: usize, k: usize| mx[r][k].as_f64().unwrap_or(0.0);
            let corners: Vec<[f64; 2]> = [(i[0] - grow, i[1] - grow), (i[2] + grow, i[1] - grow), (i[2] + grow, i[3] + grow), (i[0] - grow, i[3] + grow)].iter().map(|(px, py)| [g(0, 0) * px + g(0, 1) * py + g(0, 3), g(1, 0) * px + g(1, 1) * py + g(1, 3)]).collect();
            let (x0, x1) = (corners.iter().map(|c| c[0]).fold(f64::MAX, f64::min), corners.iter().map(|c| c[0]).fold(f64::MIN, f64::max)); let (y0, y1) = (corners.iter().map(|c| c[1]).fold(f64::MAX, f64::min), corners.iter().map(|c| c[1]).fold(f64::MIN, f64::max));
            out["canvas_quad"] = json!(corners); out["canvas_rect"] = json!([x0, y0, x1 - x0, y1 - y0]);
        }
        if m.substituted { warnings.push(format!("font_substituted: '{font}' was not found; measured with {}", m.font_file)); }
        out["clip_id"] = json!(cid); out["t"] = tr["t"].clone(); out["canvas"] = json!({"width": cw, "height": ch}); out["warnings"] = json!(warnings);
        out["note"] = json!("read-only, nothing is moved. ink excludes shadow, glow and background box; canvas_quad includes the clip transform at t");
        return Ok(out);
    }
    let text = str_arg(a, "text")?; let size = num_arg(a, "size")?;
    if !size.is_finite() || size <= 0.0 || size > 2000.0 { return Err("out_of_range: size must be > 0 and <= 2000".into()); }
    let font = a.get("font").and_then(|v| v.as_str()).unwrap_or("Montserrat"); let align = a.get("align").and_then(|v| v.as_str()).unwrap_or("center");
    if !["left", "center", "right"].contains(&align) { return Err("out_of_range: align is not allowed. Valid: left, center, right".into()); }
    let mut fx = TextFx::default(); if let Some(w) = opt(a, "weight") { fx.weight = w; } if let Some(w) = opt(a, "max_width") { if w < 0.0 || !w.is_finite() { return Err("out_of_range: max_width must be >= 0".into()); } fx.box_width = w; } if let Some(l) = opt(a, "line_height") { if !(l > 0.0 && l.is_finite()) { return Err("out_of_range: line_height must be > 0".into()); } fx.line_spacing = l; }
    if let Some(u) = a.get("uppercase").and_then(|v| v.as_bool()) { fx.uppercase = u; }
    let (w, h) = (opt(a, "canvas_width").unwrap_or(cw), opt(a, "canvas_height").unwrap_or(ch));
    let m = measure_text(text, font, size, opt(a, "letter_spacing").unwrap_or(0.0), fx.weight, align, w, h, Some(&fx))?;
    let grow = opt(a, "stroke_width").unwrap_or(0.0).max(0.0).ceil();
    let (dx, dy) = (opt(a, "x").unwrap_or(0.0), opt(a, "y").unwrap_or(0.0));
    let mut out = metrics_json(&m, grow);
    if let Some(i) = m.ink { out["canvas_rect"] = json!([i[0] - grow + dx, i[1] - grow + dy, i[2] - i[0] + 2.0 * grow, i[3] - i[1] + 2.0 * grow]); }
    if m.substituted { warnings.push(format!("font_substituted: '{font}' was not found; measured with {}", m.font_file)); }
    out["canvas"] = json!({"width": w, "height": h}); out["warnings"] = json!(warnings);
    out["note"] = json!("read-only, nothing is moved. Block centred on the canvas, then shifted by x,y like a clip's x/y; ink excludes shadow, glow and background box");
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "compute_text_bounds", mutates: false, description: "Read-only, nothing is moved: measure text exactly as the renderer lays it out. Pass clip_id (id or @name of a text clip, optional t seconds) to measure that clip with its own style, including its transform at t (canvas_quad, canvas_rect), or pass text and size (plus font, weight, letter_spacing, line_height, max_width, align, stroke_width, x, y, canvas_width, canvas_height) to measure style fields without a clip. Returns width, height, per-line text/x/y/w/h/baseline, ink box [x0,y0,x1,y1] in canvas pixels, the font file used and whether it was substituted (also listed in warnings).",
        schema: obj(vec![("clip_id", s("text clip id or @name")), ("t", json!({"type":"number"})), ("text", s("text to measure")), ("font", s("family name or file path")), ("size", json!({"type":"number"})), ("weight", json!({"type":"number"})), ("letter_spacing", json!({"type":"number"})), ("line_height", json!({"type":"number"})), ("max_width", json!({"type":"number"})), ("align", json!({"type":"string","enum":["left","center","right"]})), ("stroke_width", json!({"type":"number"})), ("uppercase", json!({"type":"boolean"})), ("x", json!({"type":"number"})), ("y", json!({"type":"number"})), ("canvas_width", json!({"type":"number"})), ("canvas_height", json!({"type":"number"}))], &[]), run: compute_text_bounds },
] }
