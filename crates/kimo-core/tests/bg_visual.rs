//! Pass 2 (visual tier, NOT the fast tier): renders contact sheets for bg_grid, bg_orbs and bg_wave into $KIMO_VISUAL_DIR so a person can look at them.
//! Without KIMO_VISUAL_DIR the rows return immediately. Pass 1 stays in bg_grid/bg_orbs/bg_wave; this only produces pixels to inspect.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn dir() -> Option<String> { std::env::var("KIMO_VISUAL_DIR").ok().filter(|d| !d.is_empty()).inspect(|d| { std::fs::create_dir_all(d).unwrap(); }) }
fn proj(w: u32, h: u32) -> (Session, String) { let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":w,"height":h,"fps":100})).unwrap(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string(); (s, t) }
fn sheet(s: &mut Session, d: &str, name: &str, times: &[f64]) { dispatch(s, "contact_sheet", &json!({"path":format!("{d}/{name}.png"),"times":times})).unwrap_or_else(|e| panic!("{name}: {e}")); }
fn put(s: &mut Session, t: &str, tool: &str, a: Value) -> Value { let mut a = a; a["track"] = json!(t); a["apply"] = json!(true); dispatch(s, tool, &a).unwrap_or_else(|e| panic!("{tool}: {e}")) }
#[test] fn visual_grid() { let Some(d) = dir() else { return };
    for (name, a) in [("grid_lines_drift", json!({"duration":4,"spacing_px":32,"opacity_pattern":0.5,"drift":[20,10]})), ("grid_dots_fade", json!({"duration":4,"mode":"dots","spacing_px":24,"line_px":4,"opacity_pattern":0.9,"fade":{"radius":150,"softness":0.8}})), ("grid_1px_moire", json!({"duration":4,"spacing_px":4,"line_px":1,"opacity_pattern":0.6}))] {
        let (mut s, t) = proj(256, 256); put(&mut s, &t, "bg_grid", a); sheet(&mut s, &d, name, &[0.0, 1.0, 2.0, 3.0]); }
    let (mut s, t) = proj(1920, 1080); put(&mut s, &t, "bg_grid", json!({"duration":2,"opacity_pattern":0.15,"drift":[30,0],"fade":{"radius":900,"softness":0.7}})); dispatch(&mut s, "render_frame", &json!({"time":1.0,"path":format!("{d}/grid_1080p.png"),"inline":"never"})).unwrap();
    let (mut s, t) = proj(1920, 1080); put(&mut s, &t, "bg_grid", json!({"duration":2,"spacing_px":80,"line_px":1.5,"opacity_pattern":0.28,"drift":[30,0],"fade":{"radius":1000,"softness":0.7}})); dispatch(&mut s, "render_frame", &json!({"time":1.0,"path":format!("{d}/grid_1080p_opacity028.png"),"inline":"never"})).unwrap();
}
#[test] fn visual_orbs() { let Some(d) = dir() else { return };
    let orbs = json!([{"cx":90,"cy":110,"radius":70,"color":[1,0.45,0.2]},{"cx":170,"cy":140,"radius":80,"color":[0.2,0.5,1],"falloff":"smooth"},{"cx":130,"cy":80,"radius":50,"color":[0.6,0.2,1],"falloff":"inverse_square","orbit":{"rx":30,"ry":20,"rate_hz":0.25,"phase":0}}]);
    for (name, extra) in [("orbs_add_dither", json!({"combine":"add","dither":1.0})), ("orbs_screen_dither", json!({"combine":"screen","dither":1.0})), ("orbs_add_nodither", json!({"combine":"add","dither":0})), ("orbs_gradient_base", json!({"base":{"type":"gradient","c1":[0.05,0.0,0.12],"c2":[0,0,0]}}))] {
        let (mut s, t) = proj(256, 256); let mut a = json!({"duration":4,"orbs":orbs}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } put(&mut s, &t, "bg_orbs", a); sheet(&mut s, &d, name, &[0.0, 1.0, 2.0, 3.0]); }
}
#[test] fn visual_wave() { let Some(d) = dir() else { return };
    let (mut s, t) = proj(256, 256); let n = 48000 * 4; let data: Vec<f64> = (0..n).map(|i| if i < 48000 * 2 { (i as f64 * 2.0 * std::f64::consts::PI * 1000.0 / 48000.0).sin() * 0.5 } else { 0.0 }).collect();
    let a = dispatch(&mut s, "import_samples", &json!({"rate":48000,"channels":1,"data":[data]})).unwrap(); let ta = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let clip = dispatch(&mut s, "add_clip", &json!({"track_id":ta,"source_type":"media","asset_id":a["asset_id"],"start":0,"duration":4})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let tv = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    put(&mut s, &tv, "bg_wave", json!({"duration":4,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":4,"bands":32,"hop_s":0.05,"chunk":true},"glow":{"radius_px":10,"intensity":0.8},"pulse":{"rate_hz":1,"amount":0.3},"stroke_px":3}));
    sheet(&mut s, &d, "wave_audio_tone_then_silence", &[0.5, 1.5, 2.5, 3.5]);
    let (mut s, t) = proj(256, 256); put(&mut s, &t, "bg_wave", json!({"duration":3,"source":"values","values":[-0.2,0.6,-0.8,0.9,0.1,-0.5,0.4],"glow":{"radius_px":12,"intensity":0.7},"stroke_px":4})); sheet(&mut s, &d, "wave_values_glow", &[0.0, 1.0, 2.0]);
}

/// Minimal PNG writer (stored deflate blocks) so the banding crop can be written without a new dependency.
fn png(path: &str, w: usize, h: usize, rgb: &[u8]) {
    fn crc(b: &[u8]) -> u32 { let mut c = 0xffff_ffffu32; for &x in b { c ^= x as u32; for _ in 0..8 { c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 }; } } !c }
    let mut raw = vec![]; for y in 0..h { raw.push(0u8); raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]); }
    let mut z = vec![0x78, 0x01]; let mut i = 0; while i < raw.len() { let n = (raw.len() - i).min(65535); z.push(if i + n == raw.len() { 1 } else { 0 }); z.extend_from_slice(&(n as u16).to_le_bytes()); z.extend_from_slice(&(!(n as u16)).to_le_bytes()); z.extend_from_slice(&raw[i..i + n]); i += n; }
    let (mut a, mut b) = (1u32, 0u32); for &x in &raw { a = (a + x as u32) % 65521; b = (b + a) % 65521; } z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10];
    let mut chunk = |k: &[u8; 4], d: &[u8]| { out.extend_from_slice(&(d.len() as u32).to_be_bytes()); let mut c = k.to_vec(); c.extend_from_slice(d); out.extend_from_slice(&c); out.extend_from_slice(&crc(&c).to_be_bytes()); };
    let mut ihdr = vec![]; ihdr.extend_from_slice(&(w as u32).to_be_bytes()); ihdr.extend_from_slice(&(h as u32).to_be_bytes()); ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); chunk(b"IHDR", &ihdr); chunk(b"IDAT", &z); chunk(b"IEND", &[]); std::fs::write(path, out).unwrap();
}
#[test] fn visual_orbs_banding_crop() { let Some(d) = dir() else { return };
    // a very shallow gradient (0.20 -> 0.24) is where 8-bit banding shows; the orb is parked far off canvas so only the base and the dither remain
    let grab = |amp: f64| { let (mut s, t) = proj(256, 256); put(&mut s, &t, "bg_orbs", json!({"duration":2,"dither":amp,"seed":5,"base":{"type":"gradient","c1":[0.20,0.20,0.20],"c2":[0.24,0.24,0.24]},"orbs":[{"cx":-9000,"cy":0,"radius":1}]}));
        kimo_core::render::render_frame(&s.project, 0.0, &kimo_core::render::Cache::new()).unwrap().d };
    let (a, b) = (grab(0.0), grab(1.0)); let (z, crop, gap, pad) = (4usize, 64usize, 16usize, 16usize); let (w, h) = (pad * 2 + crop * z * 2 + gap, pad * 2 + crop * z); let mut img = vec![24u8; w * h * 3]; let mid = { let mut lo = 255.0f32; let mut hi = 0.0f32; for y in 0..crop { for x in 0..crop { let q = (a[((96 + y) * 256 + 96 + x) * 4] * 255.0).round(); lo = lo.min(q); hi = hi.max(q); } } (lo + hi) / 2.0 };
    for (k, f) in [&a, &b].iter().enumerate() { for y in 0..crop * z { for x in 0..crop * z { let (sx, sy) = (96 + x / z, 96 + y / z); let q = (f[(sy * 256 + sx) * 4] * 255.0).round(); let v = (128.0 + (q - mid) * 60.0).clamp(0.0, 255.0) as u8; // 8-bit level, stretched x40 about level 52
        let o = ((pad + y) * w + pad + k * (crop * z + gap) + x) * 3; img[o] = v; img[o + 1] = v; img[o + 2] = v; } } }
    png(&format!("{d}/orbs_banding_crop_left_nodither_right_dither1.png"), w, h, &img);
}
#[test] fn visual_wave_crossing_a_chunk_seam() { let Some(d) = dir() else { return };
    let (mut s, t) = proj(256, 256); let rate = 8000usize; let n = rate * 4; let data: Vec<f64> = (0..n).map(|i| { let tt = i as f64 / rate as f64; (2.0 * std::f64::consts::PI * (500.0 * tt + 400.0 * tt * tt)).sin() * 0.45 }).collect();
    let a = dispatch(&mut s, "import_samples", &json!({"rate":rate,"channels":1,"data":[data]})).unwrap(); let ta = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let clip = dispatch(&mut s, "add_clip", &json!({"track_id":ta,"source_type":"media","asset_id":a["asset_id"],"start":0,"duration":4})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let tv = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let r = put(&mut s, &tv, "bg_wave", json!({"duration":4,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":4,"bands":32,"hop_s":0.01,"chunk":true},"glow":{"radius_px":10,"intensity":0.8},"stroke_px":3}));
    let det = r["estimate"]["chunk_detail"].as_array().unwrap(); assert!(det.len() >= 2, "this sheet must cross a seam: {det:?}"); let seam = det[0]["t1"].as_f64().unwrap(); eprintln!("seam at t = {seam}");
    sheet(&mut s, &d, "wave_across_chunk_seam", &[seam - 0.4, seam - 0.1, seam - 0.01, seam + 0.01, seam + 0.1, seam + 0.4]);
}
#[test] fn visual_1080p_orbs_shoulder_and_wave_glow() { let Some(d) = dir() else { return };
    let orbs = json!([{"cx":700,"cy":480,"radius":420,"color":[1,0.55,0.25],"intensity":1.6},{"cx":1000,"cy":560,"radius":460,"color":[0.3,0.55,1],"intensity":1.6},{"cx":860,"cy":380,"radius":340,"color":[0.7,0.3,1],"intensity":1.6}]);
    for (name, sh) in [("orbs1080_add_noshoulder", false), ("orbs1080_add_shoulder", true)] {
        let (mut s, t) = proj(1920, 1080); put(&mut s, &t, "bg_orbs", if sh { json!({"duration":2,"combine":"add","dither":1.0,"orbs":orbs}) } else { json!({"duration":2,"combine":"add","dither":1.0,"vignette":0,"grain":0,"ceiling":1,"orbs":orbs}) });
        if false { let id = s.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| !c.effects.is_empty()).map(|c| c.id.clone()).unwrap(); dispatch(&mut s, "add_effect", &json!({"clip_id":id,"kind":"shoulder"})).unwrap(); }
        dispatch(&mut s, "render_frame", &json!({"time":0.5,"path":format!("{d}/{name}.png"),"inline":"never"})).unwrap(); }
    let (mut s, t) = proj(1920, 1080); put(&mut s, &t, "bg_wave", json!({"duration":2,"source":"values","values":[-0.3,0.5,-0.7,0.8,0.0,-0.4,0.35,-0.1,0.5],"box":[96,300,1824,780],"glow":{"radius_px":32,"intensity":2.2},"gradient":{"from":[0.35,0.85,1],"to":[0.75,0.4,1],"angle_deg":0},"stroke_px":4}));
    dispatch(&mut s, "render_frame", &json!({"time":0.5,"path":format!("{d}/wave1080_glow.png"),"inline":"never"})).unwrap();
}
#[test] fn visual_1080p_orbs_depth_layers() { let Some(d) = dir() else { return };
    let orbs = json!([{"cx":420,"cy":680,"radius":760,"color":[0.95,0.45,0.25],"intensity":1.0,"layer":"far","orbit":{"rx":60,"ry":30,"rate_hz":0.1,"phase":0}},
        {"cx":1500,"cy":640,"radius":340,"color":[0.25,0.45,1],"intensity":0.5,"layer":"mid","orbit":{"rx":60,"ry":30,"rate_hz":0.1,"phase":1}},
        {"cx":900,"cy":350,"radius":130,"color":[0.65,0.3,1],"intensity":0.5,"layer":"near","orbit":{"rx":60,"ry":30,"rate_hz":0.1,"phase":2}}]);
    let (mut s, t) = proj(1920, 1080); put(&mut s, &t, "bg_orbs", json!({"duration":2,"combine":"screen","dither":1.0,"base":{"type":"gradient","c1":[0.03,0.03,0.07],"c2":[0.01,0.01,0.03]},"orbs":orbs}));
    dispatch(&mut s, "render_frame", &json!({"time":0.5,"path":format!("{d}/orbs1080_layers.png"),"inline":"never"})).unwrap();
}
