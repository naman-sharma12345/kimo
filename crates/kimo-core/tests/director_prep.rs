//! Director dry run (2026-10-21): an agent edited a 9:16 short from phone clips through kimo-mcp and these broke.
//! Each test is one failure it hit, reproduced through the commands it called.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_director_prep_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
/// A shaky vertical plate (180x320, 30 fps, 2 s): a textured source cropped along a jittery path, like handheld phone b-roll.
fn shaky_plate() -> String {
    let p = dir().join("shaky.mp4"); if !p.exists() {
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=260x400:r=30:d=2", "-vf", "crop=180:320:'40+14*sin(t*7.3)+8*sin(t*13.1)':'40+11*sin(t*6.1+2)+6*sin(t*11.7)'", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "16", "-pix_fmt", "yuv420p", p.to_str().unwrap()]); }
    p.to_string_lossy().into_owned()
}
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }

/// stabilize refused a 1.266667 s clip with "clip is not a media clip": 1.266667 x 30 = 38.00001 frames, rounded up to 39,
/// so the last frame sat past the clip's end. A model writes durations to 4 or 6 decimals; 38 frames is what it meant.
#[test] fn stabilize_takes_a_duration_written_to_six_decimals() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 180, "height": 320, "fps": 30}));
    let a = call(&mut ss, "import_media", json!({"path": shaky_plate()}))["asset_id"].as_str().unwrap().to_string();
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // 1.266667 s = 38.00001 frames; 1.3334 s = 40.002 frames (the second dry run wrote 4 decimals): 38 and 40 frames
    for (dur, want) in [(1.266667, 38), (1.3334, 40), (1.2, 36)] {
        let c = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.4, "duration": dur, "in_point": 0.5, "fit": "cover"}))["clip_id"].as_str().unwrap().to_string();
        let r = dispatch(&mut ss, "stabilize", &json!({"clip_id": c}));
        let r = r.unwrap_or_else(|e| panic!("stabilize on a {dur} s media clip failed: {e}"));
        assert_eq!(r["frames"], json!(want), "{dur} s: {r}");
        call(&mut ss, "delete_clip", json!({"clip_id": c}));
    }
}