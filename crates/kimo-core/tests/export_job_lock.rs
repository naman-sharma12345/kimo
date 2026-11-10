//! RV-20 (a)(b): job.lock (O_EXCL, same-host, stale reclaim, drop release) and the index entry shape. Real ffmpeg, tiny projects.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::export_job::{host_name, start_ticks};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_rv20a_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn scene() -> Session {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for i in 0..3 { c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20 + i * 3, "height": 20, "start": i as f64 * 2.0, "duration": 2.0})); } ss
}
fn args(d: &std::path::Path) -> Value { json!({"path": d.join("out.mp4").to_str().unwrap(), "chunk_s": 2, "wait": true}) }
fn lockp(d: &std::path::Path) -> std::path::PathBuf { d.join("out.mp4.chunks/job.lock") }
fn put_lock(d: &std::path::Path, pid: u32, host: &str, ticks: Option<u64>) { std::fs::create_dir_all(d.join("out.mp4.chunks")).unwrap(); std::fs::write(lockp(d), json!({"pid": pid, "host": host, "started_at": 1, "start_ticks": ticks}).to_string()).unwrap(); }
fn err(ss: &mut Session, n: &str, a: Value) -> String { dispatch(ss, n, &a).err().unwrap_or_else(|| panic!("{n} should fail")) }

#[test] fn a_live_lock_refuses_start_and_resume_and_leaves_the_directory_alone() {
    let d = tmp("live"); let mut ss = scene(); let me = std::process::id(); put_lock(&d, me, &host_name(), start_ticks(me));
    let m = err(&mut ss, "export_job_start", args(&d)); assert!(m.starts_with("job_locked:"), "{m}"); assert!(m.contains(&format!("pid {me}")), "{m}");
    assert!(!d.join("out.mp4.chunks/manifest.json").exists(), "a refused start writes nothing"); assert!(lockp(&d).is_file(), "the live owner's lock is untouched");
    assert!(!d.join("out.mp4").exists()); std::fs::remove_dir_all(&d).ok();
}
#[test] fn a_stale_lock_is_reclaimed_and_the_lock_is_released_on_every_exit() {
    let d = tmp("stale"); let mut ss = scene();
    put_lock(&d, 4_000_000, &host_name(), Some(1)); let s = c(&mut ss, "export_job_start", args(&d)); assert_eq!(s["status"]["state"], "done", "{s}"); assert!(!lockp(&d).exists(), "released after done");
    let me = std::process::id(); put_lock(&d, me, &host_name(), start_ticks(me).map(|t| t + 1)); // pid alive but a different start time = pid reuse
    let s = c(&mut ss, "export_job_start", args(&d)); assert_eq!(s["status"]["state"], "done", "{s}");
    // a failing job releases it too
    let mut bad = scene(); let d2 = tmp("fail"); let mut a = args(&d2); a["path"] = json!(d2.join("no/such/dir/out.mp4").to_str().unwrap()); a["dir"] = json!(d2.join("ch").to_str().unwrap());
    let s = c(&mut bad, "export_job_start", a); assert_eq!(s["status"]["state"], "failed", "{s}"); assert!(!d2.join("ch/job.lock").exists(), "released after failure");
    std::fs::remove_dir_all(&d).ok(); std::fs::remove_dir_all(&d2).ok();
}
#[test] fn a_lock_from_another_host_cannot_be_verified_and_is_refused() {
    let d = tmp("host"); let mut ss = scene(); put_lock(&d, 4_000_000, "some-other-host", Some(1));
    let m = err(&mut ss, "export_job_start", args(&d)); assert!(m.starts_with("job_locked:") && m.contains("some-other-host") && m.contains("same-host"), "{m}"); assert!(lockp(&d).is_file()); std::fs::remove_dir_all(&d).ok();
}
#[test] fn index_entries_carry_file_frames_bytes_sha256_and_key() {
    let d = tmp("idx"); let mut ss = scene(); c(&mut ss, "export_job_start", args(&d));
    let idx: Value = serde_json::from_slice(&std::fs::read(d.join("out.mp4.chunks/index.json")).unwrap()).unwrap();
    for i in 0..3 { let e = &idx["chunks"][i.to_string()]; assert_eq!(e["file"], format!("chunk_{i:05}.mp4")); assert_eq!(e["frames"], 50); assert!(e["bytes"].as_u64().unwrap() > 100); assert_eq!(e["sha256"].as_str().unwrap().len(), 64); assert!(e["key"].as_str().unwrap().len() >= 8); }
    std::fs::remove_dir_all(&d).ok();
}
