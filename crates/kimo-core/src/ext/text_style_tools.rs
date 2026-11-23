//! Text styling beyond one look per clip: stacked shading layers (Resolve Text+ style), mixed styling inside one clip
//! (character ranges with their own colour, size, baseline shift, super- and subscript), After Effects-style advanced
//! selectors on a text animator (expression selector, character offset for decode/scramble, per-unit skew, stacked
//! selectors), and text that dissolves into particles.
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::{AnimatorMore, ExtraSelector, ShadeLayer, Source, TextRun};

fn text_clip<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut crate::model::Clip, String> {
    let id = str_arg(a, "clip_id")?.to_string();
    let c = ss.project.find_clip_mut(&id).ok_or_else(|| format!("no such clip '{id}'"))?;
    if !matches!(c.source, Source::Text { .. }) { return Err(format!("{id} is not a text clip")); }
    Ok(c)
}
fn rgb(v: Option<&Value>) -> Option<[f64; 3]> { v.and_then(|v| v.as_array()).filter(|a| a.len() == 3).map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0), a[2].as_f64().unwrap_or(0.0)]) }
/// A number or keyframes [{time, value, ease?}] as a Prop.
fn prop_of(v: &Value, what: &str) -> Result<Prop, String> {
    match v { Value::Number(n) => Ok(Prop::new(n.as_f64().unwrap_or(0.0))), Value::Array(keys) => { let mut p = Prop::new(0.0);
        for (i, k) in keys.iter().enumerate() { let (t, x) = (k.get("time").and_then(|v| v.as_f64()), k.get("value").and_then(|v| v.as_f64())); let (Some(t), Some(x)) = (t, x) else { return Err(format!("{what}[{i}]: {{time, value}}")) };
            let e: crate::keyframe::Ease = serde_json::from_value(k.get("ease").cloned().unwrap_or(json!("in_out"))).map_err(|e| e.to_string())?; p.set_key(t, x, e); if i == 0 { p.base = x; } } Ok(p) }
        _ => Err(format!("{what}: a number or [{{time, value}}] keyframes")) }
}

fn set_text_layers(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let arr = a.get("layers").and_then(|v| v.as_array()).ok_or("layers: [{kind, color, ...}] back to front; [] removes them")?;
    let mut ls = vec![];
    for (i, l) in arr.iter().enumerate() {
        let kind = l.get("kind").and_then(|v| v.as_str()).unwrap_or("fill").to_string();
        if !["fill", "outline", "shadow", "glow", "border"].contains(&kind.as_str()) { return Err(format!("layers[{i}].kind: fill|outline|shadow|glow|border")); }
        let color = rgb(l.get("color")).ok_or_else(|| format!("layers[{i}].color: [r,g,b] 0..1"))?;
        let f = |k: &str, d: f64| l.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        ls.push(ShadeLayer { kind, color, opacity: f("opacity", 1.0).clamp(0.0, 1.0), width: f("width", 0.0).clamp(0.0, 500.0), blur: f("blur", 0.0).clamp(0.0, 200.0), dx: f("dx", 0.0), dy: f("dy", 0.0), radius: f("radius", 0.0).max(0.0), knockout: l.get("knockout").and_then(|v| v.as_bool()).unwrap_or(false) });
    }
    if ls.len() > 8 { return Err("at most 8 shading layers".into()); }
    let c = text_clip(ss, a)?; let fx = c.text_fx.get_or_insert_with(Default::default); fx.layers = ls;
    Ok(json!({"layers": fx.layers.len(), "note": "drawn back to front; while layers are set they replace the clip's own fill, outline and shadow"}))
}

fn set_text_runs(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let arr = a.get("runs").and_then(|v| v.as_array()).ok_or("runs: [{from, to, color?, scale?, baseline?, script?}]; [] removes them")?;
    let n_chars = { let c = text_clip(ss, a)?; if let Source::Text { text, .. } = &c.source { text.chars().count() } else { 0 } };
    let mut rs = vec![];
    for (i, r) in arr.iter().enumerate() {
        // either character indices, or `match`: every occurrence of a word or phrase
        if let Some(m) = r.get("match").and_then(|v| v.as_str()) {
            let c = text_clip(ss, a)?; let Source::Text { text, .. } = &c.source else { unreachable!() };
            let chars: Vec<char> = text.chars().collect(); let mc: Vec<char> = m.chars().collect(); if mc.is_empty() { return Err(format!("runs[{i}].match is empty")); }
            let mut k = 0; let mut found = 0; while k + mc.len() <= chars.len() { if chars[k..k + mc.len()].iter().zip(&mc).all(|(x, y)| x.to_lowercase().eq(y.to_lowercase())) { rs.push(run_from(r, k, k + mc.len())?); found += 1; k += mc.len(); } else { k += 1; } }
            if found == 0 { return Err(format!("runs[{i}]: '{m}' does not occur in the text")); }
            continue;
        }
        let (from, to) = (r.get("from").and_then(|v| v.as_u64()), r.get("to").and_then(|v| v.as_u64()));
        let (Some(from), Some(to)) = (from, to) else { return Err(format!("runs[{i}]: give from/to (character indices, end exclusive) or match (a word or phrase)")) };
        if from >= to || to as usize > n_chars { return Err(format!("runs[{i}]: 0 <= from < to <= {n_chars}")); }
        rs.push(run_from(r, from as usize, to as usize)?);
    }
    let c = text_clip(ss, a)?; let fx = c.text_fx.get_or_insert_with(Default::default); fx.runs = rs;
    Ok(json!({"runs": fx.runs.len()}))
}
fn run_from(r: &Value, from: usize, to: usize) -> Result<TextRun, String> {
    let script = r.get("script").and_then(|v| v.as_str()).map(String::from);
    if let Some(sc) = &script { if sc != "super" && sc != "sub" { return Err("script: super or sub".into()); } }
    Ok(TextRun { from, to, color: rgb(r.get("color")), scale: r.get("scale").and_then(|v| v.as_f64()), baseline: r.get("baseline").and_then(|v| v.as_f64()), script })
}

fn set_animator_selectors(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let aid = str_arg(a, "animator_id")?.to_string();
    let mut more = AnimatorMore::default();
    if let Some(e) = a.get("expr").and_then(|v| v.as_str()) {
        // validate now with sample values, so a typo is reported here rather than silently ignored at render time
        let v = |n: &str| -> Result<Option<f64>, String> { Ok(match n { "i" => Some(1.0), "n" => Some(4.0), "t" => Some(0.5), "k" => Some(0.5), "r" => Some(0.3), _ => None }) };
        crate::ext::batch_calc::eval(e, &v).map_err(|m| format!("expr: {m}"))?; more.expr = Some(e.to_string());
    }
    if let Some(v) = a.get("char_offset") { more.char_offset = Some(prop_of(v, "char_offset")?); }
    if let Some(v) = a.get("skew") { more.skew = Some(prop_of(v, "skew")?); }
    if let Some(sels) = a.get("selectors").and_then(|v| v.as_array()) { for (i, s) in sels.iter().enumerate() {
        let mode = s.get("mode").and_then(|v| v.as_str()).unwrap_or("add").to_string();
        if !["add", "subtract", "intersect", "min", "max", "difference"].contains(&mode.as_str()) { return Err(format!("selectors[{i}].mode: add|subtract|intersect|min|max|difference")); }
        let g = |k: &str, d: f64| -> Result<Prop, String> { s.get(k).map(|v| prop_of(v, k)).unwrap_or(Ok(Prop::new(d))) };
        more.selectors.push(ExtraSelector { mode, start: g("start", 0.0)?, end: g("end", 1.0)?, offset: g("offset", 0.0)?, smoothness: g("smoothness", 0.0)?, shape: s.get("shape").and_then(|v| v.as_str()).unwrap_or("square").to_string() });
    } }
    let c = text_clip(ss, a)?; let fx = c.text_fx.as_mut().ok_or("this clip has no text animators (add_text_animator first)")?;
    let an = fx.animators.iter_mut().find(|x| x.id == aid).ok_or_else(|| format!("no animator '{aid}' on this clip"))?;
    an.more = Some(Box::new(more));
    Ok(json!({"animator": aid, "ok": true}))
}

/// The text breaks up into particles: an emitter spawning on the text's own silhouette while the text fades out.
fn text_dissolve(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?.to_string();
    let (start, dur, ti, color) = { let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip '{cid}'"))?; let Source::Text { color, .. } = &c.source else { return Err(format!("{cid} is not a text clip")) };
        (c.start, c.duration, ss.project.tracks.iter().position(|t| t.clips.iter().any(|x| x.id == cid)).unwrap_or(0), *color) };
    let at = opt_num(a, "at", (dur - 1.2).max(0.0)).clamp(0.0, dur); let len = opt_num(a, "duration", 1.0).clamp(0.2, 6.0);
    let dir = a.get("direction").and_then(|v| v.as_str()).unwrap_or("up");
    let angle = match dir { "up" => 270.0, "down" => 90.0, "left" => 180.0, "right" => 0.0, "out" => 270.0, _ => return Err("direction: up|down|left|right|out".into()) };
    // fade the text out over the dissolve
    { let c = ss.project.find_clip_mut(&cid).unwrap(); let mut o = c.props.get("opacity").cloned().unwrap_or_else(|| Prop::new(1.0)); o.set_key(at, 1.0, crate::keyframe::Ease::InOut); o.set_key((at + len * 0.8).min(dur), 0.0, crate::keyframe::Ease::Hold); c.props.insert("opacity".into(), o); }
    let tid = ss.project.gen_id("track"); ss.project.tracks.insert(ti + 1, crate::model::Track { id: tid.clone(), clips: vec![], locked: false, muted: false, solo: false, name: "Text particles".into() });
    let pc = rgb(a.get("color")).unwrap_or(color);
    let cfg = json!({"shape": "layer", "emit_clip": cid, "emit_follow": true, "rate": opt_num(a, "amount", 2500.0), "life": 1.1, "life_var": 0.5, "speed": 140, "speed_var": 90, "angle": angle, "spread": if dir == "out" { 180 } else { 40 },
        "turbulence": 60, "turb_scale": 0.012, "drag": 0.6, "size0": 2.6, "size1": 0.6, "size_var": 0.5, "color0": pc, "color1": pc, "alpha0": 1.0, "alpha1": 0.0, "sprite": "soft", "seed": 7});
    let r = crate::commands::dispatch_nested(ss, "add_emitter", &json!({"track_id": tid, "start": start + at, "duration": (len + 1.6).min(dur - at + 1.6), "config": cfg}))?;
    Ok(json!({"track_id": tid, "emitter": r, "dissolve": {"at": at, "duration": len}}))
}

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "set_text_layers", description: "Stacked shading layers on a text clip (Resolve Text+ style), up to 8, drawn back to front: fill, outline (width px; knockout:true = outline only), shadow (dx, dy, width spread, blur), glow (width, blur) and border (a rounded box around the text: width = padding, radius). Each has color, opacity and blur. While set they replace the clip's own fill/outline/shadow; [] removes them.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("layers", json!({"type":"array","items":{"type":"object","properties":{"kind":{"type":"string","enum":["fill","outline","shadow","glow","border"]},"color":{"type":"array","items":{"type":"number"}},"opacity":{"type":"number"},"width":{"type":"number"},"blur":{"type":"number"},"dx":{"type":"number"},"dy":{"type":"number"},"radius":{"type":"number"},"knockout":{"type":"boolean"}},"required":["kind","color"]}}))], &["clip_id","layers"]),
            run: set_text_layers },
        Command { name: "set_text_runs", description: "Mixed styling inside ONE text clip: ranges with their own colour, size (scale), baseline shift (px, up positive) or script super/sub (for 10^6, H2O, footnote marks). Give from/to character indices (end exclusive) or match: a word or phrase (every occurrence). [] removes the runs.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("runs", json!({"type":"array","items":{"type":"object","properties":{"from":{"type":"integer"},"to":{"type":"integer"},"match":{"type":"string"},"color":{"type":"array","items":{"type":"number"}},"scale":{"type":"number"},"baseline":{"type":"number"},"script":{"type":"string","enum":["super","sub"]}}}}))], &["clip_id","runs"]),
            run: set_text_runs },
        Command { name: "set_animator_selectors", description: "Advanced selectors on a text animator (After Effects style): expr = expression selector, the amount per unit from i (unit index), n (units), t (s), k (the range selector's amount), r (stable random 0..1), e.g. \"smoothstep(0, 1, t*2 - i/n)\"; char_offset = selected letters step through the alphabet/digits (number or keyframes: decode and scramble effects); skew = per-unit slant; selectors = more range selectors combined by mode add|subtract|intersect|min|max|difference.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("animator_id", s("from add_text_animator")), ("expr", s("expression selector")), ("char_offset", json!({"description":"number or [{time,value}]"})), ("skew", json!({"description":"number or [{time,value}], fraction of height"})),
                ("selectors", json!({"type":"array","items":{"type":"object","properties":{"mode":{"type":"string"},"start":{},"end":{},"offset":{},"smoothness":{},"shape":{"type":"string"}}}}))], &["clip_id","animator_id"]),
            run: set_animator_selectors },
        Command { name: "text_dissolve", description: "The text breaks up into particles: an emitter spawns on the text's own silhouette (in its colour, or color) while the text fades out, drifting up/down/left/right or bursting out. at = clip-local start (default 1.2 s before the end), duration of the dissolve, amount = particles per second.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("at", n("clip-local seconds")), ("duration", n("seconds, default 1")), ("direction", json!({"type":"string","enum":["up","down","left","right","out"]})), ("amount", n("particles per second, default 2500")), ("color", json!({"type":"array","items":{"type":"number"}}))], &["clip_id"]),
            run: text_dissolve },
    ]
}
