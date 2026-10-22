//! link_property (After Effects pickwhip): drive one clip's property or effect param from another clip's property or effect param:
//! value(t) = driver(t - delay) * multiplier + offset. The driver is read exactly as the renderer evaluates it (keys, easing,
//! expressions, defaults) through sample_property and written onto the target as one linear key per frame over the target
//! clip's span (the target's own keys and expression ops are replaced). A sampled link: after editing the driver, run the
//! same call again to refresh. Any pair works: position follows position, scale follows another layer's x, an effect param
//! follows opacity, and so on.
use super::kit::*;

fn fnum(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k).filter(|v| !v.is_null()) { None => Ok(d), Some(v) => { let f = v.as_f64().ok_or_else(|| format!("{k} must be a number"))?; if !f.is_finite() { return Err(format!("{k} must be finite")); } Ok(f) } }
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = str_arg(a, "clip_id")?.to_string(); let prop = str_arg(a, "property")?.to_string();
    let did = str_arg(a, "driver_clip_id")?.to_string(); let dprop = a.get("driver_property").and_then(|v| v.as_str()).unwrap_or(&prop).to_string();
    let teff = a.get("effect_id").and_then(|v| v.as_str()).map(String::from); let deff = a.get("driver_effect_id").and_then(|v| v.as_str()).map(String::from);
    let mul = fnum(a, "multiplier", 1.0)?; let off = fnum(a, "offset", 0.0)?; let delay = fnum(a, "delay", 0.0)?;
    let fps = fnum(a, "fps", ss.project.fps.max(30.0))?; if !(fps > 0.0 && fps <= 240.0) { return Err("fps must be in (0, 240]".into()); }
    if tid == did && prop == dprop && teff == deff { return Err("a property cannot drive itself: pick another clip or property as the driver".into()); }
    if ss.project.find_clip(&did).is_none() { return Err(format!("no driver clip '{did}'")); }
    let (start, dur) = { let c = ss.project.find_clip(&tid).ok_or_else(|| format!("no clip '{tid}'"))?; (c.start, c.duration) };
    let n = ((dur * fps).floor() as usize).max(1) + 1; if n > 100_000 { return Err("the target clip is too long to link at this fps".into()); }
    let times: Vec<f64> = (0..n).map(|i| (start + i as f64 / fps).min(start + dur)).collect();
    let dtimes: Vec<f64> = times.iter().map(|t| t - delay).collect();
    let mut q = json!({"clip_id": did, "property": dprop, "times": dtimes}); if let Some(e) = &deff { q["effect_id"] = json!(e); }
    let dv = crate::commands::dispatch_nested(ss, "sample_property", &q).map_err(|e| format!("driver: {e}"))?;
    let vals: Vec<f64> = dv["values"].as_array().ok_or("driver gave no values")?.iter().map(|v| v.as_f64().unwrap_or(f64::NAN)).collect();
    // the target must exist as a property before anything changes (sample_property validates name / effect param)
    let mut tq = json!({"clip_id": tid, "property": prop, "times": [start]}); if let Some(e) = &teff { tq["effect_id"] = json!(e); }
    crate::commands::dispatch_nested(ss, "sample_property", &tq).map_err(|e| format!("target: {e}"))?;
    let tc = ss.project.find_clip(&tid).unwrap().clone();
    let mut keys = Vec::with_capacity(n); let mut lt_prev = f64::NEG_INFINITY;
    for (t, v) in times.iter().zip(&vals) { let y = v * mul + off; if !y.is_finite() { return Err(format!("the link gives a non-finite value at {t}s; nothing was changed")); } let lt = crate::render::clip_local_time(&tc, *t); if lt <= lt_prev + 1e-9 { continue; } lt_prev = lt; keys.push(json!({"t": lt, "v": y, "ease": "linear"})); }
    let mut k = json!({"clip_id": tid, "property": prop, "keys": keys, "mode": "replace_all"}); if let Some(e) = &teff { k["effect_id"] = json!(e); }
    crate::commands::dispatch_nested(ss, "set_keyframes", &k)?;
    let c = ss.project.find_clip_mut(&tid).ok_or("target vanished")?;
    let p = match &teff { Some(e) => c.effects.iter_mut().find(|x| x.id == *e).and_then(|x| x.params.get_mut(&prop)), None => c.props.get_mut(&prop) };
    let dropped = p.map(|p| { let d = p.expr.len(); p.expr.clear(); d }).unwrap_or(0);
    Ok(json!({"clip_id": tid, "property": prop, "driver_clip_id": did, "driver_property": dprop, "multiplier": mul, "offset": off, "delay": delay, "keys": keys.len(), "range": [start, start + dur], "expressions_replaced": dropped, "note": "sampled link: run the same call again after editing the driver"}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "link_property", description: "Pickwhip: drive a clip property or effect param from another clip's, value = driver(t - delay) * multiplier + offset.", mutates: true,
        schema: obj(vec![("clip_id", s("target clip")), ("property", s("target property, or effect param with effect_id")), ("effect_id", s("optional: target effect")), ("driver_clip_id", s("clip whose value drives the target")), ("driver_property", s("driver property or effect param, default = property")), ("driver_effect_id", s("optional: driver effect")), ("multiplier", n("default 1")), ("offset", n("added after the multiplier, default 0")), ("delay", n("seconds the target trails the driver, default 0")), ("fps", n("keys per second, default max(project fps, 30)"))], &["clip_id", "property", "driver_clip_id"]),
        run },
] }
