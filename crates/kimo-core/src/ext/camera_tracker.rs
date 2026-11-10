//! 3D camera tracker finishing tools on top of a stored camera solve (After Effects 3D Camera Tracker class): set_ground_plane (orient and scale the
//! solved scene from tracked ground points), place_at_track (a 3D layer sitting on a solved point), export_st_map (16-bit ST map of the solved lens),
//! plus the non-destructive `lens_undistort` / `lens_redistort` effects. Lens model: Brown-Conrady k1, k2, p1, p2 (sfm::distort_norm, OpenCV order).
use super::camera_solve::{apply_keys, getrec, solve_intr, STORE};
use super::kit::*;
use crate::effects::{Frame, Params};
use crate::sfm;
use std::collections::BTreeMap;

/// Layer scale that makes a solid/text placed at point `pi` show at its nominal pixel size on the solve's first registered frame: the
/// layer sits at camera depth z = (R X + t)_z * scale_px px, where kimo's pinhole (focal f) draws it at f / z, so scale = z / f. 1 if the
/// point is not in front of that camera.
fn depth_scale(sol: &sfm::Solution, pi: usize, scale_px: f64) -> f64 {
    let z = sol.poses.iter().flatten().next().map(|p| p.apply(&sol.points[pi])[2] * scale_px).unwrap_or(0.0);
    if z.is_finite() && z > 0.0 && sol.intr.f > 0.0 { z / sol.intr.f } else { 1.0 }
}
fn set_scale(c: &mut crate::model::Clip, s: f64) { c.props.entry("scale".to_string()).or_insert(crate::keyframe::Prop::new(1.0)).base = s; }
/// Solved point (solver units) -> Kimo world pixels, the mapping apply_camera_solve's cam_* keys assume: world = X * scale_px + (0, 0, -f).
pub fn kimo_pos(x: &sfm::V3, f: f64, scale_px: f64) -> [f64; 3] { [x[0] * scale_px, x[1] * scale_px, x[2] * scale_px - f] }

// ---------- lens effects ----------
fn lens_check(kind: &str, k: &sfm::Intr) -> Result<(), String> {
    let ok = k.f.is_finite() && k.f >= 1.0 && k.cx.is_finite() && k.cy.is_finite() && [k.k1, k.k2].iter().all(|v| v.is_finite() && v.abs() <= 1.0) && [k.p1, k.p2].iter().all(|v| v.is_finite() && v.abs() <= 0.2);
    if ok { Ok(()) } else { Err(format!("{kind}: bad lens parameters (focal >= 1 px, |k1|,|k2| <= 1, |p1|,|p2| <= 0.2)")) }
}
/// Lens model for a w x h frame. strings.solve_id: the whole model (focal, principal point, k1, k2, p1, p2) of that stored solve, scaled from the
/// solve's frame size. Otherwise explicit params; focal_px 0 takes the focal of the latest solve (an all-zero lens needs no focal at all).
fn lens_for_frame(kind: &str, params: &Params, strings: &BTreeMap<String, String>, w: usize, h: usize, t: f64) -> Result<sfm::Intr, String> {
    let g = |k: &str, d: f64| params.get(k).map(|x| x.at(t)).unwrap_or(d);
    if let Some(id) = strings.get("solve_id") {
        let (_, ki, sw, sh) = solve_intr(Some(id.as_str())).ok_or_else(|| format!("{kind}: no camera solve '{id}' in this session (solves are not saved with the project)"))?;
        let (sx, sy) = (w as f64 / sw.max(1) as f64, h as f64 / sh.max(1) as f64); let k = sfm::Intr { f: ki.f * sy, cx: ki.cx * sx, cy: ki.cy * sy, ..ki }; lens_check(kind, &k)?; return Ok(k);
    }
    let (k1, k2, p1, p2) = (g("k1", 0.0), g("k2", 0.0), g("p1", 0.0), g("p2", 0.0)); let mut f = g("focal_px", 0.0);
    if f <= 0.0 { f = match solve_intr(None) { Some((_, ki, _, sh)) => ki.f * h as f64 / sh.max(1) as f64, None if k1 == 0.0 && k2 == 0.0 && p1 == 0.0 && p2 == 0.0 => h.max(1) as f64, None => return Err(format!("{kind}: focal_px is 0 and no camera solve exists; set focal_px or strings.solve_id")) }; }
    let k = sfm::Intr { f, cx: g("cx", 0.5) * w as f64, cy: g("cy", 0.5) * h as f64, k1, k2, p1, p2 }; lens_check(kind, &k)?; Ok(k)
}
/// Alpha-weighted bilinear sample at continuous position (sx, sy) (pixel centres at +0.5). None when the position is outside the pixel-centre grid.
fn sample(src: &[f32], w: usize, h: usize, sx: f64, sy: f64) -> Option<[f32; 4]> {
    let (fx, fy) = (sx - 0.5, sy - 0.5); let e = 1e-6; let (mx, my) = ((w - 1) as f64, (h - 1) as f64);
    if !(fx >= -e && fy >= -e && fx <= mx + e && fy <= my + e) { return None; }
    let (fx, fy) = (fx.clamp(0.0, mx), fy.clamp(0.0, my)); let (x0, y0) = (fx.floor() as usize, fy.floor() as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (ax, ay) = ((fx - x0 as f64) as f32, (fy - y0 as f64) as f32); let mut acc = [0f32; 4];
    for (xx, yy, wt) in [(x0, y0, (1.0 - ax) * (1.0 - ay)), (x1, y0, ax * (1.0 - ay)), (x0, y1, (1.0 - ax) * ay), (x1, y1, ax * ay)] {
        let i = (yy * w + xx) * 4; let a = src[i + 3] * wt; acc[0] += src[i] * a; acc[1] += src[i + 1] * a; acc[2] += src[i + 2] * a; acc[3] += a; }
    if acc[3] > 0.0 { Some([acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]]) } else { Some([0.0; 4]) }
}
/// `lens_undistort`: output pixel (ideal pinhole) samples the source at distort_px(ideal). `lens_redistort`: output pixel (distorted) samples the
/// source at undistort_px(distorted). Exact per-pixel inverse mapping, bilinear, positions outside the source stay transparent.
pub fn lens_effect(kind: &str, params: &Params, strings: &BTreeMap<String, String>, f: &mut Frame, t: f64) -> Result<(), String> {
    if f.w == 0 || f.h == 0 { return Ok(()); }
    let k = lens_for_frame(kind, params, strings, f.w, f.h, t)?; let undist = kind == "lens_undistort"; let src = f.d.clone(); let (w, h) = (f.w, f.h);
    for y in 0..h { crate::guard::check(); for x in 0..w { let o = (x as f64 + 0.5, y as f64 + 0.5); let s = if undist { sfm::distort_px(o, &k) } else { sfm::undistort_px(o, &k) };
        let i = (y * w + x) * 4; f.d[i..i + 4].copy_from_slice(&sample(&src, w, h, s.0, s.1).unwrap_or([0.0; 4])); } }
    Ok(())
}

// ---------- ST map ----------
/// ST map of a lens (Nuke STMap convention): for output pixel (x, y) the source position p = distort_px (undistort map) or undistort_px (redistort map)
/// of the pixel centre, written as s = p.x / w, t = 1 - p.y / h (so an identity map is s = (x + 0.5) / w, t = 1 - (y + 0.5) / h, t = 0 at the bottom).
/// Returns per-pixel [s, t] (row 0 = top) and the number of pixels whose s or t fell outside 0..1 (clamped in the 16-bit file).
pub fn st_map(k: &sfm::Intr, w: usize, h: usize, undistort: bool) -> (Vec<[f64; 2]>, usize) {
    let mut out = Vec::with_capacity(w * h); let mut outside = 0;
    for y in 0..h { crate::guard::check(); for x in 0..w { let o = (x as f64 + 0.5, y as f64 + 0.5); let p = if undistort { sfm::distort_px(o, k) } else { sfm::undistort_px(o, k) };
        let st = [p.0 / w as f64, 1.0 - p.1 / h as f64]; if !(st[0] >= 0.0 && st[0] <= 1.0 && st[1] >= 0.0 && st[1] <= 1.0) { outside += 1; } out.push(st); } }
    (out, outside)
}
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256]; for (i, e) in table.iter_mut().enumerate() { let mut c = i as u32; for _ in 0..8 { c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 }; } *e = c; }
    let mut crc = !0u32; for &b in data { crc = table[((crc ^ b as u32) & 0xff) as usize] ^ (crc >> 8); } !crc
}
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) { out.extend_from_slice(&(data.len() as u32).to_be_bytes()); let start = out.len(); out.extend_from_slice(tag); out.extend_from_slice(data); let c = crc32(&out[start..]); out.extend_from_slice(&c.to_be_bytes()); }
/// 16-bit RGB PNG (filter 0, zlib stored blocks): deterministic and dependency-free.
pub fn png16_rgb(w: usize, h: usize, px: &[[u16; 3]]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (1 + w * 6)); for y in 0..h { raw.push(0u8); for x in 0..w { for c in px[y * w + x].iter() { raw.extend_from_slice(&c.to_be_bytes()); } } }
    let mut z = vec![0x78u8, 0x01]; let n = raw.len().div_ceil(65535).max(1);
    for (i, block) in raw.chunks(65535).enumerate() { z.push(u8::from(i + 1 == n)); let len = block.len() as u16; z.extend_from_slice(&len.to_le_bytes()); z.extend_from_slice(&(!len).to_le_bytes()); z.extend_from_slice(block); }
    let (mut a, mut b) = (1u32, 0u32); for &v in &raw { a = (a + v as u32) % 65521; b = (b + a) % 65521; } z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec(); let mut hd = Vec::new(); hd.extend_from_slice(&(w as u32).to_be_bytes()); hd.extend_from_slice(&(h as u32).to_be_bytes()); hd.extend_from_slice(&[16, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &hd); chunk(&mut out, b"IDAT", &z); chunk(&mut out, b"IEND", &[]); out
}
fn lens_from_args(a: &Value, w: usize, h: usize) -> Result<(sfm::Intr, String), String> {
    let explicit = ["k1", "k2", "p1", "p2", "focal_px"].iter().any(|k| a.get(*k).map_or(false, |v| !v.is_null()));
    let k = if explicit { let f = a.get("focal_px").and_then(|v| v.as_f64()).ok_or("explicit lens coefficients need focal_px")?;
        (sfm::Intr { f, cx: opt_num(a, "cx", 0.5) * w as f64, cy: opt_num(a, "cy", 0.5) * h as f64, k1: opt_num(a, "k1", 0.0), k2: opt_num(a, "k2", 0.0), p1: opt_num(a, "p1", 0.0), p2: opt_num(a, "p2", 0.0) }, "explicit".to_string()) }
    else { let (id, ki, sw, sh) = solve_intr(a.get("solve_id").and_then(|v| v.as_str())).ok_or_else(|| match a.get("solve_id").and_then(|v| v.as_str()) { Some(s) => format!("no solve '{s}'"), None => "no camera solve found; run solve_camera or give k1..p2 + focal_px".to_string() })?;
        let (sx, sy) = (w as f64 / sw.max(1) as f64, h as f64 / sh.max(1) as f64); (sfm::Intr { f: ki.f * sy, cx: ki.cx * sx, cy: ki.cy * sy, ..ki }, id) };
    lens_check("export_st_map", &k.0)?; Ok(k)
}
fn export_st_map(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let dir = a.get("direction").and_then(|v| v.as_str()).unwrap_or("undistort"); if !matches!(dir, "undistort" | "redistort") { return Err(format!("direction must be undistort or redistort (got '{dir}')")); }
    let path = str_arg(a, "path")?.to_string();
    let dim = |k: &str, d: u32| -> Result<usize, String> { let v = opt_num(a, k, d as f64); if v.fract() != 0.0 || !(1.0..=16384.0).contains(&v) { return Err(format!("{k} must be a whole number of pixels in 1..=16384")); } Ok(v as usize) };
    let (w, h) = (dim("width", ss.project.width)?, dim("height", ss.project.height)?); let (k, from) = lens_from_args(a, w, h)?;
    let (st, outside) = st_map(&k, w, h, dir == "undistort"); let q = |v: f64| (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
    let px: Vec<[u16; 3]> = st.iter().map(|p| [q(p[0]), q(p[1]), 0]).collect();
    std::fs::write(&path, png16_rgb(w, h, &px)).map_err(|e| format!("write {path}: {e}"))?;
    Ok(json!({"path": path, "width": w, "height": h, "direction": dir, "lens": {"focal_px": k.f, "cx": k.cx, "cy": k.cy, "k1": k.k1, "k2": k.k2, "p1": k.p1, "p2": k.p2, "from": from},
        "convention": "16-bit RGB PNG, Nuke STMap: R = s = src_x / width, G = t = 1 - src_y / height (identity: s = (x+0.5)/w, t = 1-(y+0.5)/h), B = 0. undistort map: src = distort(output); redistort map: src = undistort(output)",
        "clamped_pixels": outside}))
}

// ---------- set_ground_plane ----------
fn index_list(a: &Value, k: &str) -> Result<Vec<usize>, String> {
    let arr = a.get(k).and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, k, "array"))?;
    arr.iter().map(|v| v.as_u64().map(|x| x as usize).ok_or_else(|| format!("{k} entries must be point indices (whole numbers >= 0), got {v}"))).collect()
}
fn set_ground_plane(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let up = a.get("up").and_then(|v| v.as_str()).unwrap_or("y"); if up != "y" { return Err(format!("up must be y (got '{up}'): Kimo's vertical axis is y, pointing down, so world up is -y")); }
    let ids = index_list(a, "track_ids")?; if ids.len() < 3 { return Err(format!("set_ground_plane needs at least 3 track_ids (point indices of the solve), got {}", ids.len())); }
    let scale_pair = match a.get("scale_tracks") { None | Some(Value::Null) => None, Some(_) => { let v = index_list(a, "scale_tracks")?; if v.len() != 2 { return Err("scale_tracks must be exactly two point indices [a, b]".into()); } Some((v[0], v[1])) } };
    let dist = a.get("scale_distance").and_then(|v| v.as_f64()); if scale_pair.is_some() != dist.is_some() { return Err("scale_tracks and scale_distance go together: give both or neither".into()); }
    if let Some(d) = dist { if !(d.is_finite() && d > 0.0) { return Err("scale_distance must be a positive finite number".into()); } }
    let (id, res, applied, moves) = {
        let mut m = STORE.lock().unwrap(); let id = getrec(&m, a)?.0; let r = m.get_mut(&id).ok_or(format!("no solve '{id}'"))?;
        if r.sol.mode != "full" { return Err(format!("set_ground_plane needs a full 3D solve; solve '{id}' is {} (its points are view directions, there is no ground to fit)", r.sol.mode)); }
        let np = r.sol.points.len(); let origin = match a.get("origin_track") { None | Some(Value::Null) => ids[0], Some(v) => v.as_u64().ok_or("origin_track must be a point index")? as usize };
        let mut check = ids.clone(); check.push(origin); if let Some((pa, pb)) = scale_pair { check.push(pa); check.push(pb); }
        for &i in &check { if i >= np { return Err(format!("no point {i} in solve '{id}' (it has {np} points; ids are indices into its point list)")); } }
        let s = match (scale_pair, dist) { (Some((pa, pb)), Some(d)) => { let (xa, xb) = (r.sol.points[pa], r.sol.points[pb]); let l = sfm::norm(&[xa[0] - xb[0], xa[1] - xb[1], xa[2] - xb[2]]); if !(l > 1e-12) { return Err("scale_tracks points coincide; pick two distinct points".into()); } d / l } _ => 1.0 };
        let pts: Vec<sfm::V3> = ids.iter().map(|&i| r.sol.points[i]).collect(); let cams: Vec<sfm::V3> = r.sol.poses.iter().flatten().map(|p| p.center()).collect();
        let gp = sfm::ground_plane(&pts, &cams, &r.sol.points[origin], s, &[0.0, -1.0, 0.0])?;
        sfm::apply_similarity(&mut r.sol, gp.s, &gp.r, &gp.t);
        let hs: Vec<f64> = r.sol.poses.iter().flatten().map(|p| -p.center()[1]).collect(); let height = if hs.is_empty() { f64::NAN } else { hs.iter().sum::<f64>() / hs.len() as f64 };
        r.sol.notes.push(format!("set_ground_plane: {} points, origin point {origin}, scale {s:.6}, plane fit rms {:.6} (units before scaling)", ids.len(), gp.residual_rms));
        let f = r.sol.intr.f; let moves: Vec<(String, [f64; 3], Option<f64>)> = r.placed.iter().filter(|p| p.1 < np).map(|(c, pi, sc, sized)| (c.clone(), kimo_pos(&r.sol.points[*pi], f, *sc), if *sized { Some(depth_scale(&r.sol, *pi, *sc)) } else { None })).collect();
        (id, json!({"normal_before": gp.normal_before, "centroid_before": gp.centroid_before, "residual_rms": gp.residual_rms, "residual_rms_scaled": gp.residual_rms * gp.s, "scale": gp.s, "origin_track": origin, "up": [0.0, -1.0, 0.0], "camera_height": height, "points_used": ids.len()}), r.applied, moves)
    };
    let mut out = res; out["solve_id"] = json!(id);
    match applied { Some((sc, fov)) => { let k = apply_keys(ss, &json!({"solve_id": id, "scale_px": sc, "fov": fov}))?; out["reapplied"] = json!(true); out["keys"] = k["keys"].clone(); }
        None => { out["reapplied"] = json!(false); out["note"] = json!("the solve was not applied yet: run apply_camera_solve to write the re-oriented camera"); } }
    let mut moved = vec![]; for (cid, p, dsc) in moves { if let Some(c) = ss.project.find_clip_mut(&cid) { set_pos(c, p); if let Some(s) = dsc { set_scale(c, s); } moved.push(cid); } } out["moved_layers"] = json!(moved);
    Ok(out)
}
fn set_pos(c: &mut crate::model::Clip, p: [f64; 3]) {
    // pos_z exactly 0 would make the renderer treat the layer as 2D (render.rs checks rot_x/rot_y/pos_z != 0): nudge it off zero
    let pz = if p[2] == 0.0 { 1e-9 } else { p[2] };
    for (k, v) in [("x", p[0]), ("y", p[1]), ("pos_z", pz)] { c.props.entry(k.to_string()).or_insert(crate::keyframe::Prop::new(0.0)).base = v; }
}

// ---------- place_at_track ----------
fn place_at_track(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("null"); if !matches!(kind, "null" | "solid" | "text") { return Err(format!("kind must be null, solid or text (got '{kind}')")); }
    let pi = a.get("track_id").and_then(|v| v.as_u64()).ok_or_else(|| crate::errors::arg_err(a, "track_id", "integer"))? as usize;
    let (id, pos, start, dur, sc, applied, dscale) = { let m = STORE.lock().unwrap(); let (id, r) = getrec(&m, a)?;
        if r.sol.mode != "full" { return Err(format!("place_at_track needs a full 3D solve; solve '{id}' is {} (no depth)", r.sol.mode)); }
        let np = r.sol.points.len(); if pi >= np { return Err(format!("no point {pi} in solve '{id}' (it has {np} points; track_id is an index into its point list)")); }
        let sc = a.get("scale_px").and_then(|v| v.as_f64()).or(r.applied.map(|x| x.0)).unwrap_or(1000.0); if !(sc.is_finite() && sc > 0.0) { return Err("scale_px must be a positive finite number".into()); }
        (id.clone(), kimo_pos(&r.sol.points[pi], r.sol.intr.f, sc), r.clip_start, r.sol.poses.len() as f64 / r.fps, sc, r.applied.is_some(), depth_scale(&r.sol, pi, sc)) };
    if let Some(lt) = a.get("layer_track").and_then(|v| v.as_str()) { if !ss.project.tracks.iter().any(|t| t.id == lt) { return Err(format!("no track '{lt}'")); } }
    let (tid, made_track) = match a.get("layer_track").and_then(|v| v.as_str()) { Some(t) => (t.to_string(), false), None => (crate::commands::dispatch_nested(ss, "add_track", &json!({}))?["track_id"].as_str().ok_or("add_track returned no track_id")?.to_string(), true) };
    let color = a.get("color").cloned().unwrap_or(json!([1.0, 0.35, 0.2]));
    let made = match kind {
        "null" => crate::commands::dispatch_nested(ss, "add_null", &json!({"track_id": tid, "start": start, "duration": dur})),
        "solid" => { let sz = opt_num(a, "size", 40.0); crate::commands::dispatch_nested(ss, "add_shape", &json!({"track_id": tid, "shape": "rect", "width": sz, "height": sz, "fill": color, "start": start, "duration": dur})) }
        _ => crate::commands::dispatch_nested(ss, "add_text", &json!({"track_id": tid, "text": a.get("text").and_then(|v| v.as_str()).unwrap_or("+"), "size": opt_num(a, "size", 48.0), "color": color, "start": start, "duration": dur})),
    };
    let cid = match made.and_then(|r| r["clip_id"].as_str().map(String::from).ok_or_else(|| "layer creation returned no clip_id".to_string())) { Ok(c) => c, Err(e) => { if made_track { ss.project.tracks.retain(|t| t.id != tid); } return Err(e); } };
    let sized = kind != "null";
    if let Some(c) = ss.project.find_clip_mut(&cid) { set_pos(c, pos); if sized { set_scale(c, dscale); } }
    if let Some(r) = STORE.lock().unwrap().get_mut(&id) { r.placed.push((cid.clone(), pi, sc, sized)); }
    let mut out = json!({"clip_id": cid, "kind": kind, "layer_track": tid, "solve_id": id, "track_id": pi, "position": pos, "scale_px": sc, "span": [start, start + dur],
        "mapping": "x, y, pos_z = X*scale_px, Y*scale_px, Z*scale_px - focal_px (the cam_* key convention)"});
    if sized { out["layer_scale"] = json!(dscale); }
    if !applied { out["note"] = json!("the solve is not applied yet: run apply_camera_solve (same scale_px) so the layer sits on the track"); }
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_ground_plane", description: "Orient a camera solve from >=3 solved ground points: plane fit, normal to up (-y), origin, optional scale. Re-applies cam_* keys if applied.", mutates: true,
        schema: obj(vec![("solve_id", s("default: latest")), ("track_ids", json!({"type":"array","items":{"type":"integer"},"description":">=3 point indices of the solve (export_camera_solve points order)"})), ("origin_track", json!({"type":"integer","description":"point placed at the origin; default first id"})),
            ("scale_tracks", json!({"type":"array","items":{"type":"integer"},"description":"two point indices [a, b]"})), ("scale_distance", n("world units between scale_tracks")), ("up", json!({"type":"string","enum":["y"],"description":"vertical axis; Kimo y points down, so up = -y"}))], &["track_ids"]), run: set_ground_plane },
    Command { name: "place_at_track", description: "Add a 3D layer (null, solid or text) at a solved point of a camera solve, in the cam_* key world mapping, so it sits on the track.", mutates: true,
        schema: obj(vec![("solve_id", s("default: latest")), ("track_id", json!({"type":"integer","description":"point index of the solve"})), ("kind", json!({"type":"string","enum":["null","solid","text"],"description":"default null (invisible)"})), ("size", n("on-screen px at the first solved frame (layer scaled by depth/focal); solid default 40, text 48")),
            ("text", s("text for kind text, default +")), ("color", json!({"type":["array","string"],"description":"[r,g,b] 0..1 or hex"})), ("scale_px", n("default: the applied scale, else 1000")), ("layer_track", s("timeline track; default a new track"))], &["track_id"]), run: place_at_track },
    Command { name: "export_st_map", description: "Write a 16-bit PNG ST map (Nuke STMap: R=s, G=t) of a solved or explicit Brown-Conrady lens, undistort or redistort.", mutates: false,
        schema: obj(vec![("solve_id", s("lens from this solve (default latest)")), ("k1", n("radial k1")), ("k2", n("radial k2")), ("p1", n("tangential p1")), ("p2", n("tangential p2")), ("focal_px", n("focal px, needed with explicit k/p")), ("cx", n("principal point x, fraction of width, default 0.5")), ("cy", n("principal point y, fraction, default 0.5")),
            ("width", n("px, default project width")), ("height", n("px, default project height")), ("direction", json!({"type":"string","enum":["undistort","redistort"],"description":"default undistort"})), ("path", s("output .png path"))], &["path"]), run: export_st_map },
] }
