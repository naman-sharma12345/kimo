//! RV-20 (c): crash recovery. Untrusted leftovers are quarantined; every indexed chunk is verified (exists, size, sha256, frame count, key) before reuse.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_rv20c_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn scene() -> Session {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for i in 0..3 { c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20 + i * 3, "height": 20, "start": i as f64 * 2.0, "duration": 2.0})); } ss
}
fn args(d: &std::path::Path) -> Value { json!({"path": d.join("out.mp4").to_str().unwrap(), "chunk_s": 2, "wait": true}) }
fn ch(d: &std::path::Path) -> std::path::PathBuf { d.join("out.mp4.chunks") }
fn chunk(d: &std::path::Path, i: u64) -> std::path::PathBuf { ch(d).join(format!("chunk_{i:05}.mp4")) }
fn idx(d: &std::path::Path) -> Value { serde_json::from_slice(&std::fs::read(ch(d).join("index.json")).unwrap()).unwrap() }
fn put_idx(d: &std::path::Path, v: &Value) { std::fs::write(ch(d).join("index.json"), serde_json::to_vec_pretty(v).unwrap()).unwrap(); }
fn frames(p: &std::path::Path) -> u64 { let o = std::process::Command::new("ffprobe").args(["-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"]).arg(p).output().unwrap(); String::from_utf8_lossy(&o.stdout).trim().parse().unwrap_or(0) }
fn rendered(s: &Value) -> Vec<(u64, String)> { s["result"]["rendered"].as_array().unwrap().iter().map(|r| (r["index"].as_u64().unwrap(), r["reason"].as_str().unwrap().to_string())).collect() }
fn first(d: &std::path::Path) -> Session { let mut ss = scene(); let s = c(&mut ss, "export_job_start", args(d)); assert_eq!(s["status"]["state"], "done"); ss }
fn again(ss: &mut Session, d: &std::path::Path) -> Value { c(ss, "export_job_start", args(d))["status"].clone() }

#[test] fn a_crash_mid_chunk_leaves_untrusted_leftovers_that_are_quarantined_and_rerendered() {
    let d = tmp("crash"); let mut ss = first(&d); let keep = std::fs::read(chunk(&d, 0)).unwrap();
    // simulate SIGKILL while chunk 1 was in flight: no chunk 1/2, no index entries for them, a half-written partial on disk
    std::fs::remove_file(chunk(&d, 1)).unwrap(); std::fs::remove_file(chunk(&d, 2)).unwrap(); let mut i = idx(&d); i["chunks"].as_object_mut().unwrap().remove("1"); i["chunks"].as_object_mut().unwrap().remove("2"); put_idx(&d, &i);
    std::fs::write(ch(&d).join("chunk_00001.partial.mp4"), b"half a moov atom").unwrap(); std::fs::remove_file(d.join("out.mp4")).unwrap();
    let s = c(&mut ss, "resume_job", json!({"path": d.join("out.mp4").to_str().unwrap(), "wait": true, "chunk_s": 2})); let s = if s.get("status").is_some() { s["status"].clone() } else { s };
    assert_eq!(s["state"], "done", "{s}"); assert_eq!(rendered(&s), vec![(1, "no_previous".to_string()), (2, "no_previous".to_string())]);
    assert_eq!(s["result"]["reverified"]["ok"], 1); assert_eq!(s["result"]["reverified"]["re_rendered"], 0, "chunks that were never indexed are not 're-rendered', they are new work");
    assert_eq!(s["result"]["quarantined"][0]["reason"], "crash_or_unverified"); assert!(ch(&d).join("chunk_00001.mp4.partial").is_file() && ch(&d).join("chunk_00001.mp4.partial.marker").is_file());
    let marker: Value = serde_json::from_slice(&std::fs::read(ch(&d).join("chunk_00001.mp4.partial.marker")).unwrap()).unwrap(); assert_eq!(marker["reason"], "crash_or_unverified");
    assert_eq!(keep, std::fs::read(chunk(&d, 0)).unwrap(), "the verified chunk was reused untouched"); assert_eq!(frames(&d.join("out.mp4")), 150); std::fs::remove_dir_all(&d).ok();
}
#[test] fn a_chunk_file_without_an_index_entry_is_untrusted() {
    let d = tmp("noidx"); let mut ss = first(&d); let mut i = idx(&d); i["chunks"].as_object_mut().unwrap().remove("2"); put_idx(&d, &i);
    let s = again(&mut ss, &d); assert_eq!(rendered(&s), vec![(2, "no_previous".to_string())], "{s}"); assert_eq!(s["result"]["quarantined"][0]["file"], "chunk_00002.mp4"); assert!(ch(&d).join("chunk_00002.mp4.partial.marker").is_file()); std::fs::remove_dir_all(&d).ok();
}
#[test] fn truncated_flipped_and_miscounted_chunks_are_each_caught_with_their_own_reason() {
    let d = tmp("verify"); let mut ss = first(&d);
    let f0 = chunk(&d, 0); let b = std::fs::read(&f0).unwrap(); std::fs::write(&f0, &b[..b.len() - 40]).unwrap(); // truncated
    let f1 = chunk(&d, 1); let mut b1 = std::fs::read(&f1).unwrap(); let n = b1.len() - 3; b1[n] ^= 0xff; std::fs::write(&f1, b1).unwrap(); // same size, flipped byte
    let mut i = idx(&d); i["chunks"]["2"]["frames"] = json!(51); put_idx(&d, &i); // size and sha256 still match; the frame count does not
    let s = again(&mut ss, &d); let mut r = rendered(&s); r.sort();
    assert_eq!(r, vec![(0, "size_mismatch".to_string()), (1, "file_hash_mismatch".to_string()), (2, "frame_count_mismatch".to_string())], "{s}");
    assert_eq!(s["result"]["reverified"], json!({"ok": 0, "re_rendered": 3, "reasons": {"size_mismatch": 1, "file_hash_mismatch": 1, "frame_count_mismatch": 1}}), "{s}");
    assert_eq!(frames(&d.join("out.mp4")), 150); let s = again(&mut ss, &d); assert_eq!(s["result"]["reverified"], json!({"ok": 3, "re_rendered": 0, "reasons": {}}), "after the repair everything verifies"); std::fs::remove_dir_all(&d).ok();
}
#[test] fn a_chunk_recorded_under_a_different_key_is_not_reused() {
    let d = tmp("key"); let mut ss = first(&d); let mut i = idx(&d); i["chunks"]["1"]["key"] = json!("0000000000000000"); put_idx(&d, &i);
    let s = again(&mut ss, &d); assert_eq!(rendered(&s), vec![(1, "key_changed".to_string())], "{s}"); std::fs::remove_dir_all(&d).ok();
}
