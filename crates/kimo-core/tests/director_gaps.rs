//! Director-mode open items before the Oct 26 hackathon (DIRECTOR_DRYRUN items 11-12, SMALL_MODEL_PREP): each test was written
//! red against the behaviour a director agent or the demo machine hit.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_director_gaps_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

/// DIRECTOR_DRYRUN item 11: export returned only path and duration, so an agent delivering to a 16 MB cap had no way to
/// check the file it just wrote. The result now carries the file's size in bytes beside its duration.
#[test] fn export_reports_file_size_and_duration() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30}));
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.4, 0.6], "start": 0, "duration": 1.5}));
    let out = dir().join("size.mp4"); let r = call(&mut ss, "export", json!({"path": out, "crf": 30}));
    let on_disk = std::fs::metadata(&out).unwrap().len();
    assert!(on_disk > 0);
    assert_eq!(r["bytes"].as_u64(), Some(on_disk), "export must report the written file's size in bytes: {r}");
    assert!((r["duration"].as_f64().unwrap() - 1.5).abs() < 0.05, "duration: {r}");
}
