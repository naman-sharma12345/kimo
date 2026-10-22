//! v6 eyes wave 2: black_frames. Runs of near-black frames (start frame, end frame, length, the brightest pixel inside the run)
//! on rendered timelines with known content. Every rule and prediction here was written BEFORE the tool existed, in
//! tests/data/black_frames/THRESHOLD_ADDENDUM.md (which replaced the 0.98 pixel-share line of THRESHOLD.md before any result):
//! a frame is near-black when the Rec.709 luma of EVERY pixel is at or under luma_max; fixture line L = 0.02; min run M = 2.
//! THRESHOLD_ADDENDUM_2.md (also before any result): the tool default luma_max is 0.02, the same as the fixture line; 0 is
//! strict mode; the report carries range tags and flags frames that sit flat at about 16/255.
//! Fixtures: (i) a timeline gap, (ii) a dark grey that is not black, (a1) a dark scene with a dim object and (a2) black frames
//! with 1% and 3% logos (max luma, not mean), (b) runs of M-1 and M frames, (c1) letterbox bars and (c2) a black corner with a
//! full-cover control, (d) a fade to black whose run starts at the frame computed from the line, (e) runs touching frame 0 and
//! the last frame, (f) the brightest pixel per run, (g1/g2) a limited-range black file tagged tv and the same samples tagged pc
//! (the pinned limit), (h) the lint_frame crushed_frac cross-check, and (j) the boundary with frozen_frames: a black run that
//! is also frozen is reported by BOTH tools with the same frame numbers, and (k1, k2, THRESHOLD_ADDENDUM_3.md) the two tools are
//! independent: still-not-black is only a frozen run, black-not-still only a black run. Every frame the tool reads is a real render through
//! the engine; nothing is mocked. With KIMO_BLACK_SHEET=<dir> the fixture frames are also written as PNGs for the contact sheet.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::black_frames::{stretches, Run, Runs, DEFAULT_LUMA_MAX};
use serde_json::{json, Value};

const W: u32 = 160; const H: u32 = 90; const FPS: f64 = 24.0;
/// The fixture line, frozen in THRESHOLD.md and THRESHOLD_ADDENDUM.md before any result: grey codes 0..5 are at or under it.
const L: f64 = 0.02;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn runs(v: &Value) -> Vec<(i64, i64, u64)> { v["runs"].as_array().unwrap().iter().map(|r| (r["start_frame"].as_i64().unwrap(), r["end_frame"].as_i64().unwrap(), r["length"].as_u64().unwrap())).collect() }
fn bf(s: &mut Session, a: Value) -> Value { c(s, "black_frames", a) }
fn at_l(s: &mut Session) -> Value { bf(s, json!({"luma_max": L})) }
fn new() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); s }
fn track(s: &mut Session) -> Value { c(s, "add_track", json!({}))["track_id"].clone() }
fn solid(s: &mut Session, t: &Value, start: f64, dur: f64, color: &str) -> Value { c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": color, "start": start, "duration": dur}))["clip_id"].clone() }
fn rect(s: &mut Session, t: &Value, start: f64, dur: f64, w: f64, h: f64, fill: [f64; 3], x: f64, y: f64) {
    let id = c(s, "add_shape", json!({"track_id": t, "shape": "rect", "start": start, "duration": dur, "width": w, "height": h, "fill": fill}))["clip_id"].clone();
    if x != 0.0 { c(s, "set_property", json!({"clip_id": id, "property": "x", "value": x})); }
    if y != 0.0 { c(s, "set_property", json!({"clip_id": id, "property": "y", "value": y})); }
}
fn ml(v: &Value, i: usize) -> f64 { v["runs"][i]["max_luma"].as_f64().unwrap() }
fn quality(s: &mut Session, t: f64) -> Value { c(s, "lint_frame", json!({"times": [t], "annotate": false, "quality": true}))["frames"][0]["quality"].clone() }
fn sheet(s: &mut Session, name: &str, frames: &[i64]) {
    if let Ok(d) = std::env::var("KIMO_BLACK_SHEET") { for f in frames { let p = format!("{d}/{name}_f{f:02}.png"); c(s, "render_frame", json!({"time": *f as f64 / FPS, "path": p, "inline": "never"})); } }
}
const C4: f64 = 4.0 / 255.0;

/// (i) solid #c08040 0-1 s (frames 0-23), nothing 1-1.5 s (24-35, the opaque black canvas), solid #40c080 1.5-2.5 s (36-59).
fn gap() -> Session { let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#c08040"); solid(&mut s, &t, 1.5, 1.0, "#40c080"); s }
#[test] fn fixture_i_a_timeline_gap_is_one_run_24_to_35_with_max_luma_0() {
    let mut s = gap();
    for r in [bf(&mut s, json!({})), at_l(&mut s)] {
        eprintln!("fixture i (luma_max {}): {}", r["params"]["luma_max"], r["runs"]);
        assert_eq!(r["frames"], json!(60)); assert_eq!(r["measured"], json!([W, H]));
        assert_eq!(runs(&r), vec![(24, 35, 12)]); assert_eq!(ml(&r, 0), 0.0); assert_eq!(r["runs"][0]["max_luma_255"], json!(0.0));
        assert_eq!(r["runs"][0]["start_s"], json!(1.0)); assert_eq!(r["runs"][0]["end_s"], json!(1.5));
        assert_eq!(r["run_count"], json!(1)); assert_eq!(r["black_frames"], json!(12)); assert_eq!(r["longest_run"], json!(12));
        assert_eq!(r["range_tag"], Value::Null); assert_eq!(r["range_tags"], json!([])); assert_eq!(r["limited_range_suspect"], json!([])); assert_eq!(r["note"], Value::Null);
    }
    assert_eq!(runs(&bf(&mut s, json!({"luma_max": 0.0}))), vec![(24, 35, 12)]);
    // a window that cuts into the gap reports only the part inside it
    let w = bf(&mut s, json!({"start": 1.25, "end": 2.0, "luma_max": L})); assert_eq!(w["first_frame"], json!(30)); assert_eq!(runs(&w), vec![(30, 35, 6)]);
    sheet(&mut s, "i_gap", &[23, 24, 35, 36]);
}
/// (ii) solid #080808 for 1 s: max 8/255 = 0.0314 = 1.57 x L. Close to the line (2.9 codes over it).
#[test] fn fixture_ii_dark_grey_080808_is_not_black_close_to_the_line() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#080808");
    let d = bf(&mut s, json!({})); let l = at_l(&mut s); eprintln!("fixture ii: defaults {} at L {}", d["runs"], l["runs"]);
    assert_eq!(runs(&d), vec![]); assert_eq!(runs(&l), vec![]);
    // the line in codes: 5/255 is under L, 6/255 over (the tool's own rule on one frame each)
    for (code, black) in [(5u8, true), (6u8, false)] { let mut q = new(); let t = track(&mut q); solid(&mut q, &t, 0.0, 2.0 / FPS, &format!("#{code:02x}{code:02x}{code:02x}")); assert_eq!(runs(&at_l(&mut q)).len(), black as usize, "code {code}"); }
    sheet(&mut s, "ii_dark", &[0]);
}
/// (a1) a dark scene: solid #020202 with a #303030 rect 36x20 px (5%) over it, 1 s. Max 48/255 = 9.4 x L, so not black; its mean
/// luma is under L, so a mean rule would have called it black.
#[test] fn fixture_a1_a_dark_scene_with_a_dim_object_is_not_a_run_max_not_mean() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#020202"); let t2 = track(&mut s); rect(&mut s, &t2, 0.0, 1.0, 36.0, 20.0, [48.0 / 255.0; 3], 0.0, 0.0);
    let r = at_l(&mut s); let q = quality(&mut s, 0.5); eprintln!("fixture a1: runs {} mean_luma {} crushed_frac {}", r["runs"], q["mean_luma"], q["crushed_frac"]);
    assert_eq!(runs(&r), vec![]);
    let mean = q["mean_luma"].as_f64().unwrap(); assert!(mean < L, "mean {mean}: the mean rule (not used) would call it black"); assert!(mean > 0.0);
    sheet(&mut s, "a1_darkscene", &[12]);
}
/// (a2) black frames with a white logo: 12x12 px (1%) at 0-0.5 s, 24x18 px (3%) at 0.5-1 s. Re-predicted in the addendum (the
/// old 0.98 rule called the 1% frames black): both are NOT black, at L and at the defaults.
#[test] fn fixture_a2_black_frames_with_1_and_3_percent_logos_are_not_runs() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#000000"); let t2 = track(&mut s);
    rect(&mut s, &t2, 0.0, 0.5, 12.0, 12.0, [1.0; 3], 0.0, 0.0); rect(&mut s, &t2, 0.5, 0.5, 24.0, 18.0, [1.0; 3], 0.0, 0.0);
    let d = bf(&mut s, json!({})); let l = at_l(&mut s); eprintln!("fixture a2: defaults {} at L {}", d["runs"], l["runs"]);
    assert_eq!(runs(&d), vec![]); assert_eq!(runs(&l), vec![]);
    let q1 = quality(&mut s, 0.25); let q3 = quality(&mut s, 0.75);
    eprintln!("fixture a2: 1% frame mean_luma {} crushed_frac {}; 3% frame mean_luma {} crushed_frac {}", q1["mean_luma"], q1["crushed_frac"], q3["mean_luma"], q3["crushed_frac"]);
    assert!(q1["mean_luma"].as_f64().unwrap() < L, "1% logo: the mean rule (not used) would call it black");
    sheet(&mut s, "a2_logo", &[6, 18]);
}
/// (b) solid #c08040 0-2 s; black solids over it at 0.98-1.02 s (frame 24 only: M-1 = 1 frame) and 1.48-1.56 s (frames 36-37: M = 2).
fn min_run_clip() -> Session { let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 2.0, "#c08040"); let t2 = track(&mut s);
    solid(&mut s, &t2, 0.98, 0.04, "#000000"); solid(&mut s, &t2, 1.48, 0.08, "#000000"); s }
#[test] fn fixture_b_a_run_of_min_minus_1_is_not_reported_and_a_run_of_min_is() {
    let mut s = min_run_clip();
    let r = at_l(&mut s); eprintln!("fixture b (min_run 2): {}", r["runs"]);
    assert_eq!(r["params"]["min_run"], json!(2)); assert_eq!(runs(&r), vec![(36, 37, 2)]); assert_eq!(ml(&r, 0), 0.0);
    let one = bf(&mut s, json!({"luma_max": L, "min_run": 1})); eprintln!("fixture b (min_run 1): {}", one["runs"]);
    assert_eq!(runs(&one), vec![(24, 24, 1), (36, 37, 2)]);
    assert_eq!(runs(&bf(&mut s, json!({"luma_max": L, "min_run": 3}))), vec![]);
    sheet(&mut s, "b_minrun", &[23, 24, 25, 36, 37, 38]);
}
/// (c1) letterbox: solid #c08040 with black bars 160x12 px top and bottom; (c2) a black rect 80x45 px in the top-left quarter;
/// (c) control: a black rect covering the whole 160x90 frame.
#[test] fn fixture_c_letterbox_bars_and_a_black_corner_do_not_count_the_whole_frame_must_be_black() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#c08040"); let t2 = track(&mut s);
    rect(&mut s, &t2, 0.0, 1.0, 160.0, 12.0, [0.0; 3], 0.0, -39.0); rect(&mut s, &t2, 0.0, 1.0, 160.0, 12.0, [0.0; 3], 0.0, 39.0);
    let r = at_l(&mut s); eprintln!("fixture c1 letterbox: {}", r["runs"]); assert_eq!(runs(&r), vec![]); sheet(&mut s, "c1_letterbox", &[12]);
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#c08040"); let t2 = track(&mut s);
    rect(&mut s, &t2, 0.0, 1.0, 80.0, 45.0, [0.0; 3], -40.0, -22.5);
    let r = at_l(&mut s); eprintln!("fixture c2 corner: {}", r["runs"]); assert_eq!(runs(&r), vec![]); sheet(&mut s, "c2_corner", &[12]);
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#c08040"); let t2 = track(&mut s);
    rect(&mut s, &t2, 0.0, 1.0, 160.0, 90.0, [0.0; 3], 0.0, 0.0);
    let r = at_l(&mut s); eprintln!("fixture c control: {}", r["runs"]); assert_eq!(runs(&r), vec![(0, 23, 24)]); assert_eq!(ml(&r, 0), 0.0);
}
/// (d) solid #606060 (96/255) at 0-1 s with opacity keyed 1 -> 0 (linear), then solid #000000 1-1.5 s. Frame k is code 96 - 4k.
/// The start is computed from the line, not read from a run: the first k with (96 - 4k)/255 <= L.
#[test] fn fixture_d_a_fade_to_black_starts_at_the_first_frame_under_the_line() {
    let mut s = new(); let t = track(&mut s); let id = solid(&mut s, &t, 0.0, 1.0, "#606060"); solid(&mut s, &t, 1.0, 0.5, "#000000");
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 1.0, "ease": "linear"}));
    c(&mut s, "set_keyframe", json!({"clip_id": id, "property": "opacity", "time": 1.0, "value": 0.0, "ease": "linear"}));
    let start = (0..24).find(|k| (96.0 - 4.0 * *k as f64) / 255.0 <= L).unwrap() as i64; assert_eq!(start, 23);
    let r = at_l(&mut s); eprintln!("fixture d at L: {}", r["runs"]);
    assert_eq!(runs(&r), vec![(start, 35, (36 - start) as u64)]);
    assert!((ml(&r, 0) - C4).abs() < 1e-6, "max_luma {}", ml(&r, 0)); assert_eq!(r["runs"][0]["max_luma_255"], json!(4.0));
    let d = bf(&mut s, json!({})); eprintln!("fixture d defaults: {}", d["runs"]); assert_eq!(runs(&d), runs(&r)); assert!((ml(&d, 0) - C4).abs() < 1e-6);
    let z = bf(&mut s, json!({"luma_max": 0.0})); eprintln!("fixture d strict 0: {}", z["runs"]); assert_eq!(runs(&z), vec![(24, 35, 12)]); assert_eq!(ml(&z, 0), 0.0);
    sheet(&mut s, "d_fade", &[21, 22, 23, 24]);
}
/// (e) solid #000000 0-0.5 s, #c08040 0.5-1 s, #000000 1-1.5 s: frames 0-35, a run on each end.
fn ends() -> Session { let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 0.5, "#000000"); solid(&mut s, &t, 0.5, 0.5, "#c08040"); solid(&mut s, &t, 1.0, 0.5, "#000000"); s }
#[test] fn fixture_e_runs_touching_frame_0_and_the_last_frame_report_both_ends() {
    let mut s = ends(); let r = at_l(&mut s); eprintln!("fixture e: {}", r["runs"]);
    assert_eq!(r["frames"], json!(36)); assert_eq!(runs(&r), vec![(0, 11, 12), (24, 35, 12)]);
    assert_eq!(ml(&r, 0), 0.0); assert_eq!(ml(&r, 1), 0.0);
    assert_eq!(r["runs"][0]["start_s"], json!(0.0)); assert_eq!(r["runs"][0]["end_s"], json!(0.5)); assert_eq!(r["runs"][1]["start_s"], json!(1.0)); assert_eq!(r["runs"][1]["end_s"], json!(1.5));
    sheet(&mut s, "e_ends", &[0, 11, 12, 23, 24, 35]);
}
/// (f) solid #000000 0-0.5 s with a #040404 rect 10x10 px: the run's brightest pixel is 4/255, not the mean; a run at the default
/// (re-predicted in THRESHOLD_ADDENDUM_2.md), none at luma_max 0 (strict).
#[test] fn fixture_f_each_run_carries_its_brightest_pixel_and_pure_black_is_0() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 0.5, "#000000"); let t2 = track(&mut s); rect(&mut s, &t2, 0.0, 0.5, 10.0, 10.0, [C4; 3], 0.0, 0.0);
    let r = at_l(&mut s); eprintln!("fixture f: {}", r["runs"]);
    assert_eq!(runs(&r), vec![(0, 11, 12)]); assert!((ml(&r, 0) - C4).abs() < 1e-6, "max_luma {}", ml(&r, 0)); assert_eq!(r["runs"][0]["max_luma_255"], json!(4.0));
    let d = bf(&mut s, json!({})); eprintln!("fixture f defaults: {}", d["runs"]); assert_eq!(runs(&d), vec![(0, 11, 12)]); assert!((ml(&d, 0) - C4).abs() < 1e-6);
    assert_eq!(runs(&bf(&mut s, json!({"luma_max": 0.0}))), vec![]);
    // pure black reports exactly 0 (also asserted in i, b, c and e)
    let mut g = gap(); assert_eq!(at_l(&mut g)["runs"][0]["max_luma"], json!(0.0));
    sheet(&mut s, "f_dot", &[6]);
}
/// (g1/g2) a 1 s file written by ffmpeg (ffv1/.mkv, yuv420p, 160x90, 24 fps): 0-0.5 s grey 0x808080, 0.5-1 s black (Y=16).
fn limited(range: &str) -> Session {
    let p = std::env::temp_dir().join(format!("kimo-black-{range}-{}.mkv", std::process::id()));
    let vf = if range == "tv" { "[0:v][1:v]concat=n=2:v=1,format=yuv420p[v]".to_string() } else { "[0:v][1:v]concat=n=2:v=1,format=yuv420p,setparams=range=pc[v]".to_string() };
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=0x808080:s=160x90:r=24:d=0.5", "-f", "lavfi", "-i", "color=c=black:s=160x90:r=24:d=0.5",
        "-filter_complex", &vf, "-map", "[v]", "-c:v", "ffv1", "-color_range", range]).arg(&p).status().unwrap(); assert!(ok.success());
    let mut s = new(); let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].clone(); let t = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0})); s
}
#[test] fn fixture_g_limited_range_black_y16_reads_as_black_and_the_pc_mistag_is_the_pinned_limit() {
    let mut s = limited("tv"); let r = at_l(&mut s); eprintln!("fixture g1 tv: {}", r["runs"]);
    assert_eq!(runs(&r), vec![(12, 23, 12)]); assert_eq!(ml(&r, 0), 0.0);
    assert_eq!(r["range_tag"], json!("tv")); assert_eq!(r["range_tags"][0]["color_range"], json!("tv")); assert_eq!(r["limited_range_suspect"], json!([])); assert_eq!(r["note"], Value::Null);
    sheet(&mut s, "g1_tv", &[11, 12]);
    let mut s = limited("pc"); let r = at_l(&mut s); eprintln!("fixture g2 pc (known limit): {}", r["runs"]);
    assert_eq!(runs(&r), vec![]); assert_eq!(r["range_tag"], json!("pc")); eprintln!("fixture g2 note: {}", r["note"]);
    let sus: Vec<(i64, i64, u64)> = r["limited_range_suspect"].as_array().unwrap().iter().map(|x| (x["start_frame"].as_i64().unwrap(), x["end_frame"].as_i64().unwrap(), x["length"].as_u64().unwrap())).collect();
    assert_eq!(sus, vec![(12, 23, 12)]); assert!(r["note"].as_str().unwrap().contains("16/255"));
    let m = bf(&mut s, json!({"luma_max": 1.0, "start": 0.5, "end": 1.0})); eprintln!("fixture g2 pc black-half max_luma {}", m["runs"][0]["max_luma"]);
    sheet(&mut s, "g2_pc", &[12]);
}
/// (h) at L a frame is near-black exactly when lint_frame's crushed_frac (crush_below 0.02) is 1.
#[test] fn cross_check_h_near_black_at_l_iff_lint_frame_crushed_frac_is_1() {
    let mut s = gap(); let q = quality(&mut s, 1.25); eprintln!("h: gap frame crushed_frac {}", q["crushed_frac"]); assert_eq!(q["crushed_frac"].as_f64().unwrap(), 1.0);
    assert!(runs(&at_l(&mut s)).iter().any(|r| r.0 <= 30 && 30 <= r.1));
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 0.5, "#000000"); let t2 = track(&mut s); rect(&mut s, &t2, 0.0, 0.5, 12.0, 12.0, [1.0; 3], 0.0, 0.0);
    let q = quality(&mut s, 0.25); eprintln!("h: 1% logo frame crushed_frac {}", q["crushed_frac"]); assert!(q["crushed_frac"].as_f64().unwrap() < 1.0); assert_eq!(runs(&at_l(&mut s)), vec![]);
}
/// (j) the boundary with frozen_frames: on the same rendered clips, every black run is also a frozen run with the same frame
/// numbers. Neither tool dedupes the other; this test exists so nobody removes the overlap by accident.
#[test] fn overlap_j_a_black_run_that_is_also_frozen_shows_up_in_both_tools_with_the_same_frames() {
    for (name, mut s, black, frozen) in [
        ("i", gap(), vec![(24, 35, 12)], vec![(0, 23, 24), (24, 35, 12), (36, 59, 24)]),
        ("e", ends(), vec![(0, 11, 12), (24, 35, 12)], vec![(0, 11, 12), (12, 23, 12), (24, 35, 12)]),
    ] {
        let b = runs(&at_l(&mut s)); let f = runs(&c(&mut s, "frozen_frames", json!({"tolerance": 0.0, "min_run": 2})));
        eprintln!("overlap {name}: black_frames {b:?} frozen_frames {f:?}");
        assert_eq!(b, black); assert_eq!(f, frozen);
        for r in &b { assert!(f.contains(r), "{name}: black run {r:?} missing from frozen_frames {f:?}"); }
    }
}
/// (k1, k2) independence, pre-registered in THRESHOLD_ADDENDUM_3.md before these fixtures existed: a still picture that is not
/// black is a frozen run and no black run (k1); black frames that differ frame to frame are a black run and no frozen run at
/// tolerance 0 (k2). The two tools share no rule; (j) above is the only place they agree, because there the run is both.
fn still_not_black() -> Session { let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#c08040"); s }
fn black_not_still() -> Session {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#000000"); let t2 = track(&mut s);
    for k in (1..24).step_by(2) { solid(&mut s, &t2, k as f64 / FPS - 0.02, 0.04, "#040404"); } s
}
#[test] fn independence_k1_still_not_black_and_k2_black_not_still() {
    let fz = |s: &mut Session, tol: f64| c(s, "frozen_frames", json!({"tolerance": tol, "min_run": 2}));
    // k1: frozen_frames (0, 23, 24) max_diff 0; black_frames no run at the default and at luma_max 0.
    let mut s = still_not_black();
    let f = fz(&mut s, 0.0); let fd = c(&mut s, "frozen_frames", json!({})); let b = bf(&mut s, json!({})); let b0 = bf(&mut s, json!({"luma_max": 0.0}));
    eprintln!("k1: frozen_frames {} (defaults {}), black_frames {} (luma_max 0: {}), suspect {} note {}", f["runs"], fd["runs"], b["runs"], b0["runs"], b["limited_range_suspect"], b["note"]);
    assert_eq!(runs(&f), vec![(0, 23, 24)]); assert_eq!(f["runs"][0]["max_diff"].as_f64().unwrap(), 0.0); assert_eq!(runs(&fd), vec![(0, 23, 24)]);
    assert_eq!(runs(&b), vec![]); assert_eq!(b["run_count"], json!(0)); assert_eq!(b["black_frames"], json!(0)); assert_eq!(runs(&b0), vec![]);
    assert_eq!(b["limited_range_suspect"], json!([])); assert_eq!(b["note"], Value::Null);
    sheet(&mut s, "k1_still_not_black", &[0, 23]);
    // k2: black_frames (0, 23, 24) max 4/255 at the default, none at 0; frozen_frames none at tolerance 0, (0, 23, 24) max_diff 4/255 at 0.02.
    let mut s = black_not_still();
    let b = bf(&mut s, json!({})); let b0 = bf(&mut s, json!({"luma_max": 0.0})); let f = fz(&mut s, 0.0); let fd = c(&mut s, "frozen_frames", json!({})); let f2 = fz(&mut s, 0.02);
    eprintln!("k2: black_frames {} (luma_max 0: {}), frozen_frames tol 0 {} (defaults {}, run_count {}, held {}), tol 0.02 {}", b["runs"], b0["runs"], f["runs"], fd["runs"], f["run_count"], f["held_frames"], f2["runs"]);
    assert_eq!(runs(&b), vec![(0, 23, 24)]); assert!((ml(&b, 0) - C4).abs() < 1e-6, "k2 max_luma {}", ml(&b, 0)); assert_eq!(b["runs"][0]["max_luma_255"], json!(4.0));
    assert_eq!(runs(&b0), vec![]);
    assert_eq!(runs(&f), vec![]); assert_eq!(f["run_count"], json!(0)); assert_eq!(f["held_frames"], json!(0)); assert_eq!(runs(&fd), vec![]);
    assert_eq!(runs(&f2), vec![(0, 23, 24)]); assert!((f2["runs"][0]["max_diff"].as_f64().unwrap() - C4).abs() < 1e-6, "k2 max_diff {}", f2["runs"][0]["max_diff"]);
    sheet(&mut s, "k2_black_not_still", &[0, 1, 2, 23]);
}
/// Naman m:b3c5c470: the units sentence sits in the luma_max field of the MCP input schema itself, verbatim, so a client that
/// never reads the tool description still sees the 0..1 scale and that luma_max is compared to max_luma, never max_luma_255.
#[test] fn luma_max_schema_field_text_carries_the_units_sentence_verbatim() {
    const UNITS: &str = "Units: luma_max is on a 0..1 scale (default 0.02 = 5.1/255; 0 is strict mode, exactly black); each run reports max_luma on the same 0..1 scale and max_luma_255 on the 0..255 code scale (4/255 reads as max_luma 0.015686 and max_luma_255 4), so luma_max is compared to max_luma, never to max_luma_255.";
    let cmd = kimo_core::commands::registry_ref().iter().find(|c| c.name == "black_frames").expect("black_frames registered");
    let field = cmd.schema["properties"]["luma_max"]["description"].as_str().expect("luma_max has a schema description");
    assert!(field.contains(UNITS), "luma_max schema field text lacks the units sentence: {field}");
    assert!(cmd.description.contains(UNITS), "tool description lost the units sentence");
}
/// The run rule on its own: at or under the line joins, over it closes; min_run filters; a run reaching the last frame is kept.
#[test] fn the_run_rule_on_its_own() {
    let mut r = Runs::new(L, 2); for v in [0.5, 0.0, 0.01, 0.02, 0.0201, 0.0, 0.5, 0.0, 0.0] { r.push(v); }
    assert_eq!(r.finish(), vec![Run { start: 1, end: 3, max_luma: 0.02 }, Run { start: 7, end: 8, max_luma: 0.0 }]);
    let mut r = Runs::new(0.0, 1); for v in [0.0, 0.1, 0.0] { r.push(v); } assert_eq!(r.finish().len(), 2);
    let mut r = Runs::new(0.0, 2); r.push(0.0); assert_eq!(r.finish(), vec![]);
    assert_eq!(stretches(&[true, true, false, true, true, true], 3), vec![(3, 5)]); assert_eq!(stretches(&[true, true], 2), vec![(0, 1)]);
}
#[test] fn a_one_frame_black_clip_is_not_a_run_at_min_2_and_is_at_min_1() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0 / FPS, "#000000");
    let r = bf(&mut s, json!({})); assert_eq!(r["frames"], json!(1)); assert_eq!(runs(&r), vec![]);
    assert_eq!(runs(&bf(&mut s, json!({"min_run": 1}))), vec![(0, 0, 1)]);
}
/// THRESHOLD_ADDENDUM_2.md: "tool default luma_max = 0.02, same as fixture line; 0 is strict mode".
#[test] fn default_luma_max_equals_the_threshold_line() {
    assert_eq!(DEFAULT_LUMA_MAX, 0.02); assert_eq!(DEFAULT_LUMA_MAX, L);
    let th = std::fs::read_to_string("tests/data/black_frames/THRESHOLD.md").unwrap(); assert!(th.contains("luma_max L = 0.02"));
    let a2 = std::fs::read_to_string("tests/data/black_frames/THRESHOLD_ADDENDUM_2.md").unwrap(); assert!(a2.contains("tool default luma_max = 0.02, same as fixture line; 0 is strict mode"));
    let mut s = gap(); assert_eq!(bf(&mut s, json!({}))["params"]["luma_max"], json!(0.02));
}
/// A plain full-frame #020202 clip (real encoded black sits at 1-3/255): no run in strict mode, one run at the default.
#[test] fn plain_020202_frame_strict_mode_vs_default() {
    let mut s = new(); let t = track(&mut s); solid(&mut s, &t, 0.0, 1.0, "#020202");
    let z = bf(&mut s, json!({"luma_max": 0.0})); let d = bf(&mut s, json!({})); eprintln!("#020202: strict {} default {}", z["runs"], d["runs"]);
    assert_eq!(runs(&z), vec![]); assert_eq!(runs(&d), vec![(0, 23, 24)]);
    assert!((ml(&d, 0) - 2.0 / 255.0).abs() < 1e-6, "max_luma {}", ml(&d, 0)); assert_eq!(d["runs"][0]["max_luma_255"], json!(2.0));
}
#[test] fn the_defaults_written_out_give_the_same_bytes() {
    let mut s = gap(); let a = bf(&mut s, json!({})); let b = bf(&mut s, json!({"source": "timeline", "start": 0.0, "end": 2.5, "luma_max": 0.02, "min_run": 2}));
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
}
#[test] fn bad_arguments_are_refused_never_clamped() {
    let mut s = gap();
    for a in [json!({"luma_max": -0.1}), json!({"luma_max": 1.5}), json!({"luma_max": "x"}), json!({"min_run": 0}), json!({"min_run": 6001}), json!({"min_run": 2.5}),
              json!({"start": 2.0, "end": 1.0}), json!({"end": 9.0}), json!({"source": "bogus"}), json!({"source": "clip:nope"})]{
        let e = dispatch(&mut s, "black_frames", &a);
        assert!(e.is_err(), "{a} was accepted: {:?}", e);
    }
    let mut big = new(); let t = track(&mut big); solid(&mut big, &t, 0.0, 300.0, "#000000");
    let e = dispatch(&mut big, "black_frames", &json!({})).unwrap_err(); assert!(e.contains("at most 6000"), "{e}");
}
