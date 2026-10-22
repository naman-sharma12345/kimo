//! add_glow_text: one bold text clip with a deep glow whose amount pulses (a sine op on the effect's amount).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 1080, "height": 1920, "fps": 30})); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (s, t) }
fn glow_text(s: &mut Session, t: &str) -> String { c(s, "add_glow_text", json!({"track_id": t, "text": "WATCH THIS", "duration": 2.0}))["clip_id"].as_str().expect("clip_id").to_string() }

#[test]
fn glow_text_creates_one_clip_with_glow_effect() {
    let (mut s, t) = scene();
    let before = s.project.tracks[0].clips.len();
    let id = glow_text(&mut s, &t);
    assert_eq!(s.project.tracks[0].clips.len(), before + 1);
    let clip = s.project.find_clip(&id).expect("created clip");
    assert_eq!(clip.effects.len(), 1);
    assert_eq!(clip.effects[0].kind, "glow");
    assert_eq!(clip.effects[0].params.get("mode").expect("mode").at(0.0), 1.0);
}

#[test]
fn glow_text_amount_actually_varies_over_time() {
    let (mut s, t) = scene();
    let id = glow_text(&mut s, &t);
    let clip = s.project.find_clip(&id).unwrap();
    let amount = clip.effects[0].params.get("amount").expect("amount");
    // pulse_hz 1.6: t = 0 is the midpoint of the swing, a quarter period later is its peak
    let (a0, a1) = (amount.at(0.0), amount.at(1.0 / (4.0 * 1.6)));
    assert!((a1 - a0).abs() > 0.2, "amount should pulse: {a0} vs {a1}");
    let samples: Vec<f64> = (0..40).map(|i| amount.at(i as f64 * 0.05)).collect();
    let (lo, hi) = samples.iter().fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    assert!(hi - lo > 0.5 && lo >= 0.0 && hi <= 2.0, "swing {lo}..{hi}");
}

#[test]
fn glow_text_rejects_bad_pulse_hz() {
    let (mut s, t) = scene();
    assert!(dispatch(&mut s, "add_glow_text", &json!({"track_id": t, "text": "X", "pulse_hz": 50.0})).is_err());
    assert!(dispatch(&mut s, "add_glow_text", &json!({"track_id": t, "text": "X", "bogus": 1})).is_err());
    assert!(dispatch(&mut s, "add_glow_text", &json!({"track_id": "nope", "text": "X"})).is_err());
}

#[test]
fn glow_text_renders_without_panicking() {
    let (mut s, t) = scene();
    glow_text(&mut s, &t);
    let tm = 1.0; let mut cache = render::Cache::new();
    render::prepare_at(&s.project, &mut cache, Some(tm)).unwrap_or_else(|e| panic!("prepare: {e}"));
    let f = render::render_frame(&s.project, tm, &cache);
    assert!(f.is_ok(), "{:?}", f.err());
}
