//! RV-2: one build identity in kimo_info, saved projects and export reports; the old "1.0.0" is gone; newer project schemas are refused.
use kimo_core::buildinfo;
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn semver_ok(v: &str) -> bool { let (core, pre) = v.split_once('-').map(|(a, b)| (a, Some(b))).unwrap_or((v, None)); let p: Vec<&str> = core.split('.').collect(); p.len() == 3 && p.iter().all(|x| !x.is_empty() && x.chars().all(|ch| ch.is_ascii_digit())) && pre.map_or(true, |s| !s.is_empty()) }
#[test] fn kimo_info_reports_a_real_identity() {
    let mut s = Session::new(); let i = c(&mut s, "kimo_info", json!({}));
    let v = i["version"].as_str().unwrap(); assert!(semver_ok(v), "version {v:?} is not semver"); assert_ne!(v, "1.0.0", "must not advertise the old 1.0.0");
    assert_eq!(v, buildinfo::VERSION); assert_eq!(i["commit"], buildinfo::COMMIT); assert!(!buildinfo::COMMIT.is_empty());
    assert_eq!(i["schema_version"], buildinfo::SCHEMA_VERSION); assert!(i["features"].get("native_ocio").is_some(), "{i}");
    assert!(i["backends"]["ffmpeg"].get("available").is_some() && i["backends"]["ffprobe"].get("available").is_some(), "{i}");
    if i["backends"]["ffmpeg"]["available"] == true { assert!(!i["backends"]["ffmpeg"]["version"].as_str().unwrap().is_empty()); }
    assert_eq!(i["tools"].as_u64().unwrap() as usize, kimo_core::commands::registry_ref().len());
}
#[test] fn saved_project_records_the_identity_and_reloads() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
    let p = std::env::temp_dir().join(format!("kimo_id_{}.kimo", std::process::id())); let ps = p.to_str().unwrap();
    c(&mut s, "save_project", json!({"path": ps})); let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    assert_eq!(v["kimo"]["schema_version"], buildinfo::SCHEMA_VERSION); let me = json!({"version": buildinfo::VERSION, "commit": buildinfo::COMMIT}); assert_eq!(v["kimo"]["created_with"], me); assert_eq!(v["kimo"]["last_saved_with"], me, "saved file must carry the same version/commit as kimo_info"); assert_eq!(v["width"], 320, "project fields stay at the top level");
    let mut t = Session::new(); c(&mut t, "load_project", json!({"path": ps})); assert_eq!(t.project.width, 320);
    // a file from a future schema is refused with a clear message, and the session is untouched
    let mut fut = v.clone(); fut["kimo"]["schema_version"] = json!(buildinfo::SCHEMA_VERSION + 1); fut["kimo"]["last_saved_with"]["version"] = json!("9.9.9"); std::fs::write(&p, fut.to_string()).unwrap();
    let mut u = Session::new(); let e = dispatch(&mut u, "load_project", &json!({"path": ps})).unwrap_err(); assert!(e.contains("schema_version") && e.contains("newer") && e.contains("9.9.9"), "{e}"); assert_eq!(u.project.width, Session::new().project.width);
    // old files without the block still load
    let mut old = v.clone(); old.as_object_mut().unwrap().remove("kimo"); std::fs::write(&p, old.to_string()).unwrap(); let mut w = Session::new(); c(&mut w, "load_project", json!({"path": ps})); assert_eq!(w.project.width, 320);
    // re-save keeps created_with from the loaded file and updates only last_saved_with; saving twice is byte-stable
    let mut o = v.clone(); o["kimo"]["created_with"] = json!({"version": "1.9.0", "commit": "abc"}); o["kimo"]["last_saved_with"] = json!({"version": "1.9.0", "commit": "abc"}); std::fs::write(&p, o.to_string()).unwrap();
    let mut x = Session::new(); c(&mut x, "load_project", json!({"path": ps})); c(&mut x, "save_project", json!({"path": ps})); let a1 = std::fs::read(&p).unwrap(); let r: Value = serde_json::from_slice(&a1).unwrap();
    assert_eq!(r["kimo"]["created_with"], json!({"version": "1.9.0", "commit": "abc"})); assert_eq!(r["kimo"]["last_saved_with"], me);
    c(&mut x, "save_project", json!({"path": ps})); assert_eq!(a1, std::fs::read(&p).unwrap(), "re-save with an unchanged identity must be byte-stable");
    // a v2-style reader (unknown top-level key ignored, serde default) still reads a file with the block
    let q: kimo_core::model::Project = serde_json::from_slice(&a1).expect("Project must deserialize with the extra kimo key"); assert_eq!(q.width, 320);
    let _ = std::fs::remove_file(&p);
}
#[test] fn export_report_carries_the_identity() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("ffmpeg missing: skipping export identity check"); return; }
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.5], "start": 0, "duration": 0.5}));
    let p = std::env::temp_dir().join(format!("kimo_id_{}.mp4", std::process::id())); let r = c(&mut s, "export", json!({"path": p.to_str().unwrap()}));
    assert_eq!(r["kimo"], buildinfo::identity(), "export report: {r}"); let _ = std::fs::remove_file(&p);
}
