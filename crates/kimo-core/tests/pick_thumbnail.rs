//! pick_thumbnail: picks the best cover-image frames from a clip or the whole timeline, written as PNGs. Fixtures: a
//! blurred checker segment, a sharp well-exposed moving-checker segment and a black segment, concatenated on one track.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

const W: u32 = 160; const H: u32 = 90; const FPS: f64 = 24.0;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).err().unwrap_or_else(|| panic!("{n}: expected an error, got {:?}", dispatch(s, n, &a))) }
fn new() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); s }
fn track(s: &mut Session) -> Value { c(s, "add_track", json!({}))["track_id"].clone() }
fn pt(s: &mut Session, a: Value) -> Value { c(s, "pick_thumbnail", a) }

/// A file written by ffmpeg (ffv1/.mkv, yuv444p, WxH, 24 fps) from a full lavfi filter chain, imported as one media asset.
fn media(tag: &str, chain: &str, dur: f64) -> (PathBuf, f64) {
    let p = std::env::temp_dir().join(format!("kimo-pick-thumbnail-{tag}-{}.mkv", std::process::id()));
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", chain, "-c:v", "ffv1"]).arg(&p).status().unwrap();
    assert!(ok.success(), "ffmpeg failed for {tag}");
    (p, dur)
}
fn blurred_chain(dur: f64) -> String { format!("color=c=gray:s={W}x{H}:r={FPS}:d={dur},format=gray,geq=lum='if(mod(floor(X/8)+floor(Y/8),2),220,40)',format=yuv444p,boxblur=8:2") }
fn sharp_chain(dur: f64) -> String { format!("color=c=gray:s={W}x{H}:r={FPS}:d={dur},format=gray,geq=lum='if(mod(floor((X+30*T)/8)+floor(Y/8),2),220,40)',format=yuv444p") }
fn black_chain(dur: f64) -> String { format!("color=c=black:s={W}x{H}:r={FPS}:d={dur}") }

fn add_media_clip(s: &mut Session, t: &Value, start: f64, dur: f64, chain: &str, tag: &str) {
    let (p, dur) = media(tag, chain, dur);
    let a = c(s, "import_media", json!({"path": p}))["asset_id"].clone();
    c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": start, "duration": dur}));
}

/// Three clips back to back on one track: [0,2) blurred, [2,5) sharp (moving, well-exposed), [5,7) black.
fn three_segment_project() -> (Session, f64, f64, f64, f64) {
    let (blur_s, blur_d) = (0.0, 2.0); let (sharp_s, sharp_d) = (2.0, 3.0); let (black_s, black_d) = (5.0, 2.0);
    let mut s = new(); let t = track(&mut s);
    add_media_clip(&mut s, &t, blur_s, blur_d, &blurred_chain(blur_d), "blur");
    add_media_clip(&mut s, &t, sharp_s, sharp_d, &sharp_chain(sharp_d), "sharp");
    add_media_clip(&mut s, &t, black_s, black_d, &black_chain(black_d), "black");
    (s, sharp_s, sharp_s + sharp_d, black_s, black_s + black_d)
}

/// One long sharp, well-exposed, moving-checker clip: never frozen (keeps varying), never black, never in transition --
/// so min_gap is the only thing limiting how many distinct candidates can be picked close together.
fn long_sharp_project(dur: f64) -> Session {
    let mut s = new(); let t = track(&mut s);
    add_media_clip(&mut s, &t, 0.0, dur, &sharp_chain(dur), "long");
    s
}

#[test] fn top_pick_lands_in_the_sharp_segment_never_in_the_black_one() {
    let (mut s, sharp_s, sharp_e, black_s, black_e) = three_segment_project();
    let r = pt(&mut s, json!({"count": 1, "sample_fps": 4.0, "out_dir": std::env::temp_dir().to_string_lossy()}));
    let picks = r["picks"].as_array().unwrap(); assert_eq!(picks.len(), 1, "{r}");
    let t = picks[0]["time"].as_f64().unwrap();
    assert!(t >= sharp_s - 1e-6 && t < sharp_e + 1e-6, "top pick at {t} should be inside the sharp segment [{sharp_s}, {sharp_e})");
    assert!(!(t >= black_s && t < black_e), "top pick at {t} must never be inside the black segment [{black_s}, {black_e})");
    assert!(r["candidates_excluded"]["black"].as_u64().unwrap() > 0, "the black segment should have produced excluded black candidates: {r}");
    // every candidate ever considered must also miss the black span entirely, not just the one returned
    for p in picks { let pt = p["time"].as_f64().unwrap(); assert!(!(pt >= black_s && pt < black_e)); }
}

#[test] fn min_gap_is_respected() {
    let mut s = long_sharp_project(6.0);
    let r = pt(&mut s, json!({"count": 3, "min_gap_s": 1.5, "sample_fps": 4.0, "out_dir": std::env::temp_dir().to_string_lossy()}));
    let picks = r["picks"].as_array().unwrap();
    assert!(picks.len() >= 2, "expected at least 2 picks on a 6s clip: {r}");
    let times: Vec<f64> = picks.iter().map(|p| p["time"].as_f64().unwrap()).collect();
    for i in 0..times.len() { for j in (i + 1)..times.len() { assert!((times[i] - times[j]).abs() >= 1.5 - 1e-6, "picks {} and {} are {}s apart, under min_gap_s 1.5: {times:?}", i, j, (times[i] - times[j]).abs()); } }
}

#[test] fn result_is_deterministic_across_two_calls() {
    let dir = std::env::temp_dir().join(format!("kimo-pick-thumbnail-det-{}", std::process::id()));
    let (mut s1, _, _, _, _) = three_segment_project();
    let r1 = pt(&mut s1, json!({"count": 2, "sample_fps": 4.0, "out_dir": dir.to_string_lossy()}));
    let (mut s2, _, _, _, _) = three_segment_project();
    let r2 = pt(&mut s2, json!({"count": 2, "sample_fps": 4.0, "out_dir": dir.to_string_lossy()}));
    assert_eq!(r1, r2, "same project and args must give byte-identical output (including png paths)");
}

#[test] fn bad_args_error() {
    let (mut s, ..) = three_segment_project();
    let out = std::env::temp_dir().to_string_lossy().to_string();
    assert!(e(&mut s, "pick_thumbnail", json!({"clip_id": "no-such-clip"})).contains("missing clip"));
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"count": 0, "out_dir": out})).is_err(), "count 0 must error");
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"count": 13, "out_dir": out})).is_err(), "count 13 must error");
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"min_gap_s": -1.0, "out_dir": out})).is_err(), "negative min_gap_s must error");
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"sample_fps": 0.0, "out_dir": out})).is_err(), "sample_fps 0 must error");
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"sample_fps": -1.0, "out_dir": out})).is_err(), "negative sample_fps must error");
    assert!(dispatch(&mut s, "pick_thumbnail", &json!({"out_dir": 5})).is_err(), "out_dir as a number must error");
}
