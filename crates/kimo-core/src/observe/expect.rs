//! Expectations: an agent says what must be true after its edit, and the engine checks it on the result. A failed
//! expectation rolls the edit back (observe::wrap), so a wrong edit never stays in the project unnoticed.
//!
//! Each entry is an object; any of these keys (several per entry are allowed, all must hold):
//!   {"visible": "<clip id or @name>", "at": t}      the clip is on screen at t (in its time range, opacity > 5%)
//!   {"hidden":  "<clip id or @name>", "at": t}      the clip is not on screen at t
//!   {"inside": "title_safe"|"action_safe"|[l,t,r,b], "clip": id, "at"?: t}   the clip's box stays inside
//!   {"luma": [lo, hi], "at": t, "region"?: [x, y, w, h]}   mean brightness (0..1) of the frame or region
//!   {"no_black_frames": [a, b]}                     no black frame in [a, b] (sampled)
//!   {"readable": "<text clip>"}                    on screen long enough to read and inside title safe
//!   {"max_findings": "info"|"warning"|"error"}       nothing worse than this among the findings for this edit
//!   {"no_findings": ["code", ...]}                  none of these finding codes for this edit
use crate::commands::Session;
use crate::critic::{self, Severity};
use crate::model::{Clip, Project};
use serde_json::{json, Value};

fn find<'a>(p: &'a Project, r: &str) -> Option<&'a Clip> {
    if let Some(n) = r.strip_prefix('@') { return p.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.name.as_deref() == Some(n)).map(|c| c.as_ref()); }
    p.find_clip(r)
}

fn on_screen(p: &Project, c: &Clip, t: f64) -> bool {
    if t < c.start || t >= c.start + c.duration { return false; }
    crate::render::resolve_xform(p, c, t - c.start).opacity > 0.05
}

/// Box of any clip at timeline time t (text: estimated glyph box; everything else: its transformed frame).
fn clip_box(p: &Project, c: &Clip, t: f64) -> (f64, f64, f64, f64) {
    let lt = (t - c.start).clamp(0.0, c.duration);
    if let Some(b) = critic::text::approx_box(p, c, lt) { return b; }
    let xf = crate::render::resolve_xform(p, c, lt);
    let (w, h) = (p.width as f64 * xf.scale, p.height as f64 * xf.scale);
    let (cx, cy) = (p.width as f64 / 2.0 + xf.x, p.height as f64 / 2.0 + xf.y);
    (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0)
}

fn region(p: &Project, v: &Value) -> Option<(f64, f64, f64, f64)> {
    let (w, h) = (p.width as f64, p.height as f64);
    match v.as_str() {
        Some("title_safe") => return Some((w * 0.05, h * 0.05, w * 0.95, h * 0.95)),
        Some("action_safe") => return Some((w * 0.035, h * 0.035, w * 0.965, h * 0.965)),
        _ => {}
    }
    let a = v.as_array()?;
    if a.len() != 4 { return None; }
    Some((a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?, a[3].as_f64()?))
}

/// Mean luma of the frame at t (or a canvas-pixel region [x, y, w, h] of it), from a draft render.
fn luma_at(ss: &mut Session, t: f64, reg: Option<&Value>) -> Result<f64, String> {
    let (rgba, w, h) = { let Session { project, cache, .. } = &mut *ss; super::draft(project, cache, t, 480)? };
    let s = w as f64 / ss.project.width as f64;
    let (x0, y0, x1, y1) = match reg.and_then(|r| r.as_array()) {
        Some(a) if a.len() == 4 => { let f = |i: usize| a[i].as_f64().unwrap_or(0.0) * s; ((f(0)) as usize, (f(1)) as usize, (f(0) + f(2)) as usize, (f(1) + f(3)) as usize) }
        _ => (0, 0, w, h),
    };
    let (x1, y1) = (x1.min(w), y1.min(h));
    let (mut sum, mut n) = (0.0, 0.0);
    for y in y0..y1 { for x in x0..x1 { let i = (y * w + x) * 4; sum += 0.2126 * rgba[i] as f64 + 0.7152 * rgba[i + 1] as f64 + 0.0722 * rgba[i + 2] as f64; n += 1.0; } }
    if n == 0.0 { return Err("the region is outside the frame".into()); }
    Ok(sum / n / 255.0)
}

fn sev_of(v: &Value) -> Option<Severity> { serde_json::from_value(v.clone()).ok() }

/// Check every expectation; returns the ones that failed, each with why.
pub fn check(ss: &mut Session, list: &Value, obs: &Value) -> Vec<Value> {
    let mut fails = vec![];
    let items = list.as_array().cloned().unwrap_or_else(|| vec![list.clone()]);
    for e in items {
        let mut why: Vec<String> = vec![];
        let at = e.get("at").and_then(|v| v.as_f64());
        if let Some(r) = e.get("visible").and_then(|v| v.as_str()) {
            match (find(&ss.project, r), at) {
                (None, _) => why.push(format!("no clip {r}")),
                (Some(_), None) => why.push("visible needs at".into()),
                (Some(c), Some(t)) => if !on_screen(&ss.project, c, t) { why.push(format!("{r} is not on screen at {t:.2} s")) },
            }
        }
        if let Some(r) = e.get("hidden").and_then(|v| v.as_str()) {
            if let (Some(c), Some(t)) = (find(&ss.project, r), at) { if on_screen(&ss.project, c, t) { why.push(format!("{r} is on screen at {t:.2} s")); } }
        }
        if let Some(rg) = e.get("inside") {
            let cid = e.get("clip").and_then(|v| v.as_str()).unwrap_or("");
            match (find(&ss.project, cid), region(&ss.project, rg)) {
                (None, _) => why.push(format!("inside needs an existing clip (got '{cid}')")),
                (_, None) => why.push("inside must be title_safe, action_safe or [left, top, right, bottom]".into()),
                (Some(c), Some((rl, rt, rr, rb))) => {
                    let times: Vec<f64> = match at { Some(t) => vec![t], None => (0..5).map(|i| c.start + c.duration * (0.1 + 0.2 * i as f64)).collect() };
                    for tm in times {
                        let (cl, ct, cr, cb) = clip_box(&ss.project, c, tm);
                        if cl < rl - 1.0 || ct < rt - 1.0 || cr > rr + 1.0 || cb > rb + 1.0 { why.push(format!("{cid} box [{cl:.0}, {ct:.0}, {cr:.0}, {cb:.0}] leaves the region at {tm:.2} s")); break; }
                    }
                }
            }
        }
        if let Some(rng) = e.get("luma").and_then(|v| v.as_array()) {
            let (lo, hi) = (rng.first().and_then(|v| v.as_f64()).unwrap_or(0.0), rng.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0));
            match at { None => why.push("luma needs at".into()), Some(t) => match luma_at(ss, t, e.get("region")) {
                Ok(l) => if l < lo || l > hi { why.push(format!("brightness at {t:.2} s is {l:.3}, expected {lo}..{hi}")) },
                Err(er) => why.push(format!("could not render {t:.2} s: {er}")),
            } }
        }
        if let Some(rng) = e.get("no_black_frames").and_then(|v| v.as_array()) {
            let (a, b) = (rng.first().and_then(|v| v.as_f64()).unwrap_or(0.0), rng.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0));
            for i in 0..6 { let t = a + (b - a) * (i as f64 + 0.5) / 6.0; if let Ok(l) = luma_at(ss, t, None) { if l < 0.03 { why.push(format!("black frame at {t:.2} s")); break; } } }
        }
        if let Some(r) = e.get("readable").and_then(|v| v.as_str()) {
            match find(&ss.project, r) { None => why.push(format!("no clip {r}")), Some(c) => {
                let id = c.id.clone();
                for f in critic::text::check(&ss.project) { if f.clip_id.as_deref() == Some(id.as_str()) && (f.code == "text_too_fast" || f.code == "text_outside_title_safe") { why.push(f.message.clone()); } }
            } }
        }
        let fs: Vec<&Value> = obs.get("findings").and_then(|v| v.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
        if let Some(max) = e.get("max_findings").and_then(|v| v.as_str()).and_then(Severity::parse) {
            for f in &fs { if sev_of(&f["severity"]).map(|s| s > max).unwrap_or(false) { why.push(format!("{}: {}", f["code"].as_str().unwrap_or(""), f["message"].as_str().unwrap_or(""))); } }
        }
        if let Some(codes) = e.get("no_findings").and_then(|v| v.as_array()) {
            for f in &fs { if codes.iter().any(|c| c == &f["code"]) { why.push(format!("{}: {}", f["code"].as_str().unwrap_or(""), f["message"].as_str().unwrap_or(""))); } }
        }
        if !why.is_empty() { fails.push(json!({"expect": e, "failed": why})); }
    }
    fails
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::critic::testkit::*;
    #[test]
    fn visible_hidden_and_inside() {
        let mut ss = session();
        let t = track(&mut ss);
        let title = text(&mut ss, &t, "Hello", 1.0, 2.0);
        call(&mut ss, "set_meta", json!({"id": title, "name": "hook"}));
        let ok = check(&mut ss, &json!([{"visible": "@hook", "at": 1.5}, {"hidden": "@hook", "at": 0.5}, {"inside": "title_safe", "clip": title}]), &json!({}));
        assert!(ok.is_empty(), "{ok:?}");
        call(&mut ss, "set_property", json!({"clip_id": title, "property": "x", "value": 950.0}));
        let bad = check(&mut ss, &json!([{"inside": "title_safe", "clip": title}, {"visible": "@hook", "at": 5.0}]), &json!({}));
        assert_eq!(bad.len(), 2, "{bad:?}");
    }
    #[test]
    fn finding_thresholds() {
        let mut ss = session();
        let obs = json!({"findings": [{"code": "flash_clip", "severity": "warning", "message": "x"}]});
        assert_eq!(check(&mut ss, &json!({"max_findings": "warning"}), &obs).len(), 0);
        assert_eq!(check(&mut ss, &json!({"max_findings": "info"}), &obs).len(), 1);
        assert_eq!(check(&mut ss, &json!({"no_findings": ["flash_clip"]}), &obs).len(), 1);
    }
}
