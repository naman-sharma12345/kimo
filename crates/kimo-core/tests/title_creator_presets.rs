//! add_title creator presets: hook_box (hook line on a rounded box) and subscribe_pop (pill badge + dot + text) on a 9:16 frame.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }
fn scene() -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 1080, "height": 1920, "fps": 30})); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (s, t) }

#[test]
fn hook_box_pops_in_two_clips() {
    let (mut s, t) = scene();
    let before = s.project.tracks[0].clips.len();
    let r = c(&mut s, "add_title", json!({"track_id": t, "style": "hook_box", "text": "WAIT FOR IT", "start": 0.0, "duration": 2.5}));
    let ids = r["clip_ids"].as_array().expect("clip_ids");
    assert!(ids.len() >= 2, "{r}");
    assert_eq!(s.project.tracks[0].clips.len(), before + ids.len(), "{r}");
}

#[test]
fn subscribe_pop_three_clips_and_bounces() {
    let (mut s, t) = scene();
    let before = s.project.tracks[0].clips.len();
    let r = c(&mut s, "add_title", json!({"track_id": t, "style": "subscribe_pop", "text": "FOLLOW", "start": 0.0, "duration": 2.5}));
    let ids = r["clip_ids"].as_array().expect("clip_ids");
    assert!(ids.len() >= 3, "{r}");
    assert_eq!(s.project.tracks[0].clips.len(), before + ids.len(), "{r}");
}

#[test]
fn hook_box_and_subscribe_pop_render_without_panicking() {
    for (style, text) in [("hook_box", "WAIT FOR IT"), ("subscribe_pop", "FOLLOW")] {
        let (mut s, t) = scene();
        c(&mut s, "add_title", json!({"track_id": t, "style": style, "text": text, "start": 0.0, "duration": 2.5}));
        let tm = 1.25; let mut cache = render::Cache::new();
        render::prepare_at(&s.project, &mut cache, Some(tm)).unwrap_or_else(|e| panic!("{style} prepare: {e}"));
        let f = render::render_frame(&s.project, tm, &cache);
        assert!(f.is_ok(), "{style}: {:?}", f.err());
    }
}

#[test]
fn title_unknown_style_still_errors() {
    let (mut s, t) = scene();
    let err = e(&mut s, "add_title", json!({"track_id": t, "style": "not_a_real_style", "text": "x"}));
    assert!(err.contains("unknown style"), "{err}");
}
