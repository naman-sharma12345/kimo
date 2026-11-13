//! Engine eyes, layer 6: `detect`. Renders the composited frame at a time and finds faces, people, objects and the main
//! subject with the on-device vision models (`crate::vision`; downloaded on first use, SHA-512 pinned). Boxes are in
//! canvas pixels [x, y, w, h], top-left origin. When a model is unavailable (offline, refused download, or a build
//! without the `vision` feature) the result says so in `fallback`, and the subject comes from the saliency map instead.
use super::kit::*;
use crate::vision::{self, BBox, FrameKey, Mask};

const WHATS: [&str; 5] = ["faces", "people", "objects", "subject", "matte"];
fn r1(x: f32) -> f64 { ((x as f64) * 10.0).round() / 10.0 }
fn r3(x: f32) -> f64 { ((x as f64) * 1000.0).round() / 1000.0 }
fn bx(b: &BBox) -> Value { json!([r1(b.x), r1(b.y), r1(b.w), r1(b.h)]) }

fn mask_json(m: &Mask, thr: f32) -> Value {
    json!({"box": m.bbox(thr).map(|b| bx(&b)), "area_frac": r3(m.area_frac(thr)), "centroid": m.centroid().map(|c| json!([r1(c[0]), r1(c[1])])), "threshold": thr, "model": m.model, "grid": [m.w, m.h]})
}

fn draw_rect(f: &mut crate::effects::Frame, b: &BBox, c: [f32; 3], t: usize) {
    let (x0, y0) = (b.x.max(0.0) as usize, b.y.max(0.0) as usize); let (x1, y1) = (((b.x + b.w) as usize).min(f.w.saturating_sub(1)), ((b.y + b.h) as usize).min(f.h.saturating_sub(1)));
    if x0 >= x1 || y0 >= y1 { return; }
    let mut put = |x: usize, y: usize| { let i = (y * f.w + x) * 4; f.d[i] = c[0]; f.d[i + 1] = c[1]; f.d[i + 2] = c[2]; f.d[i + 3] = 1.0; };
    for k in 0..t { for x in x0..=x1 { put(x, (y0 + k).min(y1)); put(x, y1.saturating_sub(k).max(y0)); } for y in y0..=y1 { put((x0 + k).min(x1), y); put(x1.saturating_sub(k).max(x0), y); } }
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = a.get("time").and_then(|v| v.as_f64()).filter(|t| t.is_finite() && *t >= 0.0).ok_or("out_of_range: time must be a number of timeline seconds >= 0")?;
    let what: Vec<String> = match a.get("what") {
        None | Some(Value::Null) => vec!["faces".into(), "people".into(), "objects".into(), "subject".into()],
        Some(Value::Array(v)) if !v.is_empty() => v.iter().map(|x| x.as_str().filter(|s| WHATS.contains(s)).map(String::from).ok_or_else(|| format!("out_of_range: what holds names from {WHATS:?}"))).collect::<Result<_, _>>()?,
        Some(Value::String(s)) if WHATS.contains(&s.as_str()) => vec![s.clone()],
        _ => return Err(format!("out_of_range: what must be a non-empty array of {WHATS:?}")),
    };
    let path = match a.get("path") { None | Some(Value::Null) => None, Some(Value::String(s)) if !s.is_empty() => Some(s.clone()), _ => return Err("out_of_range: path must be a non-empty string (png)".into()) };
    let thr = match a.get("mask_threshold") { None | Some(Value::Null) => 0.5, Some(v) => v.as_f64().filter(|v| v.is_finite() && *v > 0.0 && *v < 1.0).ok_or("out_of_range: mask_threshold must be a number between 0 and 1")? as f32 };
    let has = |k: &str| what.iter().any(|w| w == k);

    crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t))?;
    let f = crate::render::render_frame(&ss.project, t, &ss.cache)?;
    let (w, h) = (f.w, f.h);
    // keyed by the composited pixels (any edit that changes the picture is a new key) and the frame number
    let key = FrameKey::new(format!("comp:{:016x}", vision::pixel_hash(&f)), (t * ss.project.fps.max(1e-6)).round() as i64);
    let mut out = json!({"time": t, "canvas": [w, h], "box_format": "[x, y, w, h] canvas px, top-left origin", "models": {}});
    let mut notes: Vec<String> = vec![];
    let mut overlay: Vec<(BBox, [f32; 3])> = vec![]; let mut tint: Vec<(std::sync::Arc<Mask>, [f32; 3])> = vec![];

    if has("faces") {
        let s = vision::detect_faces(&f, Some(key.clone()));
        out["faces"] = json!(s.value.iter().map(|fb| { overlay.push((fb.bbox, [0.1, 1.0, 0.2]));
            let p = |i: usize| json!([r1(fb.landmarks[i][0]), r1(fb.landmarks[i][1])]);
            json!({"box": bx(&fb.bbox), "score": r3(fb.score), "landmarks": {"right_eye": p(0), "left_eye": p(1), "nose": p(2), "mouth_right": p(3), "mouth_left": p(4)}}) }).collect::<Vec<_>>());
        out["models"]["faces"] = json!(s.model); if let Some(n) = s.fallback { notes.push(n); }
    }
    if has("people") || has("objects") {
        let s = vision::detect_objects(&f, Some(key.clone()));
        let det = |d: &vision::Detection| json!({"box": bx(&d.bbox), "class": d.class, "score": r3(d.score)});
        if has("people") { out["people"] = json!(s.value.iter().filter(|d| d.class_id == 0).map(|d| { overlay.push((d.bbox, [1.0, 0.55, 0.0])); det(d) }).collect::<Vec<_>>()); out["models"]["people"] = json!(s.model); }
        if has("objects") { out["objects"] = json!(s.value.iter().filter(|d| d.class_id != 0).map(|d| { overlay.push((d.bbox, [0.2, 0.6, 1.0])); det(d) }).collect::<Vec<_>>()); out["models"]["objects"] = json!(s.model); }
        if let Some(n) = s.fallback { notes.push(n); }
    }
    if has("subject") {
        let s = vision::detect_subject(&f, Some(key.clone()));
        out["subject"] = mask_json(&s.value, thr); out["models"]["subject"] = json!(s.model);
        if let Some(b) = s.value.bbox(thr) { overlay.push((b, [1.0, 0.2, 0.2])); } tint.push((s.value.clone(), [1.0, 0.2, 0.2]));
        if let Some(n) = s.fallback { notes.push(n); }
    }
    if has("matte") {
        let s = vision::detect_matte(&f, Some(key.clone()));
        out["matte"] = mask_json(&s.value, thr); out["models"]["matte"] = json!(s.model);
        tint.push((s.value.clone(), [0.7, 0.2, 1.0])); if let Some(n) = s.fallback { notes.push(n); }
    }
    out["fallback"] = if notes.is_empty() { Value::Null } else { json!(notes) };

    if let Some(p) = path {
        let mut fr = crate::effects::Frame { w: f.w, h: f.h, d: f.d.clone() };
        for (m, c) in &tint { for y in 0..h { for x in 0..w { let v = m.at(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0) * 0.45; let i = (y * w + x) * 4;
            for k in 0..3 { fr.d[i + k] = fr.d[i + k] * (1.0 - v) + c[k] * v; } } } }
        let th = (w.max(h) / 400).max(2);
        for (b, c) in &overlay { draw_rect(&mut fr, b, *c, th); }
        crate::render::write_image(&fr, &p)?; out["path"] = json!(p);
    }
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![Command { name: "detect", mutates: false,
    description: "Find faces, people, objects and the main subject in the composited frame at `time`, with on-device vision models (downloaded once on first use, a few MB each). what: any of faces (box, score, eyes/nose/mouth landmarks), people, objects (COCO classes), subject (salient-subject mask: box, area_frac, centroid), matte (person matte, larger model fetched only when asked); default faces, people, objects, subject. Boxes [x,y,w,h] in canvas px. path writes an annotated png (faces green, people orange, objects blue, subject red). Offline, `fallback` says what could not run and the subject comes from the saliency map.",
    schema: obj(vec![
        ("time", n("timeline seconds")),
        ("what", json!({"type": "array", "items": {"type": "string", "enum": WHATS}, "description": "what to find, default [faces, people, objects, subject]"})),
        ("mask_threshold", json!({"type": "number", "minimum": 0, "maximum": 1, "description": "subject/matte level counted as subject, default 0.5"})),
        ("path", s("optional annotated png")),
    ], &["time"]), run }] }
