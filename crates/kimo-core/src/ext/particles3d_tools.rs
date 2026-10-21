//! K4: 3D particle layer tools (Source::Particles3D, see particles3d.rs). Fields arrive as the nested objects the source stores.
use super::kit::*;
use crate::model::*;
use crate::particles3d::Particles3D;
/// Merge the tool's nested objects (emitter / physics / sprite / motion_blur, plus blend) over `base` field by field, then parse.
fn merged(a: &Value, base: Value) -> Result<Particles3D, String> {
    let mut v = base;
    for k in ["emitter", "physics", "sprite", "motion_blur"] { let Some(x) = a.get(k) else { continue };
        if x.is_null() { v[k] = Value::Null; continue; } let o = x.as_object().ok_or_else(|| format!("{k} must be an object"))?;
        if !v[k].is_object() { v[k] = json!({}); } for (kk, vv) in o { v[k][kk.as_str()] = vv.clone(); } }
    if let Some(b) = a.get("blend") { v["blend"] = b.clone(); }
    let mut e: Particles3D = serde_json::from_value(v).map_err(|e| format!("bad particles3d config: {e}"))?; e.normalize(); Ok(e)
}
fn schema(first: Vec<(&str, Value)>, req: &[&str]) -> Value {
    let mut v = first; v.extend([
        ("emitter", json!({"type":"object","description":"kind point|box|sphere|light, position [x,y,z], position_keys [{t,position}], size [x,y,z], radius, light_id, rate, lifetime, speed, speed_var, direction, spread_deg, seed"})),
        ("physics", json!({"type":"object","description":"gravity [x,y,z] world px/s^2, drag (linear, 1/s, >= 0); closed form"})),
        ("sprite", json!({"type":"object","description":"kind disc|layer, clip_id, texture_time birth|current, hide_source, size (world px), size_end, hardness, color, color_end, opacity_curve [[life,opacity]]"})),
        ("blend", s("additive | normal (normal = depth sorted back to front)")),
        ("motion_blur", json!({"type":"object","description":"samples 1..64, shutter_angle degrees"}))]);
    obj(v, req)
}
pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_particles3d", description: "Add a 3D particle layer seen through the project camera: point/box/sphere/light emitter, closed-form gravity+drag, disc or clip sprites.", mutates: true,
        schema: schema(vec![("track_id", s("track")), ("start", n("s")), ("duration", n("s"))], &["track_id", "start", "duration"]),
        run: |ss, a| { let tid = str_arg(a, "track_id")?.to_string(); let (start, dur) = (num_arg(a, "start")?, num_arg(a, "duration")?);
            if !start.is_finite() || start < 0.0 || !dur.is_finite() || dur <= 0.0 { return Err("start must be >= 0 and duration > 0 (seconds)".into()); }
            if !ss.project.tracks.iter().any(|t| t.id == tid) { return Err(format!("no such track {tid}")); }
            let e = merged(a, serde_json::to_value(Particles3D::default()).map_err(|e| e.to_string())?)?; e.validate(&ss.project, None)?;
            let blend = if e.blend == "additive" { "add" } else { "normal" }; let id = ss.project.gen_id("clip");
            let c = Clip { id: id.clone(), source: Source::Particles3D(e), start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, props: default_props(), grade_graph: None, effects: vec![], blend: blend.into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None };
            ss.project.tracks.iter_mut().find(|t| t.id == tid).ok_or("no such track")?.clips.push(std::sync::Arc::new(c)); Ok(json!({"clip_id": id})) } },
    Command { name: "set_particles3d", description: "Edit a 3D particle layer: only the emitter/physics/sprite/motion_blur fields you pass change (motion_blur null = off); same checks as add.", mutates: true,
        schema: schema(vec![("clip_id", s("particles3d clip"))], &["clip_id"]),
        run: |ss, a| { let id = str_arg(a, "clip_id")?.to_string();
            let c = ss.project.find_clip(&id).ok_or_else(|| format!("no such clip {id}"))?; let Source::Particles3D(cur) = &c.source else { return Err(format!("clip {id} is not a particles3d layer (add_particles3d makes one)")) };
            let e = merged(a, serde_json::to_value(cur).map_err(|e| e.to_string())?)?; e.validate(&ss.project, Some(id.as_str()))?;
            let nb = if a.get("blend").is_some() { Some(if e.blend == "additive" { "add" } else { "normal" }) } else { None };
            let cm = ss.project.find_clip_mut(&id).ok_or("no such clip")?; cm.source = Source::Particles3D(e); if let Some(b) = nb { cm.blend = b.into(); }
            Ok(json!({"clip_id": id})) } },
] }
