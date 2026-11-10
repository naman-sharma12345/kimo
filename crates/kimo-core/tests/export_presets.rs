//! export_preset: the 7 non-native-format presets added after web-1080/master/alpha-prores --
//! youtube, tiktok, prores422, dnxhr, webm-vp9, hevc, av1. Each is checked against the real encoded
//! file with ffprobe (container/codec/pix_fmt/resolution), not just that the call returns Ok, because
//! an export preset's whole job is the exact bytes a delivery target expects.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::process::Command;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }

/// A tiny landscape project with one short solid-colour clip -- fast to encode, enough to probe.
fn project(w: u32, h: u32) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": w, "height": h, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.6, 0.3, 0.1], "start": 0.0, "duration": 0.2}));
    s
}

/// Same, plus a real (non-silent) audio clip on the timeline -- for presets whose description promises
/// a muxed audio track, "exports without error" alone would not catch a silently-dropped stream.
fn project_with_audio(w: u32, h: u32) -> Session {
    let mut s = project(w, h);
    let n = (0.2 * kimo_core::audio::SR as f64) as usize;
    let pcm: Vec<f32> = (0..n).flat_map(|i| { let v = 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / kimo_core::audio::SR as f32).sin(); [v, v] }).collect();
    let wav = std::env::temp_dir().join(format!("kimo-export-preset-tone-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(wav.to_str().unwrap(), &pcm).unwrap();
    let a = c(&mut s, "import_media", json!({"path": wav.to_str().unwrap()}))["asset_id"].clone();
    c(&mut s, "add_audio_clip", json!({"asset_id": a, "start": 0.0, "in_point": 0.0, "duration": 0.2}));
    s
}

fn tmp(ext: &str) -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("kimo-export-preset-{}-{n}.{ext}", std::process::id())).to_str().unwrap().to_string()
}

fn probe(path: &str) -> Value {
    let out = Command::new("ffprobe").args(["-v", "error", "-show_streams", "-show_format", "-of", "json", path]).output().unwrap();
    assert!(out.status.success(), "ffprobe failed on {path}: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}
fn video_stream(v: &Value) -> &Value { v["streams"].as_array().unwrap().iter().find(|s| s["codec_type"] == "video").unwrap() }
fn audio_stream(v: &Value) -> Option<&Value> { v["streams"].as_array().unwrap().iter().find(|s| s["codec_type"] == "audio") }

/// `+faststart` moves the `moov` atom before `mdat` so playback can start before the whole file downloads --
/// checked directly against the file bytes rather than trusting the ffmpeg flag was accepted.
fn moov_before_mdat(path: &str) -> bool {
    let bytes = std::fs::read(path).unwrap();
    let find = |needle: &[u8]| bytes.windows(4).position(|w| w == needle);
    match (find(b"moov"), find(b"mdat")) { (Some(m), Some(d)) => m < d, _ => false }
}

#[test] fn youtube_native_resolution_high_profile_faststart() {
    let mut s = project(640, 360);
    let path = tmp("mp4");
    let r = c(&mut s, "export_preset", json!({"path": path, "preset": "youtube"}));
    assert_eq!(r["output_size"], json!([640, 360]), "youtube never resizes, unlike web-1080");
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "h264");
    assert_eq!(vs["profile"], "High");
    assert_eq!(vs["width"], 640);
    assert_eq!(vs["height"], 360);
    assert!(moov_before_mdat(&path), "youtube is +faststart tagged");
}

#[test] fn youtube_rejects_wrong_extension() {
    let mut s = project(320, 180);
    let err = dispatch(&mut s, "export_preset", &json!({"path": tmp("mov"), "preset": "youtube"})).unwrap_err();
    assert!(err.contains(".mp4"), "got: {err}");
}

#[test] fn tiktok_pads_landscape_source_to_vertical_canvas() {
    let mut s = project(640, 360); // landscape source -> tiktok's own 1080x1920 canvas, letterboxed
    let path = tmp("mp4");
    let r = c(&mut s, "export_preset", json!({"path": path, "preset": "tiktok"}));
    assert_eq!(r["output_size"], json!([1080, 1920]));
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "h264");
    assert_eq!(vs["width"], 1080);
    assert_eq!(vs["height"], 1920);
    assert!(moov_before_mdat(&path), "tiktok is +faststart tagged");
}

#[test] fn prores422_codec_pixfmt_and_muxed_audio() {
    let mut s = project_with_audio(320, 180);
    let path = tmp("mov");
    c(&mut s, "export_preset", json!({"path": path, "preset": "prores422"}));
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "prores");
    assert_eq!(vs["pix_fmt"], "yuv422p10le");
    let aud = audio_stream(&v).unwrap_or_else(|| panic!("prores422 promises audio (unlike alpha-prores); none found in {v}"));
    assert_eq!(aud["codec_name"], "aac");
}

#[test] fn dnxhr_rejects_canvas_under_256x120() {
    let mut s = project(200, 100);
    let err = dispatch(&mut s, "export_preset", &json!({"path": tmp("mov"), "preset": "dnxhr"})).unwrap_err();
    assert!(err.contains("256x120"), "got: {err}");
}

#[test] fn dnxhr_codec_and_pixfmt_at_a_valid_canvas() {
    let mut s = project(320, 180);
    let path = tmp("mov");
    c(&mut s, "export_preset", json!({"path": path, "preset": "dnxhr"}));
    let probed = probe(&path);
    let vs = video_stream(&probed);
    assert_eq!(vs["codec_name"], "dnxhd");
    assert_eq!(vs["pix_fmt"], "yuv422p");
}

#[test] fn webm_vp9_is_video_only_for_now() {
    let mut s = project_with_audio(320, 180); // real audio on the timeline, same as prores422's test --
    let path = tmp("webm");                   // proves webm-vp9 really drops it rather than this test being silent-pcm-skip
    c(&mut s, "export_preset", json!({"path": path, "preset": "webm-vp9"}));
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "vp9");
    assert_eq!(vs["pix_fmt"], "yuv420p");
    assert!(audio_stream(&v).is_none(), "webm-vp9 is documented video-only this pass; an audio stream here would be a stale doc");
}

#[test] fn hevc_tagged_hvc1_and_faststart() {
    let mut s = project(320, 180);
    let path = tmp("mp4");
    c(&mut s, "export_preset", json!({"path": path, "preset": "hevc"}));
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "hevc");
    assert_eq!(vs["codec_tag_string"], "hvc1", "tagged hvc1 for Apple/QuickTime compatibility, not the default hev1");
    assert!(moov_before_mdat(&path), "hevc is +faststart tagged");
}

#[test] fn av1_codec_and_faststart() {
    let mut s = project(320, 180);
    let path = tmp("mp4");
    c(&mut s, "export_preset", json!({"path": path, "preset": "av1"}));
    let v = probe(&path);
    let vs = video_stream(&v);
    assert_eq!(vs["codec_name"], "av1");
    assert!(moov_before_mdat(&path), "av1 is +faststart tagged");
}

#[test] fn inspect_lists_all_ten_presets() {
    let mut s = Session::new();
    let v = c(&mut s, "inspect_export_preset", json!({}));
    let names: Vec<&str> = v["presets"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    for n in ["web-1080", "master", "alpha-prores", "youtube", "tiktok", "prores422", "dnxhr", "webm-vp9", "hevc", "av1"] {
        assert!(names.contains(&n), "missing preset in inspect_export_preset list: {n}");
    }
}
