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

// ---------------------------------------------------------------- effects

/// A full-frame solid with one effect, rendered in legacy or v10; returns the centre pixel's RGB (0..255).
fn solid_with(v10: bool, rgb: [f64; 3], kind: &str, params: Value) -> [u8; 3] {
    let mut s = session(v10); let t = track(&mut s); let id = solid(&mut s, &t, rgb, 0.0, 1.0);
    c(&mut s, "add_effect", json!({"clip_id": id, "kind": kind, "params": params}));
    let f = frame(&s, 0.5); let i = ((H / 2 * W + W / 2) * 4) as usize; [f[i], f[i + 1], f[i + 2]]
}
fn to_lin(v: u8) -> f64 { let v = v as f64 / 255.0; if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn luminance(c: [u8; 3]) -> f64 { 0.2126 * to_lin(c[0]) + 0.7152 * to_lin(c[1]) + 0.0722 * to_lin(c[2]) }

#[test]
fn exposure_is_measured_in_stops_of_light() {
    let (legacy, v10) = (solid_with(false, [0.5; 3], "exposure", json!({"stops": 1.0}))[0], solid_with(true, [0.5; 3], "exposure", json!({"stops": 1.0}))[0]);
    assert_eq!(legacy, 255, "legacy doubles the encoded value: mid-grey clips to white");
    assert!((to_lin(v10) / to_lin(128) - 2.0).abs() < 0.05, "v10: +1 stop doubles the light (mid-grey 128 -> ~175), got {v10}");
}

#[test]
fn temperature_warms_without_changing_brightness() {
    let grey = [0.5; 3]; let before = luminance([128; 3]);
    let v10 = solid_with(true, grey, "temperature", json!({"amount": 1.0}));
    assert!(v10[0] > v10[2] + 20, "warmer: more red than blue {v10:?}");
    assert!((luminance(v10) / before - 1.0).abs() < 0.02, "v10 keeps the brightness: {:.4} vs {before:.4}", luminance(v10));
}

#[test]
fn saturation_keeps_the_hue() {
    // a skin-like orange, saturation up 1.6x: OkLab hue angle before and after
    let hue = |c: [u8; 3]| { let l = kimo_core::quality::oklab([to_lin(c[0]) as f32, to_lin(c[1]) as f32, to_lin(c[2]) as f32]); l[2].atan2(l[1]).to_degrees() };
    let orange = [0.8, 0.55, 0.4]; let base = hue([204, 140, 102]);
    let v10 = solid_with(true, orange, "saturation", json!({"amount": 1.6}));
    assert!((hue(v10) - base).abs() < 2.0, "v10 hue {:.1} vs original {base:.1}", hue(v10));
    let grey = solid_with(true, orange, "saturation", json!({"amount": 0.0}));
    assert!(grey.iter().max().unwrap() - grey.iter().min().unwrap() <= 1, "saturation 0 is neutral: {grey:?}");
}

#[test]
fn sharpen_adds_no_halos() {
    // a 0.7 grey square on a 0.3 grey background, sharpened hard: any value outside 0.3..0.7 is a halo
    let range = |v10: bool| { let mut s = session(v10); let (b, t) = (track(&mut s), track(&mut s)); solid(&mut s, &b, [0.3; 3], 0.0, 1.0);
        let sq = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 1.0, "width": 100, "height": 100, "fill": [0.7, 0.7, 0.7]}))["clip_id"].as_str().unwrap().to_string();
        c(&mut s, "add_effect", json!({"clip_id": sq, "kind": "sharpen", "params": {"amount": 2.0, "radius": 3.0}}));
        let f = frame(&s, 0.5); let row: Vec<u8> = (0..W).map(|x| px(&f, x, H / 2)).collect(); (*row.iter().min().unwrap(), *row.iter().max().unwrap()) };
    let (lo, hi) = (to_srgb_u8(0.3), to_srgb_u8(0.7));
    let legacy = range(false); let v10 = range(true);
    assert!(legacy.1 > hi + 10, "legacy overshoots into a bright halo {legacy:?}");
    assert!(v10.0 + 2 >= lo && v10.1 <= hi + 2, "v10 stays within the edge's own range {lo}..{hi}: {v10:?}");
}
fn to_srgb_u8(v: f64) -> u8 { (v * 255.0).round() as u8 }

#[test]
fn grain_lives_in_the_mid_tones() {
    let spread = |v10: bool, g: f64| { let mut s = session(v10); let t = track(&mut s); let id = solid(&mut s, &t, [g; 3], 0.0, 1.0);
        c(&mut s, "add_effect", json!({"clip_id": id, "kind": "grain", "params": {"amount": 0.12}}));
        let f = frame(&s, 0.5); let v: Vec<f64> = (0..W * H).map(|i| f[(i * 4) as usize] as f64).collect();
        let m = v.iter().sum::<f64>() / v.len() as f64; (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt() };
    let (mid, dark) = (spread(true, 0.5), spread(true, 0.06));
    assert!(mid > dark * 1.3, "v10 grain is strongest in the mid-tones: mid {mid:.2} vs shadows {dark:.2}");
    let (lmid, ldark) = (spread(false, 0.5), spread(false, 0.06));
    assert!((lmid / ldark.max(1e-6) - 1.0).abs() < 0.6, "legacy grain is the same everywhere: {lmid:.2} vs {ldark:.2}");
}

// ---------------------------------------------------------------- VFX

/// A clip whose picture moves 1 px left and 1 px up every frame (a crop window sliding over a test pattern), with a brightness ramp and
/// fresh noise every frame: the tracked point's true position at frame k is (160 - k, 90 - k).
fn moving_clip(v10: bool) -> (Session, String, usize) {
    let dir = std::env::temp_dir().join(format!("kimo_v10_track_{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("moving.mp4");
    if !path.exists() {
        // a still, fully textured picture (one frame of a random cell pattern), panned by exactly 1 px per frame
        let tex = dir.join("texture.png");
        let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "life=s=640x360:ratio=0.45:death_color=#203040:life_color=#e0c080:seed=7", "-frames:v", "1"]).arg(&tex).status().unwrap().success();
        assert!(ok, "ffmpeg could not make the texture");
        let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-loop", "1", "-framerate", "30", "-i"]).arg(&tex).args([
            "-vf", "crop=320:180:'20+n':'10+n',eq=brightness='0.0015*n':eval=frame,noise=alls=6:allf=t", "-frames:v", "60", "-pix_fmt", "yuv420p", "-c:v", "libx264", "-crf", "8"]).arg(&path).status().unwrap().success();
        assert!(ok, "ffmpeg could not make the test clip");
    }
    let mut s = session(v10); let t = track(&mut s);
    let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (s, id, 60)
}

#[test]
fn a_tracked_point_does_not_drift() {
    let err = |v10: bool| { let (s, id, n) = moving_clip(v10);
        let tr = kimo_core::tracking::track_point(&s.project, &id, 0.5, 0.5, 40, 14, true, 0.12).unwrap();
        let k = (tr.len() - 1).min(n - 1) as f64; let last = tr.last().unwrap();
        (last.x - (160.0 - k)).hypot(last.y - (90.0 - k)) };
    let (legacy, v10) = (err(false), err(true));
    eprintln!("tracking error after 60 frames: legacy {legacy:.2} px, v10 {v10:.2} px");
    assert!(v10 < 0.5, "v10 tracker stays on the point (error {v10:.2} px after 60 frames)");
    assert!(v10 <= legacy + 0.05, "v10 is at least as accurate as legacy ({v10:.2} vs {legacy:.2} px)");
}

#[test]
fn a_keyed_edge_has_no_green_halo() {
    // pixels: pure green screen, skin, and an edge pixel that is half skin, half screen (hair against the screen)
    let skin = [0.85f32, 0.6, 0.5]; let green = [0.1f32, 0.8, 0.15];
    let key = |v10: bool| { let _q = kimo_core::quality::enter(v10, 1080); let mut f = Frame::new(3, 1);
        for (i, c) in [green, skin, [0.5 * skin[0] + 0.5 * green[0], 0.5 * skin[1] + 0.5 * green[1], 0.5 * skin[2] + 0.5 * green[2]]].iter().enumerate() { f.d[i * 4..i * 4 + 3].copy_from_slice(c); f.d[i * 4 + 3] = 1.0; }
        let mut params = effects::Params::new(); for (k, v) in [("r", 0.1), ("g", 0.8), ("b", 0.15), ("tolerance", 0.08), ("softness", 0.25)] { params.insert(k.into(), kimo_core::keyframe::Prop::new(v)); }
        effects::apply("chroma_key", &params, &Default::default(), &mut f, 0.0, 0).unwrap(); f };
    let (legacy, v10) = (key(false), key(true));
    assert!(v10.d[3] < 0.05, "the screen is keyed out (alpha {})", v10.d[3]);
    assert!(v10.d[7] > 0.95 && (0..3).all(|j| (v10.d[4 + j] - skin[j]).abs() < 0.02), "skin is kept as it was: {:?}", &v10.d[4..8]);
    let edge_err = |f: &Frame| (0..3).map(|j| (f.d[8 + j] - skin[j]).abs()).fold(0f32, f32::max);
    eprintln!("edge colour error: legacy {:.3}, v10 {:.3} (alpha {:.2} / {:.2})", edge_err(&legacy), edge_err(&v10), legacy.d[11], v10.d[11]);
    assert!(v10.d[11] > 0.05 && v10.d[11] < 0.95, "the edge pixel is partly transparent ({})", v10.d[11]);
    // the visible defect is a green tint on the edge: green above the average of red and blue
    let cast = |f: &Frame| f.d[9] - 0.5 * (f.d[8] + f.d[10]);
    assert!(cast(&v10) <= 0.02, "v10 edge has no green cast ({:.3})", cast(&v10));
    assert!(cast(&legacy) > 0.05, "legacy edge is tinted green ({:.3})", cast(&legacy));
    // and no over-correction: removing too much screen colour turns an edge magenta (green below both red and blue)
    assert!(v10.d[9] >= v10.d[8].min(v10.d[10]) - 0.02, "v10 edge is not magenta: {:?}", &v10.d[8..11]);
    assert!(edge_err(&v10) < edge_err(&legacy), "and does better than legacy ({:.3} vs {:.3})", edge_err(&v10), edge_err(&legacy));
}
