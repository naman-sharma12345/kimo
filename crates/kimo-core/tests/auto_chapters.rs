//! v7.3: auto_chapters -- chapters an exported file from this timeline's markers, detecting scene cuts
//! first (mark_scene_cuts) if there aren't enough markers yet to satisfy mux_chapters' own minimum.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_auto_chapters_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }
/// Three visibly distinct colour blocks back to back -- two real scene cuts (enough to clear
/// mux_chapters' own 2-marker minimum) that detect_cuts (histogram/luma) should find, unlike a single
/// continuous shot.
fn two_tone_clip(name: &str, dur_each: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=c=red:s=360x640:r=30:d={dur_each}"), "-f", "lavfi", "-i", &format!("color=c=blue:s=360x640:r=30:d={dur_each}"),
        "-f", "lavfi", "-i", &format!("color=c=yellow:s=360x640:r=30:d={dur_each}"),
        "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={}:r=48000", dur_each * 3.0),
        "-filter_complex", "[0:v][1:v][2:v]concat=n=3:v=1:a=0[v]", "-map", "[v]", "-map", "3:a",
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn portrait() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }

/// No markers yet: auto_chapters must run scene-cut detection itself (the gap the v7.2 audit named --
/// mux_chapters alone needed markers to already exist) and then chapter from what it found.
#[test] fn detects_scene_cuts_then_chapters_when_no_markers_exist() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": two_tone_clip("ac1.mp4", 1.5)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.5, "fit": "cover"}));
    assert_eq!(ss.project.markers.len(), 0, "fixture should start with no markers");

    let out = dir().join("ac1_out.mp4"); let out = out.to_str().unwrap();
    call(&mut ss, "export", json!({"path": out, "crf": 30}));

    let r = call(&mut ss, "auto_chapters", json!({"path": out}));
    assert_eq!(r["used_existing_markers"], false, "{r}");
    assert_eq!(r["existing_markers"], json!(0), "{r}");
    assert_eq!(r["chaptered"], true, "{r}");
    assert!(r["markers_now"].as_u64().unwrap() >= 2, "scene-cut detection should have created markers: {r}");
    assert!(ss.project.markers.len() >= 2, "the markers must actually exist on the project now");
}

/// Markers already on the timeline (>= min_markers): no detection should run at all -- chapters straight
/// from the existing markers, nothing added.
#[test] fn uses_existing_markers_without_running_detection() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": two_tone_clip("ac2.mp4", 1.5)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.5, "fit": "cover"}));
    call(&mut ss, "add_marker", json!({"time": 0.0, "name": "Intro"}));
    call(&mut ss, "add_marker", json!({"time": 1.5, "name": "Part Two"}));
    assert_eq!(ss.project.markers.len(), 2);

    let out = dir().join("ac2_out.mp4"); let out = out.to_str().unwrap();
    call(&mut ss, "export", json!({"path": out, "crf": 30}));

    let r = call(&mut ss, "auto_chapters", json!({"path": out}));
    assert_eq!(r["used_existing_markers"], true, "{r}");
    assert_eq!(r["existing_markers"], json!(2), "{r}");
    assert_eq!(r["chaptered"], true, "{r}");
    assert_eq!(r["scene_cut_detection"]["ran"], json!(false), "detection must not run when markers already suffice: {r}");
    assert_eq!(ss.project.markers.len(), 2, "no marker should have been added");
    assert_eq!(r["mux_chapters"]["chapters"], json!(2), "{r}");
}

/// dry_run must report the plan (and whether chaptering would succeed) without creating markers or
/// touching the exported file.
#[test] fn dry_run_reports_the_plan_without_mutating_or_writing() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": two_tone_clip("ac3.mp4", 1.5)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.5, "fit": "cover"}));

    let out = dir().join("ac3_out.mp4"); let out = out.to_str().unwrap();
    call(&mut ss, "export", json!({"path": out, "crf": 30}));
    let meta_before = std::fs::metadata(out).unwrap().modified().unwrap();

    let r = call(&mut ss, "auto_chapters", json!({"path": out, "dry_run": true}));
    assert_eq!(r["dry_run"], true, "{r}");
    assert_eq!(ss.project.markers.len(), 0, "dry_run must not create markers");
    let meta_after = std::fs::metadata(out).unwrap().modified().unwrap();
    assert_eq!(meta_before, meta_after, "dry_run must not touch the exported file");
}

/// A single continuous shot has no detectable cuts: even after running mark_scene_cuts, markers stay
/// under min_markers. auto_chapters must say so plainly (ok:false, chaptered:false) rather than failing
/// loudly from mux_chapters' own error or inventing a chapter boundary.
fn solid_clip(name: &str, dur: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=c=green:s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={dur}:r=48000"),
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

#[test] fn a_single_continuous_shot_reports_it_cannot_chapter() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": solid_clip("ac4_solid.mp4", 2.0)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "fit": "cover"}));

    let out = dir().join("ac4_out.mp4"); let out = out.to_str().unwrap();
    call(&mut ss, "export", json!({"path": out, "crf": 30}));

    let r = call(&mut ss, "auto_chapters", json!({"path": out}));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["chaptered"], false, "{r}");
    assert!(r["reason"].as_str().unwrap().contains("at least"), "{r}");
}
