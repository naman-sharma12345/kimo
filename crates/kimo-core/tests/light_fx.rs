//! light_sweep (After Effects CC Light Sweep), read off flat synthetic frames (160x90 @ 10 fps):
//! a light band's gain is the frame's change from its plain value, so where the band sits, how wide it is, its colour and how
//! it treats a layer's transparent part are read directly.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn session() -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10})); let t = id(&c(&mut s, "add_track", json!({})), "track_id"); (s, t) }
fn solid(s: &mut Session, t: &str, col: [f64; 3]) -> String { id(&c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": 0, "duration": 2.0})), "clip_id") }
/// An image clip: left half opaque grey 128, right half fully transparent (a layer with an edge, like text or a sticker).
fn half_layer(s: &mut Session, t: &str) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-light-{}-half.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 4); for _y in 0..H { for x in 0..W { if x < W / 2 { o.extend_from_slice(&[128, 128, 128, 255]); } else { o.extend_from_slice(&[0, 0, 0, 0]); } } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success());
    let a = id(&c(s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    id(&c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2.0})), "clip_id")
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 3] { let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] }
fn near(got: f32, want: f32, tol: f32, what: &str) { assert!((got - want).abs() <= tol, "{what}: want {want}, got {got:.4}"); }
fn sweep(s: &mut Session, clip: &str, params: Value) -> Value { c(s, "add_effect", json!({"clip_id": clip, "kind": "light_sweep", "params": params})) }

#[test]
fn a_light_sweep_band_brightens_where_it_is_and_nowhere_else() {
    let (mut s, t) = session(); let a = solid(&mut s, &t, [0.5, 0.5, 0.5]);
    let r = dispatch(&mut s, "add_effect", &json!({"clip_id": a, "kind": "light_sweep", "params": {"position": 0.5}}));
    assert!(r.is_ok(), "light_sweep must be an effect: {}", r.unwrap_err());
    let e = id(&r.unwrap(), "effect_id"); c(&mut s, "remove_effect", json!({"clip_id": a, "effect_id": e}));
    // vertical band (angle 0) half way across, 20 % of the 160 px travel wide (32 px, half width 16), linear profile, +0.3 at its centre
    let e = id(&sweep(&mut s, &a, json!({"position": 0.5, "angle": 0, "width": 0.2, "intensity": 0.3, "shape": 0})), "effect_id");
    let f = frame(&s, 0.5);
    near(px(&f, 80, 45)[0], 0.5 + 0.3 * (1.0 - 0.5 / 16.0), 0.02, "band centre (x 80)");
    near(px(&f, 88, 45)[0], 0.5 + 0.3 * (1.0 - 8.5 / 16.0), 0.02, "half way out of the band (x 88)");
    near(px(&f, 100, 45)[0], 0.5, 0.01, "outside the band (x 100)"); near(px(&f, 40, 45)[0], 0.5, 0.01, "outside the band (x 40)");
    near(px(&f, 80, 5)[0], px(&f, 80, 85)[0], 0.005, "a vertical band is the same top to bottom");
    // position 0 and 1 park the band just off the frame: nothing changes
    for pos in [0.0, 1.0] { c(&mut s, "set_effect_param", json!({"clip_id": a, "effect_id": e, "param": "position", "value": pos}));
        let f = frame(&s, 0.5); let worst = (0..W).map(|x| (px(&f, x, 45)[0] - 0.5).abs()).fold(0f32, f32::max); assert!(worst < 0.005, "position {pos}: band must be off frame, largest change {worst}"); }
    // angle 90: a horizontal band moving top to bottom, centred on row 45
    c(&mut s, "set_effect_param", json!({"clip_id": a, "effect_id": e, "param": "position", "value": 0.5})); c(&mut s, "set_effect_param", json!({"clip_id": a, "effect_id": e, "param": "angle", "value": 90.0}));
    let f = frame(&s, 0.5); assert!(px(&f, 10, 45)[0] > 0.7 && (px(&f, 10, 5)[0] - 0.5).abs() < 0.01, "angle 90: row 45 lit {:?}, row 5 plain {:?}", px(&f, 10, 45), px(&f, 10, 5));
}

#[test]
fn the_sweep_animates_by_speed_takes_a_colour_and_keyframes() {
    let (mut s, t) = session(); let a = solid(&mut s, &t, [0.2, 0.2, 0.2]);
    // speed 0.5 sweeps per second from position 0: at 1 s it is half way (x 80), at 0.5 s a quarter of the way
    let e = id(&sweep(&mut s, &a, json!({"position": 0.0, "speed": 0.5, "width": 0.2, "intensity": 0.4, "shape": 0, "color_r": 1.0, "color_g": 0.5, "color_b": 0.0})), "effect_id");
    let f = frame(&s, 1.0); let p = px(&f, 80, 45);
    assert!(p[0] - 0.2 > 0.35, "speed 0.5 at 1 s: the band is on x 80: {p:?}");
    near((p[1] - 0.2) / (p[0] - 0.2), 0.5, 0.03, "light colour (1, 0.5, 0): green gain is half the red gain"); near(p[2], 0.2, 0.01, "no blue light");
    let f = frame(&s, 0.5); let lit: Vec<usize> = (0..W).filter(|&x| px(&f, x, 45)[0] > 0.25).collect();
    assert!(!lit.is_empty() && lit.iter().all(|&x| x < 70), "at 0.5 s the band is left of centre: lit columns {lit:?}");
    // position keyframes like any effect param
    c(&mut s, "set_effect_param", json!({"clip_id": a, "effect_id": e, "param": "speed", "value": 0.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": e, "property": "position", "time": 0.0, "value": 0.0})); c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": e, "property": "position", "time": 1.0, "value": 1.0}));
    let f = frame(&s, 0.5); assert!(px(&f, 80, 45)[0] > 0.5, "keyed position 0 -> 1 is in the middle at 0.5 s: {:?}", px(&f, 80, 45));
}

#[test]
fn light_reception_add_composite_cutout_and_edge_light_on_a_layer() {
    // band centred on the layer's edge (x 80), half width 16, +0.5 at the centre
    let base = json!({"position": 0.5, "width": 0.2, "intensity": 0.5, "shape": 0});
    let with = |extra: Value| { let mut b = base.clone(); for (k, v) in extra.as_object().unwrap() { b[k] = v.clone(); } b };
    let (mut s, t) = session(); let a = half_layer(&mut s, &t); let plain = frame(&s, 0.5); near(px(&plain, 70, 45)[0], 0.502, 0.01, "layer grey"); near(px(&plain, 85, 45)[0], 0.0, 0.005, "transparent half over black");
    sweep(&mut s, &a, with(json!({"reception": 0})));
    let f = frame(&s, 0.5); assert!(px(&f, 70, 45)[0] > 0.65, "add: light on the layer: {:?}", px(&f, 70, 45)); near(px(&f, 85, 45)[0], 0.0, 0.01, "add: no light where the layer is transparent");
    let (mut s, t) = session(); let a = half_layer(&mut s, &t); sweep(&mut s, &a, with(json!({"reception": 1})));
    let f = frame(&s, 0.5); assert!(px(&f, 85, 45)[0] > 0.25, "composite: the light shows past the layer's edge too: {:?}", px(&f, 85, 45));
    let (mut s, t) = session(); let a = half_layer(&mut s, &t); sweep(&mut s, &a, with(json!({"reception": 2})));
    let f = frame(&s, 0.5); near(px(&f, 70, 45)[0], 0.5 * (1.0 - 9.5 / 16.0), 0.03, "cutout: only the light, inside the layer"); near(px(&f, 40, 45)[0], 0.0, 0.01, "cutout: the layer outside the band is gone"); near(px(&f, 85, 45)[0], 0.0, 0.01, "cutout: nothing outside the layer");
    // edge light: only the layer's edge inside the band lights up (text / sticker shine)
    let (mut s, t) = session(); let a = half_layer(&mut s, &t); sweep(&mut s, &a, with(json!({"intensity": 0.0, "edge_intensity": 1.0, "edge_width": 3.0})));
    let f = frame(&s, 0.5); assert!(px(&f, 78, 45)[0] > 0.9, "edge pixel inside the band lights up: {:?}", px(&f, 78, 45)); near(px(&f, 70, 45)[0], 0.502, 0.01, "the layer's inside is untouched with intensity 0");
    assert!(dispatch(&mut s, "add_effect", &json!({"clip_id": a, "kind": "light_sweep", "params": {"glare": 1.0}})).unwrap_err().contains("unknown param"));
}
