//! v6 eyes: focus_series. Sharpness per sampled frame (lap_var = variance of the 4-neighbour Laplacian, tenengrad = mean Sobel
//! gradient magnitude, both on Rec.709 luma in 0..255 codes, interior pixels only). Every definition and prediction tested here
//! was committed BEFORE the tool existed, in tests/data/focus_series/PREREG.md (commit 71d8ca85): the blur-ramp clip S (S1-S8),
//! the flat and rect fixtures (F1-F3) and the real clip R (R1-R4, ignored by default: it needs a local 1080p file). Each
//! prediction is scored HIT or MISS on stderr before the test asserts, so a miss is printed as a miss. Every frame is a real render
//! through the engine; nothing is mocked. With KIMO_FOCUS_SHEET=<dir> the ramp frames 0, 17, 32, 41, 51 and 60 are also written as
//! PNGs for the contact sheet.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::focus_series::{score_plane, PxRect};
use serde_json::{json, Value};

const W: u32 = 160; const H: u32 = 90; const FPS: f64 = 24.0;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).err().unwrap_or_else(|| panic!("{n}: expected an error")) }
fn fs(s: &mut Session, a: Value) -> Value { c(s, "focus_series", a) }
fn new() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); s }
fn track(s: &mut Session) -> Value { c(s, "add_track", json!({}))["track_id"].clone() }
fn col(v: &Value, k: &str) -> Vec<f64> { v["series"].as_array().unwrap().iter().map(|x| x[k].as_f64().unwrap()).collect() }
fn frames(v: &Value) -> Vec<i64> { v["series"].as_array().unwrap().iter().map(|x| x["frame"].as_i64().unwrap()).collect() }
/// A file written by ffmpeg (ffv1/.mkv, yuv444p, 160x90, 24 fps) from a luma expression, imported as one media clip at 0.
fn media(tag: &str, lum: &str, dur: f64) -> (Session, Value) {
    let p = std::env::temp_dir().join(format!("kimo-focus-{tag}-{}.mkv", std::process::id()));
    let src = format!("color=c=black:s={W}x{H}:r=24:d={dur},format=gray,geq=lum='{lum}',format=yuv444p");
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", &src, "-c:v", "ffv1"]).arg(&p).status().unwrap(); assert!(ok.success());
    let mut s = new(); let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].clone(); let t = track(&mut s);
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": dur}))["clip_id"].clone(); (s, id)
}
const CHECKER: &str = "if(mod(floor(X/8)+floor(Y/8),2),255,0)";
/// S: the 8 px checker, 3 s, blur radius keyed linear 0 at 0.5 s to 10 at 2.5 s.
fn ramp() -> Session {
    let (mut s, id) = media("ramp", CHECKER, 3.0);
    let fx = c(&mut s, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].clone();
    c(&mut s, "set_keyframes", json!({"clip_id": id, "effect_id": fx, "property": "radius", "keys": [{"t": 0.5, "v": 0.0}, {"t": 2.5, "v": 10.0}]})); s
}
struct Score { misses: Vec<String> }
impl Score { fn new() -> Self { Score { misses: vec![] } } fn check(&mut self, id: &str, ok: bool, what: String) { eprintln!("{id}: {} {what}", if ok { "HIT" } else { "MISS" }); if !ok { self.misses.push(id.to_string()); } } }
/// Plateaus as (first frame, last frame) of equal consecutive values.
fn plateaus(v: &[f64]) -> Vec<(usize, usize)> { let mut out = vec![]; let mut a = 0; for i in 1..=v.len() { if i == v.len() || v[i] != v[i - 1] { out.push((a, i - 1)); a = i; } } out }

#[test] fn prereg_s_blur_ramp() {
    let mut s = ramp(); let r = fs(&mut s, json!({})); let lv = col(&r, "lap_var"); let tg = col(&r, "tenengrad");
    eprintln!("S lap_var by frame: {lv:?}"); eprintln!("S tenengrad by frame: {tg:?}");
    eprintln!("S sharpest {} softest {} lap_var {} tenengrad {}", r["sharpest"], r["softest"], r["lap_var"], r["tenengrad"]);
    let mut sc = Score::new();
    sc.check("S8", r["frames"] == json!(72) && r["measured"] == json!([W, H]) && frames(&r) == (0..72).collect::<Vec<i64>>(), format!("frames {} measured {}", r["frames"], r["measured"]));
    let mono = |v: &[f64]| (0..v.len() - 1).all(|f| v[f + 1] <= v[f]);
    sc.check("S1", mono(&lv) && mono(&tg), format!("lap_var non-increasing {} tenengrad non-increasing {}", mono(&lv), mono(&tg)));
    let pl = plateaus(&lv); let pt = plateaus(&tg);
    // S2: frames with the same effective box radius give identical values; exactly 6 distinct lap_var values
    let r_of = |f: usize| { let rad = (10.0 * (f as f64 / FPS - 0.5) / 2.0).clamp(0.0, 10.0) as usize; if rad < 1 { 0 } else { (rad / 2).max(1) } };
    let same_r_same_v = (0..72).all(|a| (0..72).all(|b| r_of(a) != r_of(b) || (lv[a] == lv[b] && tg[a] == tg[b])));
    let mut d = lv.clone(); d.sort_by(|a, b| a.partial_cmp(b).unwrap()); d.dedup();
    sc.check("S2", same_r_same_v && d.len() == 6, format!("same r -> identical values {same_r_same_v}; distinct lap_var values {}", d.len()));
    let falls = |v: &[f64]| (0..71).filter(|f| r_of(*f) != r_of(f + 1)).all(|f| v[f + 1] < v[f]);
    sc.check("S3", falls(&lv) && falls(&tg), format!("strict fall at every r change: lap_var {} tenengrad {}", falls(&lv), falls(&tg)));
    let want = vec![(0, 16), (17, 31), (32, 40), (41, 50), (51, 59), (60, 71)];
    sc.check("S4", pl == want && pt == want, format!("plateaus lap_var {pl:?} tenengrad {pt:?}"));
    sc.check("S5", r["sharpest"]["frame"] == json!(0) && r["softest"]["frame"] == json!(60), format!("sharpest frame {} softest frame {}", r["sharpest"]["frame"], r["softest"]["frame"]));
    let r2 = fs(&mut s, json!({"step": 2})); let f2 = frames(&r2); let l2 = col(&r2, "lap_var"); let t2 = col(&r2, "tenengrad");
    let s6 = f2 == (0..72).step_by(2).map(|x| x as i64).collect::<Vec<_>>() && f2.iter().enumerate().all(|(j, f)| l2[j] == lv[*f as usize] && t2[j] == tg[*f as usize]);
    sc.check("S6", s6, format!("step 2: {} samples, first {:?} last {:?}", f2.len(), f2.first(), f2.last()));
    let m = r["softest"]["lap_var"].clone(); let r3 = fs(&mut s, json!({"soft_max": m}));
    let runs: Vec<(i64, i64, u64)> = r3["soft_runs"].as_array().unwrap().iter().map(|x| (x["start_frame"].as_i64().unwrap(), x["end_frame"].as_i64().unwrap(), x["samples"].as_u64().unwrap())).collect();
    sc.check("S7", runs == vec![(60, 71, 12)], format!("soft_max {m}: runs {runs:?}"));
    assert_eq!(r["soft_runs"], Value::Null, "no soft_max, no runs");
    if let Ok(dir) = std::env::var("KIMO_FOCUS_SHEET") { for f in [0, 17, 32, 41, 51, 60] { c(&mut s, "render_frame", json!({"time": f as f64 / FPS, "path": format!("{dir}/ramp_f{f:02}.png"), "inline": "never"})); } }
    assert!(sc.misses.is_empty(), "MISSES: {:?}", sc.misses);
}

#[test] fn prereg_f_flat_rects_and_outside() {
    let mut sc = Score::new();
    let mut s = new(); let t = track(&mut s); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#808080", "start": 0, "duration": 0.5}));
    let r = fs(&mut s, json!({"rects": [[10, 10, 40, 40]]}));
    let f1 = r["series"].as_array().unwrap().iter().all(|x| x["lap_var"] == json!(0.0) && x["tenengrad"] == json!(0.0) && x["rects"][0]["lap_var"] == json!(0.0) && x["rects"][0]["tenengrad"] == json!(0.0));
    sc.check("F1", f1, format!("flat #808080: frame 0 {}", r["series"][0]));
    let (mut s, _) = media("split", &format!("if(gte(X,80),128,{CHECKER})"), 1.0);
    let r = fs(&mut s, json!({"rects": [[8, 8, 64, 74], [88, 8, 64, 74], [200, 200, 10, 10]]}));
    let x = &r["series"][0]; eprintln!("F2/F3 frame 0: {x}");
    let (al, at, bl, bt) = (x["rects"][0]["lap_var"].as_f64().unwrap(), x["rects"][0]["tenengrad"].as_f64().unwrap(), x["rects"][1]["lap_var"].as_f64().unwrap(), x["rects"][1]["tenengrad"].as_f64().unwrap());
    let all_frames = r["series"].as_array().unwrap().iter().all(|y| y["rects"] == x["rects"]);
    sc.check("F2", al > 0.0 && at > 0.0 && bl == 0.0 && bt == 0.0 && al > bl && at > bt && all_frames, format!("A lap_var {al} tenengrad {at}; B lap_var {bl} tenengrad {bt}; same on all 24 frames {all_frames}"));
    sc.check("F3", x["rects"][2] == json!({"lap_var": null, "tenengrad": null}) && r["rects"][2]["interior_px"] == json!(0), format!("outside rect {}", x["rects"][2]));
    assert!(sc.misses.is_empty(), "MISSES: {:?}", sc.misses);
}

/// Unit: a 64x64 checker of 8 px squares against the same board box-blurred (3 passes) at r = 1, 2, 4: both scores strictly
/// fall as r grows; a flat plane reads exactly 0; a rect over the sharp half scores above one over the blurred half.
#[test] fn unit_checker_blur_flat_and_rects() {
    let (w, h) = (64usize, 64usize);
    let board: Vec<f64> = (0..w * h).map(|i| if ((i % w) / 8 + (i / w) / 8) % 2 == 1 { 255.0 } else { 0.0 }).collect();
    let blur = |src: &[f64], r: usize| { let mut a = src.to_vec(); for _ in 0..3 { for horiz in [true, false] { let b = a.clone();
        for y in 0..h { for x in 0..w { let mut acc = 0.0; for k in -(r as isize)..=(r as isize) { let (xx, yy) = if horiz { ((x as isize + k).clamp(0, w as isize - 1) as usize, y) } else { (x, (y as isize + k).clamp(0, h as isize - 1) as usize) }; acc += b[yy * w + xx]; } a[y * w + x] = acc / (2 * r + 1) as f64; } } } } a };
    let sc: Vec<_> = [0usize, 1, 2, 4].iter().map(|r| { let p = if *r == 0 { board.clone() } else { blur(&board, *r) }; score_plane(&p, w, h, None).unwrap() }).collect();
    eprintln!("unit checker r=0,1,2,4: {:?}", sc.iter().map(|s| (s.lap_var, s.tenengrad)).collect::<Vec<_>>());
    for k in 0..3 { assert!(sc[k + 1].lap_var < sc[k].lap_var && sc[k + 1].tenengrad < sc[k].tenengrad, "step {k}"); }
    assert_eq!(sc[0].px, 62 * 62);
    let flat = vec![77.0; w * h]; let f = score_plane(&flat, w, h, None).unwrap(); assert_eq!((f.lap_var, f.tenengrad), (0.0, 0.0));
    let bl = blur(&board, 2); let half: Vec<f64> = (0..w * h).map(|i| if i % w < 32 { board[i] } else { bl[i] }).collect();
    let a = score_plane(&half, w, h, Some(PxRect::from_xywh([2.0, 2.0, 24.0, 60.0]))).unwrap(); let b = score_plane(&half, w, h, Some(PxRect::from_xywh([38.0, 2.0, 24.0, 60.0]))).unwrap();
    assert!(a.lap_var > b.lap_var && a.tenengrad > b.tenengrad, "{a:?} vs {b:?}");
    assert_eq!(score_plane(&half, w, h, Some(PxRect::from_xywh([70.0, 0.0, 5.0, 5.0]))), None);
    assert_eq!(score_plane(&flat, 2, 2, None), None);
}

#[test] fn arguments_and_limits() {
    let mut s = new(); let t = track(&mut s); let cl = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#808080", "start": 0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    assert!(e(&mut s, "focus_series", json!({"rects": [[0, 0, 0, 5]]})).contains("invalid_rect"));
    assert!(e(&mut s, "focus_series", json!({"rects": [[0, 0, 5]]})).contains("invalid_rect"));
    assert!(e(&mut s, "focus_series", json!({"rects": "x"})).contains("invalid_rect"));
    assert!(e(&mut s, "focus_series", json!({"rects": vec![[0, 0, 5, 5]; 17]})).contains("at most 16"));
    assert!(e(&mut s, "focus_series", json!({"step": 0})).contains("out_of_range"));
    assert!(e(&mut s, "focus_series", json!({"soft_max": -1})).contains("out_of_range"));
    assert!(e(&mut s, "focus_series", json!({"source": "clip:nope"})).contains("invalid_source"));
    assert!(e(&mut s, "focus_series", json!({"start": 0.5, "end": 0.25})).contains("out_of_range"));
    let r = fs(&mut s, json!({"start": 0.25, "end": 0.5, "step": 3})); assert_eq!(frames(&r), vec![6, 9]); assert_eq!(r["last_frame"], json!(9));
    let mut long = new(); let t = track(&mut long); c(&mut long, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#808080", "start": 0, "duration": 60.0}));
    assert!(e(&mut long, "focus_series", json!({})).contains("at most 1200"));
    assert_eq!(fs(&mut s, json!({"source": format!("clip:{cl}"), "end": 0.125}))["frames"], json!(3));
}

/// R: the real lossless clip (frames 240-263 frozen, 480-491 pure black). Needs the local file; run with --ignored.
#[test] #[ignore = "needs the local 1080p real clip (KIMO_FOCUS_REAL=<path>)"] fn prereg_r_real_clip() {
    let p = std::env::var("KIMO_FOCUS_REAL").expect("set KIMO_FOCUS_REAL to the real clip (tests/data/focus_series/PREREG.md, section R)");
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 1920, "height": 1080, "fps": 24.0}));
    let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].clone(); let t = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 24.0}));
    let r = fs(&mut s, json!({})); let lv = col(&r, "lap_var"); let tg = col(&r, "tenengrad");
    if let Ok(o) = std::env::var("KIMO_FOCUS_REAL_OUT") { std::fs::write(o, serde_json::to_string(&r).unwrap()).unwrap(); }
    eprintln!("R frames {} measured {} sharpest {} softest {} lap_var {} tenengrad {}", r["frames"], r["measured"], r["sharpest"], r["softest"], r["lap_var"], r["tenengrad"]);
    eprintln!("R frames 236-268 lap_var {:?}", &lv[236..=268]); eprintln!("R frames 476-495 lap_var {:?}", &lv[476..=495]);
    let mut sc = Score::new();
    sc.check("R1", (480..=491).all(|f| lv[f] == 0.0 && tg[f] == 0.0) && r["softest"]["frame"] == json!(480), format!("480-491 zeros {}; softest {}", (480..=491).all(|f| lv[f] == 0.0 && tg[f] == 0.0), r["softest"]["frame"]));
    sc.check("R2", (240..=263).all(|f| lv[f] == lv[240] && tg[f] == tg[240]), format!("240-263 lap_var {} tenengrad {}", lv[240], tg[240]));
    sc.check("R3", lv[239] != lv[240] && lv[264] != lv[240], format!("239 {} 240 {} 264 {}", lv[239], lv[240], lv[264]));
    let r4: Vec<usize> = (0..576).filter(|f| !(480..=491).contains(f) && lv[*f] <= 0.0).collect();
    sc.check("R4", r4.is_empty() && lv.len() == 576, format!("frames outside 480-491 at 0: {r4:?}"));
    assert!(sc.misses.is_empty(), "MISSES: {:?}", sc.misses);
}
