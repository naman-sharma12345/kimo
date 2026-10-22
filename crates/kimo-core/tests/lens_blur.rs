//! lens_blur (After Effects Camera Lens Blur) via add_lens_blur: bright points open into the iris shape (a hexagon for
//! 6 blades, a disc for 0), evenly lit and bright because highlights are boosted in linear light; a box/Gaussian blur of the
//! same radius smears them dim. A focus area keeps its pixels exactly sharp. Large radii (work-resolution path) keep the shape.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 120;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// A dark plate (sRGB 15/255) with a 3x3 white point at (80, 60) and a textured stripe along the top 16 rows.
fn plate() -> std::path::PathBuf {
    use std::io::Write;
    let p = std::env::temp_dir().join(format!("kimo-lens-{}.ppm", std::process::id()));
    let mut px = vec![15u8; W * H * 3];
    for y in 0..16 { for x in 0..W { let v = if (x / 2 + y / 2) % 2 == 0 { 220 } else { 30 }; for j in 0..3 { px[(y * W + x) * 3 + j] = v; } } }
    for y in 59..62 { for x in 79..82 { for j in 0..3 { px[(y * W + x) * 3 + j] = 255; } } }
    let mut f = std::fs::File::create(&p).unwrap(); write!(f, "P6\n{W} {H}\n255\n").unwrap(); f.write_all(&px).unwrap();
    let png = p.with_extension("png"); assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i", p.to_str().unwrap(), png.to_str().unwrap()]).stdin(std::process::Stdio::null()).status().unwrap().success()); png
}
fn session() -> (Session, Value) {
    let path = plate(); let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0}))["clip_id"].clone();
    (s, id)
}
fn frame(s: &Session) -> Vec<f32> { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(0.5)).unwrap();
    render::render_frame(&s.project, 0.5, &cache).unwrap_or_else(|e| panic!("a clip with add_lens_blur must render, got: {e}")).d }
/// tool "" = the untouched source, "lens" = add_lens_blur with args, "blur" = the plain blur effect for comparison.
fn scene(tool: &str, args: Value, tag: &str) -> Vec<f32> {
    let (mut s, id) = session();
    match tool { "lens" => { let mut a = args.clone(); a["clip_id"] = id.clone(); c(&mut s, "add_lens_blur", a); }
        "blur" => { c(&mut s, "add_effect", json!({"clip_id": id, "kind": "blur", "params": args})); } _ => {} }
    let f = frame(&s);
    if let Ok(d) = std::env::var("KIMO_PROOF_DIR") { save(&d, &format!("lens_{tag}"), &f); }
    f
}
fn save(d: &str, name: &str, px: &[f32]) { use std::io::Write; std::fs::create_dir_all(d).unwrap(); let b: Vec<u8> = px.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{W}x{H}"), "-i", "-", "-vf", "scale=480:360:flags=neighbor", "-frames:v", "1", &format!("{d}/{name}.png")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    ch.stdin.take().unwrap().write_all(&b).unwrap(); assert!(ch.wait().unwrap().success()); }
fn lum(f: &[f32], x: f64, y: f64) -> f32 { let (x, y) = (x.round() as usize, y.round() as usize); let i = (y * W + x) * 4; 0.2126 * f[i] + 0.7152 * f[i + 1] + 0.0722 * f[i + 2] }
/// Levels above the ground at k*r from the point: (straight down = a 6-blade corner, the two lower flats at 60/120 deg, the
/// horizontal at 0/180 deg). Only the lower half is read, away from the textured stripe at the top.
fn shape_at(f: &[f32], r: f64, k: f64) -> (f32, f32, f32) { let bg = 15.0 / 255.0;
    let at = |deg: f64| lum(f, 80.0 + k * r * deg.to_radians().cos(), 60.0 + k * r * deg.to_radians().sin()) - bg;
    (at(90.0), (at(60.0) + at(120.0)) / 2.0, (at(0.0) + at(180.0)) / 2.0) }

#[test]
fn a_bright_point_opens_into_the_iris_shape() {
    let r = 20.0;
    let hex = scene("lens", json!({"radius": r, "blades": 6, "highlight_gain": 0.0}), "hex6");
    let disc = scene("lens", json!({"radius": r, "blades": 0, "highlight_gain": 0.0}), "disc");
    let (hc, hf, _) = shape_at(&hex, r, 0.93);
    assert!(hc > 4.0 * hf.max(1e-4) && hc > 0.01, "6 blades must give a hexagon: at 0.93 r the corner direction reads {hc:.4} above the ground, the flat direction {hf:.4}");
    let (dc, _, df) = shape_at(&disc, r, 0.93);
    assert!(dc > 0.01 && df > 0.01 && (dc / df) > 0.7 && (dc / df) < 1.4, "0 blades must give a round disc: {dc:.4} vs {df:.4} at 0.93 r");
    // evenly lit: inside the iris the level is flat, not a Gaussian hill
    let ring: Vec<f32> = [0.2, 0.4, 0.6].iter().map(|k| lum(&disc, 80.0 + k * r, 60.0)).collect();
    assert!((ring[0] - ring[2]).abs() < 0.25 * ring[0], "the disc must be evenly lit: {ring:?} at 0.2/0.4/0.6 r");
}

#[test]
fn a_large_radius_keeps_the_iris_shape() {
    let r = 40.0; // work-resolution path (reduced by 3)
    let hex = scene("lens", json!({"radius": r, "blades": 6, "highlight_gain": 0.0}), "hex6_r40");
    let (hc, hf, _) = shape_at(&hex, r, 0.95);
    assert!(hc > 4.0 * hf.max(1e-4) && hc > 0.005, "at radius 40 the 6-blade iris must still be a hexagon: corner {hc:.5} vs flat {hf:.5} at 0.95 r");
}

#[test]
fn highlights_bloom_where_a_plain_blur_smears_them_dim() {
    let lens = scene("lens", json!({"radius": 10.0, "blades": 0, "highlight_gain": 4.0, "highlight_threshold": 0.8}), "bloom");
    let blur = scene("blur", json!({"radius": 10.0}), "plain_blur");
    let (l, b) = (lum(&lens, 84.0, 60.0), lum(&blur, 84.0, 60.0));
    assert!(l > 0.25 && l > 2.5 * b, "a boosted highlight must stay a bright bokeh disc: lens {l:.3} vs plain blur {b:.3} at 4 px from the point");
}

#[test]
fn the_focus_area_stays_exactly_sharp() {
    let src = scene("", json!({}), "source");
    let lens = scene("lens", json!({"radius": 8.0, "blades": 0, "focus_radius": 0.25, "falloff": 0.1, "focus_x": 0.5, "focus_y": 0.5}), "focus");
    let d_centre = (55..65).flat_map(|y| (75..85).map(move |x| (y, x))).map(|(y, x)| (lens[(y * W + x) * 4] - src[(y * W + x) * 4]).abs()).fold(0.0f32, f32::max);
    assert!(d_centre < 1e-5, "inside the focus radius the frame must be untouched, max diff {d_centre}");
    let grad = |f: &[f32]| -> f32 { (0..12).flat_map(|y| (1..W).map(move |x| (y, x))).map(|(y, x)| (f[(y * W + x) * 4] - f[(y * W + x - 1) * 4]).abs()).sum() };
    let (gs, gl) = (grad(&src), grad(&lens));
    assert!(gl < 0.3 * gs, "outside the focus area the stripe must be defocused: edge energy {gl:.1} vs {gs:.1} sharp");
}

#[test]
fn the_tool_refuses_bad_input_and_updates_in_place() {
    let (mut s, id) = session();
    let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id": id, "blades": 2}), "blades"), (json!({"clip_id": id, "radius": 0}), "radius"), (json!({"clip_id": id, "iris": 6}), "unknown argument 'iris'"),
        (json!({"clip_id": id, "focus_x": 0.3}), "focus_radius"), (json!({"clip_id": "nope"}), "no such clip"), (json!({}), "clip_id")] {
        let e = dispatch(&mut s, "add_lens_blur", &a).expect_err(&format!("{a} must be refused"));
        assert!(e.contains(want), "{a}: error should mention {want}, got: {e}");
    }
    assert_eq!(before, serde_json::to_string(&s.project).unwrap(), "refused calls must not change the project");
    let r1 = c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 6}));
    let r2 = c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 9, "blades": 0}));
    assert_eq!(r1["effect_id"], r2["effect_id"]); assert_eq!(r2["updated"], json!(true));
    let n = |s: &Session| s.project.find_clip(id.as_str().unwrap()).unwrap().effects.iter().filter(|e| e.kind == "lens_blur").count();
    assert_eq!(n(&s), 1, "a second call updates the one lens blur instead of stacking another");
    assert_eq!(c(&mut s, "add_lens_blur", json!({"clip_id": id, "remove": true}))["removed"], json!(1)); assert_eq!(n(&s), 0);
}

/// A full-frame 6 px checker (sRGB 220/30) as a clip, with add_lens_blur args (empty = untouched source).
fn checker_scene(args: Value) -> Vec<f32> {
    use std::io::Write;
    let p = std::env::temp_dir().join(format!("kimo-lens-checker-{}.ppm", std::process::id()));
    let mut px = vec![0u8; W * H * 3]; for y in 0..H { for x in 0..W { let v = if (x / 6 + y / 6) % 2 == 0 { 220 } else { 30 }; for j in 0..3 { px[(y * W + x) * 3 + j] = v; } } }
    let mut f = std::fs::File::create(&p).unwrap(); write!(f, "P6\n{W} {H}\n255\n").unwrap(); f.write_all(&px).unwrap();
    let png = p.with_extension("png"); assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i", p.to_str().unwrap(), png.to_str().unwrap()]).stdin(std::process::Stdio::null()).status().unwrap().success());
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let a = c(&mut s, "import_media", json!({"path": png.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0}))["clip_id"].clone();
    if args.as_object().is_some_and(|o| !o.is_empty()) { let mut a = args.clone(); a["clip_id"] = id; c(&mut s, "add_lens_blur", a); }
    let f = frame(&s); if let Ok(d) = std::env::var("KIMO_PROOF_DIR") { save(&d, &format!("lens_checker_{}", if args.as_object().is_some_and(|o| !o.is_empty()) { "focus" } else { "source" }), &f); } f
}

#[test]
fn focus_is_source_sharp_and_falls_off_smoothly() {
    // focus at the centre: sharp out to 0.15 of the short side (18 px), full blur from 0.40 (48 px)
    let src = checker_scene(json!({}));
    let lens = checker_scene(json!({"radius": 8.0, "blades": 0, "highlight_gain": 0.0, "focus_x": 0.5, "focus_y": 0.5, "focus_radius": 0.15, "falloff": 0.25}));
    let ring = |f: &[f32], lo: f64, hi: f64| -> f64 { let (mut e, mut n) = (0.0, 0.0); for y in 0..H { for x in 1..W { let d = ((x as f64 + 0.5 - 80.0).powi(2) + (y as f64 + 0.5 - 60.0).powi(2)).sqrt();
        if d >= lo && d < hi { e += (f[(y * W + x) * 4] - f[(y * W + x - 1) * 4]).abs() as f64; n += 1.0; } } } e / n };
    let rings = [(0.0, 16.0), (22.0, 26.0), (30.0, 34.0), (38.0, 42.0), (52.0, 58.0)];
    let k: Vec<f64> = rings.iter().map(|&(lo, hi)| ring(&lens, lo, hi) / ring(&src, lo, hi)).collect();
    let inside = (0..W * H).filter(|&i| { let (x, y) = ((i % W) as f64 + 0.5 - 80.0, (i / W) as f64 + 0.5 - 60.0); (x * x + y * y).sqrt() < 17.0 }).map(|i| (0..3).map(|j| (lens[i * 4 + j] - src[i * 4 + j]).abs()).fold(0.0f32, f32::max)).fold(0.0f32, f32::max);
    assert!(inside < 1e-6, "inside the focus radius pixels must equal the source, max diff {inside}");
    assert!(k.windows(2).all(|p| p[1] < p[0]), "sharpness must fall off monotonically with distance from the focus point: {k:.3?}");
    assert!(k[1] > 0.6 && k[2] > 0.15 && k[2] < 0.6 && k[4] < 0.15, "just past the focus edge it is still nearly sharp, mid-ramp partly soft, outside fully defocused: {k:.3?}");
}
