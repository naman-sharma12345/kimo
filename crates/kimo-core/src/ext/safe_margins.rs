//! RV-28 C3: query_safe_margins - a data table of documented safe areas and estimated platform UI zones. Never enforcement.
use super::kit::*;
use crate::commands::Session;

pub const PROFILES: [&str; 7] = ["title_safe", "action_safe", "tiktok", "reels", "shorts", "youtube", "custom"];
const AS_OF: &str = "2026-10-13";
const EST: &str = "estimate of the platform UI overlay, not published by the platform; check the current app before relying on it";
/// (name, kind, [x, y, w, h] as fractions of the canvas, notes)
type Z = (&'static str, &'static str, [f64; 4], &'static str);
fn table(profile: &str) -> Option<(&'static str, Vec<Z>)> {
    Some(match profile {
        "title_safe" => ("SMPTE-style title safe: 90% of width and height, centred (5% inset each side)", vec![("title_safe", "safe", [0.05, 0.05, 0.90, 0.90], "keep titles and lower thirds inside")]),
        "action_safe" => ("SMPTE-style action safe: 93% of width and height, centred (3.5% inset each side)", vec![("action_safe", "safe", [0.035, 0.035, 0.93, 0.93], "keep important action inside")]),
        "tiktok" => (EST, vec![("safe", "safe", [0.06, 0.10, 0.76, 0.62], "estimated area clear of UI"), ("top_bar", "avoid", [0.0, 0.0, 1.0, 0.10], "search / following tabs"), ("caption_and_sound_bar", "avoid", [0.0, 0.72, 0.82, 0.28], "username, caption, sound ticker"), ("action_rail", "avoid", [0.84, 0.40, 0.16, 0.60], "like, comment, share, profile buttons")]),
        "reels" => (EST, vec![("safe", "safe", [0.06, 0.12, 0.78, 0.60], "estimated area clear of UI"), ("top_bar", "avoid", [0.0, 0.0, 1.0, 0.12], "camera and title bar"), ("caption_bar", "avoid", [0.0, 0.74, 0.84, 0.26], "account, caption, audio"), ("action_rail", "avoid", [0.86, 0.45, 0.14, 0.55], "like, comment, share buttons")]),
        "shorts" => (EST, vec![("safe", "safe", [0.06, 0.10, 0.78, 0.66], "estimated area clear of UI"), ("top_bar", "avoid", [0.0, 0.0, 1.0, 0.10], "search and menu"), ("caption_bar", "avoid", [0.0, 0.78, 0.82, 0.22], "channel, title, subscribe"), ("action_rail", "avoid", [0.86, 0.45, 0.14, 0.55], "like, dislike, comment, share buttons")]),
        "youtube" => (EST, vec![("safe", "safe", [0.05, 0.05, 0.90, 0.80], "estimated area clear of player controls"), ("player_controls", "avoid", [0.0, 0.88, 1.0, 0.12], "progress bar and controls when visible"), ("end_screen_corner", "avoid", [0.72, 0.60, 0.28, 0.28], "end-screen cards and watermark")]),
        _ => return None,
    })
}
fn num_field(a: &Value, k: &str) -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0 && *x <= 16384.0).map(Some).ok_or(format!("out_of_range: {k} must be a number > 0 and <= 16384")) } }
fn query_safe_margins(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let profile = a.get("profile").and_then(|v| v.as_str()).unwrap_or("title_safe");
    if !PROFILES.contains(&profile) { return Err(format!("invalid_profile: '{profile}' is not allowed. Valid: {}", PROFILES.join(", "))); }
    let (pw, ph) = (ss.project.width as f64, ss.project.height as f64);
    let (mut w, mut h) = (num_field(a, "width")?, num_field(a, "height")?);
    if let Some(asp) = a.get("aspect").and_then(|v| v.as_str()) {
        let (x, y) = asp.split_once(':').and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))).filter(|(x, y)| *x > 0.0 && *y > 0.0 && x.is_finite() && y.is_finite()).ok_or("out_of_range: aspect must look like 16:9")?;
        match (w, h) { (Some(_), Some(_)) => return Err("out_of_range: give aspect with at most one of width, height".into()), (Some(wv), None) => h = Some((wv * y / x).round()), (None, Some(hv)) => w = Some((hv * x / y).round()), (None, None) => { w = Some(pw); h = Some((pw * y / x).round()); } }
    }
    let (w, h) = (w.unwrap_or(pw), h.unwrap_or(ph));
    let r = |f: [f64; 4]| json!([(f[0] * w * 100.0).round() / 100.0, (f[1] * h * 100.0).round() / 100.0, (f[2] * w * 100.0).round() / 100.0, (f[3] * h * 100.0).round() / 100.0]);
    let mut zones = vec![]; let (note, source);
    if profile == "custom" {
        let zs = a.get("zones").and_then(|v| v.as_array()).filter(|z| !z.is_empty() && z.len() <= 64).ok_or("out_of_range: profile custom needs zones: 1..64 of {name, rect:[x,y,w,h] pixels, kind: safe|avoid, notes?}")?;
        for (i, z) in zs.iter().enumerate() {
            let name = z.get("name").and_then(|v| v.as_str()).filter(|n| !n.is_empty() && n.len() <= 64).ok_or(format!("out_of_range: zones[{i}].name must be 1..64 characters"))?;
            let kind = z.get("kind").and_then(|v| v.as_str()).unwrap_or("avoid"); if !["safe", "avoid"].contains(&kind) { return Err(format!("out_of_range: zones[{i}].kind is not allowed. Valid: safe, avoid")); }
            let rc: Vec<f64> = z.get("rect").and_then(|v| v.as_array()).filter(|v| v.len() == 4).map(|v| v.iter().filter_map(|x| x.as_f64().filter(|f| f.is_finite())).collect()).filter(|v: &Vec<f64>| v.len() == 4).ok_or(format!("out_of_range: zones[{i}].rect must be [x,y,w,h] pixels"))?;
            if rc[2] <= 0.0 || rc[3] <= 0.0 || rc[0] < 0.0 || rc[1] < 0.0 || rc[0] + rc[2] > w + 1e-6 || rc[1] + rc[3] > h + 1e-6 { return Err(format!("out_of_range: zones[{i}].rect must lie inside the {w}x{h} canvas")); }
            zones.push(json!({"name": name, "rect": rc, "kind": kind, "notes": z.get("notes").and_then(|v| v.as_str()).unwrap_or("")}));
        }
        note = "user-supplied zones, echoed and validated"; source = "user";
    } else {
        let (n, t) = table(profile).unwrap();
        for (name, kind, f, notes) in t { zones.push(json!({"name": name, "rect": r(f), "kind": kind, "notes": notes})); }
        note = n; source = "documented-constant";
    }
    let est = !matches!(profile, "title_safe" | "action_safe" | "custom");
    let ov = overlaps(a, &zones, w, h)?;
    let mut out = json!({"profile": profile, "zones": zones, "source": source, "estimated": est, "as_of": AS_OF, "basis": note, "canvas": {"width": w, "height": h}, "read_only": "nothing is moved or rejected; these are numbers to compare against"});
    if let Some(Value::Object(m)) = ov { for (k, v) in m { out[k] = v; } }
    Ok(out)
}
/// v6 eyes item #3: overlap numbers for caller rects against the zones above (optional `rects`; absent = v5 bytes).
/// Each rect [x,y,w,h] in canvas pixels may reach past the canvas; areas are exact rectangle intersections on the zone rects
/// as returned (rounded to 0.01 px). Raw numbers, no verdict.
fn overlaps(a: &Value, zones: &[Value], w: f64, h: f64) -> Result<Option<Value>, String> {
    let rs = match a.get("rects") { None | Some(Value::Null) => return Ok(None), Some(v) => v.as_array().filter(|r| !r.is_empty() && r.len() <= 64).ok_or("out_of_range: rects must be an array of 1..64 of {name?, rect:[x,y,w,h] pixels}")? };
    let inter = |p: [f64; 4], q: [f64; 4]| -> f64 { let ix = (p[0] + p[2]).min(q[0] + q[2]) - p[0].max(q[0]); let iy = (p[1] + p[3]).min(q[1] + q[3]) - p[1].max(q[1]); if ix > 0.0 && iy > 0.0 { ix * iy } else { 0.0 } };
    let zr: Vec<(String, String, [f64; 4])> = zones.iter().map(|z| { let r: Vec<f64> = z["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect(); (z["name"].as_str().unwrap().to_string(), z["kind"].as_str().unwrap().to_string(), [r[0], r[1], r[2], r[3]]) }).collect();
    let (mut out, mut worst_safe, mut worst_avoid): (Vec<Value>, Option<f64>, Option<f64>) = (vec![], None, None);
    let r2 = |x: f64| { let v = (x * 100.0).round() / 100.0; if v == 0.0 { 0.0 } else { v } };
    let (mut worst, mut any_safe): (Option<(&str, f64, String, String)>, bool) = (None, false);
    for (i, it) in rs.iter().enumerate() {
        let o = it.as_object().ok_or(format!("out_of_range: rects[{i}] must be {{name?, rect:[x,y,w,h]}}"))?;
        if let Some(k) = o.keys().find(|k| *k != "name" && *k != "rect") { return Err(format!("out_of_range: rects[{i}] has unknown key {k}; allowed: name, rect")); }
        let name = match o.get("name") { None | Some(Value::Null) => format!("rect{i}"), Some(v) => v.as_str().filter(|n| !n.is_empty() && n.len() <= 64).ok_or(format!("out_of_range: rects[{i}].name must be 1..64 characters"))?.to_string() };
        let rc: Vec<f64> = o.get("rect").and_then(|v| v.as_array()).filter(|v| v.len() == 4).map(|v| v.iter().filter_map(|x| x.as_f64().filter(|f| f.is_finite() && f.abs() <= 65536.0)).collect()).filter(|v: &Vec<f64>| v.len() == 4).ok_or(format!("out_of_range: rects[{i}].rect must be [x,y,w,h] pixels, each within +-65536"))?;
        if rc[2] <= 0.0 || rc[3] <= 0.0 { return Err(format!("out_of_range: rects[{i}].rect width and height must be > 0")); }
        let r = [rc[0], rc[1], rc[2], rc[3]]; let area = r[2] * r[3]; let on = inter(r, [0.0, 0.0, w, h]);
        let mut zs = vec![];
        for (zn, zk, q) in &zr { let ov = inter(r, *q); let pct = ov * 100.0 / area;
            if zk == "safe" { let outp = 100.0 - pct; worst_safe = Some(worst_safe.map_or(outp, |m: f64| m.max(outp)));
                // steer 20:04Z: which edge of the safe zone each rect crosses and by how many px, and the smallest move that fits it
                let mut ce = vec![]; any_safe = true;
                for (edge, d) in [("top", q[1] - r[1]), ("bottom", (r[1] + r[3]) - (q[1] + q[3])), ("left", q[0] - r[0]), ("right", (r[0] + r[2]) - (q[0] + q[2]))] {
                    let d = r2(d); if d > 0.0 { ce.push(json!({"edge": edge, "px": d})); if worst.as_ref().map_or(true, |w: &(&str, f64, String, String)| d > w.1) { worst = Some((edge, d, name.clone(), zn.clone())); } } }
                let ax = |p: f64, s: f64, qp: f64, qs: f64| if p < qp { qp - p } else if p + s > qp + qs { (qp + qs) - (p + s) } else { 0.0 };
                let shift = if r[2] <= q[2] + 1e-9 && r[3] <= q[3] + 1e-9 { json!({"dx": r2(ax(r[0], r[2], q[0], q[2])), "dy": r2(ax(r[1], r[3], q[1], q[3]))}) } else { Value::Null };
                zs.push(json!({"zone": zn, "kind": zk, "overlap_px": ov, "overlap_pct": pct, "outside_pct": outp, "crossed_edges": ce, "suggested_shift": shift})); }
            else { worst_avoid = Some(worst_avoid.map_or(pct, |m: f64| m.max(pct))); zs.push(json!({"zone": zn, "kind": zk, "overlap_px": ov, "overlap_pct": pct})); } }
        out.push(json!({"name": name, "rect": rc, "area_px": area, "outside_canvas_pct": (area - on) * 100.0 / area, "zones": zs}));
    }
    let (we, wp, wr, wz) = match (&worst, any_safe) { (Some((e, p, rn, zn)), _) => (json!(e), json!(p), json!(rn), json!(zn)), (None, true) => (Value::Null, json!(0.0), Value::Null, Value::Null), (None, false) => (Value::Null, Value::Null, Value::Null, Value::Null) };
    Ok(Some(json!({"overlaps": out, "rects_outside_safe_max_pct": worst_safe, "rects_in_avoid_max_pct": worst_avoid, "worst_edge": we, "worst_edge_px": wp, "worst_edge_rect": wr, "worst_edge_zone": wz})))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "query_safe_margins", mutates: false, description: "Read-only, nothing is moved and nothing is rejected: return safe and avoid zones as pixel rects [x,y,w,h] for a canvas. profile title_safe (default, 90% of the frame) or action_safe (93%), the platform overlays tiktok, reels, shorts, youtube (labelled estimates with an as_of date, never enforcement), or custom with your own zones (validated and echoed). Canvas is the project size, or width/height, or aspect like 9:16 with one of width/height. Optional rects (1..64 of {name?, rect:[x,y,w,h]} pixels) add overlap numbers per zone: overlap_px, overlap_pct, outside_pct for safe zones, outside_canvas_pct, and the worst cases rects_outside_safe_max_pct / rects_in_avoid_max_pct. Per safe zone each rect also gets crossed_edges [{edge: top|bottom|left|right, px}] and suggested_shift {dx, dy}, the smallest move that fits it inside (null when it is too big), and the top level gets worst_edge, worst_edge_px (0 when nothing crosses), worst_edge_rect, worst_edge_zone.",
        schema: obj(vec![("profile", json!({"type":"string","enum":PROFILES})), ("width", json!({"type":"number"})), ("height", json!({"type":"number"})), ("aspect", s("W:H such as 16:9, 9:16, 1:1")), ("zones", json!({"type":"array","items":{"type":"object","properties":{"name":{"type":"string"},"rect":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4},"kind":{"type":"string","enum":["safe","avoid"]},"notes":{"type":"string"}},"required":["name","rect"]}})), ("rects", json!({"type":"array","minItems":1,"maxItems":64,"description":"rects to measure against the zones, canvas pixels; may reach past the canvas","items":{"type":"object","properties":{"name":{"type":"string"},"rect":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4}},"required":["rect"]}}))], &[]), run: query_safe_margins },
] }
