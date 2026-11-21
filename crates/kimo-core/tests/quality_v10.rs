//! v10 render quality: each test renders the same small project in legacy and in v10 and checks the bar the v10
//! plan sets (linear-light dissolves and dips, Gaussian blurs without fringes, sharper magnification, transition
//! motion blur, thin antialiased wipe edges, per-frame dither). Legacy must behave exactly as before.
use kimo_core::{commands::{dispatch, Session}, effects::{self, Frame}, render};
use serde_json::{json, Value};

const W: u32 = 320;
const H: u32 = 180;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn session(v10: bool) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 30, "render_quality": if v10 { "v10" } else { "legacy" }}));
    s
}
fn track(s: &mut Session) -> String { c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }
fn solid(s: &mut Session, t: &str, rgb: [f64; 3], start: f64, dur: f64) -> String {
    c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": rgb, "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
}
fn frame(s: &Session, t: f64) -> Vec<u8> { render::export_frame_rgba8(&s.project, t, &render::Cache::new()).unwrap() }
fn px(f: &[u8], x: u32, y: u32) -> u8 { f[((y * W + x) * 4) as usize] }
/// Pixels in row `y` that are neither (near) black nor (near) white: the width of an edge or streak.
fn partial_in_row(f: &[u8], y: u32) -> usize { (0..W).filter(|&x| { let v = px(f, x, y); v > 12 && v < 243 }).count() }

/// Black (bottom) crossfading to white (top) over 1 s, linear ease: the midpoint has opacity 0.5 exactly.
fn transition(v10: bool, kind: &str, bottom: [f64; 3], top: [f64; 3]) -> Session {
    let mut s = session(v10);
    let (b, t) = (track(&mut s), track(&mut s));
    solid(&mut s, &b, bottom, 0.0, 2.0);
    let top_id = solid(&mut s, &t, top, 0.0, 2.0);
    c(&mut s, "add_transition", json!({"clip_id": top_id, "kind": kind, "duration": 1.0, "ease": "linear"}));
    s
}

#[test]
fn a_project_says_which_look_it_renders_with_and_old_files_are_legacy() {
    let s = session(true);
    assert_eq!(serde_json::to_value(&s.project).unwrap()["render_quality"], "v10");
    let old: kimo_core::model::Project = serde_json::from_value(json!({"name": "", "width": 64, "height": 64, "fps": 25.0, "tracks": [], "assets": {}, "markers": [], "next_id": 1})).unwrap_or_default();
    assert!(!old.v10(), "a project file without the field (every file saved before v10) renders as legacy");
    assert!(serde_json::to_value(&session(false).project).unwrap().get("render_quality").is_none(), "legacy projects save exactly as before");
}

#[test]
fn a_dissolve_is_blended_in_linear_light() {
    let mid = |v10| px(&frame(&transition(v10, "crossfade", [0.0; 3], [1.0; 3]), 0.5), W / 2, H / 2);
    let (legacy, v10) = (mid(false), mid(true));
    assert!((legacy as i32 - 128).abs() <= 1, "legacy is unchanged: the gamma average 128, got {legacy}");
    assert!((v10 as i32 - 188).abs() <= 3, "v10 blends light: the midpoint of black to white is 188/255, got {v10}");
}

#[test]
fn a_dip_to_black_does_not_sag_through_grey() {
    // white under a white clip dipping to black: at a quarter of the way the dip is half on (smoothstep(0.5))
    let q = |v10| px(&frame(&transition(v10, "dip_black", [1.0; 3], [1.0; 3]), 0.25), W / 2, H / 2);
    let (legacy, v10) = (q(false), q(true));
    assert!((legacy as i32 - 128).abs() <= 2, "legacy dip mixes in gamma: 128, got {legacy}");
    assert!((v10 as i32 - 188).abs() <= 3, "v10 dip mixes light: half of white is 188/255, got {v10}");
}

#[test]
fn a_wipe_has_a_thin_antialiased_edge_instead_of_a_wide_ramp() {
    // 0.47 s: the edge falls mid-pixel (at exactly 0.5 it sits on a pixel boundary, where full/empty coverage is the right answer)
    let edge = |v10| partial_in_row(&frame(&transition(v10, "wipe_left", [0.0; 3], [1.0; 3]), 0.47), H / 2);
    let (legacy, v10) = (edge(false), edge(true));
    assert!(legacy > 15, "legacy wipe edge is softness x frame width (~25 px here), got {legacy}");
    assert!((1..=2).contains(&v10), "v10 wipe edge is antialiased but thin (2 px at 1080p), got {v10} partial pixels");
}

#[test]
fn a_slide_is_motion_blurred_by_its_own_movement() {
    // the white clip slides in from the right over 1 s: W px per second, so a 180-degree shutter at 30 fps smears it W/60 px
    let streak = |v10| partial_in_row(&frame(&transition(v10, "slide_left", [0.0; 3], [1.0; 3]), 0.5), H / 2);
    let (legacy, v10) = (streak(false), streak(true));
    let expect = W as f64 / 60.0;
    assert!(legacy <= 1, "legacy slide has a hard edge, got {legacy}");
    assert!((v10 as f64 - expect).abs() <= expect * 0.5 + 1.0, "v10 streak should be about {expect:.1} px (shutter x speed), got {v10}");
}

#[test]
fn blur_is_a_true_gaussian() {
    let sigma = 3.0f32; let n = 61usize; let mut f = Frame::new(n, n); let mid = n / 2;
    let i = (mid * n + mid) * 4; f.d[i] = 1.0; f.d[i + 3] = 1.0;
    effects::gauss_blur(&mut f, sigma, false);
    let row: Vec<f32> = (0..n).map(|x| f.d[(mid * n + x) * 4]).collect();
    let peak = row[mid];
    let rms = ((0..n).map(|x| { let d = x as f32 - mid as f32; let g = (-(d * d) / (2.0 * sigma * sigma)).exp(); (row[x] / peak - g).powi(2) }).sum::<f32>() / n as f32).sqrt();
    assert!(rms < 0.01, "blur profile vs an analytic Gaussian: RMS error {rms:.4} (bar: < 1%)");
}

/// A white square with the blur effect over a white background: any dark ring is the straight-alpha fringe.
fn blurred_square(v10: bool, radius: f64) -> Vec<u8> {
    let mut s = session(v10);
    let (b, t) = (track(&mut s), track(&mut s));
    solid(&mut s, &b, [1.0; 3], 0.0, 1.0);
    let sq = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 1.0, "width": 80, "height": 80, "fill": [1.0, 1.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "add_effect", json!({"clip_id": sq, "kind": "blur", "params": {"radius": radius}}));
    frame(&s, 0.5)
}

#[test]
fn a_blurred_layer_has_no_dark_fringe() {
    let darkest = |f: &[u8]| (0..W * H).map(|i| f[(i * 4) as usize]).min().unwrap();
    let (legacy, v10) = (darkest(&blurred_square(false, 12.0)), darkest(&blurred_square(true, 12.0)));
    assert!(legacy < 245, "legacy blurs straight alpha: a dark ring appears around the square (darkest {legacy})");
    assert!(v10 >= 253, "v10 blurs premultiplied: white on white stays white (darkest {v10})");
}

#[test]
fn blur_radius_animates_smoothly_between_whole_pixels() {
    let row = |f: Vec<u8>| -> Vec<u8> { (0..W).map(|x| px(&f, x, H / 2)).collect() };
    // over black this time, so the edge profile is visible
    let edge = |v10: bool, r: f64| { let mut s = session(v10); let t = track(&mut s);
        let sq = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 1.0, "width": 80, "height": 80, "fill": [1.0, 1.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
        c(&mut s, "add_effect", json!({"clip_id": sq, "kind": "blur", "params": {"radius": r}})); row(frame(&s, 0.5)) };
    assert_eq!(edge(false, 6.0), edge(false, 6.5), "legacy truncates the radius: 6 and 6.5 look the same");
    assert_ne!(edge(true, 6.0), edge(true, 6.5), "v10 blurs by the exact radius: 6.5 differs from 6");
}

/// Width (in output pixels, to a fraction of a pixel) of the 10%..90% rise across an edge in row `y`.
fn rise_width(f: &Frame, y: usize) -> f32 {
    let row: Vec<f32> = (0..f.w).map(|x| f.d[(y * f.w + x) * 4]).collect();
    let cross = |lvl: f32| (1..row.len()).find(|&x| row[x - 1] < lvl && row[x] >= lvl).map(|x| x as f32 - 1.0 + (lvl - row[x - 1]) / (row[x] - row[x - 1]).max(1e-6)).unwrap();
    cross(0.9) - cross(0.1)
}

#[test]
fn magnified_layers_stay_sharper() {
    // a pixel image with a hard black-to-white edge, enlarged 4x the way the compositor enlarges a layer
    let n = 64; let mut src = Frame::new(n, n);
    for y in 0..n { for x in 0..n { let i = (y * n + x) * 4; let v = if x >= n / 2 { 1.0 } else { 0.0 }; src.d[i] = v; src.d[i + 1] = v; src.d[i + 2] = v; src.d[i + 3] = 1.0; } }
    let up = |v10: bool| { let _q = kimo_core::quality::enter(v10, 1080); rise_width(&render::transform(&src, 4.0, 0.0, 0.0, 0.0), n / 2) };
    let (legacy, v10) = (up(false), up(true));
    assert!(v10 < legacy * 0.9, "v10 (bicubic) edge rise {v10:.2} px should be at least 10% shorter than legacy (bilinear) {legacy:.2} px");
}

#[test]
fn export_dither_moves_every_frame_and_leaves_alpha_alone() {
    let mut f = Frame::new(64, 64); for q in f.d.chunks_exact_mut(4) { q[0] = 0.5; q[1] = 0.25; q[2] = 0.75; q[3] = 0.5; }
    let (a, b) = (render::to_rgba8_dither(&f, 1), render::to_rgba8_dither(&f, 2));
    assert_ne!(a, b, "the dither pattern changes from frame to frame");
    assert!(a.chunks_exact(4).all(|q| q[3] == 128) && b.chunks_exact(4).all(|q| q[3] == 128), "alpha is rounded, never dithered");
    let mean = a.chunks_exact(4).map(|q| q[0] as f64).sum::<f64>() / 4096.0;
    assert!((mean - 127.5).abs() < 0.6, "dither adds no bias: mean {mean:.2}");
}
