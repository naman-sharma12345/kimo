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
    "text_overlap",
    "text_over_foreground",
    "caption_over_foreground",
    "caption_style_consistency",
];
/// Opt-in, info-only: not part of the default check set.
pub const OPT_IN: &[&str] = &["optical_balance"];
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
        ("render_budget", 64., 0., 10000.),
        ("overlap_min_px", 1., 0., 1e8),
        ("min_overlap", 0.10, 0., 1.),
        ("min_matte_overlap", 0.02, 0., 1.),
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
                if !CHECKS.contains(&k) && !OPT_IN.contains(&k) {
                    return Err(format!(
                        "invalid_check: '{k}' not implemented; allowed: {}, {} (opt-in)",
                        CHECKS.join(", "),
                        OPT_IN.join(", ")
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
    let mut budget = Budget { left: used["render_budget"].as_f64().unwrap() as usize, skipped: vec![], excluded: vec![] };
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
        text_findings(ss, *t, &checks, &used, a, &mut findings, &mut suppressed, &mut budget)?;
        layout_findings(ss, *t, &checks, &used, &mut findings, &mut suppressed, &mut budget)?;
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
            for row in frow["findings"].as_array().unwrap() {
                let v = row["rect"].as_array().unwrap();
                boxed(
                    &mut f,
                    [
                        v[0].as_f64().unwrap(),
                        v[1].as_f64().unwrap(),
                        v[2].as_f64().unwrap(),
                        v[3].as_f64().unwrap(),
                    ],
                );
            }
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
            frow["annotation"] = json!({"path":path});
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
                    match finding["measured"]["covered_layer"]["id"].as_str() {
                        Some(cv) => format!("{}\u{0}{cv}", finding["layer"]["id"].as_str().unwrap()),
                        None => finding["layer"]["id"].as_str().unwrap().into(),
                    },
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
        json!({"next_steps":crate::perception::fix_hint::next_steps(hint_steps),"frames":frames,"intervals":intervals,"summary":{"counts_by_code":counts,"ignored_by_tag":suppressed.len(),"suppressed":suppressed,"frames_checked":ts.len(),"skipped_checks":budget.skipped,"excluded_layers":budget.excluded,"omitted_findings":omitted},"params_used":used,"notes":["Advisory only, not a design certification. No edit, undo entry or export block.","Geometry is matrix-transformed source layer bounds, not effect-expanded ink. Fully masked visibility is not measured. Text uses static C2 ink, excluding shadow/glow/bg and animator deformation; contrast is a WCAG heuristic on background under that bbox, not certification. Overflow is canvas only, not own-box wrapping.","Static results cover sampled times only; audio presence does not establish speech or intentional holding."]}),
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
    budget: &mut Budget,
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
            if budget.left == 0 {
                budget.skipped.push(json!({"check":"text_contrast","t":t,"clip_id":c.id,"reason":"render_budget","render_budget":used["render_budget"]}));
                continue;
            }
            budget.left -= 1;
            let p = super::view::view_of(&ss.project, None, Some(&[c.id.clone()]));
            let mut cache = crate::render::Cache::new();
            crate::render::prepare_at(&p, &mut cache, Some(t))?;
            let bg = crate::render::render_frame(&p, t, &cache)?;
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
                emit(&c,"text_contrast","warn",r,json!({"min_ratio":min,"p10_ratio":ratios[(ratios.len()-1)/10],"p5_ratio":ratios[(ratios.len()-1)/20],"p50_ratio":ratios[(ratios.len()-1)/2],"background_spread":lum_spread(&img,x0,y0,x1,y1),"mean_ratio":mean,"spread":ratios.last().unwrap()-min,"worst_point":worst,"threshold":limit,"heuristic":true}),"WCAG contrast heuristic below threshold; bbox sampling does not certify legibility.",out,suppressed)
            }
        }
    }
    Ok(())
}

struct Budget {
    left: usize,
    skipped: Vec<Value>,
    excluded: Vec<Value>,
}
fn lum_spread(img: &Image, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for y in y0.min(img.h)..y1 {
        for x in x0.min(img.w)..x1 {
            let i = y * img.w + x;
            let l = srgb_luma([img.rgb[0][i], img.rgb[1][i], img.rgb[2][i]]);
            lo = lo.min(l);
            hi = hi.max(l);
        }
    }
    if hi < lo { 0. } else { hi - lo }
}
fn inter(a: [f64; 4], b: [f64; 4]) -> (f64, f64) {
    (
        ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.),
        ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.),
    )
}
fn style_key(c: &Clip) -> Option<String> {
    let Source::Text { font, size, color, letter_spacing, stroke_width, stroke_color, shadow, align, .. } = &c.source else { return None };
    Some(format!("{font}|{size}|{color:?}|{letter_spacing}|{stroke_width}|{stroke_color:?}|{shadow}|{align}"))
}
/// Text layout checks that need only ink rects and clip metadata (no extra renders).
fn layout_findings(
    ss: &mut Session,
    t: f64,
    checks: &BTreeSet<String>,
    used: &Value,
    out: &mut Vec<Value>,
    suppressed: &mut Vec<Value>,
    budget: &mut Budget,
) -> Result<(), String> {
    let any = ["text_overlap", "text_over_foreground", "caption_over_foreground", "caption_style_consistency", "optical_balance"];
    if !any.iter().any(|x| checks.contains(*x)) {
        return Ok(());
    }
    let eps = used["opacity_epsilon"].as_f64().unwrap();
    let live = |c: &Clip| t >= c.start && t < c.start + c.duration;
    let mut texts: Vec<(Clip, [f64; 4])> = vec![];
    let all: Vec<Clip> = ss.project.tracks.iter().filter(|tr| !tr.muted).flat_map(|tr| tr.clips.iter()).filter(|c| live(c)).map(|c| (**c).clone()).collect();
    for c in all.iter().filter(|c| matches!(c.source, Source::Text { .. })) {
        let lt = crate::render::clip_local_time(c, t);
        if crate::render::resolve_xform(&ss.project, c, lt).opacity <= eps {
            continue;
        }
        let b = crate::commands::dispatch(ss, "compute_text_bounds", &json!({"clip_id":c.id,"t":t}))?;
        let Some(v) = b["canvas_rect"].as_array() else { continue };
        texts.push((c.clone(), [v[0].as_f64().unwrap(), v[1].as_f64().unwrap(), v[2].as_f64().unwrap(), v[3].as_f64().unwrap()]));
    }
    let min_px = used["overlap_min_px"].as_f64().unwrap();
    if checks.contains("text_overlap") {
        for i in 0..texts.len() {
            for j in i + 1..texts.len() {
                let (w, h) = inter(texts[i].1, texts[j].1);
                if w >= min_px && h >= min_px {
                    let (a, b) = (&texts[i].0, &texts[j].0);
                    let smaller = (texts[i].1[2] * texts[i].1[3]).min(texts[j].1[2] * texts[j].1[3]);
                    let tag = "lint:ignore=text_overlap".to_string();
                    if a.tags.contains(&tag) || b.tags.contains(&tag) {
                        let who = if a.tags.contains(&tag) { a } else { b };
                        suppressed.push(json!({"layer":layer(who),"code":"text_overlap","tag":tag}));
                        continue;
                    }
                    emit(a, "text_overlap", "warn", texts[i].1, json!({"other":layer(b),"overlap_px":{"w":w,"h":h},"overlap_ratio_of_smaller":if smaller>0.{w*h/smaller}else{0.},"scope":"ink_rects"}), "Two text layers' ink rectangles overlap at this time.", out, suppressed);
                }
            }
        }
    }
    if checks.contains("text_over_foreground") || checks.contains("caption_over_foreground") {
        foreground_findings(ss, t, &all, &texts, checks, used, out, suppressed, budget)?;
    }
    if checks.contains("caption_style_consistency") {
        let groups: Vec<(String, Vec<String>)> = ss.project.editorial.captions.iter().map(|(k, g)| (k.clone(), g.clips.clone())).collect();
        for (gid, ids) in groups {
            let mut tally: Vec<(String, usize)> = vec![];
            for id in &ids {
                if let Some(k) = ss.project.find_clip(id).and_then(|c| style_key(c)) {
                    match tally.iter_mut().find(|(x, _)| *x == k) { Some(e) => e.1 += 1, None => tally.push((k, 1)) }
                }
            }
            let Some(mode) = tally.iter().fold(None::<&(String, usize)>, |b, e| if b.map_or(true, |b| e.1 > b.1) { Some(e) } else { b }) else { continue };
            let mode = mode.0.clone();
            for (c, r) in texts.iter().filter(|(c, _)| ids.contains(&c.id)) {
                if style_key(c).as_deref() == Some(mode.as_str()) {
                    continue;
                }
                if c.tags.iter().any(|x| x == "lint:allow_style_variation") {
                    suppressed.push(json!({"layer":layer(c),"code":"caption_style_consistency","tag":"lint:allow_style_variation"}));
                    continue;
                }
                emit(c, "caption_style_consistency", "warn", *r, json!({"caption_id":gid,"cue_style":style_key(c),"group_style":mode,"cues_in_group":ids.len(),"basis":"most common style tuple in the group; ties go to the earliest cue"}), "Caption cue style differs from the rest of its group.", out, suppressed);
            }
        }
    }
    if checks.contains("optical_balance") && !texts.is_empty() {
        let (w, h) = (ss.project.width as f64, ss.project.height as f64);
        let (mut a, mut cx, mut cy) = (0., 0., 0.);
        for (_, r) in &texts {
            let ar = r[2] * r[3];
            a += ar;
            cx += ar * (r[0] + r[2] / 2.);
            cy += ar * (r[1] + r[3] / 2.);
        }
        if a > 0. {
            let c0 = &texts[0].0;
            emit(c0, "optical_balance", "info", [0., 0., w, h], json!({"centroid_px":[cx/a,cy/a],"offset_from_centre_frac":[(cx/a-w/2.)/w,(cy/a-h/2.)/h],"text_layers":texts.len(),"basis":"ink-rect area-weighted centroid; info only"}), "Area-weighted text centroid relative to the canvas centre.", out, suppressed);
        }
    }
    Ok(())
}

fn is_text_like(ss: &Session, c: &Clip) -> bool {
    matches!(c.source, Source::Text { .. })
        || matches!(c.role.as_deref(), Some("caption" | "title" | "lower_third" | "subtitle"))
        || c.tags.iter().any(|x| x == "caption")
        || ss.project.editorial.captions.values().any(|g| g.clips.iter().any(|x| *x == c.id))
}
fn is_caption(ss: &Session, c: &Clip) -> bool {
    c.role.as_deref() == Some("caption")
        || c.tags.iter().any(|x| x == "caption")
        || ss.project.editorial.captions.values().any(|g| g.clips.iter().any(|x| *x == c.id))
}
fn clip_rect(ss: &Session, c: &Clip, t: f64) -> Option<([f64; 4], f64)> {
    let (r, o) = bounds(&ss.project, c, t)?;
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let (x0, y0, x1, y1) = (r[0].max(0.), r[1].max(0.), (r[0] + r[2]).min(w), (r[1] + r[3]).min(h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(([x0, y0, x1 - x0, y1 - y0], o))
}
/// ARCH P3 rules 1-8: text drawn over a foreground layer below it in draw order.
#[allow(clippy::too_many_arguments)]
fn foreground_findings(
    ss: &mut Session,
    t: f64,
    all: &[Clip],
    texts: &[(Clip, [f64; 4])],
    checks: &BTreeSet<String>,
    used: &Value,
    out: &mut Vec<Value>,
    suppressed: &mut Vec<Value>,
    budget: &mut Budget,
) -> Result<(), String> {
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    let canvas = w * h;
    let min_o = used["min_overlap"].as_f64().unwrap();
    let min_m = used["min_matte_overlap"].as_f64().unwrap();
    // visible non-text layers with bounds, in draw order (z = index in `all`)
    let hidden_src: BTreeSet<String> = all.iter().filter_map(|c| c.mask.as_ref().and_then(|m| m.matte.as_ref()).filter(|m| m.hide_source).map(|m| m.source.clone())).collect();
    let vis: Vec<(usize, &Clip, [f64; 4])> = all
        .iter()
        .enumerate()
        .filter(|(_, c)| !is_text_like(ss, c) && !hidden_src.contains(&c.id))
        .filter_map(|(z, c)| clip_rect(ss, c, t).filter(|(_, o)| *o > 0.05).map(|(r, _)| (z, c, r)))
        .collect();
    let bottom = vis.first().map(|v| v.1.id.clone());
    let is_bg = |c: &Clip, r: [f64; 4]| -> bool {
        if c.tags.iter().any(|x| x == "lint:background") || c.role.as_deref() == Some("background") {
            return true;
        }
        let full = r[2] * r[3] >= 0.95 * canvas;
        let matte = c.mask.as_ref().map_or(false, |m| m.matte.is_some());
        (Some(&c.id) == bottom.as_ref() && full && !matte)
            || (matches!(c.source, Source::Solid { .. } | Source::Gradient { .. }) && full && !matte)
    };
    for (z_t, (tc, ink)) in texts.iter().enumerate().map(|(_, tx)| (all.iter().position(|c| c.id == tx.0.id).unwrap_or(0), tx)) {
        let caption = is_caption(ss, tc);
        let code = if caption { "caption_over_foreground" } else { "text_over_foreground" };
        if !checks.contains(code) {
            continue;
        }
        let ink_area = ink[2] * ink[3];
        if ink_area <= 0. {
            continue;
        }
        let mut hits: Vec<(f64, usize, String, Value, Clip)> = vec![];
        for (z, l, r) in vis.iter().filter(|v| v.0 < z_t) {
            if is_bg(l, *r) {
                continue;
            }
            let (iw, ih) = inter(*ink, *r);
            if iw <= 0. || ih <= 0. {
                continue;
            }
            // own backing plate (rule 4)
            let own_tag = format!("lint:backing_of={}", tc.id);
            let adjacent = !vis.iter().any(|v| v.0 > *z && v.0 < z_t);
            let plate = matches!(l.source, Source::Shape { .. } | Source::Solid { .. })
                && r[0] + 1. <= ink[0] && r[1] + 1. <= ink[1] && r[0] + r[2] >= ink[0] + ink[2] + 1. && r[1] + r[3] >= ink[1] + ink[3] + 1.
                && adjacent && *z + 1 == z_t;
            let own_box = tc.text_fx.as_ref().and_then(|f| f.bg).map_or(false, |b| b[3] > 0.);
            if l.tags.contains(&own_tag) || plate || own_box {
                let basis = if l.tags.contains(&own_tag) { "tag" } else if own_box { "own_box" } else { "auto_backing" };
                let e = json!({"text":layer(tc),"layer":layer(l),"basis":basis});
                if !budget.excluded.contains(&e) {
                    budget.excluded.push(e);
                }
                continue;
            }
            let (ratio, basis, bbox) = if l.mask.is_some() {
                if budget.left == 0 {
                    budget.skipped.push(json!({"check":code,"t":t,"clip_id":tc.id,"layer":l.id,"reason":"render_budget","render_budget":used["render_budget"]}));
                    continue;
                }
                budget.left -= 1;
                let mut iso = vec![l.id.clone()];
                if let Some(m) = l.mask.as_ref().and_then(|m| m.matte.as_ref()) {
                    iso.push(m.source.clone());
                }
                let p = super::view::view_of(&ss.project, Some(&iso), None);
                let mut cache = crate::render::Cache::new();
                crate::render::prepare_at(&p, &mut cache, Some(t))?;
                let f = crate::render::render_frame_opts(&p, t, &cache, true)?;
                let (mut under, mut bb) = (0usize, [f.w, f.h, 0usize, 0usize]);
                let (ix0, iy0, ix1, iy1) = (ink[0].floor().max(0.) as usize, ink[1].floor().max(0.) as usize, ((ink[0] + ink[2]).ceil() as usize).min(f.w), ((ink[1] + ink[3]).ceil() as usize).min(f.h));
                let mut cells = 0usize;
                for y in 0..f.h {
                    for x in 0..f.w {
                        let on = f.d[(y * f.w + x) * 4 + 3] > 0.5;
                        if on {
                            bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x + 1), bb[3].max(y + 1)];
                        }
                        if x >= ix0 && x < ix1 && y >= iy0 && y < iy1 {
                            cells += 1;
                            if on {
                                under += 1
                            }
                        }
                    }
                }
                let ratio = if cells == 0 { 0. } else { under as f64 / cells as f64 };
                let bbox = if bb[2] > bb[0] { json!([bb[0], bb[1], bb[2] - bb[0], bb[3] - bb[1]]) } else { Value::Null };
                (ratio, if l.mask.as_ref().map_or(false, |m| m.matte.is_some()) { "matte" } else { "mask" }, bbox)
            } else {
                (iw * ih / ink_area, "rect", json!(r))
            };
            let limit = if basis == "rect" { min_o } else { min_m };
            if ratio >= limit && ratio > 0. {
                if l.tags.iter().any(|x| x == "lint:allow_overlap") {
                    suppressed.push(json!({"layer":layer(tc),"code":code,"tag":"lint:allow_overlap","covered_layer":layer(l)}));
                    continue;
                }
                hits.push((ratio, *z, l.id.clone(), json!({"overlap_ratio":ratio,"covered_layer":layer(l),"basis":basis,"subject_bbox":bbox,"threshold":limit}), (*l).clone()));
            }
        }
        hits.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let more = hits.len().saturating_sub(3);
        for (i, (_, _, _, mut m, _)) in hits.into_iter().take(3).enumerate() {
            if i == 2 && more > 0 {
                m["more"] = json!(more);
            }
            emit(tc, code, "warn", *ink, m, "Text ink overlaps a foreground layer below it in draw order.", out, suppressed);
        }
    }
    Ok(())
}
