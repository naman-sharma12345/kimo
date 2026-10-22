//! add_depth_parallax (a "3D photo" camera move on a 2D clip, After Effects / Premiere 2.5D parallax from a depth map), read off a
//! synthetic 160x90 picture whose depth map is known: a green square (depth 1, near) over a left-to-right red ramp (depth 0, far).
//! A camera pan moves near and far by different amounts, the near square hides what is behind it, the background it uncovers is
//! filled from the background (not smeared from the square), a dolly scales near more than far, and every move is keyframable.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn png(name: &str, f: impl Fn(usize, usize) -> [u8; 4]) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-dp-{}-{name}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 4); for y in 0..H { for x in 0..W { o.extend_from_slice(&f(x, y)); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn in_sq(x: usize, y: usize) -> bool { (70..90).contains(&x) && (35..55).contains(&y) }
fn ramp(x: usize) -> u8 { (x * 255 / (W - 1)) as u8 }
/// (session, picture clip, depth map clip); with_map false leaves the depth map out (the plain picture).
fn scene(with_map: bool) -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pic = png("pic", |x, y| if in_sq(x, y) { [25, 230, 25, 255] } else { [ramp(x), 25, 76, 255] });
    let dep = png("depth", |x, y| if in_sq(x, y) { [255, 255, 255, 255] } else { [0, 0, 0, 255] });
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id");
    let a = id(&c(&mut s, "import_media", json!({"path": pic})), "asset_id");
    let v = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": a, "start": 0, "duration": 2.0})), "clip_id");
    let mut m = String::new();
    if with_map { let t2 = id(&c(&mut s, "add_track", json!({})), "track_id"); let am = id(&c(&mut s, "import_media", json!({"path": dep})), "asset_id");
        m = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": am, "start": 0, "duration": 2.0})), "clip_id"); }
    (s, v, m)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 3] { let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] }
fn green(f: &Frame, x: usize, y: usize) -> bool { let p = px(f, x, y); p[1] > 0.8 && p[0] < 0.2 }
fn bg(f: &Frame, x: usize, y: usize) -> bool { px(f, x, y)[1] < 0.25 }
fn near(got: f32, want: f32, tol: f32, what: &str) { assert!((got - want).abs() <= tol, "{what}: want {want}, got {got:.4}"); }
fn parallax(s: &mut Session, v: &str, m: &str, extra: Value) -> Value { let mut a = json!({"clip_id": v, "depth_clip_id": m}); for (k, x) in extra.as_object().unwrap() { a[k] = x.clone(); } c(s, "add_depth_parallax", a) }

#[test]
fn a_camera_pan_moves_the_near_square_and_leaves_the_far_background() {
    let (plain, _, _) = scene(false); let base = frame(&plain, 0.5);
    let (mut s, v, m) = scene(true);
    let r = dispatch(&mut s, "add_depth_parallax", &json!({"clip_id": v, "depth_clip_id": m, "pan_x": 0.05, "focal_depth": 0.0}));
    assert!(r.is_ok(), "add_depth_parallax must exist: {}", r.unwrap_err());
    let f = frame(&s, 0.5);
    // pan_x 0.05 of the 160 px width = 8 px per unit of depth; focal_depth 0 keeps the far background exactly where it was
    for x in [5, 20, 50, 110, 150] { assert_eq!(px(&f, x, 45), px(&base, x, 45), "far background pixel x {x} must not move"); }
    for x in [78, 80, 90, 97] { assert!(green(&f, x, 45), "near square must have moved 8 px right (x {x} green), got {:?}", px(&f, x, 45)); }
    assert!(bg(&f, 99, 45), "x 99 is past the moved square, got {:?}", px(&f, 99, 45));
    // the strip the square uncovered (x 70..77) is filled from the background beside it, not smeared from the square
    for x in [70, 72, 74, 76] { assert!(bg(&f, x, 45), "uncovered background x {x} must be filled from the background, got {:?}", px(&f, x, 45)); }
    near(px(&f, 74, 45)[0], px(&base, 68, 45)[0], 0.08, "uncovered strip takes the background colour next to it");
    // the depth map clip itself never shows
    assert!(bg(&f, 20, 45) && px(&f, 20, 45)[0] < 0.2, "depth map clip must be hidden");
}

#[test]
fn b_the_near_square_hides_the_background_it_moves_over() {
    let (mut s, v, m) = scene(true); parallax(&mut s, &v, &m, json!({"pan_x": -0.05, "focal_depth": 0.0}));
    let f = frame(&s, 0.5);
    for x in [62, 64, 70, 81] { assert!(green(&f, x, 45), "square moved 8 px left stays solid over the background (x {x}), got {:?}", px(&f, x, 45)); }
    for x in [83, 86, 89] { assert!(bg(&f, x, 45), "uncovered background on the right (x {x}), got {:?}", px(&f, x, 45)); }
    assert!(bg(&f, 60, 45), "x 60 is left of the moved square");
    // vertical pan moves it down (pan_y fraction of the height: 0.1 * 90 = 9 px)
    let (mut s2, v2, m2) = scene(true); parallax(&mut s2, &v2, &m2, json!({"pan_y": 0.1, "focal_depth": 0.0}));
    let g = frame(&s2, 0.5); assert!(green(&g, 80, 62), "pan_y moves the square down 9 px (y 62)"); assert!(bg(&g, 80, 38), "y 38 uncovered after pan_y");
}

#[test]
fn c_focal_depth_picks_the_plane_that_stays_still() {
    let (plain, _, _) = scene(false); let base = frame(&plain, 0.5);
    let (mut s, v, m) = scene(true); parallax(&mut s, &v, &m, json!({"pan_x": 0.05, "focal_depth": 1.0}));
    let f = frame(&s, 0.5);
    for x in [70, 75, 89] { assert_eq!(px(&f, x, 45), px(&base, x, 45), "focal_depth 1: the near square stays put (x {x})"); }
    // the far background moves the other way by 8 px: what was at x 28 is now at x 20
    near(px(&f, 20, 45)[0], px(&base, 28, 45)[0], 0.01, "far background moved 8 px left");
    near(px(&f, 120, 45)[0], px(&base, 128, 45)[0], 0.01, "far background moved 8 px left (x 120)");
}

#[test]
fn d_a_dolly_scales_near_more_than_far() {
    let (plain, _, _) = scene(false); let base = frame(&plain, 0.5);
    let (mut s, v, m) = scene(true); parallax(&mut s, &v, &m, json!({"dolly": 0.2, "focal_depth": 0.0}));
    let f = frame(&s, 0.5);
    // the square (centred on the canvas centre 80,45, 20 px) grows 1.2x to 24 px: x 68..91, y 33..56; the far background is unscaled
    for (x, y) in [(68, 45), (91, 45), (80, 33), (80, 56), (69, 34)] { assert!(green(&f, x, y), "dolly: square grown to 24 px covers ({x},{y}), got {:?}", px(&f, x, y)); }
    for (x, y) in [(66, 45), (93, 45), (80, 31)] { assert!(bg(&f, x, y), "dolly: ({x},{y}) is outside the grown square"); }
    for x in [5, 30, 130] { assert_eq!(px(&f, x, 45), px(&base, x, 45), "dolly: far background x {x} unscaled"); }
    // zoom scales everything about the centre (overscan that hides the frame edge on a pan)
    let (mut s2, v2, m2) = scene(true); parallax(&mut s2, &v2, &m2, json!({"zoom": 1.25, "focal_depth": 0.0}));
    let g = frame(&s2, 0.5); near(px(&g, 0, 45)[0], px(&base, 16, 45)[0], 0.015, "zoom 1.25: the left edge shows source x 16");
}

#[test]
fn e_moves_are_keyframable_and_a_still_camera_is_bit_exact() {
    let (plain, _, _) = scene(false); let base0 = frame(&plain, 0.0); let base1 = frame(&plain, 1.0);
    let (mut s, v, m) = scene(true); let r = parallax(&mut s, &v, &m, json!({"focal_depth": 0.0}));
    let e = id(&r, "effect_id");
    assert_eq!(frame(&s, 0.5).d, frame(&plain, 0.5).d, "no move (pan 0, dolly 0, zoom 1) must leave the picture bit-exact");
    c(&mut s, "set_keyframe", json!({"clip_id": v, "effect_id": e, "property": "pan_x", "time": 0.0, "value": 0.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": v, "effect_id": e, "property": "pan_x", "time": 1.0, "value": 0.05}));
    assert_eq!(frame(&s, 0.0).d, base0.d, "pan keyed 0 at t 0: unchanged");
    let f1 = frame(&s, 1.0); assert!(green(&f1, 96, 45) && bg(&f1, 72, 45), "pan keyed 0.05 at t 1: square moved 8 px");
    let _ = base1;
    // re-running replaces the parallax rather than stacking it; remove takes it off
    let r2 = parallax(&mut s, &v, &m, json!({"pan_x": -0.05, "focal_depth": 0.0})); assert_eq!(id(&r2, "effect_id"), e, "second call updates the same effect");
    assert_eq!(s.project.find_clip(&v).unwrap().effects.iter().filter(|x| x.kind == "depth_parallax").count(), 1);
    c(&mut s, "add_depth_parallax", json!({"clip_id": v, "remove": true}));
    assert_eq!(s.project.find_clip(&v).unwrap().effects.iter().filter(|x| x.kind == "depth_parallax").count(), 0, "remove takes it off");
}

#[test]
fn f_bad_calls_are_refused_and_change_nothing() {
    let (mut s, v, m) = scene(true); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [
        (json!({"clip_id": v}), "depth_clip_id"),
        (json!({"clip_id": v, "depth_clip_id": v}), "own depth map"),
        (json!({"clip_id": v, "depth_clip_id": "nope"}), "no such clip"),
        (json!({"clip_id": "nope", "depth_clip_id": m}), "no such clip"),
        (json!({"clip_id": v, "depth_clip_id": m, "pan_x": 3}), "pan_x"),
        (json!({"clip_id": v, "depth_clip_id": m, "zoom": 0.5}), "zoom"),
        (json!({"clip_id": v, "depth_clip_id": m, "focal_depth": 2}), "focal_depth"),
        (json!({"clip_id": v, "depth_clip_id": m, "depth_channel": "purple"}), "depth_channel"),
        (json!({"clip_id": v, "depth_clip_id": m, "panx": 0.1}), "unknown argument 'panx'"),
    ] { let e = dispatch(&mut s, "add_depth_parallax", &a).expect_err(&format!("{a} must be refused")); assert!(e.contains(want), "{a}: error '{e}' must mention '{want}'"); }
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "refused calls must not change the project");
}
