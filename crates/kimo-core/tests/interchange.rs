//! export_timeline / import_timeline: a kimo edit round-trips through Premiere (FCP7) XML, FCPXML and OpenTimelineIO.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn clips(s: &Session) -> Vec<(f64, f64, f64, bool)> {
    let mut v: Vec<(f64, f64, f64, bool)> = s.project.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| matches!(c.source, kimo_core::model::Source::Media { .. }))
        .map(|c| (c.start, c.duration, c.in_point, c.transition_in.is_some())).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v
}

#[test]
fn an_edit_round_trips_through_premiere_xml_fcpxml_and_otio() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("no ffmpeg; skipped"); return; }
    let dir = std::env::temp_dir().join(format!("kimo-interchange-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    let media = dir.join("shot.mp4").to_string_lossy().to_string();
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=25:duration=6", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=6", "-shortest", "-pix_fmt", "yuv420p", &media]).status().unwrap().success());
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = c(&mut s, "import_media", json!({"path": media}))["asset_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 2.0, "in_point": 1.0}));
    let b = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 2.0, "duration": 1.6, "in_point": 3.2}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "add_transition", json!({"clip_id": b, "kind": "crossfade", "duration": 0.4}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 80, "height": 40, "start": 0.5, "duration": 1.0}));
    c(&mut s, "add_marker", json!({"time": 1.0, "name": "Scene 1"}));
    let want = clips(&s); assert_eq!(want.len(), 2);
    for (ext, fmt) in [("xml", "fcp7"), ("fcpxml", "fcpxml"), ("otio", "otio")] {
        let out = dir.join(format!("edit.{ext}")).to_string_lossy().to_string();
        let r = c(&mut s, "export_timeline", json!({"path": out, "overwrite": true})); assert_eq!(r["format"], fmt, "{r}");
        assert!(r["notes"].as_array().unwrap().iter().any(|n| n.as_str().unwrap().starts_with("shape ")), "{fmt}: the shape layer is reported, not silently lost: {r}");
        let mut s2 = Session::new(); c(&mut s2, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
        let i = c(&mut s2, "import_timeline", json!({"path": out})); assert_eq!(i["format"], fmt, "{i}");
        let got = clips(&s2);
        assert_eq!(got.len(), 2, "{fmt}: both clips come back: {got:?} {i}");
        for (g, w) in got.iter().zip(&want) { assert!((g.0 - w.0).abs() < 0.021 && (g.1 - w.1).abs() < 0.021 && (g.2 - w.2).abs() < 0.021, "{fmt}: start/duration/in point {g:?} vs {w:?}"); }
        assert!(got[1].3, "{fmt}: the crossfade comes back on the second clip: {i}");
        assert!(s2.project.markers.iter().any(|m| m.name == "Scene 1" && (m.time - 1.0).abs() < 0.021), "{fmt}: the marker comes back: {:?}", s2.project.markers);
        assert!(s2.project.tracks.iter().flat_map(|t| t.clips.iter()).all(|c| c.audio.volume.base > 0.9), "{fmt}: no audio doubled or lost");
    }
    let e = dispatch(&mut s, "import_timeline", &json!({"path": media})).unwrap_err(); assert!(e.contains("not a"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
