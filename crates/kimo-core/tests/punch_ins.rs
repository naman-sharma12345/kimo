//! add_punch_ins: jump-cut punch-ins on a synthetic red ramp (160x90 @ 10 fps): three 1 s clips of the same picture on one
//! track. Every column's red value names the source column it shows, so a zoom about a focus point is read off directly:
//! punched by k about focus F, canvas column X shows source column F + (X - F) / k, and F itself never moves.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn png(name: &str, f: impl Fn(usize, usize) -> [u8; 3]) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-punch-{}-{name}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 3); for y in 0..H { for x in 0..W { o.extend_from_slice(&f(x, y)); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
/// Three 1 s clips of the red ramp on one track (cuts at 1 s and 2 s).
fn scene() -> (Session, String, Vec<String>) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pa = png("a", |x, _| [(x * 255 / (W - 1)) as u8, 0, 40]); let aa = id(&c(&mut s, "import_media", json!({"path": pa})), "asset_id");
    let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let cl = (0..3).map(|i| id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aa, "start": i as f64, "duration": 1.0})), "clip_id")).collect();
    (s, t, cl)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
/// Source column shown at canvas column x, row y.
fn col(f: &Frame, x: usize, y: usize) -> f32 { f.d[(y * f.w + x) * 4] * (W - 1) as f32 }
fn near(got: f32, want: f32, what: &str) { assert!((got - want).abs() <= 1.5, "{what}: want {want}, got {got:.2}"); }
fn pj(s: &Session) -> String { serde_json::to_string(&s.project).unwrap() }
fn zoomed(x: f32, focus: f32, k: f32) -> f32 { focus + (x - focus) / k }

#[test]
fn alternate_clips_punch_in_about_the_focus_point() {
    let (mut s, t, cl) = scene();
    let r = c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25}));
    assert_eq!((r["punched"].as_u64(), r["wide"].as_u64()), (Some(1), Some(2)), "{r}");
    for (tm, k, what) in [(0.5, 1.0, "clip 1 wide"), (1.5, 1.25, "clip 2 punched"), (2.5, 1.0, "clip 3 wide")] {
        let f = frame(&s, tm); near(col(&f, 40, 45), zoomed(40.0, 80.0, k), what); near(col(&f, 140, 45), zoomed(140.0, 80.0, k), what); }
    let f = frame(&s, 1.5); near(col(&f, 2, 2), zoomed(2.0, 80.0, 1.25), "a punch never shows an edge: the top-left corner shows the picture"); near(col(&f, W - 3, H - 3), zoomed((W - 3) as f32, 80.0, 1.25), "bottom-right corner");
    // the focus point stays put: focus at the left quarter
    c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "focus": [0.25, 0.5]}));
    let f = frame(&s, 1.5); near(col(&f, 40, 45), 40.0, "the focus column does not move"); near(col(&f, 120, 45), zoomed(120.0, 40.0, 1.25), "zoomed about the focus");
    // the same call again gives the same picture (values are set, not multiplied)
    let once = frame(&s, 1.5).d; c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "focus": [0.25, 0.5]})); assert_eq!(frame(&s, 1.5).d, once, "a repeated call is idempotent");
    // start tight, and all
    c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "start": "tight"}));
    near(col(&frame(&s, 0.5), 40, 45), zoomed(40.0, 80.0, 1.25), "start tight: clip 1 punched"); near(col(&frame(&s, 1.5), 40, 45), 40.0, "start tight: clip 2 wide");
    let r = c(&mut s, "add_punch_ins", json!({"clip_ids": [cl[2], cl[0]], "scale": 1.5, "pattern": "all"})); assert_eq!(r["punched"], 2, "{r}");
    near(col(&frame(&s, 2.5), 40, 45), zoomed(40.0, 80.0, 1.5), "pattern all"); near(col(&frame(&s, 0.5), 40, 45), zoomed(40.0, 80.0, 1.5), "pattern all");
    let r = c(&mut s, "add_punch_ins", json!({"track_id": t, "remove": true})); assert_eq!(r["punched"], 0);
    for tm in [0.5, 1.5, 2.5] { near(col(&frame(&s, tm), 40, 45), 40.0, "remove puts every clip back to wide"); }
}

#[test]
fn ease_turns_the_punch_into_a_quick_push_in() {
    let (mut s, t, _cl) = scene();
    c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "ease": 0.4}));
    near(col(&frame(&s, 1.0), 40, 45), 40.0, "at the cut the punched clip starts wide");
    let mid = col(&frame(&s, 1.2), 40, 45); assert!(mid > 41.5 && mid < zoomed(40.0, 80.0, 1.25) - 1.5, "half way through the ease it is between wide and tight: {mid}");
    near(col(&frame(&s, 1.6), 40, 45), zoomed(40.0, 80.0, 1.25), "after the ease it holds the punch");
    near(col(&frame(&s, 2.5), 40, 45), 40.0, "the next clip is wide again");
}

/// A clip whose scale / x / y are already animated is punched on top of its own animation: the punch multiplies its scale and
/// zooms its position about the focus, the animation keeps playing underneath, a repeat call does not stack, and remove gives
/// the animation back untouched (KP5; before, such a clip was refused).
#[test]
fn a_punch_composes_with_the_clips_own_animation() {
    let (mut s, t, cl) = scene();
    // clip 2 drifts right: x 0 -> 20 px over its second; its picture at local time u is the ramp moved by 20u
    c(&mut s, "set_keyframe", json!({"clip_id": cl[1], "property": "x", "time": 0.0, "value": 0.0})); c(&mut s, "set_keyframe", json!({"clip_id": cl[1], "property": "x", "time": 1.0, "value": 20.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": cl[1], "property": "scale", "time": 0.0, "value": 1.0})); c(&mut s, "set_keyframe", json!({"clip_id": cl[1], "property": "scale", "time": 1.0, "value": 1.2}));
    let own = serde_json::to_string(&s.project.find_clip(&cl[1]).unwrap().props).unwrap();
    // without a punch, canvas column X at local time u shows source column 80 + (X - 80 - x(u)) / scale(u)
    let shown = |x: f32, xo: f32, sc: f32| 80.0 + (x - 80.0 - xo) / sc;
    let before = col(&frame(&s, 1.5), 40, 45); near(before, shown(40.0, 10.0, 1.1), "the clip's own animation at 1.5 s");
    let r = c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "focus": [0.25, 0.5]}));
    assert_eq!(r["punched"], 1, "{r}");
    // punched by k about focus F on top: the picture P(u) is mapped to F + k (P(u) - F); canvas X shows what P(u) shows at F + (X - F) / k
    let pz = |x: f32| 40.0 + (x - 40.0) / 1.25;
    for (tm, xo, sc) in [(1.1, 2.0, 1.02), (1.5, 10.0, 1.1), (1.9, 18.0, 1.18)] {
        let f = frame(&s, tm); for x in [20.0, 100.0, 150.0] { near(col(&f, x as usize, 45), shown(pz(x), xo, sc), &format!("punched on top of the animation at {tm} s, canvas x {x}")); } }
    near(col(&frame(&s, 1.5), 40, 45), before, "the focus column shows what it showed before the punch");
    let once = serde_json::to_string(&s.project).unwrap(); c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "focus": [0.25, 0.5]}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), once, "a repeated call does not stack the punch");
    c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.25, "ease": 0.4}));
    near(col(&frame(&s, 1.0), 40, 45), shown(40.0, 0.0, 1.0), "ease on an animated clip: at the cut it starts wide");
    near(col(&frame(&s, 1.6), 40, 45), shown(80.0 + (40.0 - 80.0) / 1.25, 12.0, 1.12), "ease on an animated clip: after the ease the punch holds on top of the animation");
    c(&mut s, "add_punch_ins", json!({"track_id": t, "remove": true}));
    assert_eq!(serde_json::to_string(&s.project.find_clip(&cl[1]).unwrap().props).unwrap(), own, "remove gives back the clip's own animation exactly");
    near(col(&frame(&s, 1.5), 40, 45), before, "and its picture");
}

#[test]
fn bad_input_is_refused_without_changing_the_project() {
    let (mut s, t, cl) = scene();
    let before = pj(&s);
    for (args, want) in [
        (json!({}), "track_id"),
        (json!({"track_id": t, "clip_ids": [cl[0]]}), "not both"),
        (json!({"track_id": "nope"}), "no such track"),
        (json!({"clip_ids": ["nope"]}), "no such clip"),
        (json!({"clip_ids": [cl[0], cl[0]]}), "twice"),
        (json!({"clip_ids": [cl[0]], "scale": 0.9}), "within"),
        (json!({"clip_ids": [cl[0]], "scale": 5}), "within"),
        (json!({"clip_ids": [cl[0]], "focus": [1.5, 0.5]}), "focus"),
        (json!({"clip_ids": [cl[0]], "pattern": "random"}), "must be one of"),
        (json!({"clip_ids": [cl[0]], "zoom": 2}), "unknown argument"),
    ] { let e = dispatch(&mut s, "add_punch_ins", &args).expect_err(&format!("{args} must be refused")); assert!(e.contains(want), "{args}: {e}"); assert_eq!(pj(&s), before, "{args} changed the project"); }
}

/// RV-28 follow-up: every alternate-pattern punch previously landed at the exact same zoom, cut after cut -- visibly
/// mechanical on a longer edit. variety cycles the punched clips' own zoom amount around `scale` deterministically
/// (no randomness -- same input, same output, same as everything else here), never dropping a punch down near 1.0
/// (it would stop reading as "punched in" at all), and never moving the focus point regardless of the zoom it lands at.
#[test]
fn variety_makes_consecutive_punches_land_at_different_zooms_without_losing_the_focus_point() {
    let (mut s, t, _cl) = scene();
    let r = c(&mut s, "add_punch_ins", json!({"track_id": t, "scale": 1.3, "pattern": "all", "variety": true, "focus": [0.25, 0.5]}));
    let scales: Vec<f64> = r["clips"].as_array().unwrap().iter().map(|c| c["scale"].as_f64().unwrap()).collect();
    assert_eq!(scales.len(), 3, "{r}");
    assert!(scales.iter().all(|&s| s > 1.05), "every punch should still clearly read as punched in: {scales:?}");
    assert!(scales[0] != scales[1] || scales[1] != scales[2], "consecutive punches should not all land at the identical zoom: {scales:?}");
    // deterministic: the same call again gives exactly the same zooms, not a fresh roll
    let mut s2 = scene().0;
    let r2 = c(&mut s2, "add_punch_ins", json!({"track_id": &t, "scale": 1.3, "pattern": "all", "variety": true, "focus": [0.25, 0.5]}));
    let scales2: Vec<f64> = r2["clips"].as_array().unwrap().iter().map(|c| c["scale"].as_f64().unwrap()).collect();
    assert_eq!(scales, scales2, "variety must be deterministic, not random");
    // the focus point still never moves, whatever zoom a given clip lands at (focus at the left quarter: column 40)
    for (tm, what) in [(0.5, "clip 1"), (1.5, "clip 2"), (2.5, "clip 3")] {
        near(col(&frame(&s, tm), 40, 45), 40.0, &format!("{what}: focus column unchanged by variety"));
    }
}
