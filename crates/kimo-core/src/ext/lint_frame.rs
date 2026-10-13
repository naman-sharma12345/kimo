//! U4 advisory geometry/static slice. Text ink checks join after shared text metrics land.
use super::kit::*;
use crate::model::{Clip, Source};
use crate::perception::analysis::{self, Image, Request};
use std::collections::{BTreeMap, BTreeSet};
pub const CHECKS: &[&str] = &[
    "out_of_bounds",
    "invisible_layer",
    "low_res_upscale",
    "static_frame",
    "text_contrast",
    "min_text_size",
    "safe_zone",
    "text_overflow",
];
fn number(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    let v = match a.get(k) {
        None => d,
        Some(v) => v
            .as_f64()
            .ok_or_else(|| format!("out_of_range: {k} must be numeric"))?,
    };
    if !v.is_finite() || v < lo || v > hi {
        return Err(format!("out_of_range: {k} must be {lo}..{hi}"));
    }
    Ok(v)
}
fn times(ss: &Session, a: &Value, cap: usize) -> Result<Vec<f64>, String> {
    let n = ["time", "times", "range"]
        .iter()
        .filter(|k| a.get(**k).is_some())
        .count();
    if n != 1 {
        return Err("out_of_range: choose exactly one of time, times, range".into());
    }
    let mut ts = vec![];
    if let Some(v) = a.get("time") {
        ts.push(v.as_f64().ok_or("out_of_range: time must be numeric")?)
    } else if let Some(v) = a.get("times") {
        let arr = v.as_array().ok_or("out_of_range: times must be an array")?;
        if arr.len() > cap {
            return Err(format!(
                "too_many_frames: times has {} frames; max_frames cap is {cap}",
                arr.len()
            ));
        }
        for t in arr {
            ts.push(t.as_f64().ok_or("out_of_range: times must be numeric")?)
        }
    } else {
        let r = &a["range"];
        if !r.is_object() {
            return Err("out_of_range: range must be an object".into());
        }
        let end = ss.project.duration();
        let st = number(r, "start", 0., 0., end)?;
        let en = number(r, "end", end, st, end)?;
        let step = number(r, "step", 0.5, 1e-6, 1e8)?;
        let count = ((en - st) / step).floor() as usize + 1;
        if count > cap {
            return Err(format!(
                "too_many_frames: range requests {count} frames; max_frames cap is {cap}"
            ));
        }
        for i in 0..count {
            ts.push(st + i as f64 * step)
        }
    }
    if ts.is_empty() {
        return Err("out_of_range: at least one time required".into());
    }
    for t in &ts {
        if !t.is_finite() || *t < 0. || *t > ss.project.duration() {
            return Err("out_of_range: time must be finite within project duration".into());
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup();
    Ok(ts)
}
fn layer(c: &Clip) -> Value {
    json!({"id":c.id,"name":c.name,"role":c.role})
}
fn bounds(p: &crate::model::Project, c: &Clip, t: f64) -> Option<([f64; 4], f64)> {
    let lt = crate::render::clip_local_time(c, t);
    let q = crate::render::resolve_xform(p, c, lt);
    let (w, h) = (p.width as f64, p.height as f64);
    let (sw, sh) = match &c.source {
        Source::Shape { width, height, .. } => (*width, *height),
        Source::Solid { .. } | Source::Gradient { .. } => (w, h),
        Source::Media { asset } => {
            if c.props.get("audio_only").map_or(false, |v| v.at(lt) > 0.5) {
                return None;
            }
            let a = p.assets.get(asset)?;
            if a.width == 0 || a.height == 0 {
                return None;
            }
            let fit = c.props.get("fit").map_or(0., |v| v.at(lt));
            let s = if fit >= 0.5 {
                (w / a.width as f64).max(h / a.height as f64)
            } else {
                (w / a.width as f64).min(h / a.height as f64)
            };
            (
                if fit >= 1.5 { w } else { a.width as f64 * s },
                if fit >= 1.5 { h } else { a.height as f64 * s },
            )
        }
        _ => return None,
    };
    let (r, sc) = (q.rotation.to_radians(), q.scale);
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for (x, y) in [
        (-sw / 2., -sh / 2.),
        (sw / 2., -sh / 2.),
        (sw / 2., sh / 2.),
        (-sw / 2., sh / 2.),
    ] {
        let xx = w / 2. + q.x + sc * (x * r.cos() - y * r.sin());
        let yy = h / 2. + q.y + sc * (x * r.sin() + y * r.cos());
        x0 = x0.min(xx);
        y0 = y0.min(yy);
        x1 = x1.max(xx);
        y1 = y1.max(yy)
    }
    Some(([x0, y0, x1 - x0, y1 - y0], q.opacity))
}
fn emit(
    c: &Clip,
    code: &str,
    severity: &str,
    rect: [f64; 4],
    measured: Value,
    message: &str,
    out: &mut Vec<Value>,
    suppressed: &mut Vec<Value>,
) {
    let tag = format!("lint:ignore={code}");
    let allow = code == "out_of_bounds" && c.tags.iter().any(|x| x == "lint:allow_bleed");
    if allow || c.tags.contains(&tag) {
        suppressed.push(
            json!({"layer":layer(c),"code":code,"tag":if allow{"lint:allow_bleed"}else{&tag}}),
        );
        return;
    }
    out.push(json!({"code":code,"severity":severity,"layer":layer(c),"rect":rect,"measured":measured,"message":message}));
}

fn fill_px(f: &mut crate::effects::Frame, x0: i64, y0: i64, x1: i64, y1: i64, c: [f32; 3]) {
    for y in y0.max(0)..y1.min(f.h as i64) { for x in x0.max(0)..x1.min(f.w as i64) { let i = (y as usize * f.w + x as usize) * 4; f.d[i] = c[0]; f.d[i + 1] = c[1]; f.d[i + 2] = c[2]; f.d[i + 3] = 1.0; } }
}
/// Severity colours chosen so each is at least 3:1 against both white and black.
fn sev_colour(sev: &str) -> [f32; 3] { match sev { "error" => [1.0, 0.2, 0.1], "info" => [0.0, 0.55, 0.8], _ => [0.9, 0.35, 0.0] } }
/// stroke px = max(2, round(min(w,h)/160))
pub fn stroke_px(w: usize, h: usize) -> i64 { (((w.min(h)) as f64 / 160.0).round() as i64).max(2) }
fn rects_hit(a: [i64; 4], b: [i64; 4]) -> bool { a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3] }
/// Draws the report on the render copy only (findings are never touched): a stroke OUTSIDE each finding rect with a 1px dark halo, colour by severity,
/// dashed safe-zone line and an edge marker for canvas-clipped rects, and up to 3 label chips ("code value") then "+n". Returns what it drew.
fn draw_annotations(f: &mut crate::effects::Frame, findings: &[Value]) -> Value {
    let (w, h) = (f.w as i64, f.h as i64); let s = stroke_px(f.w, f.h);
    let mut boxes = vec![]; let mut min_contrast = f64::INFINITY; let mut zone_lines = 0; let mut markers = 0;
    let mut obstacles: Vec<[i64; 4]> = vec![];
    for row in findings {
        let v: Vec<f64> = row["rect"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default(); if v.len() != 4 { continue; }
        let sev = row["severity"].as_str().unwrap_or("warn"); let col = sev_colour(sev);
        let (x0, y0, x1, y1) = (v[0].floor() as i64, v[1].floor() as i64, (v[0] + v[2]).ceil() as i64, (v[1] + v[3]).ceil() as i64);
        let inb = |x: i64, y: i64, a: i64, b: i64, c: i64, d: i64| x >= a && x < c && y >= b && y < d;
        // halo (1px, dark) then the stroke, both outside the rect
        for y in (y0 - s - 1).max(0)..(y1 + s + 1).min(h) { for x in (x0 - s - 1).max(0)..(x1 + s + 1).min(w) {
            if inb(x, y, x0, y0, x1, y1) { continue; }
            let ring = inb(x, y, x0 - s, y0 - s, x1 + s, y1 + s);
            let i = (y as usize * f.w + x as usize) * 4;
            if ring { let under = [f.d[i] as f64, f.d[i + 1] as f64, f.d[i + 2] as f64]; min_contrast = min_contrast.min(contrast_ratio([col[0] as f64, col[1] as f64, col[2] as f64], under)); f.d[i] = col[0]; f.d[i + 1] = col[1]; f.d[i + 2] = col[2]; f.d[i + 3] = 1.0; }
            else { f.d[i] = 0.0; f.d[i + 1] = 0.0; f.d[i + 2] = 0.0; f.d[i + 3] = 1.0; }
        } }
        // the halo was drawn first-come in one pass; re-stroke over it is unnecessary because ring and halo never share a pixel
        let mut clipped = vec![];
        if x0 < 0 { fill_px(f, 0, y0.max(0), 3 * s, y1.min(h), col); clipped.push("left"); markers += 1; }
        if y0 < 0 { fill_px(f, x0.max(0), 0, x1.min(w), 3 * s, col); clipped.push("top"); markers += 1; }
        if x1 > w { fill_px(f, w - 3 * s, y0.max(0), w, y1.min(h), col); clipped.push("right"); markers += 1; }
        if y1 > h { fill_px(f, x0.max(0), h - 3 * s, x1.min(w), h, col); clipped.push("bottom"); markers += 1; }
        // dashed safe-zone line on each violated edge
        if row["code"] == "safe_zone" && row["measured"]["zone"]["kind"] == "safe" {
            let z: Vec<f64> = row["measured"]["zone"]["rect"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default();
            let o = &row["measured"]["outside_px"];
            if z.len() == 4 {
                let g = [0.1f32, 0.9, 0.4]; let (dash, gap) = (6 * s, 4 * s);
                let mut vline = |x: i64, f: &mut crate::effects::Frame| { let mut y = 0; while y < h { fill_px(f, x - 1, y, x + s + 1, (y + dash).min(h), [0., 0., 0.]); fill_px(f, x, y, x + s, (y + dash).min(h), g); y += dash + gap; } zone_lines += 1; };
                if o["left"].as_f64().unwrap_or(0.) > 0. { vline(z[0].round() as i64, f); }
                if o["right"].as_f64().unwrap_or(0.) > 0. { vline((z[0] + z[2]).round() as i64 - s, f); }
                let mut hline = |y: i64, f: &mut crate::effects::Frame| { let mut x = 0; while x < w { fill_px(f, x, y - 1, (x + dash).min(w), y + s + 1, [0., 0., 0.]); fill_px(f, x, y, (x + dash).min(w), y + s, g); x += dash + gap; } zone_lines += 1; };
                if o["top"].as_f64().unwrap_or(0.) > 0. { hline(z[1].round() as i64, f); }
                if o["bottom"].as_f64().unwrap_or(0.) > 0. { hline((z[1] + z[3]).round() as i64 - s, f); }
            }
        }
        obstacles.push([x0 - s - 1, y0 - s - 1, x1 - x0 + 2 * s + 2, y1 - y0 + 2 * s + 2]);
        boxes.push(json!({"code":row["code"],"severity":sev,"rect":row["rect"],"colour":col,"clipped_edges":clipped}));
    }
    // label chips: dark chip, light text, inside the canvas, clear of every box and of earlier chips
    let size = ((f.w.min(f.h)) as f32 / 20.0).round().max(13.0); let (pad, ch_h) = ((size * 0.7).round() as i64, (size * 1.7).round() as i64);
    let mut chips = vec![];
    for (n, row) in findings.iter().enumerate() {
        let txt = if n == 3 { format!("+{}", findings.len() - 3) } else if n > 3 { break } else {
            let o = &row["measured"]["outside_px"]; let mut best: Option<(&str, f64)> = None;
            for e in ["left", "top", "right", "bottom"] { let v = o[e].as_f64().unwrap_or(0.); if v > best.map(|b| b.1).unwrap_or(0.) { best = Some((e, v)); } }
            match best { Some((e, v)) => format!("{} -{}px {}", row["code"].as_str().unwrap_or("finding"), v.round() as i64, e), None => row["code"].as_str().unwrap_or("finding").to_string() } };
        let Ok(tw) = crate::text::label_width(&txt, size) else { continue };
        let (cw, cx_h) = (tw.ceil() as i64 + 2 * pad, ch_h);
        let r = row["rect"].as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or_else(|| vec![0.; 4]);
        let (rx, ry, rw, rh) = (r[0].floor() as i64, r[1].floor() as i64, (r[2].ceil() as i64).max(1), (r[3].ceil() as i64).max(1));
        let clampx = |x: i64| x.max(0).min((w - cw).max(0)); let clampy = |y: i64| y.max(0).min((h - cx_h).max(0));
        let cands = [[clampx(rx), clampy(ry - s - 3 - cx_h)], [clampx(rx), clampy(ry + rh + s + 3)], [clampx(rx + rw + s + 3), clampy(ry)], [clampx(rx - s - 3 - cw), clampy(ry)], [0, 0], [clampx(w), clampy(h)]];
        let boxr = [rx - s - 1, ry - s - 1, rw + 2 * s + 2, rh + 2 * s + 2];
        let clear = |c: [i64; 4]| !rects_hit(c, boxr) && obstacles.iter().all(|o| !rects_hit(c, *o)) && chips.iter().all(|k: &Value| { let q: Vec<i64> = k["rect"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect(); !rects_hit(c, [q[0], q[1], q[2], q[3]]) });
        let pick = cands.iter().map(|p| [p[0], p[1], cw, cx_h]).find(|c| clear(*c)).unwrap_or([cands[0][0], cands[0][1], cw, cx_h]);
        for y in pick[1].max(0)..(pick[1] + pick[3]).min(h) { for x in pick[0].max(0)..(pick[0] + pick[2]).min(w) { let i = (y as usize * f.w + x as usize) * 4; let (rr, ex) = (pick[3] as f64 / 2.0, (x as f64 + 0.5 - pick[0] as f64).min(pick[0] as f64 + pick[2] as f64 - x as f64 - 0.5)); let cy = (y as f64 + 0.5 - pick[1] as f64 - rr).abs(); let (dx, dy) = ((rr - ex).max(0.0), cy); if dx * dx + dy * dy > rr * rr { continue; } for k in 0..3 { f.d[i + k] = f.d[i + k] * 0.2 + 0.04 * 0.8; } } }
        let _ = crate::text::draw_label(f, &txt, (pick[0] + pad) as f64, (pick[1] as f64 + (ch_h as f64 - size as f64 * 1.2) / 2.0), size, [1.0, 1.0, 1.0]);
        chips.push(json!({"text":txt,"rect":pick}));
    }
    json!({"stroke_px": s, "boxes": boxes, "chips": chips, "zone_lines": zone_lines, "edge_markers": markers, "min_contrast_vs_frame": if min_contrast.is_finite() { json!(min_contrast) } else { Value::Null }, "note": "drawn on a render copy; findings are unchanged; stroke is outside the rect with a 1px dark halo, colour by severity (error red, warn orange, info cyan); green dashes are the safe-zone edge that was violated"})
}
pub fn boxed(f: &mut crate::effects::Frame, r: [f64; 4]) {
    let x0 = r[0].floor().max(0.).min(f.w as f64) as usize;
    let y0 = r[1].floor().max(0.).min(f.h as f64) as usize;
    let x1 = (r[0] + r[2]).ceil().max(0.).min(f.w as f64) as usize;
    let y1 = (r[1] + r[3]).ceil().max(0.).min(f.h as f64) as usize;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for y in y0..y1 {
        for x in x0..x1 {
            if x == x0 || x + 1 == x1 || y == y0 || y + 1 == y1 {
                let i = (y * f.w + x) * 4;
                f.d[i..i + 4].copy_from_slice(&[1., 0.2, 0.1, 1.])
            }
        }
    }
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let params = a.get("params").cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return Err("out_of_range: params must be an object".into());
    }
    let mut used = json!({});
    for (k, d, lo, hi) in [
        ("max_frames", 120., 1., 120.),
        ("max_static_s", 3., 0., 1e8),
        ("mse_threshold", 0., 0., 1.),
        ("changed_ratio_threshold", 0., 0., 1.),
        ("pixel_threshold", 0.04, 0., 1.),
        ("opacity_epsilon", 0.001, 0., 1.),
        ("upscale_ratio", 1.5, 1., 1e8),
        ("contrast_normal", 4.5, 1., 100.),
        ("contrast_large", 3., 1., 100.),
        ("large_cap_px", 24., 0., 1e8),
        ("min_cap_px", 20., 0., 1e8),
    ] {
        used[k] = json!(number(&params, k, d, lo, hi)?);
    }
    for k in params.as_object().unwrap().keys() {
        if used.get(k).is_none() {
            return Err(format!("out_of_range: unknown params.{k}"));
        }
    }

    let cap = used["max_frames"].as_f64().unwrap();
    if cap.fract() != 0. {
        return Err("out_of_range: max_frames must be an integer".into());
    }
    let ts = times(ss, a, cap as usize)?;
    let checks = match a.get("checks") {
        None => CHECKS
            .iter()
            .map(|x| x.to_string())
            .collect::<BTreeSet<_>>(),
        Some(v) => {
            let arr=v.as_array().ok_or("invalid_check: checks must be an array; allowed: out_of_bounds, invisible_layer, low_res_upscale, static_frame")?;
            let mut s = BTreeSet::new();
            for x in arr {
                let k = x.as_str().ok_or("invalid_check: check must be a string")?;
                if !CHECKS.contains(&k) {
                    return Err(format!(
                        "invalid_check: '{k}' not implemented; allowed: {}",
                        CHECKS.join(", ")
                    ));
                }
                s.insert(k.into());
            }
            s
        }
    };
    let max = number(a, "max_findings", 200., 1., 10000.)?;
    if max.fract() != 0. {
        return Err("out_of_range: max_findings must be an integer".into());
    }
    let annotate = match a.get("annotate") {
        None => true,
        Some(v) => v
            .as_bool()
            .ok_or("out_of_range: annotate must be boolean")?,
    };
    let height = number(a, "delivery_height", ss.project.height as f64, 1., 16384.)?;
    used["delivery_height"] = json!(height);
    let mut frames = vec![];
    let mut suppressed = vec![];
    let mut counts = BTreeMap::<String, usize>::new();
    let mut intervals = vec![];
    let mut prev: Option<Image> = None;
    let mut static_start = None;
    let mut total = 0usize;
    let mut omitted = 0usize;
    let hint_opts = crate::perception::fix_hint::Opts { min_cap_1080: used["min_cap_px"].as_f64().unwrap_or(0.), upscale_limit: used["upscale_ratio"].as_f64().unwrap_or(0.) };
    let mut hint_steps: Vec<Value> = vec![];
    for (i, t) in ts.iter().enumerate() {
        crate::guard::check();
        let start = std::time::Instant::now();
        let mut findings = vec![];
        for tr in &ss.project.tracks {
            if tr.muted {
                continue;
            }
            for c in &tr.clips {
                if *t < c.start || *t >= c.start + c.duration {
                    continue;
                }
                let Some((r, opacity)) = bounds(&ss.project, c, *t) else {
                    continue;
                };
                let (w, h) = (ss.project.width as f64, ss.project.height as f64);
                let off = r[0] + r[2] <= 0. || r[1] + r[3] <= 0. || r[0] >= w || r[1] >= h;
                if checks.contains("out_of_bounds") {
                    let edges = [
                        (-r[0]).max(0.),
                        (-r[1]).max(0.),
                        (r[0] + r[2] - w).max(0.),
                        (r[1] + r[3] - h).max(0.),
                    ];
                    if edges.iter().any(|x| *x > 1e-8) {
                        emit(
                            c,
                            "out_of_bounds",
                            "warn",
                            r,
                            json!({"bounds_kind":"layer_bounds","outside_px":{"left":edges[0],"top":edges[1],"right":edges[2],"bottom":edges[3]},"fully_offscreen":off}),
                            "Layer bounds extend outside the canvas.",
                            &mut findings,
                            &mut suppressed,
                        )
                    }
                }
                if checks.contains("invisible_layer")
                    && (opacity.abs() <= used["opacity_epsilon"].as_f64().unwrap() || off)
                {
                    emit(
                        c,
                        "invisible_layer",
                        "info",
                        r,
                        json!({"opacity":opacity,"fully_offscreen":off,"sampled_only":true}),
                        "Active layer is invisible at this sample; intent is unknown.",
                        &mut findings,
                        &mut suppressed,
                    )
                }
                if checks.contains("low_res_upscale") {
                    if let Source::Media { asset } = &c.source {
                        if let Some(src) = ss.project.assets.get(asset) {
                            let ratio = (r[2] / src.width.max(1) as f64)
                                .max(r[3] / src.height.max(1) as f64);
                            if ratio > used["upscale_ratio"].as_f64().unwrap() {
                                emit(
                                    c,
                                    "low_res_upscale",
                                    "info",
                                    r,
                                    json!({"source_px":[src.width,src.height],"drawn_bounds_px":[r[2],r[3]],"ratio":ratio}),
                                    "Source pixels are smaller than the drawn layer bounds.",
                                    &mut findings,
                                    &mut suppressed,
                                )
                            }
                        }
                    }
                }
            }
        }
        text_findings(ss, *t, &checks, &used, a, &mut findings, &mut suppressed)?;
        if checks.contains("static_frame") {
            let img = analysis::render(
                ss,
                &Request {
                    time: *t,
                    source: "composite".into(),
                },
            )?;
            if let Some(old) = &prev {
                let diff = super::diff_frames::compare(
                    old,
                    &img,
                    used["pixel_threshold"].as_f64().unwrap(),
                    1,
                    1,
                );
                let same = diff["metrics"]["mse"].as_f64().unwrap()
                    <= used["mse_threshold"].as_f64().unwrap()
                    && diff["metrics"]["changed_ratio"].as_f64().unwrap()
                        <= used["changed_ratio_threshold"].as_f64().unwrap();
                if same {
                    if static_start.is_none() {
                        static_start = Some(ts[i - 1])
                    }
                } else if let Some(t0) = static_start.take() {
                    if ts[i - 1] - t0 > used["max_static_s"].as_f64().unwrap() {
                        intervals.push(static_interval(ss, t0, ts[i - 1]));
                    }
                }
            }
            prev = Some(img);
        }
        let mut frow = json!({"time":t,"findings":[]});
        for mut f in findings {
            crate::perception::fix_hint::lint(&ss.project, &mut f, &hint_opts, &mut hint_steps);
            let code = f["code"].as_str().unwrap().to_string();
            *counts.entry(code).or_default() += 1;
            if total < max as usize {
                frow["findings"].as_array_mut().unwrap().push(f);
                total += 1
            } else {
                omitted += 1
            }
        }
        if annotate && !frow["findings"].as_array().unwrap().is_empty() {
            crate::render::prepare_at(&ss.project, &mut ss.cache, Some(*t))?;
            let mut f = crate::render::render_frame(&ss.project, *t, &ss.cache)?;
            crate::color::pipeline::output(&ss.project, &mut f)?;
            let ann = draw_annotations(&mut f, frow["findings"].as_array().unwrap());
            let path = std::env::temp_dir().join(format!(
                "kimo-lint-{}-{}-{i}.png",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            crate::render::write_image(&f, path.to_str().ok_or("annotation path invalid")?)?;
            ss.media.push(crate::commands::Media {
                path: path.to_string_lossy().into(),
                w: f.w as u32,
                h: f.h as u32,
                mime: "image/png",
            });
            frow["annotation"] = {let mut a = ann; a["path"] = json!(path); a};
        }
        frow["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
        frames.push(frow);
    }
    let mut groups: BTreeMap<(String, String), Vec<(usize, Value)>> = BTreeMap::new();
    for (i, f) in frames.iter().enumerate() {
        for finding in f["findings"].as_array().unwrap() {
            groups
                .entry((
                    finding["code"].as_str().unwrap().into(),
                    finding["layer"]["id"].as_str().unwrap().into(),
                ))
                .or_default()
                .push((i, finding.clone()));
        }
    }
    for ((code, _), hits) in groups {
        let mut lo = 0;
        while lo < hits.len() {
            let mut hi = lo;
            while hi + 1 < hits.len() && hits[hi + 1].0 == hits[hi].0 + 1 {
                hi += 1
            }
            let worst = hits[lo..=hi].iter().max_by(|a,b|severity_score(&a.1).total_cmp(&severity_score(&b.1))).unwrap();
            intervals.push(json!({"code":code,"t0":ts[hits[lo].0],"t1":ts[hits[hi].0],"layers":[hits[lo].1["layer"].clone()],"worst":worst.1["measured"].clone(),"fix_hint":worst.1["fix_hint"].clone(),"note":"sampled hits only"}));
            lo = hi + 1;
        }
    }
    if let Some(t0) = static_start {
        let t1 = *ts.last().unwrap();
        if t1 - t0 > used["max_static_s"].as_f64().unwrap() {
            intervals.push(static_interval(ss, t0, t1));
        }
    }
    for iv in &intervals {
        if iv["code"] == "static_frame" {
            *counts.entry("static_frame".into()).or_default() += 1;
        }
    }
    Ok(
        json!({"next_steps":crate::perception::fix_hint::next_steps(hint_steps),"frames":frames,"intervals":intervals,"summary":{"counts_by_code":counts,"ignored_by_tag":suppressed.len(),"suppressed":suppressed,"frames_checked":ts.len(),"omitted_findings":omitted},"params_used":used,"notes":["Advisory only, not a design certification. No edit, undo entry or export block.","Geometry is matrix-transformed source layer bounds, not effect-expanded ink. Fully masked visibility is not measured. Text uses static C2 ink, excluding shadow/glow/bg and animator deformation; contrast is a WCAG heuristic on background under that bbox, not certification. Overflow is canvas only, not own-box wrapping.","Static results cover sampled times only; audio presence does not establish speech or intentional holding."]}),
    )
}
fn severity_score(f: &Value) -> f64 {
    let m = &f["measured"];
    match f["code"].as_str().unwrap_or("") {
        "out_of_bounds" => m["outside_px"]
            .as_object()
            .map(|o| o.values().filter_map(Value::as_f64).sum())
            .unwrap_or(0.),
        "low_res_upscale" => m["ratio"].as_f64().unwrap_or(0.),
        "invisible_layer" => 1. - m["opacity"].as_f64().unwrap_or(1.),
        _ => 0.,
    }
}
fn static_interval(ss: &Session, t0: f64, t1: f64) -> Value {
    let audio = ss
        .project
        .tracks
        .iter()
        .filter(|t| !t.muted)
        .flat_map(|t| t.clips.iter())
        .any(|c| {
            c.start < t1
                && c.start + c.duration > t0
                && matches!(c.source, Source::Media { .. })
                && !c.audio.mute
                && c.audio.volume.at(0.) > 0.
        });
    json!({"code":"static_frame","t0":t0,"t1":t1,"layers":[],"worst":{"duration_s":t1-t0,"audio_present":audio},"severity":"info","fix_hint":crate::perception::fix_hint::none("a static stretch may be intended; decide whether to trim, add motion or keep it")})
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"lint_frame",mutates:false,description:"Read-only advisory frame lint, report-only, not a design certification. Initial geometry/static checks: out_of_bounds (source layer bounds, not ink), invisible_layer (sampled), low_res_upscale, static_frame. Never edits or blocks export. time OR times OR range {start,end,step default .5}, max120 samples. checks defaults to implemented checks. params echoes thresholds, overridable. Tags lint:allow_bleed and lint:ignore=<code> suppress with an explicit counted list. annotate default true writes boxed PNG paths and inline images. Text checks: text_contrast (WCAG heuristic), min_text_size (capital-H ink), safe_zone (platform estimates), text_overflow (canvas only, not own-box overflow).",schema:obj(vec![("time",n("timeline seconds")),("times",json!({"type":"array","items":{"type":"number"}})),("range",json!({"type":"object"})),("checks",json!({"type":"array","items":{"type":"string"}})),("params",json!({"type":"object"})),("delivery_height",n("pixels, default canvas height")),("profile",s("safe margins profile, default title_safe")),("annotate",json!({"type":"boolean"})),("max_findings",n("default200"))],&[]),run}]
}
// Text metrics adapters staged from the delivered C1/C2/C3 contracts.
fn text_cap(c: &Clip, w: f64, h: f64) -> Result<f64, String> {
    let Source::Text {
        font,
        size,
        letter_spacing,
        align,
        ..
    } = &c.source
    else {
        return Ok(0.);
    };
    let fx = c.text_fx.clone().unwrap_or_default();
    let m = crate::text::measure_text(
        "H",
        font,
        *size,
        *letter_spacing,
        fx.weight,
        align,
        w,
        h,
        Some(&fx),
    )?;
    Ok(m.ink.map_or(0., |r| r[3] - r[1]))
}
fn outside(r: [f64; 4], zone: [f64; 4]) -> [f64; 4] {
    [
        (zone[0] - r[0]).max(0.),
        (zone[1] - r[1]).max(0.),
        (r[0] + r[2] - zone[0] - zone[2]).max(0.),
        (r[1] + r[3] - zone[1] - zone[3]).max(0.),
    ]
}
fn edges_json(e: [f64; 4]) -> Value {
    json!({"left":e[0],"top":e[1],"right":e[2],"bottom":e[3]})
}
fn srgb_luma(rgb: [f64; 3]) -> f64 {
    let linear = |x: f64| {
        let x = x.clamp(0., 1.);
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}
fn contrast_ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (srgb_luma(a), srgb_luma(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn text_findings(
    ss: &mut Session,
    t: f64,
    checks: &BTreeSet<String>,
    used: &Value,
    a: &Value,
    out: &mut Vec<Value>,
    suppressed: &mut Vec<Value>,
) -> Result<(), String> {
    if ![
        "text_contrast",
        "min_text_size",
        "safe_zone",
        "text_overflow",
    ]
    .iter()
    .any(|x| checks.contains(*x))
    {
        return Ok(());
    }
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let texts: Vec<Clip> = ss
        .project
        .tracks
        .iter()
        .filter(|tr| !tr.muted)
        .flat_map(|tr| tr.clips.iter())
        .filter(|c| {
            t >= c.start && t < c.start + c.duration && matches!(c.source, Source::Text { .. })
        })
        .map(|c| (**c).clone())
        .collect();
    let margins = if checks.contains("safe_zone") {
        Some(crate::commands::dispatch(
            ss,
            "query_safe_margins",
            &json!({"profile":a.get("profile").cloned().unwrap_or(json!("title_safe")),"zones":a.get("zones")}),
        )?)
    } else {
        None
    };
    for c in texts {
        let lt = crate::render::clip_local_time(&c, t);
        let xf = crate::render::resolve_xform(&ss.project, &c, lt);
        if xf.opacity <= used["opacity_epsilon"].as_f64().unwrap() {
            continue;
        }
        let b =
            crate::commands::dispatch(ss, "compute_text_bounds", &json!({"clip_id":c.id,"t":t}))?;
        let Some(v) = b["canvas_rect"].as_array() else {
            continue;
        };
        let r = [
            v[0].as_f64().unwrap(),
            v[1].as_f64().unwrap(),
            v[2].as_f64().unwrap(),
            v[3].as_f64().unwrap(),
        ];
        let cap = text_cap(&c, w, h)? * xf.scale.abs();
        let delivery = used["delivery_height"].as_f64().unwrap();
        let cap_delivery = cap * delivery / h;
        let cap1080 = cap_delivery * 1080. / delivery;
        if checks.contains("min_text_size")
            && cap_delivery < used["min_cap_px"].as_f64().unwrap() * delivery / 1080.
        {
            emit(
                &c,
                "min_text_size",
                "warn",
                r,
                json!({"cap_height_canvas_px":cap,"cap_height_delivery_px":cap_delivery,"cap_height_1080_px":cap1080,"metric":"capital_H_ink","font":b["font"],"line_count":b["line_count"]}),
                "Capital-H ink is below the delivery-scaled minimum.",
                out,
                suppressed,
            );
        }
        if checks.contains("text_overflow") {
            let e = outside(r, [0., 0., w, h]);
            if e.iter().any(|x| *x > 1e-8) {
                emit(
                    &c,
                    "text_overflow",
                    "warn",
                    r,
                    json!({"outside_px":edges_json(e),"scope":"canvas_only"}),
                    "Text ink extends outside the canvas; own-box wrapping is not checked.",
                    out,
                    suppressed,
                )
            }
        }
        if let Some(m) = &margins {
            for z in m["zones"].as_array().unwrap() {
                let zv = z["rect"].as_array().unwrap();
                let zr = [
                    zv[0].as_f64().unwrap(),
                    zv[1].as_f64().unwrap(),
                    zv[2].as_f64().unwrap(),
                    zv[3].as_f64().unwrap(),
                ];
                let e = outside(r, zr);
                let overlap = (r[0] + r[2]).min(zr[0] + zr[2]) - r[0].max(zr[0]);
                let overlap_y = (r[1] + r[3]).min(zr[1] + zr[3]) - r[1].max(zr[1]);
                let hit = if z["kind"] == "safe" {
                    e.iter().any(|x| *x > 1e-8)
                } else {
                    overlap > 0. && overlap_y > 0.
                };
                if hit {
                    let est = m["estimated"] == true;
                    emit(
                        &c,
                        "safe_zone",
                        "warn",
                        r,
                        json!({"zone":z,"outside_px":edges_json(e),"estimated":m["estimated"],"as_of":m["as_of"],"basis":m["basis"],"profile":m["profile"]}),
                        if est {
                            "Text intersects a platform estimate; check the current app before relying on it."
                        } else {
                            "Text violates the selected geometric safe/avoid zone."
                        },
                        out,
                        suppressed,
                    );
                }
            }
        }
        if checks.contains("text_contrast") {
            let Source::Text { color, .. } = c.source else {
                continue;
            };
            let p = super::view::view_of(&ss.project, None, Some(&[c.id.clone()]));
            let bg = crate::render::render_frame(&p, t, &crate::render::Cache::new())?;
            let img = Image::from_frame(&bg);
            let (x0, y0, x1, y1) = (
                r[0].floor().max(0.) as usize,
                r[1].floor().max(0.) as usize,
                (r[0] + r[2]).ceil().max(0.).min(w) as usize,
                (r[1] + r[3]).ceil().max(0.).min(h) as usize,
            );
            let mut ratios = vec![];
            let mut worst = [0usize; 2];
            let mut min = f64::INFINITY;
            for y in y0.min(img.h)..y1 {
                for x in x0.min(img.w)..x1 {
                    let i = y * img.w + x;
                    let b = [img.rgb[0][i], img.rgb[1][i], img.rgb[2][i]];
                    let opacity = xf.opacity.clamp(0., 1.);
                    let fg = [0, 1, 2].map(|k| opacity * color[k] + (1. - opacity) * b[k]);
                    let ratio = contrast_ratio(fg, b);
                    if ratio < min {
                        min = ratio;
                        worst = [x, y]
                    }
                    ratios.push(ratio);
                }
            }
            if ratios.is_empty() {
                continue;
            }
            ratios.sort_by(f64::total_cmp);
            let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
            let limit = used[if cap1080 >= used["large_cap_px"].as_f64().unwrap() {
                "contrast_large"
            } else {
                "contrast_normal"
            }]
            .as_f64()
            .unwrap();
            if min < limit {
                emit(&c,"text_contrast","warn",r,json!({"min_ratio":min,"p10_ratio":ratios[(ratios.len()-1)/10],"mean_ratio":mean,"spread":ratios.last().unwrap()-min,"worst_point":worst,"threshold":limit,"heuristic":true}),"WCAG contrast heuristic below threshold; bbox sampling does not certify legibility.",out,suppressed)
            }
        }
    }
    Ok(())
}
