//! Graph-editor control over keyframes from tools: After Effects-style speed and influence per key, copying an ease
//! from one property to another, and a read-out of the value and speed graphs.
//!
//! Speed and influence map onto a key's tangent handles (keyframe.rs `Handle`): the outgoing handle reaches
//! `influence` of the way to the next key in time (dt) and leaves at `speed` units per second (dv = speed x dt); the
//! incoming handle likewise toward the previous key. Influence 33.3% with speed 0 is AE's Easy Ease.
use super::kit::*;
use crate::keyframe::{Handle, Prop};

fn prop_mut<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut Prop, String> {
    let cid = str_arg(a, "clip_id")?.to_string(); let k = str_arg(a, "property")?.to_string();
    let c = ss.project.find_clip_mut(&cid).ok_or_else(|| format!("no such clip '{cid}'"))?;
    match a.get("effect_id").and_then(|v| v.as_str()) {
        Some(e) => c.effects.iter_mut().find(|x| x.id == e).ok_or_else(|| format!("no effect '{e}' on {cid}"))?.params.get_mut(&k).ok_or_else(|| format!("effect {e} has no keyed '{k}'")),
        None => c.props.get_mut(&k).ok_or_else(|| format!("{cid} has no '{k}' property")),
    }
}
fn key_index(p: &Prop, t: f64) -> Result<usize, String> {
    p.keys.iter().position(|k| (k.t - t).abs() < 1e-3).ok_or_else(|| format!("no key at {t} s (keys at {:?})", p.keys.iter().map(|k| (k.t * 1000.0).round() / 1000.0).collect::<Vec<_>>()))
}

fn set_key_velocity(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = num_arg(a, "time")?;
    let infl = |k: &str| -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_f64().filter(|x| (0.1..=100.0).contains(x)).map(|x| Some(x / 100.0)).ok_or_else(|| format!("{k}: 0.1..100 (percent)")) } };
    let (oi, ii) = (infl("out_influence")?, infl("in_influence")?);
    let (os, is) = (a.get("out_speed").and_then(|v| v.as_f64()), a.get("in_speed").and_then(|v| v.as_f64()));
    let p = prop_mut(ss, a)?; let i = key_index(p, t)?;
    let n = p.keys.len();
    if os.is_some() || oi.is_some() {
        if i + 1 >= n { return Err("the last key has no outgoing segment".into()); }
        let seg = p.keys[i + 1].t - p.keys[i].t; let prev = p.keys[i].out;
        let f = oi.unwrap_or_else(|| prev.map_or(1.0 / 3.0, |h| (h.dt / seg).clamp(0.001, 1.0))); let dt = f * seg;
        let sp = os.unwrap_or_else(|| prev.map_or(0.0, |h| if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }));
        p.keys[i].out = Some(Handle { dt, dv: sp * dt });
    }
    if is.is_some() || ii.is_some() {
        if i == 0 { return Err("the first key has no incoming segment".into()); }
        let seg = p.keys[i].t - p.keys[i - 1].t; let prev = p.keys[i].inn;
        let f = ii.unwrap_or_else(|| prev.map_or(1.0 / 3.0, |h| (h.dt / seg).clamp(0.001, 1.0))); let dt = f * seg;
        let sp = is.unwrap_or_else(|| prev.map_or(0.0, |h| if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }));
        p.keys[i].inn = Some(Handle { dt, dv: sp * dt });
        // the segment arriving here is shaped by the previous key's out handle too; give it one if it has none, so the
        // incoming ease is not overridden by that key's preset ease
        if p.keys[i - 1].out.is_none() { p.keys[i - 1].out = Some(Handle { dt: seg / 3.0, dv: (p.keys[i].v - p.keys[i - 1].v) / 3.0 }); }
    }
    let k = &p.keys[i];
    Ok(json!({"time": k.t, "in": k.inn.map(|h| json!({"speed": if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }, "dt": h.dt})), "out": k.out.map(|h| json!({"speed": if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }, "dt": h.dt}))}))
}

/// Copies each source key's ease (preset ease, bezier and handles as influence + speed relative to the segment's own
/// change in value) onto the target keys with the same index, rescaled to the target's segments.
fn copy_ease(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let src = prop_mut(ss, a)?.clone();
    let targets = a.get("to").and_then(|v| v.as_array()).ok_or("to: [{clip_id?, property, effect_id?}, ...]")?.clone();
    let mut done = vec![];
    for (ti, t) in targets.iter().enumerate() {
        let mut ta = t.clone(); if ta.get("clip_id").is_none() { ta["clip_id"] = a["clip_id"].clone(); }
        let p = prop_mut(ss, &ta).map_err(|e| format!("to[{ti}]: {e}"))?;
        let n = p.keys.len().min(src.keys.len());
        for i in 0..n {
            p.keys[i].ease = src.keys[i].ease; p.keys[i].bez = src.keys[i].bez;
            // each handle keeps its influence and its speed relative to the segment's average speed, so the shape transfers across units
            if let Some(h) = src.keys[i].out { if i + 1 < n {
                let (sseg, sval) = (src.keys[i + 1].t - src.keys[i].t, src.keys[i + 1].v - src.keys[i].v);
                let (tseg, tval) = (p.keys[i + 1].t - p.keys[i].t, p.keys[i + 1].v - p.keys[i].v);
                let f = (h.dt / sseg.max(1e-9)).clamp(0.0, 1.0); let rel_speed = if sval.abs() > 1e-9 { (h.dv / h.dt.max(1e-9)) / (sval / sseg.max(1e-9)) } else { 0.0 };
                let dt = f * tseg; p.keys[i].out = Some(Handle { dt, dv: rel_speed * (tval / tseg.max(1e-9)) * dt }); } }
            if let Some(h) = src.keys[i].inn { if i > 0 {
                let (sseg, sval) = (src.keys[i].t - src.keys[i - 1].t, src.keys[i].v - src.keys[i - 1].v);
                let (tseg, tval) = (p.keys[i].t - p.keys[i - 1].t, p.keys[i].v - p.keys[i - 1].v);
                let f = (h.dt / sseg.max(1e-9)).clamp(0.0, 1.0); let rel_speed = if sval.abs() > 1e-9 { (h.dv / h.dt.max(1e-9)) / (sval / sseg.max(1e-9)) } else { 0.0 };
                let dt = f * tseg; p.keys[i].inn = Some(Handle { dt, dv: rel_speed * (tval / tseg.max(1e-9)) * dt }); } }
        }
        done.push(json!({"target": t, "keys": n}));
    }
    Ok(json!({"copied": done}))
}

fn key_graph(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let p = prop_mut(ss, a)?.clone();
    if p.keys.len() < 2 { return Ok(json!({"keys": p.keys.len(), "note": "fewer than two keys: nothing animates", "no_history": true})); }
    let (t0, t1) = (p.keys[0].t, p.keys[p.keys.len() - 1].t); let n = a.get("samples").and_then(|v| v.as_u64()).unwrap_or(48).clamp(4, 400) as usize;
    let dt = (t1 - t0) / n as f64; let r = |v: f64| (v * 10000.0).round() / 10000.0;
    let pts: Vec<Value> = (0..=n).map(|i| { let t = t0 + i as f64 * dt; let h = 1e-3; json!({"t": r(t), "value": r(p.at(t)), "speed": r((p.at((t + h).min(t1)) - p.at((t - h).max(t0))) / ((t + h).min(t1) - (t - h).max(t0)).max(1e-9))}) }).collect();
    let keys: Vec<Value> = p.keys.iter().map(|k| json!({"t": r(k.t), "value": r(k.v), "ease": k.ease, "bez": k.bez,
        "in": k.inn.map(|h| json!({"speed": r(if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }), "dt": r(h.dt)})), "out": k.out.map(|h| json!({"speed": r(if h.dt > 0.0 { h.dv / h.dt } else { 0.0 }), "dt": r(h.dt)}))})).collect();
    Ok(json!({"keys": keys, "graph": pts, "no_history": true}))
}

pub fn commands() -> Vec<Command> {
    let target = || vec![("clip_id", s("clip")), ("property", s("x|y|scale|rotation|opacity... or an effect param")), ("effect_id", s("optional: an effect's param"))];
    let mut sv = target(); sv.extend([("time", n("clip-local time of the key")), ("in_speed", n("units per second arriving at the key (0 = eases in)")), ("in_influence", n("percent 0.1..100 of the previous segment the incoming handle reaches (33.3 = AE easy ease)")),
        ("out_speed", n("units per second leaving the key")), ("out_influence", n("percent 0.1..100 of the next segment the outgoing handle reaches"))]);
    let mut ce = target(); ce.push(("to", json!({"type":"array","items":{"type":"object","properties":{"clip_id":{"type":"string"},"property":{"type":"string"},"effect_id":{"type":"string"}},"required":["property"]},"description":"properties to receive the ease, key by key"})));
    let mut kg = target(); kg.push(("samples", n("graph samples, default 48")));
    vec![
        Command { name: "set_key_velocity", description: "Graph-editor control of one key: speed (units/s) and influence (%) of its incoming and outgoing handles, like After Effects' Keyframe Velocity. speed 0 + influence 33.3 = easy ease; higher influence = a longer, softer ease; a non-zero speed carries momentum through the key.", mutates: true,
            schema: obj(sv, &["clip_id","property","time"]), run: set_key_velocity },
        Command { name: "copy_ease", description: "Copy one property's keyframe easing (preset ease, bezier, handle speed and influence) onto other properties key by key, rescaled to each target's own timing and values, so x, y, scale and opacity move with the same feel.", mutates: true,
            schema: obj(ce, &["clip_id","property","to"]), run: copy_ease },
        Command { name: "key_graph", description: "Read a keyframed property's value graph and speed graph (sampled) and each key's ease, bezier and handle speed/influence. Read-only.", mutates: false,
            schema: obj(kg, &["clip_id","property"]), run: key_graph },
    ]
}
