//! RV-27 A5: transform nodes. `add_null` (an invisible transform-only clip), `set_parent` (compose a clip's x/y/scale/rotation/opacity with a parent's) and `get_transform` (the resolved transform, matrices and corners, from the same resolver the renderer uses).
use super::kit::*;
use crate::keyframe::Prop;
use crate::model::Inherit;
use crate::render::{clip_local_time, compose_xform, local_xform, resolve_xform, Xform};

const MAX_DEPTH: usize = 64;
fn parse_inherit(a: &Value, base: Inherit) -> Result<Inherit, String> {
    let mut i = base; let Some(v) = a.get("inherit") else { return Ok(i) }; if v.is_null() { return Ok(i); }
    let o = v.as_object().ok_or("inherit must be an object {position, scale, rotation, opacity} of true/false")?;
    for (k, x) in o { let b = x.as_bool().ok_or_else(|| format!("inherit.{k} must be true or false"))?; match k.as_str() { "position" => i.position = b, "scale" => i.scale = b, "rotation" => i.rotation = b, "opacity" => i.opacity = b,
        "time" => return Err("inherit.time is not supported: parents do not pass time on (v1). Valid: position, scale, rotation, opacity".into()), o => return Err(format!("inherit.{o} is not allowed. Valid: position, scale, rotation, opacity")) } }
    Ok(i)
}
/// Iterative walk up from `parent_id`: a fast-fail that never recurses and is capped at MAX_DEPTH links.
fn check_cycle(ss: &Session, child: &str, parent_id: &str) -> Result<(), String> {
    if child == parent_id { return Err(format!("parent_cycle: clip {child} cannot be its own parent")); }
    let mut cur = parent_id.to_string(); let mut path = vec![child.to_string()]; let mut hops = 0usize;
    loop { if cur == child { path.push(cur); return Err(format!("parent_cycle: making {parent_id} the parent of {child} would loop ({})", path.join(" -> "))); } path.push(cur.clone());
        match ss.project.find_clip(&cur).and_then(|c| c.parent.clone()) { Some(n) => { cur = n; hops += 1; if hops >= MAX_DEPTH { break; } } None => return Ok(()) } }
    Err(format!("parent_depth: the parent chain above {parent_id} is deeper than {MAX_DEPTH} links"))
}
fn shift(p: &mut Prop, add: f64, mul: f64) { p.base = p.base * mul + add; for k in p.keys.iter_mut() { k.v = k.v * mul + add; } }
fn ensure<'a>(c: &'a mut crate::model::Clip, k: &str) -> &'a mut Prop { c.props.entry(k.to_string()).or_insert_with(|| super::curves::static_default(k).unwrap_or_else(|| Prop::new(0.0))) }

fn add_null(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = str_arg(a, "track_id")?; let mut shape = json!({"track_id": tid, "shape": "rect", "width": 1, "height": 1, "fill": [0, 0, 0, 0], "start": a.get("start").cloned().unwrap_or(json!(0.0)), "duration": a.get("duration").cloned().unwrap_or(json!(5.0))});
    if let Some(v) = a.get("start") { shape["start"] = v.clone(); } let r = crate::commands::dispatch_nested(ss, "add_shape", &shape)?; let id = r["clip_id"].as_str().ok_or("add_shape returned no clip_id")?.to_string();
    for k in ["x", "y", "scale", "rotation", "opacity"] { if let Some(v) = a.get(k) { let f = v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| format!("{k} must be a finite number"))?; crate::commands::dispatch_nested(ss, "set_property", &json!({"clip_id": id, "property": k, "value": f}))?; } }
    Ok(json!({"clip_id": id, "null": true, "note": "invisible transform-only clip: parent other clips to it with set_parent"}))
}
fn set_parent(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?.to_string(); let mode = a.get("mode").and_then(|v| v.as_str()).unwrap_or("keep_world"); if !["keep_world", "keep_local"].contains(&mode) { return Err("mode is not allowed. Valid: keep_world, keep_local".into()); }
    let parent_id = match a.get("parent_id") { None | Some(Value::Null) => None, Some(Value::String(s)) => Some(s.clone()), _ => return Err("parent_id must be a clip id or null (unparent)".into()) };
    let child = ss.project.find_clip(&cid).ok_or(format!("no clip '{cid}'"))?; let inherit = parse_inherit(a, child.inherit)?;
    if let Some(pid) = &parent_id { ss.project.find_clip(pid).ok_or(format!("no clip '{pid}'"))?; check_cycle(ss, &cid, pid)?; }
    let at = match a.get("at") { None | Some(Value::Null) => child.start, Some(v) => v.as_f64().filter(|f| f.is_finite()).ok_or("at must be a finite number of timeline seconds")? };
    let lt0 = clip_local_time(child, at); let before = local_xform(child, lt0); let world = resolve_xform(&ss.project, child, lt0);
    let pw = match &parent_id { Some(pid) => { let p = ss.project.find_clip(pid).unwrap(); resolve_xform(&ss.project, p, clip_local_time(p, at)) } None => Xform { x: 0.0, y: 0.0, scale: 1.0, rotation: 0.0, opacity: 1.0 } };
    let mut adj = None;
    if mode == "keep_world" {
        if inherit.scale && pw.scale == 0.0 { return Err("parent_scale_zero: the parent's scale is 0 at that time, so the child's world position cannot be preserved; use mode keep_local or choose another time (at)".into()); }
        let sl = if inherit.scale { world.scale / pw.scale } else { world.scale }; let rl = if inherit.rotation { world.rotation - pw.rotation } else { world.rotation };
        let (mut px, mut py) = if inherit.position { (world.x - pw.x, world.y - pw.y) } else { (world.x, world.y) };
        if inherit.position { if inherit.rotation { let r = (-pw.rotation).to_radians(); let (sn, co) = (r.sin(), r.cos()); let (rx, ry) = (px * co - py * sn, px * sn + py * co); px = rx; py = ry; } if inherit.scale { px /= pw.scale; py /= pw.scale; } }
        let ol = if inherit.opacity && pw.opacity != 0.0 { world.opacity / pw.opacity } else if inherit.opacity { before.opacity } else { world.opacity };
        adj = Some((px - before.x, py - before.y, rl - before.rotation, if before.scale != 0.0 { sl / before.scale } else { 1.0 }, if before.opacity != 0.0 { ol / before.opacity } else { 1.0 }));
    }
    let c = ss.project.find_clip_mut(&cid).unwrap(); c.parent = parent_id.clone(); c.inherit = inherit;
    let mut out = json!({"clip_id": cid, "parent": parent_id, "mode": mode, "inherit": {"position": inherit.position, "scale": inherit.scale, "rotation": inherit.rotation, "opacity": inherit.opacity}});
    if let Some((dx, dy, dr, ms, mo)) = adj { shift(ensure(c, "x"), dx, 1.0); shift(ensure(c, "y"), dy, 1.0); shift(ensure(c, "rotation"), dr, 1.0); shift(ensure(c, "scale"), 0.0, ms); shift(ensure(c, "opacity"), 0.0, mo);
        out["adjusted"] = json!({"x": dx, "y": dy, "rotation": dr, "scale_factor": ms, "opacity_factor": mo, "at": at, "note": "local x/y/rotation keys were shifted and scale/opacity keys scaled so the world pose at `at` is unchanged; expressions are untouched"}); }
    Ok(out)
}
fn m4(x: &Xform, cx: f64, cy: f64) -> [[f64; 4]; 4] { let r = x.rotation.to_radians(); let (s, co) = (r.sin() * x.scale, r.cos() * x.scale); [[co, -s, 0.0, cx + x.x - (co * cx - s * cy)], [s, co, 0.0, cy + x.y - (s * cx + co * cy)], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] }
fn m4_inv(m: &[[f64; 4]; 4]) -> Option<[[f64; 4]; 4]> { let det = m[0][0] * m[1][1] - m[0][1] * m[1][0]; if det.abs() < 1e-300 { return None; } let (a, b, c, d) = (m[1][1] / det, -m[0][1] / det, -m[1][0] / det, m[0][0] / det); Some([[a, b, 0.0, -(a * m[0][3] + b * m[1][3])], [c, d, 0.0, -(c * m[0][3] + d * m[1][3])], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]) }
/// unit quaternion [w, x, y, z] for rotations about x then y then z (degrees): q = qz * qy * qx
pub fn euler_to_quat(rx: f64, ry: f64, rz: f64) -> [f64; 4] { let h = |d: f64| (d.to_radians() / 2.0, ); let ((sx, cx), (sy, cy), (sz, cz)) = ((h(rx).0.sin(), h(rx).0.cos()), (h(ry).0.sin(), h(ry).0.cos()), (h(rz).0.sin(), h(rz).0.cos()));
    [cz * cy * cx + sz * sy * sx, cz * cy * sx - sz * sy * cx, cz * sy * cx + sz * cy * sx, sz * cy * cx - cz * sy * sx] }
/// spherical linear interpolation of unit quaternions along the shorter arc
pub fn quat_slerp(a: [f64; 4], b: [f64; 4], u: f64) -> [f64; 4] { let mut b = b; let mut d: f64 = (0..4).map(|i| a[i] * b[i]).sum(); if d < 0.0 { d = -d; for x in b.iter_mut() { *x = -*x; } }
    if d > 0.9995 { let mut q = [0.0; 4]; for i in 0..4 { q[i] = a[i] + (b[i] - a[i]) * u; } let n = q.iter().map(|x| x * x).sum::<f64>().sqrt(); return [q[0] / n, q[1] / n, q[2] / n, q[3] / n]; }
    let th = d.clamp(-1.0, 1.0).acos(); let (s0, s1) = (((1.0 - u) * th).sin() / th.sin(), (u * th).sin() / th.sin()); let mut q = [0.0; 4]; for i in 0..4 { q[i] = a[i] * s0 + b[i] * s1; } q }
pub(crate) fn get_transform(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?; let c = ss.project.find_clip(cid).ok_or(format!("no clip '{cid}'"))?;
    let space = a.get("space").and_then(|v| v.as_str()).unwrap_or("world"); if !["local", "world", "canvas_px"].contains(&space) { return Err("space is not allowed. Valid: local, world, canvas_px".into()); }
    let t = match a.get("t") { None | Some(Value::Null) => c.start, Some(v) => v.as_f64().filter(|f| f.is_finite()).ok_or("t must be a finite number of timeline seconds")? };
    let lt = clip_local_time(c, t); let local = local_xform(c, lt); let world = resolve_xform(&ss.project, c, lt); let (w, h) = (ss.project.width as f64, ss.project.height as f64); let (cx, cy) = (w / 2.0, h / 2.0);
    let x = if space == "local" { local } else { world }; let (ox, oy) = if space == "canvas_px" { (cx, cy) } else { (0.0, 0.0) };
    // world/local matrices act on canvas-centred coordinates; canvas_px acts on top-left pixel coordinates
    let m = if space == "canvas_px" { m4(&x, cx, cy) } else { m4(&x, 0.0, 0.0) }; let inv = m4_inv(&m);
    let mc = m4(&x, cx, cy); let corners: Vec<Value> = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].iter().map(|(px, py)| json!([mc[0][0] * px + mc[0][1] * py + mc[0][3], mc[1][0] * px + mc[1][1] * py + mc[1][3]])).collect(); let _ = (ox, oy);
    let (rx, ry) = (local_prop(c, "rot_x", lt), local_prop(c, "rot_y", lt));
    let xf = |q: &Xform| json!({"x": q.x, "y": q.y, "scale": q.scale, "rotation": q.rotation, "opacity": q.opacity});
    Ok(json!({"clip_id": cid, "t": t, "local_time": lt, "space": space, "parent": c.parent, "local": xf(&local), "world": xf(&world), "transform": xf(&x), "matrix": m, "inverse": inv, "singular": inv.is_none(), "quad_canvas_px": corners, "euler_deg": [rx, ry, x.rotation], "quaternion_wxyz": euler_to_quat(rx, ry, x.rotation), "canvas": {"width": w, "height": h}}))
}
fn local_prop(c: &crate::model::Clip, k: &str, lt: f64) -> f64 { c.props.get(k).map(|p| p.at(lt)).unwrap_or(0.0) }
#[allow(dead_code)] fn _compose_is_used(a: &Xform, b: &Xform) -> Xform { compose_xform(a, b, &Inherit::default()) }
pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_null", description: "Add an invisible transform-only clip (a null) to parent other clips to. Optional x, y, scale, rotation, opacity set its starting pose; animate it like any clip with set_keyframes.", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("start", json!({"type":"number"})), ("duration", json!({"type":"number"})), ("x", json!({"type":"number"})), ("y", json!({"type":"number"})), ("scale", json!({"type":"number"})), ("rotation", json!({"type":"number","description":"degrees"})), ("opacity", json!({"type":"number"}))], &["track_id", "start", "duration"]), run: add_null },
    Command { name: "set_parent", description: "Make a clip follow another clip's transform (position, scale, rotation, opacity), or unparent it with parent_id null. World = parent world composed with the child's local pose: positions are scaled and rotated by the parent, scales multiply, rotations add, opacities multiply. inherit {position,scale,rotation,opacity} switches parts off. mode keep_world (default) shifts the child's local keys so its pose at `at` (timeline seconds, default the clip start) does not jump; keep_local keeps local values and the pose moves. Errors: parent_cycle, parent_depth (chain over 64), parent_scale_zero. 3D tilt and time are not inherited.", mutates: true,
        schema: obj(vec![("clip_id", s("child clip")), ("parent_id", json!({"type":["string","null"],"description":"parent clip id, or null to unparent"})), ("mode", json!({"type":"string","enum":["keep_world","keep_local"]})), ("inherit", json!({"type":"object","properties":{"position":{"type":"boolean"},"scale":{"type":"boolean"},"rotation":{"type":"boolean"},"opacity":{"type":"boolean"}}})), ("at", json!({"type":"number","description":"timeline seconds used by keep_world"}))], &["clip_id"]), run: set_parent },
    Command { name: "get_transform", description: "Read a clip's resolved transform at a timeline time with the renderer's own math, including matrix, corners and rotation. Read a clip's resolved transform at a timeline time with the renderer's own math: local and world x, y, scale, rotation, opacity, a 4x4 matrix and its inverse (for unprojecting canvas points), the four layer corners in canvas pixels, and the rotation as euler degrees and a unit quaternion [w,x,y,z]. space: world (default), local, or canvas_px (matrix acts on top-left pixel coordinates). Works at sub-frame times.", mutates: false,
        schema: obj(vec![("clip_id", s("clip")), ("t", json!({"type":"number","description":"timeline seconds (default clip start)"})), ("space", json!({"type":"string","enum":["local","world","canvas_px"]}))], &["clip_id"]), run: get_transform },
] }
