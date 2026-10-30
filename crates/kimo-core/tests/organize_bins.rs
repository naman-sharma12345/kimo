//! v7.6 feature: organize_bins -- file every asset in the media pool into bins automatically, by kind
//! (video/image/audio) or by usage (used/unused), creating bins as needed, in one call. create_bin and
//! move_asset_to_bin already do the mechanics one asset at a time; this is the sweep.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(ss: &mut Session, n: &str, v: Value) -> Result<Value, String> { dispatch(ss, n, &v) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    ss
}

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_organize_bins_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn video(name: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=64x36:r=25:d=1", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn wav(name: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", "sine=f=440:d=1", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn png(name: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", "color=c=red:s=16x16", "-frames:v", "1", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

fn asset_bin(ss: &mut Session, asset_id: &str) -> Option<String> {
    let found = c(ss, "find_assets", json!({"limit": 500}));
    for a in found["matches"].as_array().unwrap() { if a["asset_id"] == asset_id { return a["bin"].as_str().map(String::from); } }
    panic!("asset {asset_id} not found");
}

#[test]
fn by_kind_sorts_video_image_and_audio_into_their_own_bins() {
    let mut ss = new_proj();
    let v = c(&mut ss, "import_media", json!({"path": video("ob1.mp4")}))["asset_id"].as_str().unwrap().to_string();
    let i = c(&mut ss, "import_media", json!({"path": png("ob1.png")}))["asset_id"].as_str().unwrap().to_string();
    let au = c(&mut ss, "import_media", json!({"path": wav("ob1.wav")}))["asset_id"].as_str().unwrap().to_string();

    let r = c(&mut ss, "organize_bins", json!({"by": "kind"}));
    assert_eq!(r["moved"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(asset_bin(&mut ss, &v).as_deref(), Some("Video"));
    assert_eq!(asset_bin(&mut ss, &i).as_deref(), Some("Image"));
    assert_eq!(asset_bin(&mut ss, &au).as_deref(), Some("Audio"));
}

#[test]
fn by_usage_sorts_used_vs_unused() {
    let mut ss = new_proj();
    let used = c(&mut ss, "import_media", json!({"path": video("ob2.mp4")}))["asset_id"].as_str().unwrap().to_string();
    let unused = c(&mut ss, "import_media", json!({"path": video("ob3.mp4")}))["asset_id"].as_str().unwrap().to_string();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": used, "start": 0.0, "duration": 1.0}));

    c(&mut ss, "organize_bins", json!({"by": "usage"}));
    assert_eq!(asset_bin(&mut ss, &used).as_deref(), Some("Used"));
    assert_eq!(asset_bin(&mut ss, &unused).as_deref(), Some("Unused"));
}

#[test]
fn only_unfiled_default_leaves_a_manually_filed_asset_alone() {
    let mut ss = new_proj();
    let a = c(&mut ss, "import_media", json!({"path": video("ob4.mp4")}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "move_asset_to_bin", json!({"asset_id": a, "bin": "My Picks"}));
    let r = c(&mut ss, "organize_bins", json!({"by": "kind"}));
    assert_eq!(r["skipped_already_filed"], 1, "{r}");
    assert_eq!(asset_bin(&mut ss, &a).as_deref(), Some("My Picks"), "manual filing must survive a default sweep");
}

#[test]
fn only_unfiled_false_re_files_everything() {
    let mut ss = new_proj();
    let a = c(&mut ss, "import_media", json!({"path": video("ob5.mp4")}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "move_asset_to_bin", json!({"asset_id": a, "bin": "My Picks"}));
    c(&mut ss, "organize_bins", json!({"by": "kind", "only_unfiled": false}));
    assert_eq!(asset_bin(&mut ss, &a).as_deref(), Some("Video"));
}

#[test]
fn dry_run_reports_the_plan_and_changes_nothing() {
    let mut ss = new_proj();
    let a = c(&mut ss, "import_media", json!({"path": video("ob6.mp4")}))["asset_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "organize_bins", json!({"by": "kind", "dry_run": true}));
    assert_eq!(r["would_move"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(asset_bin(&mut ss, &a), None, "dry_run must not actually move anything");
}

#[test]
fn whole_sweep_is_one_undo_step() {
    let mut ss = new_proj();
    c(&mut ss, "import_media", json!({"path": video("ob7.mp4")}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "import_media", json!({"path": png("ob7.png")}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "import_media", json!({"path": wav("ob7.wav")}))["asset_id"].as_str().unwrap().to_string();
    let before = c(&mut ss, "history_list", json!({}))["undo"].as_array().unwrap().len();
    c(&mut ss, "organize_bins", json!({"by": "kind"}));
    let after = c(&mut ss, "history_list", json!({}))["undo"].as_array().unwrap().len();
    assert_eq!(after, before + 1, "organize_bins over 3 assets should be exactly one undo step");
}

#[test]
fn unknown_by_value_errors() {
    let mut ss = new_proj();
    assert!(try_c(&mut ss, "organize_bins", json!({"by": "nonsense"})).is_err());
}
