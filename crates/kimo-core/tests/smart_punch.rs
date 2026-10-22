//! add_smart_punch: automatic subject-follow punch-in on a synthetic still (160x90 @ 10 fps, one 2 s clip): a dark frame with
//! one bright square in the upper right, so the saliency centroid is pulled well away from the canvas centre.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn png(name: &str, f: impl Fn(usize, usize) -> [u8; 3]) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-smartpunch-{}-{name}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 3); for y in 0..H { for x in 0..W { o.extend_from_slice(&f(x, y)); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
/// One 2 s clip of a dark frame with a bright 20x20 square centred at (120, 30).
fn scene() -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pa = png("spot", |x, y| if (110..130).contains(&x) && (20..40).contains(&y) { [240, 230, 200] } else { [20, 20, 30] });
    let aa = id(&c(&mut s, "import_media", json!({"path": pa})), "asset_id");
    let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let cl = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aa, "start": 0.0, "duration": 2.0})), "clip_id");
    (s, cl)
}

#[test]
fn smart_punch_adds_scale_and_position_keys() {
    let (mut s, cl) = scene();
    let r = c(&mut s, "add_smart_punch", json!({"clip_id": cl, "zoom": 1.4, "samples": 4}));
    let cents = r["centroids"].as_array().expect("centroids"); assert_eq!(cents.len(), 4, "{r}");
    let scales = r["scales"].as_array().expect("scales"); assert_eq!(scales.len(), 4, "{r}");
    assert!((scales[0].as_f64().unwrap() - 1.0).abs() < 1e-9, "starts wide: {r}");
    assert!((scales[3].as_f64().unwrap() - 1.4).abs() < 0.05, "ends at zoom: {r}");
    for p in cents { let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()); assert!(x > W as f64 / 2.0 && y < H as f64 / 2.0, "the bright square (upper right) pulls the focus: {r}"); }
    let clip = s.project.find_clip(&cl).unwrap();
    let sc = clip.props.get("scale").unwrap(); assert!(sc.keys.len() >= 4, "scale keys: {}", sc.keys.len());
    assert!((sc.at(clip.duration) - 1.4).abs() < 0.05, "scale at the clip end: {}", sc.at(clip.duration));
    assert!((sc.at(0.0) - 1.0).abs() < 1e-9, "scale at the clip start: {}", sc.at(0.0));
    for k in ["x", "y"] { assert!(clip.props.get(k).unwrap().keys.len() >= 4, "{k} keys"); }
    // zooming about a point right of / above the centre moves the picture left / down
    assert!(clip.props.get("x").unwrap().at(clip.duration) < 0.0 && clip.props.get("y").unwrap().at(clip.duration) > 0.0, "position follows the focus");
    let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(1.9)).unwrap(); render::render_frame(&s.project, 1.9, &cache).unwrap();
}

#[test]
fn smart_punch_rejects_unknown_clip() {
    let (mut s, _cl) = scene();
    let e = dispatch(&mut s, "add_smart_punch", &json!({"clip_id": "nope"})).unwrap_err(); assert!(e.contains("no such clip"), "{e}");
}

#[test]
fn smart_punch_rejects_bad_zoom() {
    let (mut s, cl) = scene();
    assert!(dispatch(&mut s, "add_smart_punch", &json!({"clip_id": cl, "zoom": 10.0})).is_err());
    assert!(dispatch(&mut s, "add_smart_punch", &json!({"clip_id": cl, "samples": 1})).is_err());
    assert!(dispatch(&mut s, "add_smart_punch", &json!({"clip_id": cl, "bogus": 1})).is_err());
}
