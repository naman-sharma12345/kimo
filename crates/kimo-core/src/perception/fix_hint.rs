//! U-FIX: advisory fix hints for perception findings. A hint names one existing primitive tool and ready-to-call args, why (with the
//! measured number) and a confidence. Hints are never applied. next_steps is the severity-ordered shortlist of runnable hints (max 3).
use crate::model::{Clip, Project, Source};
use serde_json::{json, Value};

pub const KINDS: [&str; 5] = ["set_param", "move", "resize", "retime", "none"];
pub fn hint(kind: &str, tool: &str, args: Value, why: String, confidence: &str) -> Value { json!({"kind": kind, "tool": tool, "args": args, "why": why, "confidence": confidence}) }
pub fn none(why: &str) -> Value { json!({"kind": "none", "why": why}) }
/// Value of a property that is a plain constant (no keyframes, no expressions); None when animated, so a one-shot hint would be wrong.
fn constant(c: &Clip, k: &str, default: f64) -> Option<f64> { match c.props.get(k) { None => Some(default), Some(p) if p.keys.is_empty() && p.expr.is_empty() => Some(p.base), _ => None } }
fn r2(v: f64) -> f64 { (v * 100.0).round() / 100.0 }
fn edges(v: &Value) -> [f64; 4] { let g = |k: &str| v[k].as_f64().unwrap_or(0.); [g("left"), g("top"), g("right"), g("bottom")] }
/// Hints that bring a layer whose bounds overshoot a rectangle by `outside` (px per edge) back inside: shift x and y, or shrink when it is
/// wider or taller than the rectangle. Returns every call needed, first call is the hint.
fn shift_steps(c: &Clip, outside: &Value, rect: [f64; 4], what: &str) -> Result<Vec<(Value, String)>, String> {
    if c.parent.is_some() { return Err("the layer has a parent, so its x/y are relative to the parent".into()); }
    let e = edges(outside); let (wide, tall) = (e[0] > 1e-8 && e[2] > 1e-8, e[1] > 1e-8 && e[3] > 1e-8);
    if wide || tall {
        let sc = constant(c, "scale", 1.0).ok_or("scale is animated")?;
        let f = ((rect[2] - e[0] - e[2]) / rect[2]).min((rect[3] - e[1] - e[3]) / rect[3]).min(1.0).max(0.01) * 0.999;
        return Ok(vec![(json!({"clip_id": c.id, "property": "scale", "value": r2(sc * f * 100.0).max(0.01) / 100.0}), format!("{what} is larger than the target area (overshoot left {:.1}, right {:.1}, top {:.1}, bottom {:.1} px); scale to {:.1}% of the current size", e[0], e[2], e[1], e[3], f * 100.0))]);
    }
    let mut v = vec![];
    for (axis, d) in [("x", e[0] - e[2]), ("y", e[1] - e[3])] { if d.abs() > 1e-8 { let cur = constant(c, axis, 0.0).ok_or(format!("{axis} is animated"))?; v.push((json!({"clip_id": c.id, "property": axis, "value": cur + d + d.signum() * 0.01}), format!("{what} overshoots by {:.1} px on {}; move {axis} by {:.2} px", d.abs(), if axis == "x" { if d > 0. { "the left" } else { "the right" } } else if d > 0. { "the top" } else { "the bottom" }, d))); } }
    Ok(v)
}
type Step = (&'static str, Value, String, &'static str, &'static str);
pub struct Opts { pub min_cap_1080: f64, pub upscale_limit: f64 }
fn prop_step(args: Value, why: String, kind: &'static str, conf: &'static str) -> Step { ("set_property", args, why, kind, conf) }
/// Attach `fix_hint` to one lint finding (a JSON row from lint_frame) and push the runnable steps for next_steps.
pub fn lint(p: &Project, f: &mut Value, o: &Opts, steps: &mut Vec<Value>) {
    let code = f["code"].as_str().unwrap_or("").to_string(); let id = f["layer"]["id"].as_str().unwrap_or("").to_string(); let sev = f["severity"].as_str().unwrap_or("info").to_string();
    let m = f["measured"].clone(); let (w, h) = (p.width as f64, p.height as f64);
    let Some(c) = p.find_clip(&id) else { f["fix_hint"] = none("the layer is no longer in the project"); return };
    let res: Result<Vec<Step>, String> = (|| match code.as_str() {
        "out_of_bounds" => { if m["fully_offscreen"] == true { return Err("the layer is fully off canvas; its intended position is unknown".into()); } Ok(shift_steps(c, &m["outside_px"], [0., 0., w, h], "the layer bounds")?.into_iter().map(|(a, y)| prop_step(a, y, "move", "exact")).collect()) }
        "text_overflow" => Ok(shift_steps(c, &m["outside_px"], [0., 0., w, h], "the text ink")?.into_iter().map(|(a, y)| prop_step(a, y, "move", "exact")).collect()),
        "safe_zone" => { if m["zone"]["kind"] != "safe" { return Err("the text overlaps an avoid zone; where to place it is a design choice".into()); } let z = m["zone"]["rect"].as_array().ok_or("zone rect missing")?; let zr = [0, 1, 2, 3].map(|i| z[i].as_f64().unwrap_or(0.)); let conf = if m["estimated"] == true { "estimate" } else { "exact" }; Ok(shift_steps(c, &m["outside_px"], zr, "the text ink")?.into_iter().map(|(a, y)| prop_step(a, y, "move", conf)).collect()) }
        "invisible_layer" => { if m["fully_offscreen"] == true { return Err("the layer is off canvas; intent unknown".into()); } constant(c, "opacity", 1.0).ok_or("opacity is animated")?; Ok(vec![prop_step(json!({"clip_id": c.id, "property": "opacity", "value": 1.0}), format!("opacity is {:.3} at this sample; set it to 1 if the layer should show", m["opacity"].as_f64().unwrap_or(0.)), "set_param", "estimate")]) }
        "low_res_upscale" => { let ratio = m["ratio"].as_f64().ok_or("ratio missing")?; let sc = constant(c, "scale", 1.0).ok_or("scale is animated")?; Ok(vec![prop_step(json!({"clip_id": c.id, "property": "scale", "value": sc * o.upscale_limit / ratio * 0.99}), format!("the layer is drawn at {ratio:.2}x its source pixels (limit {}); scale it down below the limit", o.upscale_limit), "resize", "estimate")]) }
        "min_text_size" => { let Source::Text { size, .. } = &c.source else { return Err("not a text clip".into()) }; let cap = m["cap_height_1080_px"].as_f64().ok_or("cap missing")?; if cap <= 0. { return Err("measured cap height is 0".into()); } let k = cap / m["cap_height_canvas_px"].as_f64().filter(|v| *v > 0.).unwrap_or(1.); let eff = (cap - k).max(k * 0.5); let ns = (size * o.min_cap_1080 / eff * 1.02).ceil(); Ok(vec![("set_text", json!({"clip_id": c.id, "size": ns}), format!("capital-H ink is {cap:.1} px at 1080p, minimum {}; raise size from {size} to {ns}", o.min_cap_1080), "resize", "estimate")]) }
        "text_over_foreground" | "caption_over_foreground" => {
            if c.parent.is_some() { return Err("the text has a parent, so its x/y are relative to the parent".into()); }
            let rect = |v: &Value| -> Option<[f64; 4]> { let a = v.as_array()?; if a.len() < 4 { return None; } Some([0, 1, 2, 3].map(|i| a[i].as_f64().unwrap_or(0.))) };
            let ink = rect(&f["rect"]).ok_or("ink rect missing")?; let sub = rect(&m["subject_bbox"]).ok_or("the subject box is unknown")?;
            let safe = [0.05 * w, 0.05 * h, 0.95 * w, 0.95 * h];
            // candidate moves: put the ink just above, below, left or right of the subject box, inside the title-safe rectangle
            let mut best: Option<(f64, &str, f64)> = None;
            for (axis, d) in [("y", sub[1] - 1.0 - (ink[1] + ink[3])), ("y", sub[1] + sub[3] + 1.0 - ink[1]), ("x", sub[0] - 1.0 - (ink[0] + ink[2])), ("x", sub[0] + sub[2] + 1.0 - ink[0])] {
                let n = if axis == "x" { [ink[0] + d, ink[1]] } else { [ink[0], ink[1] + d] };
                if n[0] < safe[0] || n[1] < safe[1] || n[0] + ink[2] > safe[2] || n[1] + ink[3] > safe[3] { continue; }
                if best.map_or(true, |b| d.abs() < b.0) { best = Some((d.abs(), axis, d)); }
            }
            let (_, axis, d) = best.ok_or("no position inside the title-safe area clears the subject box")?;
            let cur = constant(c, axis, 0.0).ok_or(format!("{axis} is animated"))?;
            Ok(vec![prop_step(json!({"clip_id": c.id, "property": axis, "value": r2(cur + d)}), format!("move {axis} by {:.1} px so the text ink clears the covered layer's box inside the title-safe area (the box is {})", d, m["basis"].as_str().unwrap_or("rect")), "move", "estimate")])
        }
        "text_contrast" => Err(format!("contrast ratio {:.2} is below the limit; the fix depends on the background, so pick a text colour or add a backing shape", m["min_ratio"].as_f64().unwrap_or(0.))),
        other => Err(format!("no automatic fix for {other}")),
    })();
    match res {
        Ok(v) if !v.is_empty() => { let mut it = v.into_iter(); let (tool, args, why, kind, conf) = it.next().unwrap(); let more: Vec<Step> = it.collect();
            let why = if more.is_empty() { why } else { format!("{why}; a second call follows in next_steps") };
            f["fix_hint"] = hint(kind, tool, args.clone(), why.clone(), conf); steps.push(json!({"tool": tool, "args": args, "why": why, "severity": sev}));
            for (t, a, y, _, _) in more { steps.push(json!({"tool": t, "args": a, "why": y, "severity": sev})); } }
        Ok(_) => f["fix_hint"] = none("nothing to change"),
        Err(why) => f["fix_hint"] = none(&why),
    }
}
/// Severity-ordered shortlist (warn before info), at most 3, runnable hints only.
pub fn next_steps(mut steps: Vec<Value>) -> Value {
    steps.sort_by_key(|s| match s["severity"].as_str() { Some("error") => 0, Some("warn") => 1, _ => 2 });
    Value::Array(steps.into_iter().take(3).map(|s| json!({"tool": s["tool"], "args": s["args"], "why": s["why"]})).collect())
}
