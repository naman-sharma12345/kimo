//! 3D compositor MCP surface (comp3d.rs): set_3d_compositor (project setting: depth sorting, intersections, shadows, depth of
//! field) and set_3d_layer_shadows (per-layer casts_shadows / accepts_shadows). Every check runs before the project changes.
use super::kit::*;
use crate::comp3d::MAX_SAMPLES;
use crate::keyframe::Prop;
type Map = serde_json::Map<String, Value>;

fn obj_of<'a>(a: &'a Value, k: &str) -> Result<Option<&'a Map>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::Object(m)) => Ok(Some(m)), Some(_) => Err(format!("{k} must be an object")) } }
fn known(m: &Map, pre: &str, ok: &[&str]) -> Result<(), String> { for k in m.keys() { if !ok.contains(&k.as_str()) { return Err(format!("unknown field '{pre}.{k}' (valid: {})", ok.join(", "))); } } Ok(()) }
fn flag(m: &Map, k: &str, cur: bool, pre: &str) -> Result<bool, String> { match m.get(k) { None | Some(Value::Null) => Ok(cur), Some(Value::Bool(b)) => Ok(*b), Some(v) => v.as_f64().filter(|x| *x == 0.0 || *x == 1.0).map(|x| x != 0.0).ok_or(format!("{pre}{k} must be a boolean")) } }
fn num(m: &Map, k: &str, cur: f64, pre: &str) -> Result<f64, String> { match m.get(k) { None | Some(Value::Null) => Ok(cur), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or(format!("{pre}{k} must be a finite number")) } }
fn count(m: &Map, cur: u32, pre: &str) -> Result<u32, String> { match m.get("samples") { None | Some(Value::Null) => Ok(cur), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or(format!("{pre}samples must be a number"))?;
    if x.fract() != 0.0 || x < 1.0 || x > MAX_SAMPLES as f64 { return Err(format!("{pre}samples must be an integer 1..{MAX_SAMPLES}, got {x}")); } Ok(x as u32) } } }

const DOF_KEYS: &[&str] = &["enabled", "focus_distance", "aperture", "max_blur_px", "samples"];
const SH_KEYS: &[&str] = &["enabled", "light_id", "softness", "samples", "color", "opacity"];

pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_3d_compositor", description: "3D compositor for rot_x/rot_y/pos_z layers: per-pixel depth sort, plane intersections, cast shadows, depth of field. Off = old stack order.", mutates: true,
        schema: obj(vec![("enabled", json!({"type":"boolean","description":"compositor on/off; consecutive 3D layers form one group, a 2D layer breaks it; only fields you pass change"})),
            ("dof", json!({"type":"object","description":"thin-lens depth of field: CoC px = aperture*f*|1/focus_distance - 1/z|, f = (H/2)/tan(fov/2)","properties":{"enabled":{"type":"boolean"},"focus_distance":{"type":"number","description":"camera depth px, 0 = focal length"},"aperture":{"type":"number","description":"lens diameter, world px, 0..10000"},"max_blur_px":{"type":"number","description":"CoC cap px 0..512, default 64"},"samples":{"type":"integer","description":"1..256, default 32"}}})),
            ("shadows", json!({"type":"object","description":"exact ray-plane shadows from project lights (see set_light); layers cast/accept per set_3d_layer_shadows","properties":{"enabled":{"type":"boolean"},"light_id":{"type":["string","null"],"description":"one project light; null = all enabled"},"softness":{"type":"number","description":"light radius px, 0 = hard"},"samples":{"type":"integer","description":"1..256, default 16"},"color":crate::commands::col_schema("shadow colour, default black", false),"opacity":{"type":"number","description":"0..1, default 1"}}}))], &[]),
        run: |ss, a| { let top = a.as_object().ok_or("arguments must be an object")?; let mut c = ss.project.comp3d.clone().unwrap_or_default();
            c.enabled = flag(top, "enabled", c.enabled, "")?;
            if let Some(m) = obj_of(a, "dof")? { known(m, "dof", DOF_KEYS)?; let d = &mut c.dof;
                d.enabled = flag(m, "enabled", d.enabled, "dof.")?; d.focus_distance = num(m, "focus_distance", d.focus_distance, "dof.")?; d.aperture = num(m, "aperture", d.aperture, "dof.")?;
                d.max_blur_px = num(m, "max_blur_px", d.max_blur_px, "dof.")?; d.samples = count(m, d.samples, "dof.")?; }
            if let Some(m) = obj_of(a, "shadows")? { known(m, "shadows", SH_KEYS)?; let s = &mut c.shadows;
                s.enabled = flag(m, "enabled", s.enabled, "shadows.")?; s.softness = num(m, "softness", s.softness, "shadows.")?; s.opacity = num(m, "opacity", s.opacity, "shadows.")?; s.samples = count(m, s.samples, "shadows.")?;
                match m.get("light_id") { None => {} Some(Value::Null) => s.light_id = None, Some(Value::String(id)) => s.light_id = Some(id.clone()), Some(_) => return Err("shadows.light_id must be a light id string or null".into()) }
                if m.contains_key("color") { s.color = crate::commands::color(&a["shadows"], "color", s.color)?; } }
            c.validate(&ss.project.lights)?;
            let cam = crate::three_d::Camera::from_props(&ss.project.camera, 0.0); let f = (ss.project.height as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan();
            let focus = if c.dof.focus_distance > 0.0 { c.dof.focus_distance } else { f };
            ss.project.comp3d = Some(c.clone()); Ok(json!({"comp3d": c, "focal_px_at_0": f, "focus_distance_px": focus})) } },
    Command { name: "set_3d_layer_shadows", description: "Per-layer 3D compositor shadows: casts_shadows (default off, as After Effects), accepts_shadows (default on). Needs compositor shadows on.", mutates: true,
        schema: obj(vec![("clip_id", s("clip")), ("casts_shadows", json!({"type":"boolean","description":"this layer darkens other 3D layers"})), ("accepts_shadows", json!({"type":"boolean","description":"other layers' shadows fall on this layer"}))], &["clip_id"]),
        run: |ss, a| { let cid = str_arg(a, "clip_id")?.to_string(); let top = a.as_object().ok_or("arguments must be an object")?;
            if !top.contains_key("casts_shadows") && !top.contains_key("accepts_shadows") { return Err("pass casts_shadows and/or accepts_shadows".into()); }
            let c = ss.project.find_clip(&cid).ok_or(format!("no such clip '{cid}'"))?;
            let cur = |k: &str, d: bool| c.props.get(k).map_or(d, |p| p.base != 0.0);
            let (cs, ac) = (flag(top, "casts_shadows", cur("casts_shadows", false), "")?, flag(top, "accepts_shadows", cur("accepts_shadows", true), "")?);
            let c = ss.project.find_clip_mut(&cid).ok_or(format!("no such clip '{cid}'"))?;
            for (k, v) in [("casts_shadows", cs), ("accepts_shadows", ac)] { if top.get(k).map_or(false, |x| !x.is_null()) { c.props.entry(k.to_string()).or_insert(Prop::new(0.0)).base = if v { 1.0 } else { 0.0 }; } }
            Ok(json!({"clip_id": cid, "casts_shadows": cs, "accepts_shadows": ac})) } },
] }
