//! Premiere "Time Interpolation" on speed ramps: set_speed_ramp takes frame_blend off|frame_mix|optical_flow, so a slow-motion
//! ramp shows in-between pictures instead of repeating the nearest source frame (the stutter of 30 fps phone footage at 25%).
//! Source: lossless ffv1, 10 fps, 32x16; frame k is the solid colour (20k, 10k+5, 200-15k)/255, so expected pixels come from k.
use kimo_core::{commands::{Session, dispatch}, render};
use serde_json::{json, Value};

const FPS: f64 = 10.0;
fn src(k: usize) -> [f32; 3] { [(20 * k) as f32 / 255.0, (10 * k + 5) as f32 / 255.0, (200 - 15 * k) as f32 / 255.0] }
fn eotf(v: f64) -> f64 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn oetf(l: f64) -> f64 { if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 } }
fn media(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("kimo-speedinterp-{tag}-{}.mkv", std::process::id()));
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "nullsrc=s=32x16:r=10:d=2.4,format=rgb24,geq=r='20*N':g='10*N+5':b='200-15*N'", "-c:v", "ffv1", "-pix_fmt", "bgr0", p.to_str().unwrap()]).stdin(std::process::Stdio::null()).status().unwrap().success());
    p
}
/// Moving square: black 64x32, white 8x8 square whose left edge is at x = 8 + 4k on frame k (rows 12..20).
fn square_media(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("kimo-speedinterp-sq-{tag}-{}.mkv", std::process::id()));
    let g = "nullsrc=s=64x32:r=10:d=1.2,format=rgb24,geq=r='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)':g='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)':b='if(between(X,8+4*N,15+4*N)*between(Y,12,19),255,0)'";
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", g, "-c:v", "ffv1", "-pix_fmt", "bgr0", p.to_str().unwrap()]).stdin(std::process::Stdio::null()).status().unwrap().success());
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
fn ramp(s: &mut Session, id: &str, args: Value) -> Result<Value, String> { let mut a = args; a["clip_id"] = json!(id); dispatch(s, "set_speed_ramp", &a) }
fn mix(a: [f32; 3], b: [f32; 3], w: f64) -> [f32; 3] { let mut o = [0f32; 3]; for c in 0..3 { o[c] = oetf((1.0 - w) * eotf(a[c] as f64) + w * eotf(b[c] as f64)) as f32; } o }
fn near(d: &[f32], want: [f32; 3], tol: f32, what: &str) { for (i, px) in d.chunks(4).enumerate() { for c in 0..3 { assert!((px[c] - want[c]).abs() < tol, "{what}: pixel {i} ch {c}: got {} want {}", px[c], want[c]); } assert_eq!(px[3], 1.0, "{what}: alpha"); } }

#[test]
fn slow_ramp_with_frame_mix_shows_in_between_frames() {
    let p = media("mix"); let (mut s, id) = session(&p, 32, 16, 2.0);
    // constant 25% speed as a one-key ramp: source = clip/4. t = 0.9 -> source 0.225 s -> frame 2.25 (weight 0.25 on frame 3)
    let r = ramp(&mut s, &id, json!({"keys": [[0, 0.25]], "frame_blend": "frame_mix"})).unwrap();
    assert_eq!(r["frame_blend"], json!("frame_mix"), "result echoes the interpolation: {r}");
    near(&frame(&s, 0.9), mix(src(2), src(3), 0.25), 2e-5, "25% ramp frame_mix t=0.9 (source frame 2.25)");
    near(&full_frame(&s, 0.9), mix(src(2), src(3), 0.25), 2e-5, "same with the whole clip decoded");
    near(&frame(&s, 1.0), mix(src(2), src(3), 0.5), 2e-5, "t=1.0 (source frame 2.5) is the half mix");
    // exactly on a source frame the picture is that frame, untouched
    solid(&frame(&s, 0.8), src(2), "t=0.8 is source frame 2 exactly");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn eased_ramp_blends_across_the_speed_change() {
    let p = media("ease"); let (mut s, id) = session(&p, 32, 16, 2.0);
    ramp(&mut s, &id, json!({"keys": [[0, 1.0], [1.0, 0.2], [2.0, 1.0]], "ease": "in_out", "frame_blend": "frame_mix"})).unwrap();
    let c = s.project.find_clip(&id).unwrap().clone();
    for t in [0.33, 0.77, 1.21, 1.63] {
        let x = c.source_time(t) * FPS; let k = (x + 1e-6).floor(); let w = x - k;
        if w < 1e-3 || w > 0.999 { continue; }
        near(&frame(&s, t), mix(src(k as usize), src(k as usize + 1), w), 3e-5, &format!("eased ramp t={t} (source frame {x:.4})"));
    }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn ramp_without_frame_blend_still_shows_the_nearest_frame() {
    let p = media("off"); let (mut s, id) = session(&p, 32, 16, 2.0);
    ramp(&mut s, &id, json!({"keys": [[0, 0.25]]})).unwrap();
    solid(&frame(&s, 0.9), src(2), "default (frame sampling) t=0.9");
    ramp(&mut s, &id, json!({"keys": [[0, 0.25]], "frame_blend": "frame_mix"})).unwrap();
    ramp(&mut s, &id, json!({"keys": [[0, 0.25]], "frame_blend": "off"})).unwrap();
    solid(&frame(&s, 0.9), src(2), "frame_blend off t=0.9");
    assert!(!serde_json::to_string(&s.project).unwrap().contains("speed_blend"), "off leaves nothing in the project json");
    std::fs::remove_file(p).unwrap();
}

#[test]
fn optical_flow_ramp_moves_the_square_to_the_in_between_position() {
    let p = square_media("flow"); let (mut s, id) = session(&p, 64, 32, 2.0);
    let col = |d: &[f32], x: usize| -> f32 { (12..20).map(|y| d[(y * 64 + x) * 4]).sum::<f32>() / 8.0 };
    // 50% speed: t = 0.5 -> source 0.25 s = frame 2.5: frame 2 square at cols 16..23, frame 3 at 20..27, the true in-between 18..25
    ramp(&mut s, &id, json!({"keys": [[0, 0.5]], "frame_blend": "frame_mix"})).unwrap();
    let m = frame(&s, 0.5);
    let ghost = oetf(0.5) as f32;
    for x in [16, 17, 26, 27] { assert!((col(&m, x) - ghost).abs() < 2e-5, "frame_mix ghost col {x}: {}", col(&m, x)); }
    ramp(&mut s, &id, json!({"keys": [[0, 0.5]], "frame_blend": "optical_flow"})).unwrap();
    let of = frame(&s, 0.5);
    for x in 19..=24 { assert!(col(&of, x) > 0.9, "optical_flow interior col {x}: {}", col(&of, x)); }
    for x in [16, 27] { assert!(col(&of, x) < 0.5 * ghost, "optical_flow ghost col {x}: {} (frame_mix ghost {ghost})", col(&of, x)); }
    std::fs::remove_file(p).unwrap();
}

#[test]
fn frame_blend_is_validated_and_cleared_with_the_ramp() {
    let p = media("val"); let (mut s, id) = session(&p, 32, 16, 1.0);
    let e = ramp(&mut s, &id, json!({"keys": [[0, 0.5]], "frame_blend": "bogus"})).unwrap_err();
    assert!(e.contains("frame_blend") && e.contains("optical_flow"), "names the valid modes: {e}");
    assert!(s.project.find_clip(&id).unwrap().speed_curve.is_none(), "a refused call changes nothing");
    ramp(&mut s, &id, json!({"keys": [[0, 0.5]], "frame_blend": "optical_flow"})).unwrap();
    assert!(serde_json::to_string(&s.project).unwrap().contains("speed_blend"));
    let r = ramp(&mut s, &id, json!({"keys": []})).unwrap(); assert_eq!(r["cleared"], json!(true));
    assert!(!serde_json::to_string(&s.project).unwrap().contains("speed_blend"), "clearing the ramp clears its interpolation");
    std::fs::remove_file(p).unwrap();
}
