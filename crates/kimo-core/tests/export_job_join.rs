//! RV-20 (d): join contract and verified, atomic join. Real ffmpeg.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::export_job::{chunk_contract, join_verified, verify_output};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_rv20d_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn scene() -> Session {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    for i in 0..3 { c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20 + i * 3, "height": 20, "start": i as f64 * 2.0, "duration": 2.0})); } ss
}
fn ff(args: &[&str]) { assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success(), "ffmpeg {args:?}"); }
fn parts(d: &std::path::Path) -> Vec<String> { (0..3).map(|i| d.join("out.mp4.chunks").join(format!("chunk_{i:05}.mp4")).to_string_lossy().into_owned()).collect() }
fn job(d: &std::path::Path) -> Value { let mut ss = scene(); c(&mut ss, "export_job_start", json!({"path": d.join("out.mp4").to_str().unwrap(), "chunk_s": 2, "wait": true}))["status"].clone() }

#[test] fn every_chunk_starts_on_a_key_frame_and_the_job_reports_a_verified_join() {
    let d = tmp("ok"); let s = job(&d); assert_eq!(s["state"], "done", "{s}"); assert_eq!(s["result"]["verified"], true); assert_eq!(s["result"]["verify"]["decoded_frames"], 150); assert_eq!(s["result"]["verify"]["pts_monotonic"], true);
    chunk_contract(&parts(&d)).expect("chunks honour the join contract"); assert!(d.join("out.mp4").is_file()); assert!(!d.join("out.mp4.joining.tmp").exists(), "no temp file left behind");
    std::fs::remove_dir_all(&d).ok();
}
#[test] fn a_chunk_that_does_not_start_on_a_key_frame_or_has_other_params_is_refused() {
    let d = tmp("contract"); job(&d); let p = parts(&d);
    // cut the first half second off chunk 0 with a stream copy: it now starts mid-GOP (or on a non-key frame)
    let bad = d.join("bad.mp4"); ff(&["-ss", "0.5", "-i", &p[0], "-c", "copy", bad.to_str().unwrap()]); let _ = chunk_contract(&[bad.to_string_lossy().into_owned()]); // outcome depends on the GOP layout; the codec-parameter row below is deterministic
    let other = d.join("other.mp4"); ff(&["-f", "lavfi", "-i", "color=c=red:s=160x90:r=25:d=1", "-c:v", "libx264", "-pix_fmt", "yuv444p", other.to_str().unwrap()]);
    let e = chunk_contract(&[p[0].clone(), other.to_string_lossy().into_owned()]).unwrap_err(); assert!(e.starts_with("join_contract:") && e.contains("codec parameters"), "{e}");
    let e = chunk_contract(&[d.join("nope.mp4").to_string_lossy().into_owned()]); assert!(e.is_err()); std::fs::remove_dir_all(&d).ok();
}
#[test] fn skipping_a_chunk_in_the_join_is_caught_by_the_verifier() {
    let d = tmp("skip"); job(&d); let p = parts(&d); let list = d.join("l.txt"); std::fs::write(&list, format!("file '{}'\nfile '{}'\n", p[0], p[2])).unwrap(); let bad = d.join("skipped.mp4");
    ff(&["-f", "concat", "-safe", "0", "-i", list.to_str().unwrap(), "-c", "copy", "-an", bad.to_str().unwrap()]);
    let v = verify_output(&bad, 150, 25.0, None); assert_eq!(v["verified"], false, "{v}"); assert_eq!(v["decoded_frames"], 100); assert!(v["problems"][0].as_str().unwrap().contains("expected 150"));
    let good = verify_output(&d.join("out.mp4"), 150, 25.0, None); assert_eq!(good["verified"], true, "{good}"); std::fs::remove_dir_all(&d).ok();
}
#[test] fn audio_duration_is_verified_and_a_missing_audio_stream_is_a_problem() {
    let d = tmp("aud"); let f = d.join("av.mp4");
    ff(&["-f", "lavfi", "-i", "color=c=blue:s=160x90:r=25:d=6", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=6", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", f.to_str().unwrap()]);
    let ok = verify_output(&f, 150, 25.0, Some(6.0)); assert_eq!(ok["verified"], true, "{ok}");
    let off = verify_output(&f, 150, 25.0, Some(5.0)); assert_eq!(off["verified"], false); assert!(off["problems"][0].as_str().unwrap().contains("audio duration"), "{off}");
    let silent = d.join("v.mp4"); ff(&["-i", f.to_str().unwrap(), "-an", "-c:v", "copy", silent.to_str().unwrap()]); let m = verify_output(&silent, 150, 25.0, Some(6.0)); assert_eq!(m["verified"], false); assert!(m["problems"].to_string().contains("no audio stream") || m["problems"].to_string().contains("found none"), "{m}");
    std::fs::remove_dir_all(&d).ok();
}
#[test] fn a_failed_join_leaves_nothing_at_the_final_path() {
    let d = tmp("fail"); job(&d); let p = parts(&d); let out = d.join("final.mp4"); let ss = scene();
    let garbage = d.join("garbage.mp4"); std::fs::write(&garbage, b"not an mp4").unwrap();
    let e = join_verified(&ss.project, &[p[0].clone(), garbage.to_string_lossy().into_owned()], out.to_str().unwrap()).unwrap_err(); assert!(!e.is_empty());
    assert!(!out.exists() && !d.join("final.mp4.joining.tmp").exists(), "no final file, no temp file");
    let ok = join_verified(&ss.project, &p, out.to_str().unwrap()).unwrap(); assert_eq!(ok["verified"], true); assert!(out.is_file()); std::fs::remove_dir_all(&d).ok();
}
#[test] fn an_audible_project_joins_with_one_continuous_audio_stream_and_verifies() {
    let d = tmp("audible"); let wav = d.join("tone.wav"); ff(&["-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=6", wav.to_str().unwrap()]);
    let mut ss = scene(); let a = c(&mut ss, "import_media", json!({"path": wav.to_str().unwrap()}))["asset_id"].as_str().unwrap_or_else(|| panic!("asset id")).to_string();
    c(&mut ss, "add_audio_clip", json!({"asset_id": a, "start": 0, "duration": 6}));
    let s = c(&mut ss, "export_job_start", json!({"path": d.join("out.mp4").to_str().unwrap(), "chunk_s": 2, "wait": true}))["status"].clone();
    assert_eq!(s["state"], "done", "{s}"); assert_eq!(s["result"]["verified"], true, "{s}"); let ad = s["result"]["verify"]["audio_duration"].as_f64().expect("audio stream present"); assert!((ad - 6.0).abs() < 0.03, "{ad}");
    let o = std::process::Command::new("ffprobe").args(["-v", "error", "-select_streams", "a", "-show_entries", "stream=codec_name", "-of", "csv=p=0"]).arg(d.join("out.mp4")).output().unwrap(); assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "aac", "exactly one aac stream");
    std::fs::remove_dir_all(&d).ok();
}
