//! dehaze (Resolve FX Dehaze): remove atmospheric haze with the dark channel prior (He, Sun, Tang 2009). Haze model
//! I = J*t + A*(1-t); airlight A from the haziest pixels, transmission t = 1 - amount*dark(I/A) refined edge-aware,
//! recovered J = (I - A)/max(t, 0.1) + A. A catalog effect and a grade-node kind, usable under masks.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(p: &Params, f: &mut Frame) { kimo_core::color::apply3("dehaze", p, f, 0.0, 0).unwrap_or_else(|e| panic!("dehaze: {e}")); }
const COLS: [[f32; 3]; 6] = [[0.8, 0.3, 0.02], [0.02, 0.6, 0.7], [0.5, 0.02, 0.6], [0.7, 0.65, 0.03], [0.02, 0.25, 0.1], [0.35, 0.02, 0.02]];
/// 64x64 haze-free scene: 8x8 blocks of saturated colours (each has a channel near 0, as real outdoor colours do), textured.
fn scene() -> Vec<[f32; 3]> { (0..64 * 64).map(|i| { let (x, y) = (i % 64, i / 64); let c = COLS[(x / 8 + 3 * (y / 8)) % 6]; let k = 0.75 + 0.25 * (((x * 7 + y * 3) % 5) as f32 / 4.0); c.map(|v| v * k) }).collect() }
/// The scene seen through haze (t = 0.5, airlight 0.9 grey), with a sky band (rows 0..12) that is pure airlight.
fn hazy(j: &[[f32; 3]], t: f32, a: f32) -> Frame { let mut d = Vec::with_capacity(64 * 64 * 4);
    for (i, p) in j.iter().enumerate() { let sky = i / 64 < 12; for v in p { d.push(if sky { a } else { v * t + a * (1.0 - t) }); } d.push(1.0); }
    Frame { w: 64, h: 64, d } }
fn err(f: &Frame, j: &[[f32; 3]]) -> f32 { let mut s = 0.0; let mut n = 0; for i in 12 * 64..64 * 64 { for c in 0..3 { s += (f.d[i * 4 + c] - j[i][c]).abs(); n += 1; } } s / n as f32 }

#[test]
fn dehaze_recovers_the_scene_behind_haze() {
    let j = scene(); let mut f = hazy(&j, 0.5, 0.9); let e0 = err(&f, &j); assert!(e0 > 0.15, "fixture is hazy: {e0}");
    run(&pr(&[("amount", 1.0), ("radius", 30.0)]), &mut f); let e1 = err(&f, &j);
    assert!(e1 < 0.3 * e0, "dehaze must bring the hazy scene back towards the clear one: mean error {e0} -> {e1}");
    // contrast between blocks comes back
    let lum = |f: &Frame, i: usize| 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2];
    let h = hazy(&j, 0.5, 0.9); let span = |f: &Frame| { let v: Vec<f32> = (12 * 64..64 * 64).map(|i| lum(f, i)).collect(); v.iter().cloned().fold(0.0, f32::max) - v.iter().cloned().fold(1.0, f32::min) };
    assert!(span(&f) > 1.6 * span(&h), "luma range must open up: {} -> {}", span(&h), span(&f));
    for i in 0..64 * 64 { assert_eq!(f.d[i * 4 + 3], 1.0, "alpha kept"); }
}

#[test]
fn dehaze_leaves_a_clear_frame_nearly_alone_and_zero_is_identity() {
    let j = scene(); let src: Vec<f32> = j.iter().flat_map(|p| [p[0], p[1], p[2], 1.0]).collect();
    let mut f = Frame { w: 64, h: 64, d: src.clone() }; run(&pr(&[("amount", 0.8), ("radius", 30.0)]), &mut f);
    let moved = f.d.iter().zip(&src).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(moved < 0.05, "a haze-free frame has nothing to remove: largest change {moved}");
    let mut g = hazy(&j, 0.5, 0.9); let before = g.d.clone(); run(&pr(&[("amount", 0.0)]), &mut g); assert_eq!(g.d, before, "amount 0 is bit-exact identity");
    let mut weak = hazy(&j, 0.5, 0.9); run(&pr(&[("amount", 0.5), ("radius", 30.0)]), &mut weak); let mut strong = hazy(&j, 0.5, 0.9); run(&pr(&[("amount", 1.0), ("radius", 30.0)]), &mut strong);
    assert!(err(&weak, &j) > err(&strong, &j) && err(&weak, &j) < err(&hazy(&j, 0.5, 0.9), &j), "amount scales the removal");
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
#[test]
fn dehaze_is_a_listed_effect_grade_node_and_refuses_bad_values() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":32,"fps":25})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.85,0.85,0.88],"color2":[0.5,0.55,0.6],"angle":90,"start":0,"duration":2}))["clip_id"].clone();
    let before = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"dehaze","params":{"amount":0.7}}));
    let after = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    assert!(before.iter().zip(&after).any(|(a, b)| (a - b).abs() > 1e-3), "dehaze must change a hazy render");
    c(&mut s, "add_node", json!({"clip_id":id,"node_id":"haze","kind":"grade","effect_kind":"dehaze"}));
    let mut f = Frame { w: 1, h: 1, d: vec![0.5, 0.4, 0.3, 1.0] };
    for (k, v) in [("amount", 1.5), ("radius", 0.0), ("air_r", 2.0)] {
        let e = kimo_core::color::apply3("dehaze", &pr(&[(k, v)]), &mut f, 0.0, 0).expect_err(&format!("{k} {v} must be refused")); assert!(e.contains(k), "'{e}' lacks '{k}'");
        assert_eq!(f.d, vec![0.5, 0.4, 0.3, 1.0], "a refused effect leaves the frame untouched"); }
}
