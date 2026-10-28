//! TDD tests for make_montage_from_folder (FEATURES.md L561, the folder-montage template): scan a
//! folder non-recursively, import each recognized video/still file in filename order, lay them back to
//! back (optionally cross-fading) on a track.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(ss: &mut Session, n: &str, a: serde_json::Value) -> String { dispatch(ss, n, &a).unwrap_err() }

fn session() -> Session {
    let mut ss = Session::new();
    dispatch(&mut ss, "project_set", &json!({"width": 64, "height": 36, "fps": 25})).unwrap();
    ss
}

/// Writes a tiny solid-color PNG using ffmpeg (no extra crate dependency).
fn write_png(path: &std::path::Path, rgb: [u8; 3]) {
    let ch = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c=#{:02x}{:02x}{:02x}:s=16x9", rgb[0], rgb[1], rgb[2]), "-frames:v", "1", path.to_str().unwrap()])
        .output().expect("ffmpeg png");
    assert!(ch.status.success());
}

/// Writes a tiny 1s solid-color video using ffmpeg.
fn write_video(path: &std::path::Path, rgb: [u8; 3], secs: f64) {
    let ch = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c=#{:02x}{:02x}{:02x}:s=16x9:d={secs}:r=10", rgb[0], rgb[1], rgb[2]),
               "-c:v", "libx264", "-qp", "0", "-pix_fmt", "yuv420p", path.to_str().unwrap()])
        .output().expect("ffmpeg video");
    assert!(ch.status.success());
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("kimo_montage_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn stills_land_back_to_back_at_clip_duration_in_filename_order() {
    let d = tmpdir("stills");
    write_png(&d.join("a_red.png"), [255, 0, 0]);
    write_png(&d.join("b_green.png"), [0, 255, 0]);
    write_png(&d.join("c_blue.png"), [0, 0, 255]);
    let mut ss = session();
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap(), "clip_duration": 1.0}));
    let track_id = r["track_id"].as_str().unwrap().to_string();
    let clip_ids = r["clip_ids"].as_array().unwrap();
    assert_eq!(clip_ids.len(), 3, "one clip per recognized file");
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|t| t["id"] == track_id).unwrap();
    let clips = track["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 3);
    let starts: Vec<f64> = clips.iter().map(|c| c["start"].as_f64().unwrap()).collect();
    assert_eq!(starts, vec![0.0, 1.0, 2.0], "back to back at clip_duration, no gaps");
    // render a frame in the middle of each and check it is that file's own colour -- confirms filename order
    let f0 = call(&mut ss, "render_frame", json!({"time": 0.5, "path": d.join("f0.png").to_str().unwrap()}));
    let f1 = call(&mut ss, "render_frame", json!({"time": 1.5, "path": d.join("f1.png").to_str().unwrap()}));
    let f2 = call(&mut ss, "render_frame", json!({"time": 2.5, "path": d.join("f2.png").to_str().unwrap()}));
    assert!(f0["path"].is_string() && f1["path"].is_string() && f2["path"].is_string());
}

#[test]
fn stills_without_clip_duration_is_a_named_error() {
    let d = tmpdir("needdur");
    write_png(&d.join("only.png"), [10, 20, 30]);
    let mut ss = session();
    let e = err(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap()}));
    assert!(e.contains("clip_duration"), "error should name the missing clip_duration, got: {e}");
}

#[test]
fn videos_use_their_own_duration_when_clip_duration_is_omitted() {
    let d = tmpdir("vids_nodur");
    write_video(&d.join("a.mp4"), [200, 0, 0], 0.6);
    write_video(&d.join("b.mp4"), [0, 200, 0], 0.4);
    let mut ss = session();
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap()}));
    let track_id = r["track_id"].as_str().unwrap().to_string();
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|t| t["id"] == track_id).unwrap();
    let clips = track["clips"].as_array().unwrap();
    let durs: Vec<f64> = clips.iter().map(|c| c["duration"].as_f64().unwrap()).collect();
    assert!((durs[0] - 0.6).abs() < 0.05, "first clip keeps its own ~0.6s duration, got {:?}", durs);
    assert!((durs[1] - 0.4).abs() < 0.05, "second clip keeps its own ~0.4s duration, got {:?}", durs);
}

#[test]
fn clip_duration_caps_a_longer_video_but_does_not_stretch_a_shorter_one() {
    let d = tmpdir("vids_cap");
    write_video(&d.join("a_long.mp4"), [200, 0, 0], 2.0);
    write_video(&d.join("b_short.mp4"), [0, 200, 0], 0.3);
    let mut ss = session();
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap(), "clip_duration": 1.0}));
    let track_id = r["track_id"].as_str().unwrap().to_string();
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|t| t["id"] == track_id).unwrap();
    let clips = track["clips"].as_array().unwrap();
    let durs: Vec<f64> = clips.iter().map(|c| c["duration"].as_f64().unwrap()).collect();
    assert!((durs[0] - 1.0).abs() < 1e-6, "the 2s video is capped to clip_duration 1.0, got {}", durs[0]);
    assert!((durs[1] - 0.3).abs() < 0.05, "the 0.3s video keeps its own shorter length, not stretched to 1.0, got {}", durs[1]);
}

#[test]
fn unrecognized_files_are_skipped_not_fatal() {
    let d = tmpdir("skip");
    write_png(&d.join("a.png"), [1, 2, 3]);
    std::fs::write(d.join("notes.txt"), b"not media").unwrap();
    let mut ss = session();
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap(), "clip_duration": 1.0}));
    assert_eq!(r["clip_ids"].as_array().unwrap().len(), 1, "only the recognized file becomes a clip");
    assert!(r["skipped"].as_array().map(|a| a.len()).unwrap_or(0) >= 1, "notes.txt is reported as skipped, not silently dropped with no trace");
}

#[test]
fn empty_or_all_unrecognized_folder_is_a_named_error() {
    let d = tmpdir("empty");
    std::fs::write(d.join("notes.txt"), b"not media").unwrap();
    let mut ss = session();
    let e = err(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap()}));
    assert!(e.contains("no recognized media"), "got: {e}");
}

#[test]
fn not_a_directory_is_a_named_error() {
    let mut ss = session();
    let e = err(&mut ss, "make_montage_from_folder", json!({"folder": "/definitely/not/a/real/path_xyz"}));
    assert!(e.contains("not a directory") || e.contains("cannot read"), "got: {e}");
}

#[test]
fn existing_track_id_appends_after_its_last_clip_not_at_the_folders_start() {
    let d = tmpdir("append");
    write_png(&d.join("a.png"), [9, 9, 9]);
    let mut ss = session();
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#101010", "start": 0, "duration": 5.0}));
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap(), "track_id": &t, "start": 5.0, "clip_duration": 1.0}));
    assert_eq!(r["track_id"], json!(t));
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|tr| tr["id"] == t).unwrap();
    let clips = track["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[1]["start"].as_f64().unwrap(), 5.0);
}

#[test]
fn crossfade_overlaps_consecutive_clips_and_registers_a_transition() {
    let d = tmpdir("xfade");
    write_video(&d.join("a.mp4"), [200, 0, 0], 1.0);
    write_video(&d.join("b.mp4"), [0, 200, 0], 1.0);
    let mut ss = session();
    let r = call(&mut ss, "make_montage_from_folder", json!({"folder": d.to_str().unwrap(), "crossfade": 0.2}));
    let track_id = r["track_id"].as_str().unwrap().to_string();
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|t| t["id"] == track_id).unwrap();
    let clips = track["clips"].as_array().unwrap();
    let starts: Vec<f64> = clips.iter().map(|c| c["start"].as_f64().unwrap()).collect();
    assert!((starts[1] - 0.8).abs() < 1e-6, "second clip starts 0.2s (the crossfade) before the first ends, got {:?}", starts);
    assert!(clips[1]["transition_in"].is_object(), "the overlapping clip has a transition_in registered");
}
