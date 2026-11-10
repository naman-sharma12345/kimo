//! v7.3: `import_edl` -- a standard CMX3600 EDL or a simple single-video-track OTIO JSON timeline becomes
//! real clips on the project's timeline, resolving each event's source media by an explicit map, a
//! directory scan by filename, or a path already embedded in the file itself. Tiny fixed fixtures, no
//! network, no real video files for parsing-only cases -- real tiny shape clips get imported where a
//! resolvable media path is needed end to end.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_edl_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }

const CMX3600_TWO_EVENTS: &str = "\
TITLE: My Sequence
FCM: NON-DROP FRAME

001  REEL1    V     C        00:00:00:00 00:00:05:00 00:00:00:00 00:00:05:00
* FROM CLIP NAME: clip_a.mp4
002  REEL2    V     C        00:00:01:12 00:00:03:12 00:00:05:00 00:00:07:00
* FROM CLIP NAME: clip_b.mp4
";

fn make_media(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    // a real tiny video file ffprobe can actually read, so an end-to-end import produces a real clip
    let p = dir.join(name);
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=red:s=32x18:d=10:r=25", "-pix_fmt", "yuv420p"]).arg(&p).status().unwrap();
    assert!(status.success());
    p
}

#[test] fn imports_two_cmx3600_events_as_two_clips_at_their_record_in_and_duration() {
    let dir = tmp("basic"); let a = make_media(&dir, "clip_a.mp4"); let b = make_media(&dir, "clip_b.mp4");
    let edl = dir.join("seq.edl"); std::fs::write(&edl, CMX3600_TWO_EVENTS).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap()}));
    assert_eq!(r["imported"], 2, "{r}"); assert_eq!(r["events_total"], 2); assert_eq!(r["unresolved"].as_array().unwrap().len(), 0, "{r}"); assert_eq!(r["format"], "cmx3600");
    let track_id = r["track_id"].as_str().unwrap().to_string();
    let tl = c(&mut ss, "get_project", json!({}));
    let track = tl["tracks"].as_array().unwrap().iter().find(|t| t["id"] == track_id).unwrap();
    let clips = track["clips"].as_array().unwrap(); assert_eq!(clips.len(), 2, "{clips:?}");
    let c0 = clips.iter().find(|c| (c["start"].as_f64().unwrap() - 0.0).abs() < 1e-6).unwrap();
    assert!((c0["duration"].as_f64().unwrap() - 5.0).abs() < 1e-6, "{c0}");
    let c1 = clips.iter().find(|c| (c["start"].as_f64().unwrap() - 5.0).abs() < 1e-6).unwrap();
    assert!((c1["duration"].as_f64().unwrap() - 2.0).abs() < 1e-6, "{c1}: record_out 7s - record_in 5s = 2s");
    let _ = (a, b);
}

#[test] fn source_in_point_comes_from_the_edls_source_in_timecode_not_zero() {
    let dir = tmp("inpoint"); make_media(&dir, "clip_b.mp4");
    let edl_text = "\
001  REEL2    V     C        00:00:01:12 00:00:03:12 00:00:05:00 00:00:07:00\n\
* FROM CLIP NAME: clip_b.mp4\n";
    let edl = dir.join("seq.edl"); std::fs::write(&edl, edl_text).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap()}));
    assert_eq!(r["imported"], 1, "{r}");
    let tl = c(&mut ss, "get_project", json!({}));
    let clip = tl["tracks"][0]["clips"][0].clone();
    // 00:00:01:12 @ 25fps = 1 + 12/25 = 1.48s
    assert!((clip["in_point"].as_f64().unwrap() - 1.48).abs() < 1e-6, "{clip}");
}

#[test] fn an_unresolvable_clip_name_is_reported_not_silently_dropped_or_a_hard_failure() {
    let dir = tmp("missing"); // no media files at all
    let edl = dir.join("seq.edl"); std::fs::write(&edl, CMX3600_TWO_EVENTS).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap()}));
    assert_eq!(r["imported"], 0, "{r}"); assert_eq!(r["events_total"], 2);
    let unresolved = r["unresolved"].as_array().unwrap(); assert_eq!(unresolved.len(), 2, "{r}");
    assert_eq!(unresolved[0]["clip_name"], "clip_a.mp4"); assert!(unresolved[0]["record_in"].as_f64().is_some());
}

#[test] fn media_map_takes_priority_over_media_dir_for_a_given_clip_name() {
    let dir = tmp("map"); let other_dir = tmp("map_other");
    make_media(&dir, "clip_a.mp4"); // would resolve via media_dir...
    let real = make_media(&other_dir, "the_real_one.mp4"); // ...but media_map should win
    let edl_text = "001  REEL1    V     C        00:00:00:00 00:00:05:00 00:00:00:00 00:00:05:00\n* FROM CLIP NAME: clip_a.mp4\n";
    let edl = dir.join("seq.edl"); std::fs::write(&edl, edl_text).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap(), "media_map": {"clip_a.mp4": real.to_str().unwrap()}}));
    assert_eq!(r["imported"], 1, "{r}");
    let tl = c(&mut ss, "get_project", json!({}));
    let asset_id = tl["tracks"][0]["clips"][0]["source"]["asset"].as_str().unwrap().to_string();
    let asset = tl["assets"][&asset_id].clone();
    assert_eq!(asset["path"], real.to_str().unwrap(), "{asset}");
}

#[test] fn dry_run_reports_without_creating_any_track_or_clip() {
    let dir = tmp("dry"); make_media(&dir, "clip_a.mp4"); make_media(&dir, "clip_b.mp4");
    let edl = dir.join("seq.edl"); std::fs::write(&edl, CMX3600_TWO_EVENTS).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let before = c(&mut ss, "get_project", json!({}));
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap(), "dry_run": true}));
    assert_eq!(r["imported"], 2, "{r}"); assert_eq!(r["dry_run"], true);
    let after = c(&mut ss, "get_project", json!({})); assert_eq!(before, after, "dry_run must not touch the project");
}

#[test] fn an_explicit_track_id_is_used_instead_of_creating_a_new_track() {
    let dir = tmp("track"); make_media(&dir, "clip_a.mp4"); make_media(&dir, "clip_b.mp4");
    let edl = dir.join("seq.edl"); std::fs::write(&edl, CMX3600_TWO_EVENTS).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let existing = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "import_edl", json!({"path": edl.to_str().unwrap(), "media_dir": dir.to_str().unwrap(), "track_id": existing}));
    assert_eq!(r["track_id"], existing, "{r}");
    let tl = c(&mut ss, "get_project", json!({})); assert_eq!(tl["tracks"].as_array().unwrap().len(), 1, "no extra track should be created");
}

const OTIO_SIMPLE: &str = r#"{
  "OTIO_SCHEMA": "Timeline.1",
  "tracks": {
    "OTIO_SCHEMA": "Stack.1",
    "children": [
      {
        "OTIO_SCHEMA": "Track.1",
        "kind": "Video",
        "children": [
          {
            "OTIO_SCHEMA": "Clip.2",
            "name": "shot_01",
            "source_range": { "start_time": { "value": 0, "rate": 25 }, "duration": { "value": 50, "rate": 25 } }
          },
          {
            "OTIO_SCHEMA": "Gap.1",
            "source_range": { "start_time": { "value": 0, "rate": 25 }, "duration": { "value": 25, "rate": 25 } }
          },
          {
            "OTIO_SCHEMA": "Clip.2",
            "name": "shot_02",
            "source_range": { "start_time": { "value": 10, "rate": 25 }, "duration": { "value": 75, "rate": 25 } }
          }
        ]
      }
    ]
  }
}"#;

#[test] fn imports_an_otio_timeline_with_a_gap_at_the_correct_record_positions() {
    let dir = tmp("otio"); make_media(&dir, "shot_01.mp4"); make_media(&dir, "shot_02.mp4");
    let otio = dir.join("seq.otio"); std::fs::write(&otio, OTIO_SIMPLE).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": otio.to_str().unwrap(), "media_dir": dir.to_str().unwrap()}));
    assert_eq!(r["format"], "otio", "{r}"); assert_eq!(r["imported"], 2, "{r}");
    let tl = c(&mut ss, "get_project", json!({}));
    let clips = tl["tracks"][0]["clips"].as_array().unwrap();
    let shot1 = clips.iter().find(|c| (c["start"].as_f64().unwrap() - 0.0).abs() < 1e-6).unwrap();
    assert!((shot1["duration"].as_f64().unwrap() - 2.0).abs() < 1e-6, "{shot1}: 50 frames @ 25fps = 2s");
    // shot_02 starts after shot_01 (2s) + the 1s gap = 3s
    let shot2 = clips.iter().find(|c| (c["start"].as_f64().unwrap() - 3.0).abs() < 1e-6).unwrap();
    assert!((shot2["duration"].as_f64().unwrap() - 3.0).abs() < 1e-6, "{shot2}: 75 frames @ 25fps = 3s");
    assert!((shot2["in_point"].as_f64().unwrap() - 0.4).abs() < 1e-6, "{shot2}: start_time 10 frames @ 25fps = 0.4s");
}

#[test] fn format_is_auto_detected_from_content_not_from_the_file_extension() {
    let dir = tmp("autodetect"); make_media(&dir, "shot_01.mp4"); make_media(&dir, "shot_02.mp4");
    let path = dir.join("seq.txt"); // deliberately not .otio
    std::fs::write(&path, OTIO_SIMPLE).unwrap();
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let r = c(&mut ss, "import_edl", json!({"path": path.to_str().unwrap(), "media_dir": dir.to_str().unwrap()}));
    assert_eq!(r["format"], "otio", "{r}"); assert_eq!(r["imported"], 2);
}

#[test] fn a_missing_edl_file_is_a_clear_error() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let e = dispatch(&mut ss, "import_edl", &json!({"path": "/no/such/file.edl"})).unwrap_err();
    assert!(e.to_lowercase().contains("no such file") || e.contains("/no/such/file.edl"), "{e}");
}
