//! Camera Lens Blur with a depth (blur) map, After Effects style: add_lens_blur takes depth_clip_id, another clip of the
//! project read per pixel (channel luma by default). The blur at a pixel grows with how far the map's value there is from
//! focal_depth (1 = white is in focus, the default, so a white subject matte keeps the subject sharp); invert flips the map.
//! The map clip is hidden from the picture by default. Plate: a 4 px black/white checker; map: left half white, right half black.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 120;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn png(tag: &str, px: impl Fn(usize, usize) -> u8) -> std::path::PathBuf {
    use std::io::Write;
    let p = std::env::temp_dir().join(format!("kimo-lensdepth-{tag}-{}.ppm", std::process::id()));
    let mut b = vec![0u8; W * H * 3]; for y in 0..H { for x in 0..W { let v = px(x, y); for j in 0..3 { b[(y * W + x) * 3 + j] = v; } } }
    let mut f = std::fs::File::create(&p).unwrap(); write!(f, "P6\n{W} {H}\n255\n").unwrap(); f.write_all(&b).unwrap();
    let out = p.with_extension("png"); assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i", p.to_str().unwrap(), out.to_str().unwrap()]).stdin(std::process::Stdio::null()).status().unwrap().success());
    let _ = std::fs::remove_file(&p); out
}
/// (session, plate clip id, map clip id); the map sits on the track above the plate, same span.
fn session(map: impl Fn(usize, usize) -> u8) -> (Session, Value, Value) {
    let plate = png("plate", |x, y| if (x / 4 + y / 4) % 2 == 0 { 230 } else { 20 });
    let mapf = png("map", map);
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let a = c(&mut s, "import_media", json!({"path": plate.to_str().unwrap()}))["asset_id"].clone();
    let b = c(&mut s, "import_media", json!({"path": mapf.to_str().unwrap()}))["asset_id"].clone();
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t0, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0}))["clip_id"].clone();
    let m = c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": b, "start": 0, "duration": 1.0}))["clip_id"].clone();
    (s, id, m)
}
fn frame(s: &Session) -> Vec<f32> { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(0.5)).unwrap(); render::render_frame(&s.project, 0.5, &cache).unwrap().d }
/// mean |difference| between horizontal neighbours (red channel) over columns x0..x1, rows 20..100: a 4 px checker is ~0.2 (an edge every 4 px), a blur ~0
fn contrast(f: &[f32], x0: usize, x1: usize) -> f32 { let mut a = 0f32; let mut n = 0f32; for y in 20..100 { for x in x0..x1 - 1 { a += (f[(y * W + x) * 4] - f[(y * W + x + 1) * 4]).abs(); n += 1.0; } } a / n }
fn region_equal(a: &[f32], b: &[f32], x0: usize, x1: usize) -> usize { let mut bad = 0; for y in 0..H { for x in x0..x1 { let i = (y * W + x) * 4; if a[i..i + 4] != b[i..i + 4] { bad += 1; } } } bad }
fn half(x: usize, _y: usize) -> u8 { if x < W / 2 { 255 } else { 0 } }
fn source_frame() -> Vec<f32> { let (mut s, _, m) = session(half); c(&mut s, "delete_clip", json!({"clip_id": m})); frame(&s) }

#[test]
fn depth_map_keeps_the_white_side_sharp_and_blurs_the_black_side() {
    let src = source_frame(); let (sl, sr) = (contrast(&src, 10, 64), contrast(&src, 96, 150));
    assert!(sl > 0.15 && sr > 0.15, "the plate is a hard checker: {sl} {sr}");
    let (mut s, id, m) = session(half);
    let r = c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 8, "blades": 0, "depth_clip_id": m}));
    assert_eq!(r["depth"]["clip_id"], m, "result names the depth map: {r}");
    let f = frame(&s);
    // focal_depth defaults to 1 (white in focus): the left (white) half, away from the seam, is the untouched plate, bit for bit,
    // which also proves the map clip itself is not drawn over the picture
    assert_eq!(region_equal(&f, &src, 0, 64), 0, "white side of the map must stay exactly sharp (and the map hidden)");
    let (l, rr) = (contrast(&f, 10, 64), contrast(&f, 96, 150));
    assert!(rr < 0.2 * sr, "black side (far from focus) must be defocused: contrast {rr} vs source {sr}");
    assert!((l - sl).abs() < 1e-6, "white side contrast unchanged: {l} vs {sl}");
}

#[test]
fn focal_depth_and_invert_choose_what_is_in_focus() {
    let src = source_frame(); let sr = contrast(&src, 90, 150);
    let (mut s, id, m) = session(half);
    c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 8, "blades": 0, "depth_clip_id": m, "focal_depth": 0.0}));
    let f = frame(&s);
    assert_eq!(region_equal(&f, &src, 96, W), 0, "focal_depth 0: the black side is in focus, exactly");
    assert!(contrast(&f, 10, 64) < 0.2 * sr, "focal_depth 0: the white side is defocused");
    c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 8, "blades": 0, "depth_clip_id": m, "invert": true}));
    let g = frame(&s);
    assert_eq!(region_equal(&g, &src, 96, W), 0, "invert: white-in-focus on the inverted map is the black side");
    assert!(contrast(&g, 10, 64) < 0.2 * sr, "invert: the white side is defocused");
}

#[test]
fn blur_grows_with_distance_from_the_focal_depth() {
    // a horizontal depth ramp: map value = x / W; focal_depth 0 -> blur grows left to right
    let src = source_frame();
    let (mut s, id, m) = session(|x, _| ((x as f32 / (W - 1) as f32) * 255.0).round() as u8);
    c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 10, "blades": 0, "depth_clip_id": m, "focal_depth": 0.0}));
    let f = frame(&s);
    let bands: Vec<f32> = [(4, 30), (50, 76), (100, 126), (130, 156)].iter().map(|&(a, b)| contrast(&f, a, b)).collect();
    for w in bands.windows(2) { assert!(w[1] < w[0], "contrast must fall as depth leaves focus: {bands:?}"); }
    assert!(bands[0] > 0.6 * contrast(&src, 4, 30), "near the focal depth the picture stays mostly sharp: {bands:?}");
}

#[test]
fn no_depth_map_is_the_plain_lens_blur_and_refusals_name_the_problem() {
    let (mut s, id, m) = session(half);
    c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 8, "blades": 0, "depth_clip_id": m}));
    // calling again without a map replaces the effect: an even blur everywhere, and the map clip shows again (it is no longer a map)
    c(&mut s, "add_lens_blur", json!({"clip_id": id, "radius": 8, "blades": 0}));
    let ef = s.project.find_clip(id.as_str().unwrap()).unwrap().effects.iter().find(|e| e.kind == "lens_blur").unwrap().clone();
    assert!(ef.strings.is_empty(), "no map strings left: {:?}", ef.strings);
    for (bad, want) in [(json!({"clip_id": id, "depth_clip_id": id}), "itself"), (json!({"clip_id": id, "depth_clip_id": "nope"}), "nope"),
        (json!({"clip_id": id, "depth_clip_id": m, "focal_depth": 1.5}), "focal_depth"), (json!({"clip_id": id, "depth_clip_id": m, "depth_channel": "purple"}), "depth_channel"),
        (json!({"clip_id": id, "focal_depth": 0.5}), "depth_clip_id")] {
        let e = dispatch(&mut s, "add_lens_blur", &bad).unwrap_err(); assert!(e.contains(want), "{bad} -> {e}");
    }
}
