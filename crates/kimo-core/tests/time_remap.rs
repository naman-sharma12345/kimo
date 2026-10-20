//! Time remap (AE-style source-time curve) with frame blending. Source: lossless ffv1, 10 fps, 32x16; frame k is the solid colour
//! (20k, 10k+5, 200-15k)/255, so every expected pixel is computed by hand from k.
use kimo_core::{commands::{Session, dispatch}, render};
use serde_json::{json, Value};

const FPS: f64 = 10.0;
fn src(k: usize) -> [f32; 3] { [(20 * k) as f32 / 255.0, (10 * k + 5) as f32 / 255.0, (200 - 15 * k) as f32 / 255.0] }
fn eotf(v: f64) -> f64 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn oetf(l: f64) -> f64 { if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 } }

fn media(tag: &str, with_audio: bool) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("kimo-timeremap-{tag}-{}.mkv", std::process::id()));
    let mut a: Vec<String> = ["-v", "error", "-y", "-f", "lavfi", "-i", "nullsrc=s=32x16:r=10:d=2.4,format=rgb24,geq=r='20*N':g='10*N+5':b='200-15*N'"].iter().map(|s| s.to_string()).collect();
    if with_audio { a.extend(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=2.4"].iter().map(|s| s.to_string())); }
    a.extend(["-c:v", "ffv1", "-pix_fmt", "bgr0"].iter().map(|s| s.to_string()));
    if with_audio { a.extend(["-c:a", "pcm_s16le"].iter().map(|s| s.to_string())); }
    a.push(p.to_str().unwrap().into());
    assert!(std::process::Command::new("ffmpeg").args(&a).status().unwrap().success());
    p
}
/// Moving square: black 64x32, white 8x8 square whose left edge is at x = 8 + 4k on frame k (rows 12..20).
fn square_media(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("kimo-timeremap-sq-{tag}-{}.mkv", std::process::id()));
    let g = "nullsrc=s=64x32:r=10:d=1.2,format=rgb24,geq=r='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)':g='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)':b='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)'";
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", g, "-c:v", "ffv1", "-pix_fmt", "bgr0", p.to_str().unwrap()]).status().unwrap().success());
    p
}
fn session(path: &std::path::Path, w: u32, h: u32, dur: f64) -> (Session, String) {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width": w, "height": h, "fps": FPS})).unwrap();
    let a = dispatch(&mut s, "import_media", &json!({"path": path})).unwrap()["asset_id"].clone();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let id = dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "media", "asset_id": a, "start": 0, "duration": dur})).unwrap()["clip_id"].as_str().unwrap().to_string();
    (s, id)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { let mut c = render::Cache::new(); render::prepare_at(&s.project, &mut c, Some(t)).unwrap(); render::render_frame(&s.project, t, &c).unwrap().d }
fn full_frame(s: &Session, t: f64) -> Vec<f32> { let mut c = render::Cache::new(); render::prepare(&s.project, &mut c).unwrap(); render::render_frame(&s.project, t, &c).unwrap().d }
fn solid(d: &[f32], want: [f32; 3], what: &str) { for (i, px) in d.chunks(4).enumerate() { assert_eq!([px[0], px[1], px[2], px[3]], [want[0], want[1], want[2], 1.0], "{what}: pixel {i}"); } }
fn remap(s: &mut Session, id: &str, args: Value) -> Value { let mut a = args; a["clip_id"] = json!(id); dispatch(s, "set_time_remap", &a).unwrap() }

#[test]
fn guard_plain_clip_shows_source_frame_k_exactly() {
    let p = media("guard", false); let (s, _) = session(&p, 32, 16, 1.0);
    for k in [0usize, 3, 7] { solid(&frame(&s, k as f64 / FPS), src(k), &format!("plain clip t={k}/10")); }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn linear_remap_at_frame_times_is_bit_identical_in_every_blend_mode() {
    let p = media("ident", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    for mode in ["off", "frame_mix", "optical_flow"] {
        remap(&mut s, &id, json!({"keys": [[0, 0], [1, 1]], "frame_blend": mode}));
        for k in 0..10usize { solid(&frame(&s, k as f64 / FPS), src(k), &format!("{mode} k={k}")); }
    }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn held_segment_is_byte_identical_to_the_source_frame() {
    let p = media("hold", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    let reference = frame(&s, 0.4); // plain clip: source frame 4
    remap(&mut s, &id, json!({"keys": [[0, 0.4, "hold"], [0.6, 0.4], [1.0, 0.8]], "frame_blend": "frame_mix"}));
    for t in [0.0, 0.15, 0.37, 0.55, 0.59] { assert_eq!(frame(&s, t), reference, "hold at t={t}"); assert_eq!(full_frame(&s, t), reference, "hold (full decode) at t={t}"); }
    // after the hold: linear 0.4 -> 0.8 over t 0.6 -> 1.0, so t=0.8 is source 0.6 = frame 6 exactly
    solid(&frame(&s, 0.8), src(6), "after hold t=0.8");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn reverse_plays_source_frames_in_reverse_order() {
    let p = media("rev", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    remap(&mut s, &id, json!({"keys": [[0, 1.0], [1, 0.0]], "frame_blend": "frame_mix"}));
    for k in 0..10usize { let t = k as f64 / FPS; solid(&frame(&s, t), src(10 - k), &format!("reverse t={t}")); solid(&full_frame(&s, t), src(10 - k), &format!("reverse full t={t}")); }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn frame_mix_blends_the_two_nearest_frames_by_weight_in_linear_light() {
    let p = media("mix", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    // source time = clip time; t = 0.225 s -> source frame 2.25: frames 2 and 3, weight 0.25 on frame 3
    remap(&mut s, &id, json!({"keys": [[0, 0], [1, 1]], "frame_blend": "frame_mix"}));
    let (a, b) = (src(2), src(3)); let mut want = [0f32; 3];
    for c in 0..3 { want[c] = oetf(0.75 * eotf(a[c] as f64) + 0.25 * eotf(b[c] as f64)) as f32; }
    // hand check of red: frame 2 R = 40/255 -> 0.0212190 linear, frame 3 R = 60/255 -> 0.0451862; 0.75/0.25 mix 0.0272108 -> sRGB 0.1799965
    assert!((want[0] - 0.179_996_5).abs() < 2e-6, "hand value {}", want[0]);
    let d = frame(&s, 0.225);
    for (i, px) in d.chunks(4).enumerate() { for c in 0..3 { assert!((px[c] - want[c]).abs() < 2e-6, "pixel {i} ch {c}: got {} want {}", px[c], want[c]); } assert_eq!(px[3], 1.0); }
    // the naive sRGB-space average would be 0.75*40/255 + 0.25*60/255 = 0.1764706: linear light is measurably brighter
    assert!(d[0] > 0.1764706 + 1e-3);
    // weight 0.5 at t = 0.25 (frames 2 and 3)
    let half: Vec<f32> = (0..3).map(|c| oetf(0.5 * eotf(a[c] as f64) + 0.5 * eotf(b[c] as f64)) as f32).collect();
    let d = frame(&s, 0.25); for c in 0..3 { assert!((d[c] - half[c]).abs() < 2e-6, "w=0.5 ch {c}: {} vs {}", d[c], half[c]); }
    assert_eq!(full_frame(&s, 0.225)[..4], frame(&s, 0.225)[..4], "windowed and full decode agree");
    // blending off: the frame at floor(source frame) is shown, bit-identical
    remap(&mut s, &id, json!({"frame_blend": "off"}));
    solid(&frame(&s, 0.225), src(2), "off");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn slow_motion_ramp_blends_between_frames() {
    let p = media("slow", false); let (mut s, id) = session(&p, 32, 16, 2.0);
    // 25% speed: source = clip/4. t = 0.9 -> source 0.225 -> frame 2.25
    remap(&mut s, &id, json!({"keys": [[0, 0], [2, 0.5]], "frame_blend": "frame_mix"}));
    let (a, b) = (src(2), src(3));
    let d = frame(&s, 0.9);
    for c in 0..3 { let w = oetf(0.75 * eotf(a[c] as f64) + 0.25 * eotf(b[c] as f64)) as f32; assert!((d[c] - w).abs() < 2e-6, "ch {c}: {} vs {w}", d[c]); }
    // t = 0.8 -> source 0.2 = frame 2 exactly
    solid(&frame(&s, 0.8), src(2), "slow exact");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn optical_flow_moves_the_square_to_the_in_between_position() {
    let p = square_media("flow"); let (mut s, id) = session(&p, 64, 32, 1.0);
    // frame 2: square cols 16..23, frame 3: cols 20..27. Halfway the true square is cols 18..25.
    let col = |d: &[f32], x: usize| -> f32 { (12..20).map(|y| d[(y * 64 + x) * 4]).sum::<f32>() / 8.0 };
    remap(&mut s, &id, json!({"keys": [[0, 0], [1, 1]], "frame_blend": "frame_mix"}));
    let mix = frame(&s, 0.25);
    remap(&mut s, &id, json!({"frame_blend": "optical_flow"}));
    let of = frame(&s, 0.25);
    // frame mix: cols 16..19 and 24..27 are each covered by one frame only (half-intensity ghosts), 20..23 by both
    let ghost = oetf(0.5) as f32;
    for x in [16, 17, 18, 19, 24, 25, 26, 27] { assert!((col(&mix, x) - ghost).abs() < 2e-6, "mix ghost col {x}: {}", col(&mix, x)); }
    // optical flow: the in-between interior (cols 19..24) is solid, the outer ghost columns (16, 27) are nearly gone
    for x in 19..=24 { assert!(col(&of, x) > 0.9, "flow interior col {x}: {}", col(&of, x)); }
    for x in [16, 27] { assert!(col(&of, x) < 0.5 * ghost, "flow ghost col {x}: {} (mix ghost {ghost})", col(&of, x)); }
    let cx = |d: &[f32]| -> f32 { let (mut m, mut sx) = (0f32, 0f32); for x in 0..64 { let c = col(d, x); m += c; sx += c * (x as f32 + 0.5); } sx / m };
    assert!((cx(&of) - 22.0).abs() < 0.5, "flow centroid {} (true in-between centre 22.0)", cx(&of));
    std::fs::remove_file(p).unwrap();
}

#[test]
fn split_keeps_the_remap_continuous() {
    let p = media("split", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    remap(&mut s, &id, json!({"keys": [[0, 1.0], [1, 0.0]], "frame_blend": "frame_mix"}));
    let before: Vec<Vec<f32>> = [0.55, 0.7, 0.825].iter().map(|t| frame(&s, *t)).collect();
    dispatch(&mut s, "split_clip", &json!({"clip_id": id, "at": 0.5})).unwrap();
    for (i, t) in [0.55, 0.7, 0.825].iter().enumerate() { assert_eq!(frame(&s, *t), before[i], "after split t={t}"); }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn audio_follows_the_remap_and_holds_are_silent() {
    let p = media("audio", true);
    let crossings = |pcm: &[f32], t0: f64, t1: f64| -> usize { let (a, b) = ((t0 * 48000.0) as usize, (t1 * 48000.0) as usize); (a + 1..b).filter(|&i| pcm[(i - 1) * 2] < 0.0 && pcm[i * 2] >= 0.0).count() };
    let rms = |pcm: &[f32], t0: f64, t1: f64| -> f64 { let (a, b) = ((t0 * 48000.0) as usize, (t1 * 48000.0) as usize); ((a..b).map(|i| (pcm[i * 2] as f64).powi(2)).sum::<f64>() / (b - a) as f64).sqrt() };
    let (mut s, id) = session(&p, 32, 16, 1.0);
    let plain = kimo_core::audio::mix(&s.project);
    let base = crossings(&plain, 0.2, 0.8); assert!((base as i64 - 264).abs() <= 1, "440 Hz over 0.6 s: {base}");
    // 2x: source = 2 * clip time -> 880 Hz
    remap(&mut s, &id, json!({"keys": [[0, 0], [1, 2]]}));
    let fast = kimo_core::audio::mix(&s.project);
    let n = crossings(&fast, 0.2, 0.8); assert!((n as i64 - 528).abs() <= 2, "2x: {n} crossings, want ~528");
    // hold 0..0.5 then 1x: silent while held, 440 Hz after
    remap(&mut s, &id, json!({"keys": [[0, 0.3, "hold"], [0.5, 0.3], [1, 0.8]]}));
    let held = kimo_core::audio::mix(&s.project);
    assert!(rms(&held, 0.05, 0.45) < 1e-4, "held rms {}", rms(&held, 0.05, 0.45));
    let (prms, hrms) = (rms(&plain, 0.2, 0.8), rms(&held, 0.55, 0.95)); assert!((hrms / prms - 1.0).abs() < 0.02, "after hold rms {hrms} vs plain {prms}");
    let n = crossings(&held, 0.55, 0.95); assert!((n as i64 - 176).abs() <= 1, "1x after hold: {n}");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn set_time_remap_validates_and_clears_and_default_json_is_unchanged() {
    let p = media("val", false); let (mut s, id) = session(&p, 32, 16, 1.0);
    let before = serde_json::to_string(&s.project).unwrap();
    assert!(!before.contains("time_remap"), "no remap key serialized by default");
    for bad in [json!({"clip_id": id, "keys": [[0, -0.1], [1, 1]]}), json!({"clip_id": id, "keys": [[0, 0], [1, 1]], "frame_blend": "bogus"}), json!({"clip_id": id, "keys": [[0, 0, "nope"], [1, 1]]}), json!({"clip_id": id, "frame_blend": "frame_mix"}), json!({"clip_id": id, "keys": [[0, 0], [1, f64::NAN]]})] {
        assert!(dispatch(&mut s, "set_time_remap", &bad).is_err(), "must refuse {bad}");
    }
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let solid_id = dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 1})).unwrap()["clip_id"].clone();
    assert!(dispatch(&mut s, "set_time_remap", &json!({"clip_id": solid_id, "keys": [[0, 0], [1, 1]]})).is_err(), "media clips only");
    dispatch(&mut s, "delete_clip", &json!({"clip_id": solid_id})).unwrap();
    let r = remap(&mut s, &id, json!({"keys": [[0, 0.2], [1, 0.9]], "frame_blend": "frame_mix"}));
    assert_eq!(r["frame_blend"], json!("frame_mix")); assert_eq!(r["keys"], json!(2));
    assert!(serde_json::to_string(&s.project).unwrap().contains("time_remap"));
    let r = remap(&mut s, &id, json!({"keys": []})); assert_eq!(r["cleared"], json!(true));
    dispatch(&mut s, "delete_track", &json!({"track_id": tr})).unwrap();
    assert!(!serde_json::to_string(&s.project).unwrap().contains("time_remap"), "cleared remap leaves no trace");
    std::fs::remove_file(p).unwrap();
}
