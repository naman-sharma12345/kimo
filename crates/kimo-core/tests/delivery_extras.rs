//! delivery_extras: burn_in_timecode, add_watermark, mux_chapters.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use kimo_core::timecode;
use serde_json::{json, Value};
use std::process::Command;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn project(w: u32, h: u32, fps: f64) -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": w, "height": h, "fps": fps})); s }
fn track(s: &mut Session) -> String { c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }

fn rendered(s: &Session, t: f64) -> kimo_core::effects::Frame { let mut cache = Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &kimo_core::effects::Frame, x: i64, y: i64) -> [f32; 4] {
    if x < 0 || y < 0 || x as usize >= f.w || y as usize >= f.h { return [0.0, 0.0, 0.0, 1.0]; }
    let o = ((y as usize) * f.w + x as usize) * 4; [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}
/// Brightest pixel (sum of RGB) in a square window centred at (cx, cy) -- used instead of a single exact
/// pixel so a test isn't pinned to this engine's precise glyph-baseline/line-height arithmetic.
fn max_brightness_near(f: &kimo_core::effects::Frame, cx: i64, cy: i64, half: i64) -> f32 {
    let mut m = 0.0f32;
    for y in (cy - half)..(cy + half) { for x in (cx - half)..(cx + half) { let p = px(f, x, y); m = m.max(p[0] + p[1] + p[2]); } }
    m
}
/// Bounding box (min_x, max_x, min_y, max_y) of pixels close to `color` (within `tol` per channel), or None.
fn bbox_of_color(f: &kimo_core::effects::Frame, color: [f32; 3], tol: f32) -> Option<(i64, i64, i64, i64)> {
    let (mut x0, mut x1, mut y0, mut y1) = (i64::MAX, i64::MIN, i64::MAX, i64::MIN);
    for y in 0..f.h { for x in 0..f.w {
        let p = px(f, x as i64, y as i64);
        if (p[0] - color[0]).abs() < tol && (p[1] - color[1]).abs() < tol && (p[2] - color[2]).abs() < tol {
            x0 = x0.min(x as i64); x1 = x1.max(x as i64); y0 = y0.min(y as i64); y1 = y1.max(y as i64);
        }
    }}
    if x1 < x0 { None } else { Some((x0, x1, y0, y1)) }
}

// ---------- dyn_text tokens (render.rs) that burn_in_timecode relies on ----------

#[test] fn dyn_text_tc_token_matches_known_non_drop_boundary() {
    assert_eq!(render::dyn_text("{tc:0}", 3600.0, 25.0), "01:00:00:00");
    assert_eq!(render::dyn_text("prefix {tc:0} suffix", 1.0, 24.0), "prefix 00:00:01:00 suffix");
}

#[test] fn dyn_text_tcd_token_matches_timecode_label_drop_frame() {
    let r = 30000. / 1001.;
    assert_eq!(render::dyn_text("{tcd:0}", 60.06, r), timecode::label(60.06, r, true).unwrap());
    assert_eq!(render::dyn_text("{tcd:0}", 60.06, r), "00:01:00;02");
    // wrong fps for drop-frame: a dashed placeholder, not a silently wrong label
    assert_eq!(render::dyn_text("{tcd:0}", 1.0, 30.0), "--:--:--;--");
}

// ---------- burn_in_timecode ----------

#[test] fn burn_in_timecode_creates_exactly_one_dynamic_text_clip() {
    let mut s = project(320, 180, 24.0);
    let t = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1, 0.1, 0.1], "start": 0.0, "duration": 1.0}));
    let t2 = track(&mut s);
    let r = c(&mut s, "burn_in_timecode", json!({"track_id": t2, "start": 0.0, "duration": 0.25, "background_opacity": 0.0}));
    assert_eq!(r["first_label"], timecode::label(0.0, 24.0, false).unwrap());
    assert_eq!(r["last_label"], timecode::label(0.25, 24.0, false).unwrap());
    let clip_ids = r["clip_ids"].as_array().unwrap();
    assert_eq!(clip_ids.len(), 1, "no backing plate (background_opacity 0): exactly one clip, not one per frame");
    let clip = s.project.find_clip(clip_ids[0].as_str().unwrap()).unwrap();
    match &clip.source { kimo_core::model::Source::Text { text, .. } => assert_eq!(text, "{tc:0}"), other => panic!("expected a dynamic Text source, got {other:?}") }
}

#[test] fn burn_in_timecode_has_no_range_length_limit() {
    // the old per-frame-clip design capped this at 6000 frames; a single dynamic clip has no such limit.
    let mut s = project(64, 36, 60.0);
    let t = track(&mut s);
    let r = c(&mut s, "burn_in_timecode", json!({"track_id": t, "duration": 200.0, "background_opacity": 0.0})); // 12000 frames at 60fps
    assert_eq!(r["clip_ids"].as_array().unwrap().len(), 1);
}

#[test] fn burn_in_drop_frame_needs_matching_fps() {
    let mut s = project(320, 180, 30.0); // plain 30, not 30000/1001
    let t = track(&mut s);
    let err = dispatch(&mut s, "burn_in_timecode", &json!({"track_id": t, "duration": 0.2, "drop_frame": true})).unwrap_err();
    assert!(err.contains("drop-frame"), "got: {err}");
}

#[test] fn burn_in_timecode_offset_shifts_displayed_clock_not_timeline_position() {
    let mut s = project(320, 180, 24.0);
    let t = track(&mut s);
    let r = c(&mut s, "burn_in_timecode", json!({"track_id": t, "start": 0.0, "duration": 0.125, "timecode_start": 3600.0, "background_opacity": 0.0}));
    assert_eq!(r["first_label"], "01:00:00:00");
    let clip_ids = r["clip_ids"].as_array().unwrap();
    let clip = s.project.find_clip(clip_ids[0].as_str().unwrap()).unwrap();
    match &clip.source { kimo_core::model::Source::Text { text, .. } => assert_eq!(text, "{tc:3600}"), _ => unreachable!() }
}

#[test] fn burn_in_timecode_renders_visible_text_that_actually_advances() {
    let mut s = project(320, 180, 24.0);
    let bg = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": bg, "source_type": "solid", "color": [0.0, 0.0, 0.0], "start": 0.0, "duration": 1.0}));
    let t2 = track(&mut s);
    c(&mut s, "burn_in_timecode", json!({"track_id": t2, "start": 0.0, "duration": 0.9, "position": "bottom_left", "color": [1.0, 1.0, 1.0], "background_opacity": 0.0}));
    let f0 = rendered(&s, 0.1);
    // bottom-left corner region should have real bright pixels (the clock), somewhere in the middle untouched
    assert!(max_brightness_near(&f0, 40, 160, 35) > 0.3, "expected a visible burned-in clock near bottom-left");
    assert!(max_brightness_near(&f0, 160, 90, 10) < 0.05, "canvas centre should stay plain background");
    // a later frame must render DIFFERENT pixels in that region -- proof the text is resolved per frame
    // (render-time dyn_text), not a single static string baked in once.
    let f1 = rendered(&s, 0.85);
    assert_ne!(f0.d, f1.d, "the timecode clip should show a different label at a different time");
}

// ---------- add_watermark ----------

#[test] fn add_watermark_text_renders_near_chosen_corner() {
    let mut s = project(320, 180, 24.0);
    let bg = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": bg, "source_type": "solid", "color": [0.0, 0.0, 0.0], "start": 0.0, "duration": 1.0}));
    let wt = track(&mut s);
    c(&mut s, "add_watermark", json!({"track_id": wt, "text": "W", "position": "bottom_right", "opacity": 1.0, "size": 60.0, "color": [1.0, 1.0, 1.0], "margin": 0.02}));
    let f = rendered(&s, 0.5);
    // near the bottom-right corner, bright text-on-black should raise luminance well above the black background
    assert!(max_brightness_near(&f, 300, 150, 40) > 0.3, "expected bright watermark pixels near bottom-right");
    assert!(max_brightness_near(&f, 20, 20, 10) < 0.05, "top-left should be untouched background");
}

#[test] fn add_watermark_image_sizes_to_exact_fraction_of_canvas_width() {
    let mut s = project(400, 300, 24.0);
    let bg = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": bg, "source_type": "solid", "color": [0.05, 0.05, 0.05], "start": 0.0, "duration": 1.0}));
    // a flat-colour square logo (1:1) on a 4:3 canvas -- contain-fit binds on height, not width, so this
    // also proves the size math compensates for that (a naive width_frac*fitted_w would undershoot).
    let logo_path = std::env::temp_dir().join(format!("kimo-watermark-logo-{}.mkv", std::process::id()));
    let ok = Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=0xff3366:s=100x100:r=24:d=1", "-c:v", "ffv1"]).arg(&logo_path).status().unwrap();
    assert!(ok.success());
    let wt = track(&mut s);
    c(&mut s, "add_watermark", json!({"track_id": wt, "image": logo_path.to_str().unwrap(), "position": "top_left", "size": 0.25, "margin": 0.0, "opacity": 1.0}));
    let f = rendered(&s, 0.5);
    let (x0, x1, y0, y1) = bbox_of_color(&f, [1.0, 0.2, 0.4], 0.06).expect("logo colour not found in rendered frame");
    let want_w = 0.25 * 400.0; // 100 px
    assert!(((x1 - x0 + 1) as f64 - want_w).abs() <= 2.0, "expected ~{want_w}px wide logo, got {} ({x0}..{x1})", x1 - x0 + 1);
    let want_h = want_w * 100.0 / 100.0; // square source -> square on screen regardless of canvas aspect
    assert!(((y1 - y0 + 1) as f64 - want_h).abs() <= 2.0, "expected ~{want_h}px tall logo, got {} ({y0}..{y1})", y1 - y0 + 1);
    assert!(x0 < 15, "top_left watermark should sit near the left edge, got x0={x0}");
    assert!(y0 < 15, "top_left watermark should sit near the top edge, got y0={y0}");
}

#[test] fn add_watermark_requires_exactly_one_of_text_or_image() {
    let mut s = project(320, 180, 24.0);
    let t = track(&mut s);
    let e1 = dispatch(&mut s, "add_watermark", &json!({"track_id": t, "duration": 1.0})).unwrap_err();
    assert!(e1.contains("text or image"), "got: {e1}");
    let e2 = dispatch(&mut s, "add_watermark", &json!({"track_id": t, "duration": 1.0, "text": "a", "image": "b"})).unwrap_err();
    assert!(e2.contains("not both"), "got: {e2}");
}

// ---------- mux_chapters ----------

#[test] fn mux_chapters_writes_real_container_chapters() {
    let mut s = project(320, 180, 24.0);
    let t = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": 0.0, "duration": 3.0}));
    c(&mut s, "add_marker", json!({"time": 0.0, "name": "Intro", "duration": 0.0, "color": "#ffffff"}));
    c(&mut s, "add_marker", json!({"time": 1.5, "name": "Main", "duration": 0.0, "color": "#ffffff"}));
    let path = std::env::temp_dir().join(format!("kimo-chapters-{}.mp4", std::process::id()));
    let path_str = path.to_str().unwrap().to_string();
    c(&mut s, "export_preset", json!({"path": path_str, "preset": "youtube"}));
    let r = c(&mut s, "mux_chapters", json!({"path": path_str}));
    assert_eq!(r["chapters"], 2);
    let probe = Command::new("ffprobe").args(["-v", "error", "-show_chapters", "-of", "json", &path_str]).output().unwrap();
    let v: Value = serde_json::from_slice(&probe.stdout).unwrap();
    let chs = v["chapters"].as_array().unwrap();
    assert_eq!(chs.len(), 2);
    assert_eq!(chs[0]["tags"]["title"], "Intro");
    assert_eq!(chs[1]["tags"]["title"], "Main");
    assert_eq!(chs[0]["start_time"], "0.000000");
    assert_eq!(chs[1]["start_time"], "1.500000");
}

#[test] fn mux_chapters_needs_at_least_two_markers() {
    let mut s = project(320, 180, 24.0);
    let t = track(&mut s);
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": 0.0, "duration": 1.0}));
    let path = std::env::temp_dir().join(format!("kimo-chapters-onemark-{}.mp4", std::process::id()));
    let path_str = path.to_str().unwrap().to_string();
    c(&mut s, "export_preset", json!({"path": path_str, "preset": "youtube"}));
    let err = dispatch(&mut s, "mux_chapters", &json!({"path": path_str})).unwrap_err();
    assert!(err.contains("at least 2"), "got: {err}");
}
