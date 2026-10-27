//! `replace_clip_media`: swap a media clip's source asset while keeping its position, duration, in_point,
//! effects, keyframes, mask, grade graph and every other property (FEATURES.md "replace clip (keep effects)").
//! Unlike `relink_media` (which repairs a broken path reference to the *same* logical asset), this points the clip
//! at a genuinely different asset the caller already imported.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::process::Command;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }

fn make_video(tag: &str, color: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("kimo-replaceclip-{tag}-{}.mp4", std::process::id()));
    assert!(Command::new("ffmpeg").args(["-v","error","-y","-f","lavfi","-i",&format!("color=c={color}:s=32x18:r=25:d=2"),"-c:v","libx264","-pix_fmt","yuv420p",path.to_str().unwrap()]).status().unwrap().success());
    path
}

#[test]
fn replace_clip_media_keeps_position_duration_and_effects() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 32, "height": 18, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let red = make_video("a", "red"); let blue = make_video("b", "blue");
    let a1 = c(&mut s, "import_media", json!({"path": red.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let a2 = c(&mut s, "import_media", json!({"path": blue.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let clip_id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 1.0, "duration": 1.0, "in_point": 0.2}))["clip_id"].as_str().unwrap().to_string();
    let eff_id = c(&mut s, "add_effect", json!({"clip_id": clip_id, "kind": "glow", "params": {"amount": 0.5}}))["effect_id"].as_str().unwrap().to_string();
    c(&mut s, "set_keyframe", json!({"clip_id": clip_id, "property": "opacity", "time": 1.0, "value": 0.3}));
    c(&mut s, "set_keyframe", json!({"clip_id": clip_id, "property": "opacity", "time": 2.0, "value": 1.0}));

    let r = c(&mut s, "replace_clip_media", json!({"clip_id": clip_id, "asset_id": a2}));
    assert_eq!(r["asset_id"], json!(a2));

    let clip = s.project.tracks[0].clips.iter().find(|x| x.id == clip_id).unwrap();
    // source swapped
    match &clip.source { kimo_core::model::Source::Media { asset } => assert_eq!(asset, &a2), o => panic!("expected Media source, got {o:?}") }
    // position/duration/in_point unchanged
    assert!((clip.start - 1.0).abs() < 1e-9); assert!((clip.duration - 1.0).abs() < 1e-9); assert!((clip.in_point - 0.2).abs() < 1e-9);
    // effect stack untouched
    assert_eq!(clip.effects.len(), 1); assert_eq!(clip.effects[0].id, eff_id);
    // keyframes untouched
    assert!(clip.props.get("opacity").unwrap().keys.len() == 2);
    std::fs::remove_file(&red).ok(); std::fs::remove_file(&blue).ok();
}

#[test]
fn replace_clip_media_can_set_a_new_in_point() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 32, "height": 18, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let red = make_video("c", "green");
    let a1 = c(&mut s, "import_media", json!({"path": red.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let clip_id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "replace_clip_media", json!({"clip_id": clip_id, "asset_id": a1, "in_point": 0.5}));
    assert!((r["in_point"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    std::fs::remove_file(&red).ok();
}

#[test]
fn replace_clip_media_rejects_a_non_media_clip() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 32, "height": 18, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let clip_id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1,0,0], "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "replace_clip_media", json!({"clip_id": clip_id, "asset_id": "whatever"}));
    assert!(e.to_lowercase().contains("media"), "got: {e}");
}

#[test]
fn replace_clip_media_rejects_an_unknown_asset() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 32, "height": 18, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let red = make_video("d", "yellow");
    let a1 = c(&mut s, "import_media", json!({"path": red.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let clip_id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "replace_clip_media", json!({"clip_id": clip_id, "asset_id": "no-such-asset"}));
    assert!(e.to_lowercase().contains("asset"), "got: {e}");
    std::fs::remove_file(&red).ok();
}
