//! link_property (After Effects pickwhip): drive one clip's property or effect param from another clip's property or effect param:
//! value(t) = driver(t - delay) * multiplier + offset. LIVE: the link is a standing record (Project.links), re-evaluated from the
//! driver's CURRENT keys/easing/expressions at every render and every sample_property read (resolve_links / eval_link_live below)
//! -- an edit to the driver after link_property ran shows up on the next render or sample, no need to call link_property again.
//! The call still writes one linear key per frame over the target clip's span as a last-bake snapshot (the target's own keys and
//! expression ops are replaced) so a tool or saved file that is not link-aware still sees a reasonable value; re-running the call
//! retargets the link (new driver, new multiplier/offset/delay). Any pair works: position follows position, scale follows another
//! layer's x, an effect param follows opacity, and so on.
use super::kit::*;
use crate::model::Project;

/// A standing link_property record: exactly the call's own arguments, enough to recompute the target's value from the driver's
/// current state at any time. One entry per (clip_id, property, effect_id) target; link_property replaces the entry for the same
/// target rather than accumulating duplicates. Absent/empty for every project that never called link_property (Project.links'
/// skip_serializing_if), so old project files stay byte-stable.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub clip_id: String, pub property: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub effect_id: Option<String>,
    pub driver_clip_id: String, pub driver_property: String, #[serde(default, skip_serializing_if = "Option::is_none")] pub driver_effect_id: Option<String>,
    pub multiplier: f64, pub offset: f64, pub delay: f64, pub fps: f64,
}

fn fnum(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k).filter(|v| !v.is_null()) { None => Ok(d), Some(v) => { let f = v.as_f64().ok_or_else(|| format!("{k} must be a number"))?; if !f.is_finite() { return Err(format!("{k} must be finite")); } Ok(f) } }
}

/// A clip/effect property's value at timeline time `t`, read the same way sample_property reads it (clip-local time via
/// render::clip_local_time, falling back to the standard default for an unkeyed standard property) -- but as a pure function of
/// a `&Project`, no Session, so it can run from inside the renderer and from a read-only command without mutating anything.
fn eval_property_at(p: &Project, clip_id: &str, property: &str, effect_id: Option<&str>, t: f64) -> Result<f64, String> {
    let c = p.find_clip(clip_id).ok_or_else(|| format!("no clip '{clip_id}'"))?;
    let lt = crate::render::clip_local_time(c, t);
    if let Some(id) = effect_id {
        let e = c.effects.iter().find(|x| x.id == id).ok_or_else(|| format!("no effect '{id}' on clip '{clip_id}'"))?;
        let pr = e.params.get(property).ok_or_else(|| format!("effect '{id}' has no param '{property}'"))?;
        return Ok(pr.at(lt));
    }
    if let Some(pr) = c.props.get(property) { return Ok(pr.at(lt)); }
    if let Some(pr) = crate::ext::curves::static_default(property) { return Ok(pr.at(lt)); }
    Err(format!("clip '{clip_id}' has no property '{property}'"))
}

/// Recompute one link's target keys from the driver's CURRENT state in `src`, writing them onto the matching clip/property in
/// `out` (a clone being built up by `resolve_links`). Best-effort: a link whose clip or property has since been removed is left
/// alone rather than failing the whole render (the same posture link_property's bake already had toward a vanished target).
fn apply_link(src: &Project, out: &mut Project, link: &Link) -> Result<(), String> {
    let tc = src.find_clip(&link.clip_id).ok_or_else(|| format!("no clip '{}'", link.clip_id))?.clone();
    let fps = if link.fps.is_finite() && link.fps > 0.0 { link.fps } else { 30.0 };
    let n = ((tc.duration * fps).floor() as usize).max(1) + 1;
    if n > 100_000 { return Err("target clip too long to link at this fps".into()); }
    let times: Vec<f64> = (0..n).map(|i| (tc.start + i as f64 / fps).min(tc.start + tc.duration)).collect();
    let mut keys = Vec::with_capacity(n); let mut lt_prev = f64::NEG_INFINITY;
    for t in &times {
        let v = eval_property_at(src, &link.driver_clip_id, &link.driver_property, link.driver_effect_id.as_deref(), t - link.delay)?;
        let y = v * link.multiplier + link.offset; if !y.is_finite() { return Err(format!("link gives a non-finite value at {t}s")); }
        let lt = crate::render::clip_local_time(&tc, *t); if lt <= lt_prev + 1e-9 { continue; } lt_prev = lt;
        keys.push(crate::keyframe::Key { t: lt, v: y, ease: crate::keyframe::Ease::Linear, bez: None, out: None, inn: None });
    }
    let c = out.find_clip_mut(&link.clip_id).ok_or("target vanished")?;
    let prop = match &link.effect_id {
        Some(e) => c.effects.iter_mut().find(|x| x.id == *e).and_then(|x| x.params.get_mut(&link.property)),
        None => c.props.get_mut(&link.property),
    };
    let prop = prop.ok_or_else(|| format!("clip '{}' has no property '{}'", link.clip_id, link.property))?;
    prop.keys = keys; prop.expr.clear();
    Ok(())
}

/// The project as rendering / sample_property should see it: every registered link resolved fresh from its CURRENT driver.
/// Zero cost (no clone) when there are no links, which is every project that never called link_property. Runs up to
/// `links.len()` passes so a chain (B follows A, C follows B) converges within one call; a genuine cycle just stops changing
/// once it stabilizes rather than looping forever. A link whose target or driver has since vanished is skipped, not fatal --
/// the rest of the project still renders.
pub fn resolve_links(p: &Project) -> std::borrow::Cow<'_, Project> {
    if p.links.is_empty() { return std::borrow::Cow::Borrowed(p); }
    let mut out = p.clone();
    for _ in 0..p.links.len().max(1) {
        let snapshot = out.clone();
        for link in &p.links { let _ = apply_link(&snapshot, &mut out, link); }
    }
    std::borrow::Cow::Owned(out)
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
    // Register (or retarget) the standing live link: a render or sample_property read from now on recomputes this
    // target fresh from the driver's current state, so an edit to the driver's keys after this call still propagates.
    let link = Link { clip_id: tid.clone(), property: prop.clone(), effect_id: teff.clone(), driver_clip_id: did.clone(), driver_property: dprop.clone(), driver_effect_id: deff.clone(), multiplier: mul, offset: off, delay, fps };
    ss.project.links.retain(|l| !(l.clip_id == link.clip_id && l.property == link.property && l.effect_id == link.effect_id));
    ss.project.links.push(link);
    Ok(json!({"clip_id": tid, "property": prop, "driver_clip_id": did, "driver_property": dprop, "multiplier": mul, "offset": off, "delay": delay, "keys": keys.len(), "range": [start, start + dur], "expressions_replaced": dropped, "note": "live link: the target recomputes from the driver's current state at every render and sample_property call; re-run link_property only to change the driver, multiplier, offset or delay"}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "link_property", description: "Pickwhip: drive a clip property or effect param from another clip's, value = driver(t - delay) * multiplier + offset. LIVE: re-evaluated from the driver's current keys/easing/expressions at every render and sample_property call, so editing the driver later is picked up automatically -- call this again only to retarget (new driver/multiplier/offset/delay).", mutates: true,
        schema: obj(vec![("clip_id", s("target clip")), ("property", s("target property, or effect param with effect_id")), ("effect_id", s("optional: target effect")), ("driver_clip_id", s("clip whose value drives the target")), ("driver_property", s("driver property or effect param, default = property")), ("driver_effect_id", s("optional: driver effect")), ("multiplier", n("default 1")), ("offset", n("added after the multiplier, default 0")), ("delay", n("seconds the target trails the driver, default 0")), ("fps", n("keys per second, default max(project fps, 30)"))], &["clip_id", "property", "driver_clip_id"]),
        run },
] }
