//! Data-driven and variable text in ONE clip: source-text keyframes (the text itself changes at set times, for lists,
//! scoreboards, lyrics, data read from a CSV) and variable-font axes / tabular figures on a text clip.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::model::{Source, TextKey};

fn text_clip<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut crate::model::Clip, String> {
    let id = str_arg(a, "clip_id")?.to_string();
    let c = ss.project.find_clip_mut(&id).ok_or_else(|| format!("no such clip '{id}'"))?;
    if !matches!(c.source, Source::Text { .. }) { return Err(format!("{id} is not a text clip")); }
    Ok(c)
}

fn set_text_keyframes(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let keys = a.get("keys").and_then(|v| v.as_array()).ok_or("keys: [{time, text}, ...]")?;
    let mut parsed: Vec<TextKey> = vec![];
    for (i, k) in keys.iter().enumerate() {
        let t = k.get("time").and_then(|v| v.as_f64()).filter(|t| t.is_finite() && *t >= 0.0).ok_or_else(|| format!("keys[{i}].time: clip-local seconds >= 0"))?;
        let text = k.get("text").and_then(|v| v.as_str()).ok_or_else(|| format!("keys[{i}].text: a string"))?;
        parsed.push(TextKey { t, text: text.replace("\\n", "\n") });
    }
    if parsed.len() > 5000 { return Err("at most 5000 text keyframes".into()); }
    let replace = a.get("replace").and_then(|v| v.as_bool()).unwrap_or(true);
    let c = text_clip(ss, a)?; let fx = c.text_fx.get_or_insert_with(Default::default);
    if replace { fx.text_keys.clear(); }
    for k in parsed { fx.text_keys.retain(|x| (x.t - k.t).abs() > 1e-6); fx.text_keys.push(k); }
    fx.text_keys.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap_or(std::cmp::Ordering::Equal));
    Ok(json!({"text_keys": fx.text_keys.len(), "note": "before the first key the clip shows its own text; {num:...}, {t:...} and {tc} tokens still work inside key text"}))
}

fn set_font_axes(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let c = text_clip(ss, a)?; let fx = c.text_fx.get_or_insert_with(Default::default);
    if let Some(ax) = a.get("axes").and_then(|v| v.as_object()) {
        for (tag, v) in ax {
            if tag.len() != 4 || !tag.is_ascii() { return Err(format!("axis '{tag}': a 4-letter tag such as wdth, slnt, opsz, ital")); }
            if tag == "wght" { return Err("weight is set_text_weight's (axis wght)".into()); }
            let prop = match v {
                Value::Null => { fx.axes.remove(tag); continue; }
                Value::Number(n) => Prop::new(n.as_f64().unwrap_or(0.0)),
                Value::Array(keys) => { let mut p = Prop::new(0.0); for (i, k) in keys.iter().enumerate() {
                    let (t, x) = (k.get("time").and_then(|v| v.as_f64()), k.get("value").and_then(|v| v.as_f64()));
                    let (Some(t), Some(x)) = (t, x) else { return Err(format!("axes.{tag}[{i}]: {{time, value}}")) };
                    let e: Ease = serde_json::from_value(k.get("ease").cloned().unwrap_or(json!("in_out"))).map_err(|e| e.to_string())?;
                    p.set_key(t, x, e); if i == 0 { p.base = x; } } p }
                _ => return Err(format!("axes.{tag}: a number, [{{time, value, ease?}}] keyframes, or null to remove")),
            };
            fx.axes.insert(tag.clone(), prop);
        }
    }
    if let Some(b) = a.get("tabular").and_then(|v| v.as_bool()) { fx.tabular = b; }
    Ok(json!({"axes": fx.axes.keys().collect::<Vec<_>>(), "tabular": fx.tabular, "note": "axes a font does not have are ignored; values outside an axis's range are clamped by the font"}))
}

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "set_text_keyframes", description: "Source-text keyframes: the text of ONE text clip changes at set times (clip-local seconds), for lists, scoreboards, lyrics, step counters or rows of data (read a CSV yourself and pass its rows). Before the first key the clip shows its own text. replace (default true) clears earlier keys.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("keys", json!({"type":"array","items":{"type":"object","properties":{"time":{"type":"number"},"text":{"type":"string"}},"required":["time","text"]}})), ("replace", json!({"type":"boolean"}))], &["clip_id","keys"]),
            run: set_text_keyframes },
        Command { name: "set_font_axes", description: "Variable-font axes beyond weight on a text clip, fixed or keyframed: wdth (width), slnt (slant), opsz (optical size), ital, or a font's custom 4-letter axis. tabular:true gives every digit the same width so counters and timers do not jiggle.", mutates: true,
            schema: obj(vec![("clip_id", s("text clip")), ("axes", json!({"type":"object","description":"{\"wdth\": 75} or {\"wdth\": [{\"time\":0,\"value\":75},{\"time\":1,\"value\":125}]}; null removes an axis"})), ("tabular", json!({"type":"boolean","description":"tabular (fixed-width) figures"}))], &["clip_id"]),
            run: set_font_axes },
    ]
}
