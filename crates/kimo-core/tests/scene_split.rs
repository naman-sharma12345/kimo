//! scene_split (Premiere "Scene Edit Detection"): one clip that holds several shots is split at the detected content cuts into
//! separate timeline clips, one per shot. Read off a generated red/green/blue/white video (64x48 @ 24 fps, 1 s per shot).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn shots_file(tag: &str, cols: &[&str]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("kimo-scene-split-{tag}-{}.mp4", std::process::id()));
    let mut args: Vec<String> = vec!["-v".into(), "error".into(), "-y".into()];
    for col in cols { args.extend(["-f", "lavfi", "-i"].iter().map(|x| x.to_string())); args.push(format!("color=c={col}:size=64x48:rate=24:duration=1")); }
    let ins: String = (0..cols.len()).map(|i| format!("[{i}:v]")).collect();
    args.extend(["-filter_complex".to_string(), format!("{ins}concat=n={}:v=1:a=0[v]", cols.len()), "-map".into(), "[v]".into(), "-c:v".into(), "mpeg4".into(), "-q:v".into(), "2".into(), path.to_str().unwrap().into()]);
    assert!(std::process::Command::new("ffmpeg").args(&args).status().unwrap().success());
    path
}
fn setup(tag: &str, cols: &[&str]) -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 24}));
    let asset = c(&mut s, "import_media", json!({"path": shots_file(tag, cols)}))["asset_id"].clone();
    let tr = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": asset, "start": 2.0, "duration": cols.len() as f64, "fit": "stretch"}))["clip_id"].as_str().unwrap().to_string();
    (s, tr, clip)
}
fn track_clips(s: &Session, tr: &str) -> Vec<(f64, f64, f64)> { let mut v: Vec<(f64, f64, f64)> = s.project.tracks.iter().find(|t| t.id == tr).unwrap().clips.iter().map(|c| (c.start, c.duration, c.in_point)).collect(); v.sort_by(|a, b| a.0.total_cmp(&b.0)); v }
const F: f64 = 1.0 / 24.0 + 1e-6;

#[test]
fn a_one_clip_of_four_shots_becomes_four_clips() {
    let (mut s, tr, clip) = setup("a", &["red", "green", "blue", "white"]);
    let r = dispatch(&mut s, "scene_split", &json!({"clip_id": clip}));
    assert!(r.is_ok(), "scene_split must exist: {}", r.unwrap_err()); let r = r.unwrap();
    assert_eq!(r["clips"].as_array().unwrap().len(), 4, "four shots -> four clips: {r}");
    let got = track_clips(&s, &tr); assert_eq!(got.len(), 4, "{got:?}");
    for (i, (st, d, ip)) in got.iter().enumerate() {
        assert!((st - (2.0 + i as f64)).abs() <= F, "shot {i} starts on its cut: {got:?}");
        assert!((d - 1.0).abs() <= 2.0 * F, "shot {i} lasts its shot: {got:?}");
        assert!((ip - i as f64).abs() <= F, "shot {i} plays its own part of the source: {got:?}");
    }
    // nothing lost: the shots still tile the original span with no gap or overlap
    for w in got.windows(2) { assert!((w[0].0 + w[0].1 - w[1].0).abs() < 1e-9, "no gap/overlap: {got:?}"); }
    assert!((got[3].0 + got[3].1 - 6.0).abs() < 1e-9);
    assert_eq!(r["clips"][0]["clip_id"], json!(clip), "the first shot keeps the original clip id");
}

#[test]
fn b_min_shot_drops_short_shots_and_a_dry_run_changes_nothing() {
    let (mut s, tr, clip) = setup("b", &["red", "green", "blue"]);
    let r = c(&mut s, "scene_split", json!({"clip_id": clip, "dry_run": true}));
    assert_eq!(r["cuts"].as_array().unwrap().len(), 2, "dry run reports the two cuts: {r}");
    assert_eq!(track_clips(&s, &tr).len(), 1, "dry run leaves the clip whole");
    let r = c(&mut s, "scene_split", json!({"clip_id": clip, "min_shot_s": 1.5}));
    assert_eq!(track_clips(&s, &tr).len(), 1, "every shot is shorter than min_shot_s, so nothing is split: {r}");
}

#[test]
fn c_a_single_shot_is_left_alone_and_bad_calls_are_refused() {
    let (mut s, tr, clip) = setup("c", &["red", "red"]);
    let r = c(&mut s, "scene_split", json!({"clip_id": clip}));
    assert_eq!(r["clips"].as_array().unwrap().len(), 1, "no cut detected -> one clip: {r}");
    assert_eq!(track_clips(&s, &tr).len(), 1);
    assert!(dispatch(&mut s, "scene_split", &json!({"clip_id": "nope"})).is_err());
    assert!(dispatch(&mut s, "scene_split", &json!({"clip_id": clip, "sensitivity": 2.0})).is_err());
}
