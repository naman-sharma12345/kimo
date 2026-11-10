//! film_burn transition: outgoing blue, incoming green (160x90 @ 10 fps, 1 s transition from t 0); heat read as the warm excess over both shots.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn scene(seed: f64) -> Result<Session, String> {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let t0 = id(&c(&mut s, "add_track", json!({})), "track_id"); c(&mut s, "add_clip", json!({"track_id": t0, "source_type": "solid", "color": [0.0, 0.0, 0.6], "start": 0, "duration": 2.0}));
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id"); let b = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [0.0, 0.6, 0.0], "start": 0, "duration": 2.0})), "clip_id");
    dispatch(&mut s, "add_transition", &json!({"clip_id": b, "kind": "film_burn", "duration": 1.0, "seed": seed})).map_err(|e| e.to_string())?; Ok(s)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn ch(f: &Frame, j: usize) -> Vec<f32> { (0..f.w * f.h).map(|i| f.d[i * 4 + j]).collect() }
fn mean(v: &[f32]) -> f32 { v.iter().sum::<f32>() / v.len() as f32 }

#[test]
fn film_burn_blooms_hot_and_cuts_at_the_peak() {
    let s = scene(3.0); assert!(s.is_ok(), "film_burn must be a transition kind: {}", s.err().unwrap()); let s = s.unwrap();
    let f0 = frame(&s, 0.0); assert!(ch(&f0, 0).iter().all(|v| *v < 0.01) && (mean(&ch(&f0, 2)) - 0.6).abs() < 0.01, "start: the outgoing blue shot, untouched");
    let fp = frame(&s, 0.5); let (r, g, b) = (mean(&ch(&fp, 0)), mean(&ch(&fp, 1)), mean(&ch(&fp, 2)));
    assert!(r > 0.9, "peak: the frame is burnt out hot (mean red {r})"); assert!(r > g, "peak is warm: r {r} g {g} b {b}");
    assert!(r - b > 0.3, "peak is orange/white, not blue: r {r} b {b}");
    let f1 = frame(&s, 1.0); assert!((mean(&ch(&f1, 1)) - 0.6).abs() < 0.01 && ch(&f1, 0).iter().all(|v| *v < 0.01), "end: the incoming green shot, clean");
}

#[test]
fn the_burn_spreads_from_a_point_with_a_ragged_edge() {
    let s = scene(3.0).unwrap(); let f = frame(&s, 0.2); let r = ch(&f, 0);
    let hot = r.iter().filter(|v| **v > 0.5).count(); let cold = r.iter().filter(|v| **v < 0.02).count();
    assert!(hot > 50 && cold > 50, "early on part of the frame burns and part is still clean: hot {hot} cold {cold}");
    let f2 = frame(&s, 0.35); let hot2 = ch(&f2, 0).iter().filter(|v| **v > 0.5).count(); assert!(hot2 > hot, "the burn grows towards the cut: {hot} -> {hot2}");
}

#[test]
fn film_burn_is_seeded_and_deterministic() {
    let a = frame(&scene(3.0).unwrap(), 0.25); let a2 = frame(&scene(3.0).unwrap(), 0.25); assert_eq!(a.d, a2.d, "same seed, same frame");
    let b = frame(&scene(9.0).unwrap(), 0.25); let diff = a.d.iter().zip(&b.d).filter(|(x, y)| (*x - *y).abs() > 0.05).count(); assert!(diff > 200, "another seed burns from elsewhere: {diff} px differ");
}
