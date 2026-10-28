//! TDD tests for versioned project-file migrations (FEATURES.md L560): load_project migrates a raw file
//! forward to this build's schema_version before deserializing it into a Project.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn write_v1_fixture(path: &std::path::Path, track_names: &[&str]) {
    let tracks: Vec<serde_json::Value> = track_names.iter().map(|n| json!({"id": format!("t{}", rand_id()), "clips": [], "locked": false, "muted": false, "name": n})).collect();
    let v = json!({"width": 64, "height": 36, "fps": 25, "name": "untitled", "tracks": tracks, "assets": {}, "next_id": 1,
        "kimo": {"schema_version": 1, "created_with": {"version": "6.0.0", "commit": "deadbeef"}, "last_saved_with": {"version": "6.0.0", "commit": "deadbeef"}}});
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
fn rand_id() -> u32 { use std::time::{SystemTime, UNIX_EPOCH}; (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos()) % 100000 }

#[test]
fn loading_a_v1_file_names_its_blank_tracks() {
    let path = std::env::temp_dir().join(format!("kimo_mig_v1_{}.kimo", std::process::id()));
    write_v1_fixture(&path, &["", ""]);
    let mut ss = Session::new();
    call(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    let proj = call(&mut ss, "get_project", json!({}));
    let names: Vec<String> = proj["tracks"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["Track 1", "Track 2"]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn loading_a_v1_file_leaves_already_named_tracks_alone() {
    let path = std::env::temp_dir().join(format!("kimo_mig_v1_named_{}.kimo", std::process::id()));
    write_v1_fixture(&path, &["A-roll", ""]);
    let mut ss = Session::new();
    call(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    let proj = call(&mut ss, "get_project", json!({}));
    let names: Vec<String> = proj["tracks"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["A-roll", "Track 2"]);
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_file_with_no_kimo_header_at_all_is_treated_as_v1_and_migrated() {
    // Pre-identity files (older than buildinfo.rs itself) carry no "kimo" block at all.
    let path = std::env::temp_dir().join(format!("kimo_mig_noheader_{}.kimo", std::process::id()));
    let v = json!({"width": 64, "height": 36, "fps": 25, "name": "untitled", "tracks": [{"id": "t1", "clips": [], "locked": false, "muted": false, "name": ""}], "assets": {}, "next_id": 1});
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let mut ss = Session::new();
    call(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    let proj = call(&mut ss, "get_project", json!({}));
    assert_eq!(proj["tracks"][0]["name"], "Track 1");
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_file_already_at_the_current_schema_version_is_not_touched() {
    let path = std::env::temp_dir().join(format!("kimo_mig_current_{}.kimo", std::process::id()));
    let v = json!({"width": 64, "height": 36, "fps": 25, "name": "untitled", "tracks": [{"id": "t1", "clips": [], "locked": false, "muted": false, "name": ""}], "assets": {}, "next_id": 1,
        "kimo": {"schema_version": kimo_core::buildinfo::SCHEMA_VERSION, "created_with": {"version": "7.0.0", "commit": "cafebabe"}, "last_saved_with": {"version": "7.0.0", "commit": "cafebabe"}}});
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let mut ss = Session::new();
    call(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    let proj = call(&mut ss, "get_project", json!({}));
    assert_eq!(proj["tracks"][0]["name"], "", "already current -- no migration should have touched it");
    std::fs::remove_file(&path).ok();
}

#[test]
fn a_file_newer_than_this_build_is_refused_not_migrated() {
    let path = std::env::temp_dir().join(format!("kimo_mig_future_{}.kimo", std::process::id()));
    let v = json!({"width": 64, "height": 36, "fps": 25, "name": "untitled", "tracks": [], "assets": {}, "next_id": 1,
        "kimo": {"schema_version": kimo_core::buildinfo::SCHEMA_VERSION + 1, "created_with": {"version": "99.0.0", "commit": "future"}, "last_saved_with": {"version": "99.0.0", "commit": "future"}}});
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let mut ss = Session::new();
    let e = dispatch(&mut ss, "load_project", &json!({"path": path.to_str().unwrap()})).unwrap_err();
    assert!(e.contains("newer than this build"), "got: {e}");
    std::fs::remove_file(&path).ok();
}

#[test]
fn saving_a_migrated_project_writes_the_current_schema_version_so_it_never_re_migrates() {
    let path = std::env::temp_dir().join(format!("kimo_mig_resave_{}.kimo", std::process::id()));
    write_v1_fixture(&path, &[""]);
    let mut ss = Session::new();
    call(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    call(&mut ss, "save_project", json!({"path": path.to_str().unwrap()}));
    let saved = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(v["kimo"]["schema_version"].as_u64().unwrap(), kimo_core::buildinfo::SCHEMA_VERSION as u64);
    assert_eq!(v["tracks"][0]["name"], "Track 1", "the migrated name round-trips through a save");
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(format!("{}.history", path.to_str().unwrap())).ok();
}
