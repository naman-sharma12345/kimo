use kimo_core::{
    commands::{dispatch, Session},
    render,
};
use serde_json::{json, Value};
use std::process::Command;

fn verify_webm(path: &str, expected_duration: f64, expected_size: (u32, u32)) {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=duration:format=duration",
            "-of",
            "json",
            path,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let metadata: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        metadata["streams"][0]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .is_none(),
        "fixture must lack stream duration"
    );
    let mut s = Session::new();
    dispatch(
        &mut s,
        "project_set",
        &json!({"width":64,"height":48,"fps":25}),
    )
    .unwrap();
    let a = dispatch(&mut s, "import_media", &json!({"path":path})).unwrap();
    assert_eq!(a["width"].as_u64().unwrap(), expected_size.0 as u64);
    assert_eq!(a["height"].as_u64().unwrap(), expected_size.1 as u64);
    assert!((a["duration"].as_f64().unwrap() - expected_duration).abs() < 0.001);
    assert_eq!(a["fps"], 25.);
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap();
    let c=dispatch(&mut s,"add_clip",&json!({"track_id":tr["track_id"],"source_type":"media","asset_id":a["asset_id"],"start":0,"duration":a["duration"]})).unwrap();
    let end = expected_duration - 0.08;
    render::prepare_at(&s.project, &mut s.cache, Some(end)).unwrap();
    let frames = s.cache.frames_of(c["clip_id"].as_str().unwrap()).unwrap();
    assert!(frames.len() > 0);
    assert!(frames.first > 0, "must decode late window");
    assert_eq!(
        render::render_frame(&s.project, end, &s.cache)
            .unwrap()
            .d
            .len(),
        64 * 48 * 4
    );
}
#[test]
fn generated_webm_uses_container_duration() {
    let path = std::env::temp_dir().join(format!("kimo-webm-probe-{}.webm", std::process::id()));
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=25:duration=1.2",
            "-c:v",
            "libvpx",
            path.to_str().unwrap()
        ])
        .status()
        .unwrap()
        .success());
    verify_webm(path.to_str().unwrap(), 1.2, (64, 48));
    std::fs::remove_file(path).unwrap();
}
#[test]
fn still_and_audio_remain_compatible() {
    let dir = std::env::temp_dir();
    let png = dir.join(format!("kimo-probe-{}.png", std::process::id()));
    let wav = dir.join(format!("kimo-probe-{}.wav", std::process::id()));
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:size=16x16",
            "-frames:v",
            "1",
            png.to_str().unwrap()
        ])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=1.25",
            wav.to_str().unwrap()
        ])
        .status()
        .unwrap()
        .success());
    let image = render::probe(png.to_str().unwrap()).unwrap();
    assert_eq!((image.0, image.1, image.2), (16, 16, 0.));
    assert_eq!(
        render::probe(wav.to_str().unwrap()).unwrap(),
        (0, 0, 1.25, 0.)
    );
    std::fs::remove_file(png).unwrap();
    std::fs::remove_file(wav).unwrap();
}
#[test]
fn failed_probe_cannot_import() {
    let mut s = Session::new();
    let before = serde_json::to_value(&s.project).unwrap();
    assert!(dispatch(
        &mut s,
        "import_media",
        &json!({"path":"/missing/kimo-probe-not-a-file.webm"})
    )
    .is_err());
    assert_eq!(before, serde_json::to_value(&s.project).unwrap());
}
/// Download original fixture from OpenCV, then run with KIMO_FACEOCC2_WEBM.
#[test]
#[ignore = "external real-footage fixture; set KIMO_FACEOCC2_WEBM and run --ignored"]
fn faceocc2_original_webm_import_and_late_decode() {
    let path = std::env::var("KIMO_FACEOCC2_WEBM")
        .expect("set KIMO_FACEOCC2_WEBM to original faceocc2.webm");
    verify_webm(&path, 32.48, (320, 240));
}
