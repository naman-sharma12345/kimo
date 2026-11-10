//! add_morph_cut (Premiere Morph Cut) on a lossless synthetic talking head (ffv1 .mkv, 320x180 @ 10 fps): a bright "face" with
//! features on a textured ground drifts right; a 1 s silence is cut out, so the face jumps 30 px at the cut. With the morph cut
//! the frame mid-morph shows ONE face between the two positions (a crossfade would show two half-faces), the first morph frame
//! continues the outgoing shot, and after the morph the incoming clip renders exactly as without it.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const SW: usize = 320; const SH: usize = 180; const N: usize = 40; const FPS: f64 = 10.0;
fn face_x(t: f64) -> f64 { 120.0 + 15.0 * t } // source seconds -> face centre x
fn tex(x: usize, y: usize) -> [u8; 3] { let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F); h ^= h >> 29; h = h.wrapping_mul(0xBF58476D1CE4E5B9); h ^= h >> 32;
    let n = (h % 31) as i32 - 15; let s = (((x as f64) * 0.21).sin() * 10.0 + ((y as f64) * 0.17).cos() * 10.0) as i32; [(60 + n + s).clamp(0, 255) as u8, (64 + n + s).clamp(0, 255) as u8, (70 + n + s).clamp(0, 255) as u8] }
fn plate() -> std::path::PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-morph-{}.mkv", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{SW}x{SH}"), "-r", "10", "-i", "-", "-c:v", "ffv1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); for i in 0..N { let cx = face_x(i as f64 / FPS); let mut o = Vec::with_capacity(SW * SH * 3);
        for y in 0..SH { for x in 0..SW { let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - 90.0); let e = (dx / 30.0).powi(2) + (dy / 40.0).powi(2);
            let eye = ((dx.abs() - 11.0).powi(2) + (dy + 10.0).powi(2)).sqrt() < 4.0; let mouth = dx.abs() < 12.0 && (dy - 16.0).abs() < 2.5;
            if e < 1.0 { if eye || mouth { o.extend_from_slice(&[40, 20, 20]); } else { o.extend_from_slice(&[235, 190, 150]); } } else { o.extend_from_slice(&tex(x, y)); } } }
        si.write_all(&o).unwrap(); } }
    assert!(ch.wait().unwrap().success()); path
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// Clip A = source 0..1 s at 0, clip B = source 2..3 s at 1 s (the silence 1..2 s removed): the face jumps from x 135 to 150.
fn scene() -> (Session, String, String, String) {
    let path = plate(); let mut s = Session::new(); c(&mut s, "project_set", json!({"width": SW, "height": SH, "fps": FPS}));
    let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let ca = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0, "in_point": 0.0}))["clip_id"].as_str().unwrap().to_string();
    let cb = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 1.0, "duration": 1.0, "in_point": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (s, t, ca, cb)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
/// (full-tone face px, half-tone px, centroid x of face px). Face skin is (235,190,150); half-tone = clearly between ground and skin.
fn face(f: &Frame) -> (usize, usize, f64) { let (mut full, mut half, mut sx) = (0usize, 0usize, 0f64);
    for y in 0..f.h { for x in 0..f.w { let i = (y * f.w + x) * 4; let r = f.d[i] * 255.0; let g = f.d[i + 1] * 255.0;
        if r > 205.0 && g > 160.0 { full += 1; sx += x as f64; } else if r > 120.0 && r < 185.0 && g > 100.0 { half += 1; } } }
    (full, half, if full > 0 { sx / full as f64 } else { -1.0 }) }
fn save(name: &str, f: &Frame) { use std::io::Write; if let Ok(d) = std::env::var("KIMO_PROOF_DIR") { std::fs::create_dir_all(&d).unwrap();
    let px: Vec<u8> = f.d.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", f.w, f.h), "-i", "-", "-frames:v", "1", &format!("{d}/{name}.png")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    ch.stdin.take().unwrap().write_all(&px).unwrap(); assert!(ch.wait().unwrap().success()); } }
fn pj(s: &Session) -> Value { serde_json::to_value(&s.project).unwrap() }

#[test]
fn a_jump_cut_morphs_as_one_face_and_lands_exactly_on_the_incoming_clip() {
    let (mut s, _t, ca, cb) = scene();
    let plain_mid = frame(&s, 1.2); let plain_after = frame(&s, 1.5); let last_a = frame(&s, 0.95);
    let r = c(&mut s, "add_morph_cut", json!({"clip_id": cb, "duration": 0.4})); assert_eq!(r["clip_ids"][0], cb, "{r}");
    let (full_a, _, xa) = face(&last_a); let (_, _, xb) = face(&plain_mid);
    let mid = frame(&s, 1.2); save("morph_mid", &mid); save("jump_mid", &plain_mid); save("morph_from", &last_a);
    // crossfade reference at the same weight, for the record and for the ghosting bound
    let k = { let u: f64 = 0.5; if u < 0.5 { 2.0 * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(2) / 2.0 } };
    let xf = Frame { w: mid.w, h: mid.h, d: last_a.d.iter().zip(&plain_mid.d).map(|(a, b)| a * (1.0 - k as f32) + b * k as f32).collect() }; save("crossfade_mid", &xf);
    let (full_m, half_m, xm) = face(&mid); let (_, half_x, _) = face(&xf);
    assert!(xm > xa + 4.0 && xm < xb - 4.0, "mid-morph the face must sit between the outgoing ({xa:.1}) and incoming ({xb:.1}) positions, got {xm:.1}");
    assert!(full_m as f64 > 0.75 * full_a as f64, "mid-morph the face must stay one solid face: {full_m} full-tone px vs {full_a} in the outgoing frame");
    assert!((half_m as f64) < 0.5 * half_x as f64, "mid-morph must not ghost like a crossfade: {half_m} half-tone px vs {half_x} in a crossfade");
    let first = frame(&s, 1.0); let (_, _, x0) = face(&first); assert!((x0 - xa).abs() < 3.0, "the first morph frame continues the outgoing shot: face at {x0:.1}, outgoing {xa:.1}");
    assert_eq!(frame(&s, 1.5).d, plain_after.d, "after the morph the incoming clip renders exactly as without it");
    let _ = ca;
}

#[test]
fn track_mode_remove_and_refusals() {
    let (mut s, t, ca, cb) = scene();
    let r = c(&mut s, "add_morph_cut", json!({"track_id": t})); assert_eq!(r["clip_ids"], json!([cb]), "only the clip after the jump cut: {r}");
    let r = c(&mut s, "add_morph_cut", json!({"track_id": t, "remove": true})); assert_eq!(r["removed"], 1);
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let solid = c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = pj(&s);
    for (args, want) in [(json!({"clip_id": ca}), "no clip ending where it starts"), (json!({"clip_id": solid}), "needs a media clip"), (json!({"clip_id": cb, "duration": 5}), "between 0.1 and 2"),
        (json!({}), "needs clip_id"), (json!({"clip_id": cb, "track_id": t}), "not both"), (json!({"track_id": t2}), "has no jump cuts"), (json!({"clip_id": cb, "fade": 1}), "unknown argument 'fade'")] {
        let e = dispatch_ex(&mut s, "add_morph_cut", &args).unwrap_err().message; assert!(e.contains(want), "{args}: error '{e}' lacks '{want}'"); assert_eq!(pj(&s), before, "{args}: a refused call changed the project"); }
}
