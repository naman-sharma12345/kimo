//! Director dry run (2026-10-21): an agent edited a 9:16 short from phone clips through kimo-mcp and these broke.
//! Each test is one failure it hit, reproduced through the commands it called.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_director_prep_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
/// A shaky vertical plate (180x320, 30 fps, 2 s): a textured source cropped along a jittery path, like handheld phone b-roll.
fn shaky_plate() -> String {
    let p = dir().join("shaky.mp4"); if !p.exists() {
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=260x400:r=30:d=2", "-vf", "crop=180:320:'40+14*sin(t*7.3)+8*sin(t*13.1)':'40+11*sin(t*6.1+2)+6*sin(t*11.7)'", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "16", "-pix_fmt", "yuv420p", p.to_str().unwrap()]); }
    p.to_string_lossy().into_owned()
}
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn portrait(w: u32, h: u32) -> (Session, String) {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 30}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0, 0, 0], "start": 0, "duration": 4}));
    let t2 = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); (ss, t2)
}
/// Horizontal extent (first, last column) of bright pixels (text) in a rendered frame.
fn bright_cols(ss: &Session, t: f64) -> (usize, usize, usize) { let (lo, hi, _, _, n) = bright_box(ss, t); (lo, hi, n) }
/// Bounding box (x0, x1, y0, y1, count) of bright pixels (text) in a rendered frame.
fn bright_box(ss: &Session, t: f64) -> (usize, usize, usize, usize, usize) {
    let f = render_frame(&ss.project, t, &Cache::new()).unwrap(); let (w, h) = (f.w, f.h); let (mut lo, mut hi, mut top, mut bot, mut n) = (usize::MAX, 0, usize::MAX, 0, 0);
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; if f.d[i] + f.d[i + 1] + f.d[i + 2] > 1.5 { lo = lo.min(x); hi = hi.max(x); top = top.min(y); bot = bot.max(y); n += 1; } } }
    (lo, hi, top, bot, n)
}

/// stabilize refused a 1.266667 s clip with "clip is not a media clip": 1.266667 x 30 = 38.00001 frames, rounded up to 39,
/// so the last frame sat past the clip's end. A model writes durations to 4 or 6 decimals; 38 frames is what it meant.
#[test] fn stabilize_takes_a_duration_written_to_six_decimals() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 180, "height": 320, "fps": 30}));
    let a = call(&mut ss, "import_media", json!({"path": shaky_plate()}))["asset_id"].as_str().unwrap().to_string();
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // 1.266667 s = 38.00001 frames; 1.3334 s = 40.002 frames (the second dry run wrote 4 decimals): 38 and 40 frames
    for (dur, want) in [(1.266667, 38), (1.3334, 40), (1.2, 36)] {
        let c = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.4, "duration": dur, "in_point": 0.5, "fit": "cover"}))["clip_id"].as_str().unwrap().to_string();
        let r = dispatch(&mut ss, "stabilize", &json!({"clip_id": c}));
        let r = r.unwrap_or_else(|e| panic!("stabilize on a {dur} s media clip failed: {e}"));
        assert_eq!(r["frames"], json!(want), "{dur} s: {r}");
        call(&mut ss, "delete_clip", json!({"clip_id": c}));
    }
}

/// add_captions on a 9:16 canvas wrapped lines at 1.65 x the canvas height (2112 px on 720x1280), so a long cue ran off both edges.
#[test] fn portrait_captions_wrap_inside_the_frame() {
    let (mut ss, t) = portrait(360, 640);
    let srt = dir().join("long.srt"); std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:02,000\nAn agent cuts, colours, and captions them into a short for you.\n").unwrap();
    for style in ["subtitle", "boxed", "yellow"] {
        let r = call(&mut ss, "add_captions", json!({"track_id": t, "source": "srt", "path": srt, "style": style}));
        let (lo, hi, top, bot, n) = bright_box(&ss, 1.0); let (w, h) = (360usize, 640usize);
        assert!(n > 200, "{style}: caption not drawn ({n} px)");
        assert!(lo >= w / 50 && hi < w - w / 50, "{style}: caption spans columns {lo}..{hi} of {w}: it runs off the frame");
        // and it stays above the bottom 15% that phone apps cover with their own buttons and text
        assert!(top >= h / 2 && bot < h * 85 / 100, "{style}: caption spans rows {top}..{bot} of {h}: outside the lower-middle safe band");
        call(&mut ss, "burn_in", json!({"caption_id": r["caption_id"], "enabled": false}));
    }
}

/// add_title sized its type from the canvas height, so on 9:16 "impact" set 355 px Anton on a 720 px wide frame.
#[test] fn titles_fit_a_portrait_canvas() {
    for style in ["impact", "cinematic", "kinetic", "elegant", "neon"] {
        let (mut ss, t) = portrait(360, 640);
        call(&mut ss, "add_title", json!({"track_id": t, "style": style, "text": "KIMO DIRECTOR", "start": 0, "duration": 3}));
        let (lo, hi, n) = bright_cols(&ss, 2.2); let w = 360usize;
        assert!(n > 100, "{style}: title not drawn ({n} px)");
        assert!(lo >= w / 50 && hi < w - w / 50, "{style}: title spans columns {lo}..{hi} of {w}: it runs off the frame");
    }
}

/// add_title paced its reveal for the 4 s default: at duration 1, "impact" drew nothing until 0.9 s (second dry run).
#[test] fn a_short_title_is_on_screen_by_mid_duration() {
    for style in ["impact", "kinetic", "elegant"] {
        let drawn = |dur: f64, t: f64| { let (mut ss, tr) = portrait(360, 640); call(&mut ss, "add_title", json!({"track_id": tr, "style": style, "text": "KIMO\nDIRECTOR", "start": 0, "duration": dur})); bright_cols(&ss, t).2 };
        let (short, full) = (drawn(1.0, 0.55), drawn(4.0, 3.0));
        assert!(full > 100 && short * 2 > full, "{style}: at 0.55 s of a 1 s title {short} px of text are drawn; fully revealed it is {full}");
    }
}