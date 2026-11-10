//! Edits that move a clip's head along its content without changing what plays (split_clip, set_time_mode hold_from_here, roll_edit,
//! slide) must not change any frame: clip-local keyframes, expressions, effect-param keys and speed ramps on the moved clip continue
//! where they were instead of replaying from the clip's new start (Premiere and AE keep keyframes on their frames through a razor cut).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn frame(s: &Session, t: f64) -> Vec<f32> { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap().d }
fn maxdiff(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max) }
fn same(before: &[(f64, Vec<f32>)], s: &Session, what: &str) { for (t, f) in before { let d = maxdiff(f, &frame(s, *t)); assert!(d < 1e-4, "{what}: the frame at {t} s changed (max pixel difference {d}); the animation replayed from the new clip start"); } }

/// A 4 s solid on a 160x90 project with keyed opacity, keyed x with a bezier ease, a wiggle on y, a sin on rotation, a spring on
/// scale, and a keyed blur radius.
fn animated(s: &mut Session, t: &Value, start: f64, color: [f64; 3]) -> String {
    let id = c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": color, "start": start, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    c(s, "set_keyframe", json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 0.2})); c(s, "set_keyframe", json!({"clip_id": id, "property": "opacity", "time": 4.0, "value": 1.0}));
    c(s, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 0.5, "value": -60.0, "ease": "in_out"})); c(s, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 3.5, "value": 60.0}));
    c(s, "set_keyframe", json!({"clip_id": id, "property": "scale", "time": 0.0, "value": 0.5}));
    c(s, "set_expression", json!({"clip_id": id, "property": "y", "op": "wiggle", "freq": 1.5, "amp": 12.0}));
    c(s, "set_expression", json!({"clip_id": id, "property": "rotation", "op": "sin", "freq": 0.7, "amp": 20.0}));
    c(s, "set_expression", json!({"clip_id": id, "property": "scale", "op": "spring", "stiffness": 60.0, "damping": 4.0, "mass": 1.0, "from": 0.3, "to": 0.6, "t0": 1.0}));
    let e = c(s, "add_effect", json!({"clip_id": id, "kind": "blur", "params": {"radius": 0.0}}))["effect_id"].as_str().unwrap().to_string();
    c(s, "set_keyframe", json!({"clip_id": id, "effect_id": e, "property": "radius", "time": 0.0, "value": 0.0})); c(s, "set_keyframe", json!({"clip_id": id, "effect_id": e, "property": "radius", "time": 4.0, "value": 8.0}));
    id
}
fn session() -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 10})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); (s, t) }

#[test]
fn split_keeps_every_animation_on_the_right_half() {
    let (mut s, t) = session(); let id = animated(&mut s, &t, 0.0, [1.0, 0.4, 0.1]);
    let before: Vec<(f64, Vec<f32>)> = [0.5, 1.4, 2.0, 2.6, 3.3, 3.9].iter().map(|t| (*t, frame(&s, *t))).collect();
    c(&mut s, "split_clip", json!({"clip_id": id, "at": 1.7}));
    same(&before, &s, "split_clip at 1.7 s");
    let right = s.project.tracks[0].clips.iter().find(|x| x.id != id).unwrap().id.clone();
    c(&mut s, "split_clip", json!({"clip_id": right, "at": 3.0})); same(&before, &s, "a second split at 3.0 s");
}

#[test]
fn hold_from_here_keeps_the_animation_running_over_the_held_frame() {
    let (mut s, t) = session(); let id = animated(&mut s, &t, 0.0, [0.2, 0.6, 1.0]);
    let before: Vec<(f64, Vec<f32>)> = [2.5, 3.5].iter().map(|t| (*t, frame(&s, *t))).collect();
    c(&mut s, "set_time_mode", json!({"clip_id": id, "mode": "freeze", "at": 2.0, "hold_from_here": true}));
    same(&before, &s, "hold_from_here at 2.0 s (a solid looks the same frozen)");
}

#[test]
fn roll_and_slide_keep_the_neighbours_animation() {
    let (mut s, t) = session(); let a = animated(&mut s, &t, 0.0, [1.0, 0.0, 0.0]); let b = animated(&mut s, &t, 4.0, [0.0, 0.0, 1.0]);
    let before: Vec<(f64, Vec<f32>)> = [4.8, 5.5, 7.0].iter().map(|t| (*t, frame(&s, *t))).collect();
    c(&mut s, "roll_edit", json!({"left_clip_id": a, "right_clip_id": b, "delta": 0.6}));
    same(&before, &s, "roll_edit +0.6 s");
}

#[test]
fn split_of_a_speed_ramp_continues_the_ramp() {
    use std::io::Write;
    // 8 s source whose grey level is its frame number (0..80 of 255), lossless.
    let path = std::env::temp_dir().join(format!("kimo-splitkeys-{}.mkv", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "gray", "-s", "32x18", "-r", "10", "-i", "-", "-c:v", "ffv1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    { let mut si = ch.stdin.take().unwrap(); for i in 0..80u8 { si.write_all(&vec![i * 3; 32 * 18]).unwrap(); } } assert!(ch.wait().unwrap().success());
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 32, "height": 18, "fps": 10}));
    let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 3.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_speed_ramp", json!({"clip_id": id, "keys": [[0, 1.0], [2, 3.0]], "ease": "linear"}));
    let before: Vec<(f64, Vec<f32>)> = [1.5, 2.2, 2.9].iter().map(|t| (*t, frame(&s, *t))).collect();
    // split at 1.0 s: the ramp has played 1.5 s of source there (a whole frame count, so the decode seek is frame-exact)
    c(&mut s, "split_clip", json!({"clip_id": id, "at": 1.0}));
    same(&before, &s, "split of a speed-ramped clip");
}
