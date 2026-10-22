//! v6 eyes wave 2: frozen_frames. Runs of frames that stand still (start frame, end frame, length, the largest pixel
//! difference inside the run) on rendered timelines with known content: a clip with a 12-frame hold between two moves, a slow
//! pan (a gradient sliding half a pixel per frame) that must report nothing (it passes via its edge rows), and a still under
//! film grain that is caught only at a stated tolerance. A labelled KNOWN LIMITATION test pins an edge-free slow pan that reads
//! as a hold for frames 0-5 at 6.5/255. Edge cases: a hold at frame 0, a hold on the last frame, two holds back to back, a one-frame clip,
//! and black frames (a hold only when they repeat). Every frame the tool reads is a real render through the engine; nothing
//! is mocked. With KIMO_FROZEN_SHEET=<dir> the fixture frames are also written as PNGs for the contact sheet.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::frozen_frames::{max_diff, Run, Runs};
use serde_json::{json, Value};

const W: u32 = 160; const H: u32 = 90; const FPS: f64 = 24.0;
/// The tolerance for the grain still (fixture iii), derived before the result run (tests/data/frozen_frames/THRESHOLD.md):
/// film_finish grain 0.02 adds at most 0.01 per frame, so two frames differ by at most 0.02 = 5.1/255 before rounding and at most
/// 6/255 after 8-bit rounding; 6.5/255 sits between that bound and the first difference the grain cannot produce (7/255).
const GRAIN_TOL: f64 = 6.5 / 255.0;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn runs(v: &Value) -> Vec<(i64, i64, u64)> { v["runs"].as_array().unwrap().iter().map(|r| (r["start_frame"].as_i64().unwrap(), r["end_frame"].as_i64().unwrap(), r["length"].as_u64().unwrap())).collect() }
fn new() -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); (s, t) }
/// A solid moving 70 px right over its clip (clip-local keys), so every frame differs from every other one.
fn moving(s: &mut Session, t: &Value, start: f64, dur: f64, color: &str) {
    let id = c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": color, "start": start, "duration": dur}))["clip_id"].clone();
    c(s, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 0.0, "value": 0.0, "ease": "linear"}));
    c(s, "set_keyframe", json!({"clip_id": id, "property": "x", "time": dur, "value": 70.0, "ease": "linear"}));
}
fn still(s: &mut Session, t: &Value, start: f64, dur: f64, color: &str) { c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": color, "start": start, "duration": dur})); }
fn sheet(s: &mut Session, name: &str, frames: &[i64]) {
    if let Ok(d) = std::env::var("KIMO_FROZEN_SHEET") { for f in frames { let p = format!("{d}/{name}_f{f:02}.png"); c(s, "render_frame", json!({"time": *f as f64 / FPS, "path": p, "inline": "never"})); } }
}

/// (i) moving 0-1 s (frames 0-23), the same picture held 1-1.5 s (24-35: a still solid, 12 frames), moving again 1.5-2.5 s (36-59).
fn known() -> Session { let (mut s, t) = new(); moving(&mut s, &t, 0.0, 1.0, "#c08040"); still(&mut s, &t, 1.0, 0.5, "#4080c0"); moving(&mut s, &t, 1.5, 1.0, "#40c080"); s }
#[test] fn fixture_i_a_known_12_frame_hold_is_reported_exactly() {
    let mut s = known(); let r = c(&mut s, "frozen_frames", json!({}));
    eprintln!("fixture i: {}", r["runs"]);
    assert_eq!(r["frames"], json!(60)); assert_eq!(r["compared"], json!([W, H]));
    assert_eq!(runs(&r), vec![(24, 35, 12)]); assert_eq!(r["runs"][0]["max_diff"], json!(0.0)); assert_eq!(r["runs"][0]["max_diff_255"], json!(0));
    assert_eq!(r["runs"][0]["start_s"], json!(1.0)); assert_eq!(r["runs"][0]["end_s"], json!(1.5));
    assert_eq!(r["run_count"], json!(1)); assert_eq!(r["held_frames"], json!(11)); assert_eq!(r["longest_run"], json!(12));
    // min_run: 12 keeps it, 13 drops it; a window that cuts into the hold reports only the part inside
    assert_eq!(runs(&c(&mut s, "frozen_frames", json!({"min_run": 12}))), vec![(24, 35, 12)]);
    assert_eq!(runs(&c(&mut s, "frozen_frames", json!({"min_run": 13}))), vec![]);
    let w = c(&mut s, "frozen_frames", json!({"start": 1.25, "end": 2.0})); assert_eq!(w["first_frame"], json!(30)); assert_eq!(runs(&w), vec![(30, 35, 6)]);
    sheet(&mut s, "i_known", &[23, 24, 35, 36]);
}
/// (ii) a vertical gradient sliding down half a pixel per frame for 2 s (12 px per second). Close to the line: passes via edge
/// rows; edge-free pan at about 1.1 counts per frame reads as hold for frames 0-5 at 0.0255 (the known-limitation test below).
fn pan() -> Session { let (mut s, t) = new();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": [0.9, 0.35, 0.1], "color2": [0.1, 0.35, 0.9], "start": 0, "duration": 2.0}))["clip_id"].clone();
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "y", "time": 0.0, "value": 0.0, "ease": "linear"}));
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "y", "time": 2.0, "value": 24.0, "ease": "linear"})); s }
#[test] fn fixture_ii_a_slow_pan_reports_nothing() {
    let mut s = pan(); let r = c(&mut s, "frozen_frames", json!({}));
    eprintln!("fixture ii: frames {} runs {}", r["frames"], r["runs"]);
    assert_eq!(r["frames"], json!(48)); assert_eq!(r["runs"], json!([])); assert_eq!(r["run_count"], json!(0)); assert_eq!(r["held_frames"], json!(0)); assert_eq!(r["longest_run"], Value::Null);
    sheet(&mut s, "ii_pan", &[0, 1, 24, 47]);
}
/// KNOWN LIMITATION, pinned as current behaviour (not a goal; the threshold and the algorithm are not changed for it): an
/// edge-free slow pan. The same gradient and the same half-pixel-per-frame slide as (ii), laid over a solid of the gradient's own
/// top colour, so the strip the slide uncovers joins the gradient with no step and there is no moving edge (largest row-to-row
/// step in any frame: 3/255, the gradient's own slope). Interior drift is about 1.1 counts per frame. Frames are compared with
/// their run's first frame, so the 6.5/255 line applies to the total drift from that frame: frame 5 is 6/255 off frame 0 and
/// still reads as a hold, frame 6 is 7/255 off and breaks it, and each new run holds again until it crosses the line. The
/// 6-frame figure belongs to this 1.1-counts-per-frame fixture only; it is not a general bound.
fn edge_free_pan() -> Session { let (mut s, t) = new();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9, 0.35, 0.1], "start": 0, "duration": 2.0}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "gradient", "color": [0.9, 0.35, 0.1], "color2": [0.1, 0.35, 0.9], "start": 0, "duration": 2.0}))["clip_id"].clone();
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "y", "time": 0.0, "value": 0.0, "ease": "linear"}));
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "y", "time": 2.0, "value": 24.0, "ease": "linear"})); s }
#[test] fn known_limitation_an_edge_free_slow_pan_reads_as_a_hold_for_frames_0_to_5_at_6_5_over_255() {
    let mut s = edge_free_pan(); let r = c(&mut s, "frozen_frames", json!({"tolerance": GRAIN_TOL}));
    let f5 = c(&mut s, "frozen_frames", json!({"start": 0.0, "end": 6.0 / FPS, "tolerance": 1.0}));
    let f6 = c(&mut s, "frozen_frames", json!({"start": 0.0, "end": 7.0 / FPS, "tolerance": 1.0}));
    eprintln!("known limitation (edge-free pan): tolerance {GRAIN_TOL} runs {} | frames 0-5 vs 0 max {} | frames 0-6 vs 0 max {}", r["runs"], f5["runs"][0]["max_diff_255"], f6["runs"][0]["max_diff_255"]);
    let want: Vec<(i64, i64, u64)> = (0..8).map(|k| (6 * k, 6 * k + 5, 6)).collect();
    assert_eq!(runs(&r), want, "pinned: holds 0-5, breaks at 6, then every 6 frames");
    assert_eq!(r["runs"][0]["max_diff_255"], json!(6)); assert_eq!(r["run_count"], json!(8)); assert_eq!(r["held_frames"], json!(40)); assert_eq!(r["longest_run"], json!(6));
    assert_eq!(f5["runs"][0]["max_diff_255"], json!(6)); assert_eq!(f6["runs"][0]["max_diff_255"], json!(7));
    sheet(&mut s, "iv_edgefree_pan", &[0, 5, 6, 47]);
}
/// (iii) a still gradient under film_finish grain (grain 0.02, bloom / halation / vignette 0) for 6 s; the grain is seeded by
/// the timeline frame, so no two frames are identical, but the picture never changes. The fixture reads frames 120-143 (5-6 s).
fn grain_clip(s: &mut Session, t: &Value, start: f64, dur: f64) -> Value {
    let id = c(s, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": [0.9, 0.35, 0.1], "color2": [0.1, 0.35, 0.9], "start": start, "duration": dur}))["clip_id"].clone();
    c(s, "add_effect", json!({"clip_id": id, "kind": "film_finish", "params": {"grain": 0.02, "bloom": 0.0, "halation": 0.0, "vignette": 0.0}})); id }
fn grain() -> Session { let (mut s, t) = new(); grain_clip(&mut s, &t, 0.0, 6.0); s }
#[test] fn fixture_iii_a_grainy_still_is_caught_at_the_stated_tolerance_and_missed_at_0() {
    let mut s = grain(); let w = |t: Value| { let mut a = json!({"start": 5.0, "end": 6.0}); if !t.is_null() { a["tolerance"] = t; } a };
    let r0 = c(&mut s, "frozen_frames", w(Value::Null)); let rt = c(&mut s, "frozen_frames", w(json!(GRAIN_TOL)));
    eprintln!("fixture iii: tolerance 0 runs {} | tolerance {GRAIN_TOL} runs {}", r0["runs"], rt["runs"]);
    assert_eq!(r0["frames"], json!(24)); assert_eq!(r0["first_frame"], json!(120)); assert_eq!(r0["runs"], json!([]));
    assert_eq!(runs(&rt), vec![(120, 143, 24)]);
    let m = rt["runs"][0]["max_diff"].as_f64().unwrap(); assert!(m > 0.0 && m <= GRAIN_TOL, "{m}");
    // the pan is not swallowed at the grain tolerance: comparing with the run's first frame stops a drift from chaining
    let mut p = pan(); let pr = c(&mut p, "frozen_frames", json!({"tolerance": GRAIN_TOL})); eprintln!("pan at tolerance {GRAIN_TOL}: {}", pr["runs"]); assert_eq!(pr["runs"], json!([]));
    sheet(&mut s, "iii_grain", &[120, 121, 143]);
}

/// Negative control for (iii), stated in THRESHOLD.md before it ran: the same grainy still with ONE frame (timeline frame 132) shifted
/// down 8 px (a one-frame copy of the clip on a second track; the base shows through the 8 uncovered top rows). At the same frozen
/// tolerance the hold must break at frame 132: (120..131) and (133..143), the one-frame run at 132 being under min_run.
#[test] fn fixture_iii_negative_control_a_one_frame_8px_offset_breaks_the_hold_at_the_same_tolerance() {
    let mut s = grain(); let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = grain_clip(&mut s, &t2, 132.0 / FPS, 1.0 / FPS);
    for k in [0.0, 1.0 / FPS] { c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "y", "time": k, "value": 8.0, "ease": "linear"})); }
    let r = c(&mut s, "frozen_frames", json!({"start": 5.0, "end": 6.0, "tolerance": GRAIN_TOL}));
    let d = c(&mut s, "frozen_frames", json!({"start": 131.0 / FPS, "end": 133.0 / FPS, "tolerance": 1.0}));
    let e = c(&mut s, "frozen_frames", json!({"start": 132.0 / FPS, "end": 134.0 / FPS, "tolerance": 1.0}));
    eprintln!("fixture iii control: tolerance {GRAIN_TOL} runs {} | frame 132 vs 131 {} | frame 133 vs 132 {}", r["runs"], d["runs"], e["runs"]);
    assert_eq!(runs(&r), vec![(120, 131, 12), (133, 143, 11)]);
    assert!(d["runs"][0]["max_diff"].as_f64().unwrap() > GRAIN_TOL);
    sheet(&mut s, "iii_control", &[131, 132, 133]);
}

#[test] fn a_hold_at_frame_0() { let (mut s, t) = new(); still(&mut s, &t, 0.0, 0.5, "#c08040"); moving(&mut s, &t, 0.5, 0.5, "#4080c0");
    assert_eq!(runs(&c(&mut s, "frozen_frames", json!({}))), vec![(0, 11, 12)]); }
#[test] fn a_hold_on_the_last_frame() { let (mut s, t) = new(); moving(&mut s, &t, 0.0, 0.5, "#c08040"); still(&mut s, &t, 0.5, 0.5, "#4080c0");
    let r = c(&mut s, "frozen_frames", json!({})); assert_eq!(r["frames"], json!(24)); assert_eq!(runs(&r), vec![(12, 23, 12)]); }
#[test] fn two_holds_back_to_back() { let (mut s, t) = new(); still(&mut s, &t, 0.0, 0.5, "#c08040"); still(&mut s, &t, 0.5, 0.25, "#4080c0");
    let r = c(&mut s, "frozen_frames", json!({})); assert_eq!(runs(&r), vec![(0, 11, 12), (12, 17, 6)]); assert_eq!(r["held_frames"], json!(16)); }
#[test] fn a_one_frame_clip() { let (mut s, t) = new();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#c08040", "start": 0, "duration": 1.0 / FPS}))["clip_id"].as_str().unwrap().to_string();
    for a in [json!({}), json!({"source": format!("clip:{id}")})] { let r = c(&mut s, "frozen_frames", a); assert_eq!(r["frames"], json!(1)); assert_eq!(r["runs"], json!([])); assert_eq!(r["run_count"], json!(0)); }
}
#[test] fn black_frames_are_a_hold_only_when_identical() {
    // black 0-0.5 s, then a near-black 3/255 grey 0.5-1 s: two holds at tolerance 0 (no black special case joins them), one at 3/255
    let (mut s, t) = new(); still(&mut s, &t, 0.0, 0.5, "#000000"); still(&mut s, &t, 0.5, 0.5, "#030303");
    let r = c(&mut s, "frozen_frames", json!({})); assert_eq!(runs(&r), vec![(0, 11, 12), (12, 23, 12)]);
    let r = c(&mut s, "frozen_frames", json!({"tolerance": 3.0 / 255.0 + 1e-9})); assert_eq!(runs(&r), vec![(0, 23, 24)]); assert_eq!(r["runs"][0]["max_diff_255"], json!(3));
    // a black frame between two moves is no run at all
    let (mut s, t) = new(); moving(&mut s, &t, 0.0, 0.5, "#c08040"); still(&mut s, &t, 0.5, 1.0 / FPS, "#000000"); moving(&mut s, &t, 0.5 + 1.0 / FPS, 0.5, "#4080c0");
    assert_eq!(c(&mut s, "frozen_frames", json!({}))["runs"], json!([]));
}
#[test] fn the_defaults_written_out_give_the_same_bytes() {
    let mut s = known(); let a = serde_json::to_string(&c(&mut s, "frozen_frames", json!({}))).unwrap();
    let b = serde_json::to_string(&c(&mut s, "frozen_frames", json!({"tolerance": 0, "min_run": 2, "source": "timeline", "start": 0.0, "end": 2.5}))).unwrap();
    assert_eq!(a, b);
}
#[test] fn the_run_rule_on_its_own() {
    let px = |v: f64| [vec![v, 0.5], vec![0.5, 0.5], vec![0.5, 0.5]];
    let cut = |vals: &[f64], tol: f64, min: usize| { let mut r = Runs::new(tol, min); for v in vals { r.push(px(*v)); } r.finish() };
    let run = |s, e, m| Run { start: s, end: e, max_diff: m };
    // a drift of 0.01 per frame: compared with the run's first frame it splits at tolerance 0.015 instead of chaining
    assert_eq!(cut(&[0.0, 0.01, 0.02, 0.03], 0.015, 2), vec![run(0, 1, 0.01), run(2, 3, 0.03 - 0.02)]);
    assert_eq!(cut(&[0.0, 0.01, 0.02, 0.03], 0.0, 2), vec![]);
    assert_eq!(cut(&[0.2, 0.2, 0.2, 0.7, 0.7], 0.0, 2), vec![run(0, 2, 0.0), run(3, 4, 0.0)]);
    assert_eq!(cut(&[0.2, 0.2, 0.2, 0.7, 0.7], 0.0, 3), vec![run(0, 2, 0.0)]);
    assert_eq!(cut(&[0.4], 0.0, 2), vec![]); assert_eq!(cut(&[], 0.0, 2), vec![]);
    assert_eq!(max_diff(&px(0.25), &px(0.5)), 0.25); assert_eq!(max_diff(&px(0.5), &[vec![0.5], vec![0.5], vec![0.5]]), 1.0);
}
#[test] fn refused_arguments() {
    let mut s = known();
    for (a, frag) in [(json!({"start": 2.0, "end": 1.0}), "start/end"), (json!({"end": 9.0}), "start/end"), (json!({"tolerance": 2}), "tolerance"), (json!({"tolerance": -0.1}), "tolerance"),
        (json!({"tolerance": "x"}), "tolerance"), (json!({"min_run": 1}), "min_run"), (json!({"min_run": 2.5}), "min_run"), (json!({"scale": 8}), "scale"), (json!({"source": "nope"}), "invalid_source"),
        (json!({"source": "clip:missing"}), "missing clip"), (json!({"start": 1.01, "end": 1.03}), "no project frame")] {
        let e = dispatch(&mut s, "frozen_frames", &a).expect_err(&a.to_string()); assert!(e.contains(frag), "{a}: {e}");
    }
}
