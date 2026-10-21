//! auto_reframe (Premiere Auto Reframe) end to end on lossless synthetic plates (ffv1 .mkv, 320x180 @ 10 fps) in a 9:16 project
//! (90x160): an orange disc on a dim textured ground is the subject. The reframed render must keep the disc in frame and near the
//! middle while it crosses the whole 16:9 frame (also at speed 2), hold a steady window on a disc that does not move, decode
//! the window at full resolution (sharper than the old letterbox-and-zoom), keep the window across a split, and refuse what it
//! cannot do without touching the project. Proof frames go to KIMO_PROOF_DIR when it is set.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const SW: usize = 320; const SH: usize = 180; const FPS: f64 = 10.0; const N: usize = 40;
const PW: usize = 90; const PH: usize = 160;

fn tex(x: usize, y: usize) -> [u8; 3] { // dim blue-grey ground with fixed fine noise and soft stripes
    let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F); h ^= h >> 29; h = h.wrapping_mul(0xBF58476D1CE4E5B9); h ^= h >> 32;
    let n = (h % 21) as i32 - 10; let s = (((x as f64) * 0.35).sin() * 6.0) as i32;
    [(52 + n + s).clamp(0, 255) as u8, (60 + n + s).clamp(0, 255) as u8, (74 + n + s).clamp(0, 255) as u8]
}
fn plate(tag: &str, disc_x: impl Fn(f64) -> f64) -> std::path::PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-reframe-{tag}-{}.mkv", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{SW}x{SH}"), "-r", "10", "-i", "-", "-c:v", "ffv1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); for i in 0..N { let cx = disc_x(i as f64 / FPS); let mut o = Vec::with_capacity(SW * SH * 3);
        for y in 0..SH { for x in 0..SW { let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - 90.0).powi(2)).sqrt(); if d < 12.0 { o.extend_from_slice(&[232, 122, 40]); } else { o.extend_from_slice(&tex(x, y)); } } }
        si.write_all(&o).unwrap(); } }
    assert!(ch.wait().unwrap().success()); path
}
fn moving(t: f64) -> f64 { 40.0 + 240.0 * t / 4.0 }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene(path: &std::path::Path, speed: f64) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": PW, "height": PH, "fps": FPS}));
    let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.0 / speed, "speed": speed}))["clip_id"].as_str().unwrap().to_string(); (s, id)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
/// (orange pixel count, centroid x / width)
fn disc(f: &Frame) -> (usize, f64) { let (mut n, mut sx) = (0usize, 0f64);
    for y in 0..f.h { for x in 0..f.w { let i = (y * f.w + x) * 4; let (r, g, b) = (f.d[i], f.d[i + 1], f.d[i + 2]); if r > 0.7 && g > 0.3 && g < 0.65 && b < 0.35 { n += 1; sx += x as f64 + 0.5; } } }
    (n, if n > 0 { sx / n as f64 / f.w as f64 } else { -1.0 }) }
fn grad_energy(f: &Frame) -> f64 { let mut e = 0.0; for y in 1..f.h - 1 { for x in 1..f.w - 1 { let l = |x: usize, y: usize| { let i = (y * f.w + x) * 4; 0.3 * f.d[i] as f64 + 0.59 * f.d[i + 1] as f64 + 0.11 * f.d[i + 2] as f64 };
    e += (l(x + 1, y) - l(x - 1, y)).powi(2) + (l(x, y + 1) - l(x, y - 1)).powi(2); } } e / ((f.w - 2) * (f.h - 2)) as f64 }
fn save(name: &str, f: &Frame) { use std::io::Write; if let Ok(d) = std::env::var("KIMO_PROOF_DIR") { std::fs::create_dir_all(&d).unwrap();
    let px: Vec<u8> = f.d.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", f.w, f.h), "-i", "-", "-frames:v", "1", &format!("{d}/{name}.png")]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    ch.stdin.take().unwrap().write_all(&px).unwrap(); assert!(ch.wait().unwrap().success()); } }
fn pj(s: &Session) -> Value { serde_json::to_value(&s.project).unwrap() }

#[test]
fn follows_a_subject_across_the_whole_frame() {
    let path = plate("move", moving);
    // area of the disc in the 9:16 frame: cover scale 160/180, radius 12 -> 10.7 px, ~357 px
    for speed in [1.0, 2.0] {
        let (mut s, id) = scene(&path, speed);
        let r = c(&mut s, "auto_reframe", json!({"clip_id": id}));
        assert_eq!(r["axis"], "x", "{r}"); assert!(r["subject_kept"].as_f64().unwrap() > r["centre_crop_kept"].as_f64().unwrap(), "tracking must keep more of the subject than a centre crop: {r}");
        for k in 0..5 { let tc = (0.2 + 0.9 * k as f64) / speed; let f = frame(&s, tc); let (n, cx) = disc(&f);
            if speed == 1.0 { save(&format!("reframe_t{k}"), &f); }
            assert!(n > 250, "speed {speed}: at {tc:.2} s the subject is not in the reframed frame (orange px {n}, want > 250 of ~357)");
            assert!((0.25..=0.75).contains(&cx), "speed {speed}: at {tc:.2} s the subject sits at {cx:.2} of the width, want the middle half"); }
    }
}

#[test]
fn a_still_subject_gets_a_steady_window_and_full_resolution() {
    let path = plate("still", |_| 220.0);
    let (mut s, id) = scene(&path, 1.0);
    let legacy = { let mut l = Session::new(); l.project = s.project.clone(); // the old way: letterbox (contain) then zoom to cover and pan
        let c0 = l.project.find_clip_mut(&id).unwrap(); let sc = PH as f64 / (PW as f64 * SH as f64 / SW as f64); c0.props.get_mut("scale").unwrap().base = sc; c0.props.get_mut("x").unwrap().base = -(220.0 / SW as f64 - 0.5) * PW as f64 * sc; frame(&l, 2.0) };
    let r = c(&mut s, "auto_reframe", json!({"clip_id": id, "motion": "default"}));
    let pr = r["pan_range"].as_array().unwrap(); let span = pr[1].as_f64().unwrap() - pr[0].as_f64().unwrap();
    assert!(span < 0.02, "a subject that never moves must get a steady window: pan_range {pr:?}");
    let f = frame(&s, 2.0); let (n, cx) = disc(&f); save("reframe_still", &f); save("legacy_zoom_still", &legacy);
    assert!(n > 250 && (0.25..=0.75).contains(&cx), "still subject: orange px {n}, centroid {cx:.2}");
    let (ge, gl) = (grad_energy(&f), grad_energy(&legacy));
    assert!(ge > 1.5 * gl, "the reframed window must be decoded at full resolution: gradient energy {ge:.5} vs letterbox-and-zoom {gl:.5}");
    let st = c(&mut s, "auto_reframe", json!({"clip_id": id, "motion": "static"})); assert_eq!(st["keys"], 1, "{st}");
}

#[test]
fn fixed_window_remove_undo_and_split() {
    let path = plate("move2", moving);
    let (mut s, id) = scene(&path, 1.0);
    let plain = frame(&s, 0.2); let before = pj(&s);
    c(&mut s, "auto_reframe", json!({"clip_id": id, "x": 0})); assert!(disc(&frame(&s, 0.2)).0 > 250, "x = 0 shows the left edge, where the disc starts");
    c(&mut s, "auto_reframe", json!({"clip_id": id, "x": 1})); assert_eq!(disc(&frame(&s, 0.2)).0, 0, "x = 1 shows the right edge, the disc is not there at 0.2 s");
    let r = c(&mut s, "auto_reframe", json!({"clip_id": id, "remove": true})); assert_eq!(r["removed"], true);
    assert_eq!(pj(&s), before, "remove restores the clip exactly"); assert_eq!(frame(&s, 0.2).d, plain.d, "remove renders exactly as before");
    c(&mut s, "auto_reframe", json!({"clip_id": id})); let keyed = pj(&s);
    c(&mut s, "undo", json!({})); assert_eq!(pj(&s), before, "undo is byte-exact"); c(&mut s, "redo", json!({})); assert_eq!(pj(&s), keyed, "redo is byte-exact");
    let at = frame(&s, 2.6); c(&mut s, "split_clip", json!({"clip_id": id, "at": 2.0}));
    let after = frame(&s, 2.6); let diff = at.d.iter().zip(&after.d).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
    assert!(diff < 1.0 / 255.0, "a split must keep the reframe window: max pixel difference {diff} at 2.6 s");
}

#[test]
fn refusals_leave_the_project_unchanged() {
    let path = plate("move3", moving);
    let (mut s, id) = scene(&path, 1.0);
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let solid = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = pj(&s);
    for (args, want) in [(json!({"clip_id": solid}), "needs a media"), (json!({"clip_id": id, "motion": "wobbly"}), "valid: static, slow, default, fast"),
        (json!({"clip_id": id, "x": 1.5}), "between 0 and 1"), (json!({"clip_id": id, "x": 0.2, "motion": "slow"}), "not both"), (json!({"clip_id": id, "zoom": 2}), "unknown argument 'zoom'"), (json!({"clip_id": "nope"}), "no such clip")] {
        let e = dispatch_ex(&mut s, "auto_reframe", &args).unwrap_err().message; assert!(e.contains(want), "{args}: error '{e}' lacks '{want}'"); assert_eq!(pj(&s), before, "{args}: a refused call changed the project"); }
    c(&mut s, "project_set", json!({"width": 320, "height": 180}));
    let e = dispatch_ex(&mut s, "auto_reframe", &json!({"clip_id": id})).unwrap_err().message; assert!(e.contains("already the canvas aspect"), "{e}");
}

/// Proof frames (run with --ignored and KIMO_PROOF_DIR): a 640x360 street-like plate, a figure walking left to right, framed for
/// 9:16 three ways at five times: centre crop (fit cover), the old letterbox-and-zoom, and auto_reframe.
#[test]
#[ignore = "writes proof frames to KIMO_PROOF_DIR"]
fn proof_frames_walking_figure() {
    use std::io::Write;
    let (w, h, n) = (640usize, 360usize, 40usize);
    let path = std::env::temp_dir().join(format!("kimo-reframe-proof-{}.mkv", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-r", "10", "-i", "-", "-c:v", "ffv1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    { let mut si = ch.stdin.take().unwrap(); for i in 0..n { let t = i as f64 / 10.0; let fx = 90.0 + 460.0 * t / 4.0; let bob = (t * 9.0).sin().abs() * 6.0; let mut o = Vec::with_capacity(w * h * 3);
        for y in 0..h { for x in 0..w { let (xf, yf) = (x as f64, y as f64);
            let mut px: [u8; 3] = if yf < 230.0 { let k = yf / 230.0; [(120.0 + 60.0 * k) as u8, (150.0 + 50.0 * k) as u8, (200.0 + 20.0 * k) as u8] } else { let [a, b, c] = tex(x, y); [a.saturating_add(40), b.saturating_add(36), c.saturating_add(20)] };
            for (bx, bw, bh) in [(20.0, 70.0, 150.0), (130.0, 50.0, 110.0), (420.0, 90.0, 170.0), (560.0, 60.0, 120.0)] { if xf >= bx && xf < bx + bw && yf >= 230.0 - bh && yf < 230.0 { px = [90, 95, 110]; if ((xf - bx) as i32 / 12 + (yf as i32) / 16) % 3 == 0 { px = [200, 190, 120]; } } }
            let (hx, hy) = (fx, 205.0 - bob - 70.0); if ((xf - hx).powi(2) + (yf - hy).powi(2)).sqrt() < 13.0 { px = [224, 172, 140]; }
            if (xf - fx).abs() < 14.0 && yf > hy + 12.0 && yf < 205.0 - bob { px = [200, 40, 50]; }
            if (xf - fx - 7.0 * (t * 9.0).sin()).abs() < 5.0 && yf >= 205.0 - bob && yf < 232.0 { px = [40, 40, 70]; }
            o.extend_from_slice(&px); } }
        si.write_all(&o).unwrap(); } }
    assert!(ch.wait().unwrap().success());
    let (pw, ph) = (202usize, 360usize);
    let mk = |fit: &str| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": pw, "height": ph, "fps": 10}));
        let a = c(&mut s, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
        let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.0, "fit": fit}))["clip_id"].as_str().unwrap().to_string(); (s, id) };
    let (centre, _) = mk("cover");
    let (mut zoom, zid) = mk("contain"); { let c0 = zoom.project.find_clip_mut(&zid).unwrap(); c0.props.get_mut("scale").unwrap().base = ph as f64 / (pw as f64 * h as f64 / w as f64); }
    let (mut rf, rid) = mk("contain"); let r = c(&mut rf, "auto_reframe", json!({"clip_id": rid})); eprintln!("auto_reframe: {r}");
    for (k, tt) in [0.3, 1.1, 1.9, 2.7, 3.5].iter().enumerate() { save(&format!("pf_centre_{k}"), &frame(&centre, *tt)); save(&format!("pf_zoom_{k}"), &frame(&zoom, *tt)); save(&format!("pf_reframe_{k}"), &frame(&rf, *tt)); }
    let mut full = Session::new(); c(&mut full, "project_set", json!({"width": w, "height": h, "fps": 10})); let a = c(&mut full, "import_media", json!({"path": path.to_str().unwrap()}))["asset_id"].clone(); let t = c(&mut full, "add_track", json!({}))["track_id"].clone();
    c(&mut full, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.0})); for (k, tt) in [0.3, 1.1, 1.9, 2.7, 3.5].iter().enumerate() { save(&format!("pf_source_{k}"), &frame(&full, *tt)); }
}
