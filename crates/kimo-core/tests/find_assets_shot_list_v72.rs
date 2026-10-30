//! v7.2 additions: find_assets grows into the "shot list" -- kind/hash/origin surfaced (already on the
//! Asset model, just not returned before), used_by_clips (reverse lookup: how many timeline clips
//! reference this asset, across every track), and opt-in probe_audio for has_audio. Backward compatible:
//! existing callers (e.g. tests/bins_subclips_colors.rs) that only read asset_id/width/height/etc are
//! untouched; the new fields are additive.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn silent_fixture(name: &str) -> String {
    let p = format!("/tmp/kimo-findassets-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=25:duration=1", "-pix_fmt", "yuv420p", &p])
        .status().unwrap().success());
    p
}
fn audio_fixture(name: &str) -> String {
    let p = format!("/tmp/kimo-findassets-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=25:duration=1", "-f", "lavfi", "-i", "sine=frequency=1000:duration=1",
               "-pix_fmt", "yuv420p", "-shortest", &p])
        .status().unwrap().success());
    p
}

#[test] fn kind_hash_origin_ride_along_as_already_stored_on_the_asset() {
    let mut s = Session::new();
    let p = silent_fixture("a");
    let aid = c(&mut s, "import_media", json!({"path": p.clone()}))["asset_id"].as_str().unwrap().to_string();
    let m = c(&mut s, "find_assets", json!({}))["matches"].as_array().unwrap().clone();
    let entry = m.iter().find(|x| x["asset_id"] == aid).unwrap();
    // Not asserting specific values for kind/hash/origin (import_media's own choices, not this
    // feature's) -- only that the fields now ride along (were entirely absent before this pass).
    assert!(entry.get("kind").is_some());
    assert!(entry.get("hash").is_some());
    assert!(entry.get("origin").is_some());
    std::fs::remove_file(&p).ok();
}

#[test] fn used_by_clips_counts_every_track_and_zero_for_an_unused_asset() {
    let mut s = Session::new();
    let p = silent_fixture("b");
    let aid = c(&mut s, "import_media", json!({"path": p.clone()}))["asset_id"].as_str().unwrap().to_string();
    let m = c(&mut s, "find_assets", json!({}))["matches"].as_array().unwrap().clone();
    assert_eq!(m.iter().find(|x| x["asset_id"] == aid).unwrap()["used_by_clips"], 0, "imported but not placed yet");

    let t1 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aid, "start": 0, "duration": 1.0}));
    c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": aid, "start": 0, "duration": 1.0}));
    let m2 = c(&mut s, "find_assets", json!({}))["matches"].as_array().unwrap().clone();
    assert_eq!(m2.iter().find(|x| x["asset_id"] == aid).unwrap()["used_by_clips"], 2, "placed on two tracks");
    std::fs::remove_file(&p).ok();
}

#[test] fn has_audio_is_null_without_probe_audio_and_populated_with_it() {
    let mut s = Session::new();
    let silent = silent_fixture("c"); let with_audio = audio_fixture("d");
    let a_silent = c(&mut s, "import_media", json!({"path": silent.clone()}))["asset_id"].as_str().unwrap().to_string();
    let a_audio = c(&mut s, "import_media", json!({"path": with_audio.clone()}))["asset_id"].as_str().unwrap().to_string();

    let m = c(&mut s, "find_assets", json!({}))["matches"].as_array().unwrap().clone();
    for e in &m { assert!(e["has_audio"].is_null(), "probe_audio defaults to false: no ffprobe spent unless asked"); }

    let m2 = c(&mut s, "find_assets", json!({"probe_audio": true}))["matches"].as_array().unwrap().clone();
    assert_eq!(m2.iter().find(|x| x["asset_id"] == a_silent).unwrap()["has_audio"], false);
    assert_eq!(m2.iter().find(|x| x["asset_id"] == a_audio).unwrap()["has_audio"], true);
    std::fs::remove_file(&silent).ok(); std::fs::remove_file(&with_audio).ok();
}

#[test] fn existing_bin_and_name_glob_filters_still_work_exactly_as_before() {
    let mut s = Session::new();
    let p1 = silent_fixture("e"); let p2 = silent_fixture("f");
    let a1 = c(&mut s, "import_media", json!({"path": p1.clone()}))["asset_id"].as_str().unwrap().to_string();
    let _a2 = c(&mut s, "import_media", json!({"path": p2.clone()}))["asset_id"].as_str().unwrap().to_string();
    c(&mut s, "move_asset_to_bin", json!({"asset_id": a1, "bin": "Interviews"}));
    let filed = c(&mut s, "find_assets", json!({"bin": "Interviews"}))["matches"].as_array().unwrap().clone();
    assert_eq!(filed.len(), 1); assert_eq!(filed[0]["asset_id"], a1);
    let unfiled = c(&mut s, "find_assets", json!({"bin": Value::Null}))["matches"].as_array().unwrap().clone();
    assert_eq!(unfiled.len(), 1);
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn is_read_only() {
    let mut s = Session::new();
    let p = silent_fixture("g");
    c(&mut s, "import_media", json!({"path": p.clone()}));
    let h = serde_json::to_string(&s.project).unwrap(); let u = s.undo.len();
    c(&mut s, "find_assets", json!({"probe_audio": true}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), h); assert_eq!(s.undo.len(), u);
    std::fs::remove_file(&p).ok();
}

