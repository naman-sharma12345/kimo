//! v7.6 feature: project_stats -- one-call aggregate project summary for an agent (duration, clip/track
//! counts by kind, unused media, caption/marker counts) instead of reading the whole project or paging
//! through find_assets and counting by hand.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    ss
}

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_project_stats_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn clip_file(name: &str, dur: f64) -> String {
    let p = dir().join(name);
    if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=64x36:r=25:d={dur}"), "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

#[test]
fn empty_project_reports_zeroed_counts() {
    let mut ss = new_proj();
    let r = c(&mut ss, "project_stats", json!({}));
    assert_eq!(r["duration"], 0.0, "{r}");
    assert_eq!(r["track_count"], 0, "{r}");
    assert_eq!(r["clip_count"], 0, "{r}");
    assert_eq!(r["asset_count"], 0, "{r}");
    assert_eq!(r["unused_asset_count"], 0, "{r}");
    assert_eq!(r["marker_count"], 0, "{r}");
    assert_eq!(r["caption_group_count"], 0, "{r}");
}

#[test]
fn counts_clips_by_kind_and_duration_is_the_furthest_clip_end() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1, 0.1, 0.1], "start": 0.0, "duration": 2.0}));
    c(&mut ss, "add_text", json!({"track_id": t, "text": "hi", "start": 2.0, "duration": 1.5}));
    let r = c(&mut ss, "project_stats", json!({}));
    assert_eq!(r["clip_count"], 2, "{r}");
    assert_eq!(r["clip_count_by_kind"]["generated"], 1, "{r}"); // solid is "generated"
    assert_eq!(r["clip_count_by_kind"]["text"], 1, "{r}");
    assert!((r["duration"].as_f64().unwrap() - 3.5).abs() < 1e-6, "{r}");
}

#[test]
fn an_imported_but_unplaced_asset_is_reported_unused() {
    let mut ss = new_proj();
    let used_asset = c(&mut ss, "import_media", json!({"path": clip_file("ps_used.mp4", 1.0)}))["asset_id"].as_str().unwrap().to_string();
    let unused_asset = c(&mut ss, "import_media", json!({"path": clip_file("ps_unused.mp4", 1.0)}))["asset_id"].as_str().unwrap().to_string();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": used_asset, "start": 0.0, "duration": 1.0}));

    let r = c(&mut ss, "project_stats", json!({}));
    assert_eq!(r["asset_count"], 2, "{r}");
    assert_eq!(r["used_asset_count"], 1, "{r}");
    assert_eq!(r["unused_asset_count"], 1, "{r}");
    let unused = r["unused_assets"].as_array().unwrap();
    assert_eq!(unused.len(), 1, "{r}");
    assert_eq!(unused[0]["asset_id"], json!(unused_asset), "{r}");
    assert_eq!(r["clip_count_by_kind"]["media"], 1, "{r}");
}

#[test]
fn deleting_the_clip_that_used_an_asset_makes_it_unused_again() {
    let mut ss = new_proj();
    let asset = c(&mut ss, "import_media", json!({"path": clip_file("ps_del.mp4", 1.0)}))["asset_id"].as_str().unwrap().to_string();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": asset, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    assert_eq!(c(&mut ss, "project_stats", json!({}))["unused_asset_count"], 0);
    c(&mut ss, "delete_clip", json!({"clip_id": clip}));
    assert_eq!(c(&mut ss, "project_stats", json!({}))["unused_asset_count"], 1);
}

#[test]
fn counts_markers_captions_bins_and_track_flags() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_track", json!({"track_id": t, "muted": true}));
    c(&mut ss, "add_marker", json!({"time": 1.0, "name": "m1"}));
    c(&mut ss, "add_marker", json!({"time": 2.0, "name": "m2"}));
    c(&mut ss, "create_bin", json!({"name": "ps_bin"}));
    let tc = c(&mut ss, "add_text", json!({"track_id": t, "text": "hello world", "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_caption_cues", json!({"track_id": t, "cues": [{"start": 0.0, "end": 1.0, "text": "hello"}]}));
    let _ = tc;

    let r = c(&mut ss, "project_stats", json!({}));
    assert_eq!(r["marker_count"], 2, "{r}");
    assert_eq!(r["bin_count"], 1, "{r}");
    assert_eq!(r["tracks_muted"], 1, "{r}");
    assert_eq!(r["caption_group_count"], 1, "{r}");
    assert_eq!(r["caption_cue_count"].as_u64().unwrap(), 1, "{r}");
}

#[test]
fn reports_clips_with_effects_and_keyframes() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 1.0, "color": [0.5, 0.5, 0.5, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_effect", json!({"clip_id": clip, "kind": "blur"}));
    c(&mut ss, "set_keyframe", json!({"clip_id": clip, "property": "opacity", "time": 0.0, "value": 1.0}));
    let r = c(&mut ss, "project_stats", json!({}));
    assert_eq!(r["clips_with_effects"], 1, "{r}");
    assert_eq!(r["clips_with_keyframes"], 1, "{r}");
}

#[test]
fn is_read_only_and_does_not_add_a_history_entry() {
    let mut ss = new_proj();
    let before = c(&mut ss, "history_list", json!({}))["undo"].as_array().map(|a| a.len()).unwrap_or(0);
    c(&mut ss, "project_stats", json!({}));
    let after = c(&mut ss, "history_list", json!({}))["undo"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(before, after, "project_stats must not mutate history");
}
