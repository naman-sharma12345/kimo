//! saber (Video Copilot Saber / a neon light stroke, After Effects): a hot core and a coloured glow drawn along a layer's alpha
//! outline (text, a logo, a shape), revealed along a direction (write-on), with flicker and wobble. Read off a synthetic 160x90
//! layer: an opaque grey rectangle x 60..100, y 30..60 on a transparent frame, so the outline, the glow beyond it and the reveal
//! are measured directly. The rendered frame is composited over the project's opaque black background, so 'nothing drawn' reads as black.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn rect(x: usize, y: usize) -> bool { (60..100).contains(&x) && (30..60).contains(&y) }
fn session() -> (Session, String) {
    use std::io::Write;
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let path = std::env::temp_dir().join(format!("kimo-saber-{}-rect.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 4); for y in 0..H { for x in 0..W { if rect(x, y) { o.extend_from_slice(&[77, 77, 77, 255]); } else { o.extend_from_slice(&[0, 0, 0, 0]); } } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success());
    let a = id(&c(&mut s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    let v = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2.0})), "clip_id");
    (s, v)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 4] { let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2], f.d[i + 3]] }
fn lum(p: [f32; 4]) -> f32 { (p[0] + p[1] + p[2]) / 3.0 }
fn saber(s: &mut Session, v: &str, params: Value) -> Value { c(s, "add_effect", json!({"clip_id": v, "kind": "saber", "params": params})) }

#[test]
fn a_the_outline_gets_a_hot_core_and_a_coloured_glow_beyond_it() {
    let (mut s, v) = session(); let base = frame(&s, 0.5);
    let r = dispatch(&mut s, "add_effect", &json!({"clip_id": v, "kind": "saber", "params": {}}));
    assert!(r.is_ok(), "saber must be an effect: {}", r.unwrap_err());
    let f = frame(&s, 0.5);
    // on the outline (left edge x 60, top edge y 30): the white-hot core
    for (x, y) in [(60, 45), (99, 45), (80, 30), (80, 59)] { let p = px(&f, x, y); assert!(p[0] > 0.85 && p[1] > 0.85 && p[2] > 0.85 && p[3] > 0.95, "core on the outline at ({x},{y}) must be near white, got {p:?}"); }
    // beyond the outline, over the transparent part: the glow (default electric blue), fading with distance
    let g: Vec<[f32; 4]> = [56, 52, 46, 38].iter().map(|x| px(&f, *x, 45)).collect();
    for (k, p) in g.iter().enumerate() { assert!(p[3] > 0.0 && p[2] > p[0], "glow outside the outline (x {}) must be blue-ish and visible, got {p:?}", [56, 52, 46, 38][k]); }
    for k in 1..g.len() { assert!(g[k][3] * lum(g[k]) < g[k - 1][3] * lum(g[k - 1]), "glow must fade with distance from the outline: {:?}", g); }
    assert_eq!(&px(&f, 5, 5)[..3], &[0.0, 0.0, 0.0], "far from the outline nothing is drawn (the black background shows)");
    // outline mode leaves the layer's inside alone (deep inside, past the core and the inner glow)
    let (pi, bi) = (px(&f, 80, 45), px(&base, 80, 45)); for j in 0..4 { assert!((pi[j] - bi[j]).abs() < 0.02, "deep inside the rectangle must stay as it was: {pi:?} vs {bi:?}"); }
}

#[test]
fn b_colours_size_and_fill_mode_do_what_they_say() {
    let (mut s, v) = session(); saber(&mut s, &v, json!({"glow_r": 1.0, "glow_g": 0.1, "glow_b": 0.1, "core_size": 1}));
    let f = frame(&s, 0.5); let p = px(&f, 54, 45); assert!(p[0] > p[1] && p[0] > p[2], "glow_r/g/b red: glow must be red, got {p:?}");
    let (mut s2, v2) = session(); saber(&mut s2, &v2, json!({"core_size": 6}));
    let f2 = frame(&s2, 0.5); assert!(lum(px(&f2, 64, 45)) > 0.85, "core_size 6 reaches 4 px inside the edge, got {:?}", px(&f2, 64, 45));
    let (mut s3, v3) = session(); saber(&mut s3, &v3, json!({"core_size": 1}));
    let f3 = frame(&s3, 0.5); assert!(lum(px(&f3, 64, 45)) < 0.7, "core_size 1 stays on the edge, got {:?}", px(&f3, 64, 45));
    // mode 1 (fill): the whole shape burns, not only its outline
    let (mut s4, v4) = session(); saber(&mut s4, &v4, json!({"mode": 1}));
    let f4 = frame(&s4, 0.5); assert!(lum(px(&f4, 80, 45)) > 0.85, "fill mode: the inside is the core, got {:?}", px(&f4, 80, 45));
    // composite 1: the saber alone (the layer itself is hidden): the inside of the outline goes transparent
    let (mut s5, v5) = session(); saber(&mut s5, &v5, json!({"composite": 1}));
    let f5 = frame(&s5, 0.5); assert!(lum(px(&f5, 80, 45)) < 0.05, "composite 1 hides the layer (the background shows through), got {:?}", px(&f5, 80, 45)); assert!(lum(px(&f5, 60, 45)) > 0.85, "composite 1 keeps the light");
}

#[test]
fn c_the_reveal_writes_the_stroke_on_along_its_angle() {
    let (s0, _) = session(); let base = frame(&s0, 0.0);
    let (mut s, v) = session(); let e = id(&saber(&mut s, &v, json!({"start": 0.0, "end": 0.5, "reveal_angle": 0})), "effect_id");
    let f = frame(&s, 0.5);
    assert!(lum(px(&f, 60, 45)) > 0.85, "left edge (revealed half) burns");
    let (pr, br) = (px(&f, 99, 45), px(&base, 99, 45)); for j in 0..4 { assert!((pr[j] - br[j]).abs() < 1e-6, "right edge (not yet revealed) untouched: {pr:?} vs {br:?}"); }
    assert_eq!(&px(&f, 106, 45)[..3], &[0.0, 0.0, 0.0], "no glow past the unrevealed right edge");
    // keyframed write-on: end 0 at t 0 -> nothing at all (bit-exact), end 1 at t 1 -> the whole outline
    c(&mut s, "set_keyframe", json!({"clip_id": v, "effect_id": e, "property": "end", "time": 0.0, "value": 0.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": v, "effect_id": e, "property": "end", "time": 1.0, "value": 1.0}));
    assert_eq!(frame(&s, 0.0).d, base.d, "end 0: nothing revealed, the layer is bit-exact");
    let f1 = frame(&s, 1.0); assert!(lum(px(&f1, 99, 45)) > 0.85 && lum(px(&f1, 60, 45)) > 0.85, "end 1: whole outline burns");
    // reveal_angle 90: top to bottom
    let (mut s2, v2) = session(); saber(&mut s2, &v2, json!({"start": 0.0, "end": 0.5, "reveal_angle": 90}));
    let f2 = frame(&s2, 0.5); assert!(lum(px(&f2, 80, 30)) > 0.85, "angle 90: top edge revealed first"); assert!(lum(px(&f2, 80, 59)) < 0.5, "angle 90: bottom edge not yet");
}

#[test]
fn d_flicker_varies_over_time_deterministically_and_off_is_steady() {
    let (mut s, v) = session(); saber(&mut s, &v, json!({"flicker": 1.0, "flicker_speed": 10}));
    let vals: Vec<f32> = (0..10).map(|k| { let f = frame(&s, k as f64 * 0.1); let p = px(&f, 52, 45); p[3] * lum(p) }).collect();
    let (mn, mx) = vals.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    assert!(mx - mn > 0.05, "flicker 1 must make the glow vary over time, got {vals:?}");
    assert_eq!(frame(&s, 0.3).d, frame(&s, 0.3).d, "flicker is deterministic (same time, same frame)");
    let (mut s2, v2) = session(); saber(&mut s2, &v2, json!({}));
    assert_eq!(frame(&s2, 0.2).d, frame(&s2, 0.7).d, "no flicker and no wobble: the saber is steady");
    // distortion wobbles the stroke: frames differ over time
    let (mut s3, v3) = session(); saber(&mut s3, &v3, json!({"distortion": 3, "distortion_speed": 2}));
    assert_ne!(frame(&s3, 0.2).d, frame(&s3, 0.7).d, "distortion animates the stroke");
}

#[test]
fn e_list_effects_carries_saber_with_its_params() {
    let mut s = Session::new(); let l = c(&mut s, "list_effects", json!({}));
    let e = l.as_array().unwrap().iter().find(|x| x["kind"] == "saber").expect("saber in list_effects");
    let names: Vec<String> = e["params"].as_array().map(|a| a.iter().filter_map(|p| p["name"].as_str().map(String::from)).collect()).unwrap_or_else(|| e["params"].as_object().unwrap().keys().cloned().collect());
    for k in ["mode", "core_size", "core_r", "glow_r", "glow_intensity", "glow_spread", "start", "end", "reveal_angle", "flicker", "flicker_speed", "distortion", "composite"] { assert!(names.iter().any(|n| n == k), "saber param {k} missing: {names:?}"); }
}
