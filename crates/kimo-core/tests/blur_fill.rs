//! add_blur_fill: a landscape red ramp (160x90) on a portrait 90x160 canvas. Without a fill the bars above and below the
//! letterboxed clip are black; with one they show a blurred, darker, cover-scaled copy of the same picture, while the clip
//! itself stays sharp in the middle.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 90; const H: usize = 160;
fn png(name: &str) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-bf-{}-{name}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "160x90", "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = vec![]; for _y in 0..90 { for x in 0..160usize { o.extend_from_slice(&[(x * 255 / 159) as u8, if (x / 8) % 2 == 0 { 200 } else { 0 }, 40]); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn scene() -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let aa = id(&c(&mut s, "import_media", json!({"path": png("a")})), "asset_id"); let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let v = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aa, "start": 0, "duration": 2.0})), "clip_id");
    (s, v)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 3] { let i = (y * f.w + x) * 4; [f.d[i] * 255.0, f.d[i + 1] * 255.0, f.d[i + 2] * 255.0] }
fn pj(s: &Session) -> String { serde_json::to_string(&s.project).unwrap() }

#[test]
fn the_bars_show_a_blurred_darker_copy_and_the_clip_stays_sharp() {
    let (mut s, v) = scene();
    let plain = frame(&s, 0.5); assert!(px(&plain, 45, 10)[0] < 1.0 && px(&plain, 45, 150)[0] < 1.0, "letterboxed: black bars {:?}", px(&plain, 45, 10));
    let ntracks = s.project.tracks.len();
    let r = c(&mut s, "add_blur_fill", json!({"clip_id": v, "dim": 0.0, "blur": 6}));
    assert_eq!(r["created"], true, "{r}"); assert_eq!(s.project.tracks.len(), ntracks + 1, "the fill gets its own track");
    let f0 = frame(&s, 0.5); let top0 = px(&f0, 45, 10);
    assert!(top0[0] > 60.0, "the top bar shows the picture (centre columns of the ramp, red ~128): {top0:?}");
    assert!(top0[1] > 40.0 && top0[1] < 160.0, "blurred: the 8 px green stripes are averaged (not 0 or 200): {top0:?}");
    for x in [10, 45, 80] { assert_eq!(px(&f0, x, 80), px(&plain, x, 80), "the clip itself is unchanged in the middle (column {x})"); }
    c(&mut s, "add_blur_fill", json!({"clip_id": v, "dim": 1.0, "blur": 6})); assert_eq!(s.project.tracks.len(), ntracks + 1, "a second call updates the one fill");
    let top1 = px(&frame(&s, 0.5), 45, 10); let ratio = top1[0] / top0[0]; assert!(ratio > 0.3 && ratio < 0.8, "dim 1 stop darkens the fill: {top0:?} -> {top1:?}");
    let fill = s.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|x| x.id != v).unwrap(); assert!(fill.audio.mute, "the fill is muted (no doubled audio)");
    let r = c(&mut s, "add_blur_fill", json!({"clip_id": v, "remove": true})); assert_eq!(r["removed"], 1, "{r}");
    assert_eq!(s.project.tracks.len(), ntracks, "its empty track goes too"); assert!(px(&frame(&s, 0.5), 45, 10)[0] < 1.0, "black bars again");
}

#[test]
fn bad_input_is_refused_without_changing_the_project() {
    let (mut s, v) = scene(); let t = s.project.tracks[0].id.clone();
    let so = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1, 0, 0], "start": 3, "duration": 1})), "clip_id");
    let before = pj(&s);
    for (args, want) in [
        (json!({}), "clip_id"),
        (json!({"clip_id": "nope"}), "no such clip"),
        (json!({"clip_id": so}), "not a media clip"),
        (json!({"clip_id": v, "blur": 900}), "within"),
        (json!({"clip_id": v, "dim": -1}), "within"),
        (json!({"clip_id": v, "zoom": 0.5}), "within"),
        (json!({"clip_id": v, "radius": 4}), "unknown argument"),
    ] { let e = dispatch(&mut s, "add_blur_fill", &args).expect_err(&format!("{args} must be refused")); assert!(e.contains(want), "{args}: {e}"); assert_eq!(pj(&s), before, "{args} changed the project"); }
    assert_eq!(c(&mut s, "add_blur_fill", json!({"clip_id": v, "remove": true}))["removed"], 0, "removing a fill that is not there is a no-op");
}
