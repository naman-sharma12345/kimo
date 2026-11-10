//! Kimo-exclusive motion tools (FEATURES.md L242): bind_beats, track_beats, motion_extract, motion_diff --
//! all four already shipped pre-v7 (tools_4.rs / beatcut_tools.rs), uncredited and untested by a dedicated
//! file until this pass. A real trackable video is a static noise field (deterministic LCG, written as a
//! P6 PPM, no external fixture) panned by ffmpeg's `crop` filter -- a whole-frame tracker needs real texture,
//! which a flat colour or a pure ramp does not give it.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::io::Write;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }

fn hash_seed(s: &str) -> u64 { let mut h: u64 = 0xcbf29ce484222325; for b in s.bytes() { h ^= b as u64; h = h.wrapping_mul(0x100000001b3); } h }
/// Deterministic pseudo-random RGB noise field, written as a binary PPM (P6) -- real texture for a tracker
/// with no external fixture file and no PNG encoder dependency.
fn write_noise_ppm(path: &std::path::Path, w: usize, h: usize, seed: u64) {
    let mut s = seed;
    let mut next = move || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; s };
    let mut buf = Vec::with_capacity(w * h * 3);
    for _ in 0..w * h { let v = next(); buf.push((v & 0xff) as u8); buf.push(((v >> 8) & 0xff) as u8); buf.push(((v >> 16) & 0xff) as u8); }
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n{w} {h}\n255\n").unwrap();
    f.write_all(&buf).unwrap();
}

/// Pans a 400x300 noise field through a 160x90 crop window: `x(t) = x0 + vx*t`, `y(t) = y0 + vy*t`, so the
/// tracker should read global dx(t) ~= -vx*t, dy(t) ~= -vy*t (content moves opposite the crop window).
fn panning_clip(name: &str, x0: i32, y0: i32, vx: f64, vy: f64, frames: usize, fps: f64) -> std::path::PathBuf {
    // a fresh noise file per call (not shared across the parallel test threads in this binary -- a shared
    // path with a racy exists()-then-write let one thread's ffmpeg read another's still-being-written file).
    let noise = std::env::temp_dir().join(format!("kimo-motion-{}-{name}-noise.ppm", std::process::id()));
    write_noise_ppm(&noise, 400, 300, 0xC0FFEE ^ std::process::id() as u64 ^ hash_seed(name));
    let out = std::env::temp_dir().join(format!("kimo-motion-{}-{name}.mp4", std::process::id()));
    let vf = format!("crop=160:90:x='{x0}+{vx}*t':y='{y0}+{vy}*t'");
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-loop", "1", "-i", noise.to_str().unwrap(),
        "-vf", &vf, "-pix_fmt", "yuv444p", "-c:v", "libx264", "-qp", "0", "-r", &fps.to_string(), "-frames:v", &frames.to_string(), out.to_str().unwrap()])
        .status().expect("ffmpeg"); assert!(st.success(), "ffmpeg pan render failed");
    out
}

fn media_clip(s: &mut Session, t: &str, path: &std::path::Path, dur: f64) -> String {
    let a = id(&c(s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    id(&c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": dur})), "clip_id")
}

fn session(fps: f64) -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": fps})); let t = id(&c(&mut s, "add_track", json!({})), "track_id"); (s, t) }

#[test] fn motion_extract_reads_a_real_pan_and_can_drive_another_layer() {
    let path = panning_clip("extract", 60, 60, 8.0, 0.0, 20, 10.0); // pans +8px/s in x, 2s clip
    let (mut s, t) = session(10.0);
    let clip = media_clip(&mut s, &t, &path, 2.0);
    // NOTE: not named `null` -- that bare token is JSON's null literal inside the json! macro, not a Rust
    // variable reference, and silently sends {"target_clip_id": null} regardless of what it is set to.
    let null_clip = id(&c(&mut s, "add_null", json!({"track_id": t, "start": 0.0, "duration": 2.0})), "clip_id");
    let r = c(&mut s, "motion_extract", json!({"clip_id": clip, "target_clip_id": null_clip}));
    let dx_range = r["range"]["dx"].as_array().unwrap(); let dx_lo = dx_range[0].as_f64().unwrap(); let dx_hi = dx_range[1].as_f64().unwrap();
    // the view pans +8px/s * ~1.9s =~ 15.2px right over the clip; content should read as moving LEFT (negative dx)
    assert!(dx_lo < -5.0 && dx_lo > -20.0, "expected a clearly negative dx around -15px, range was [{dx_lo}, {dx_hi}]");
    assert!(dx_hi < 2.0, "frame 0 should be ~0 and nothing should drift positive for a one-directional pan, range was [{dx_lo}, {dx_hi}]");
    // target_clip_id must have actually received x keyframes tracking the same motion
    let nc = s.project.find_clip(&null_clip).unwrap();
    let x0 = nc.props.get("x").unwrap().at(0.0); let x1 = nc.props.get("x").unwrap().at(1.9);
    assert!((x1 - x0) < -5.0, "the null's own x should have moved left along with the extracted motion, got x0={x0} x1={x1}");
}

#[test] fn motion_extract_invert_flips_the_sign_like_a_stabiliser() {
    let path = panning_clip("invert", 60, 60, 8.0, 0.0, 20, 10.0);
    let (mut s, t) = session(10.0);
    let clip = media_clip(&mut s, &t, &path, 2.0);
    let null_a = id(&c(&mut s, "add_null", json!({"track_id": t, "start": 0.0, "duration": 2.0})), "clip_id");
    let null_b = id(&c(&mut s, "add_null", json!({"track_id": t, "start": 0.0, "duration": 2.0})), "clip_id");
    c(&mut s, "motion_extract", json!({"clip_id": clip, "target_clip_id": null_a}));
    c(&mut s, "motion_extract", json!({"clip_id": clip, "target_clip_id": null_b, "invert": true}));
    let dx_a = s.project.find_clip(&null_a).unwrap().props.get("x").unwrap().at(1.9) - s.project.find_clip(&null_a).unwrap().props.get("x").unwrap().at(0.0);
    let dx_b = s.project.find_clip(&null_b).unwrap().props.get("x").unwrap().at(1.9) - s.project.find_clip(&null_b).unwrap().props.get("x").unwrap().at(0.0);
    assert!(dx_a < -1.0 && dx_b > 1.0, "invert=true should move the opposite way from the plain match-move, got {dx_a} vs {dx_b}");
}

#[test] fn motion_diff_is_near_zero_for_identical_motion_and_large_for_different_motion() {
    let path_a = panning_clip("diffa", 60, 60, 8.0, 0.0, 20, 10.0);
    let path_static = panning_clip("diffstatic", 100, 60, 0.0, 0.0, 20, 10.0);
    let (mut s, t) = session(10.0);
    let clip_a1 = media_clip(&mut s, &t, &path_a, 2.0);
    let clip_a2 = media_clip(&mut s, &t, &path_a, 2.0); // same panning footage imported twice: motion should match itself
    let clip_static = media_clip(&mut s, &t, &path_static, 2.0);
    let same = c(&mut s, "motion_diff", json!({"clip_a": clip_a1, "clip_b": clip_a2}));
    let diff = c(&mut s, "motion_diff", json!({"clip_a": clip_a1, "clip_b": clip_static}));
    let same_dx_rms = same["channels"]["dx_px"]["rms_diff"].as_f64().unwrap();
    let diff_dx_rms = diff["channels"]["dx_px"]["rms_diff"].as_f64().unwrap();
    assert!(same_dx_rms < 1.0, "the same panning footage compared to itself should read ~0 dx RMS diff, got {same_dx_rms}");
    assert!(diff_dx_rms > same_dx_rms + 3.0, "a moving clip vs a static clip should read a clearly larger dx RMS diff ({diff_dx_rms} vs {same_dx_rms})");
}

#[test] fn track_beats_returns_real_beat_times_and_a_pulse_curve_from_a_click_track() {
    // the same 120bpm synthetic click-track technique as beat_cut.rs's e_beats_come_from_a_music_clip_on_the_timeline
    let path = std::env::temp_dir().join(format!("kimo-motion-{}-click.wav", std::process::id()));
    let sr = 44100usize; let n = sr * 6; let mut pcm = vec![0i16; n];
    for k in 0..12 { let t0 = ((0.25 + 0.5 * k as f64) * sr as f64) as usize; for i in 0..(sr * 12 / 1000) { if t0 + i < n { let e = (-(i as f64) / (sr as f64 * 0.003)).exp(); pcm[t0 + i] = (e * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / sr as f64).sin() * 28000.0) as i16; } } }
    let mut wav = Vec::new(); let db = (n * 2) as u32; wav.extend_from_slice(b"RIFF"); wav.extend_from_slice(&(36 + db).to_le_bytes()); wav.extend_from_slice(b"WAVEfmt "); wav.extend_from_slice(&16u32.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(sr as u32).to_le_bytes()); wav.extend_from_slice(&((sr * 2) as u32).to_le_bytes()); wav.extend_from_slice(&2u16.to_le_bytes()); wav.extend_from_slice(&16u16.to_le_bytes()); wav.extend_from_slice(b"data"); wav.extend_from_slice(&db.to_le_bytes()); for v in &pcm { wav.extend_from_slice(&v.to_le_bytes()); }
    std::fs::write(&path, wav).unwrap();
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 30.0}));
    let a = id(&c(&mut s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    let r = c(&mut s, "track_beats", json!({"asset_id": a, "duration": 6.0}));
    let beats: Vec<f64> = r["beats"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(beats.len() >= 9, "6s of 120bpm clicks should give ~11 beats, got {}", beats.len());
    let bpm = r["bpm"].as_f64().unwrap(); assert!((bpm - 120.0).abs() < 3.0, "bpm ~120, got {bpm}");
    // feed the raw beat times straight into bind_beats -- the composable path to beat-driven motion on ANY
    // property (not just beat_cut's picture-editing use of the same detector).
    let mut s2 = Session::new(); c(&mut s2, "project_set", json!({"width": 160, "height": 90, "fps": 30.0}));
    let t2 = id(&c(&mut s2, "add_track", json!({})), "track_id");
    let clip2 = id(&c(&mut s2, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [1.0, 1.0, 1.0], "start": 0.0, "duration": 6.0})), "clip_id");
    let bb = c(&mut s2, "bind_beats", json!({"clip_id": clip2, "property": "rotation", "beats": beats, "amount": 5.0, "curve": "kick"}));
    assert_eq!(bb["beats"].as_u64().unwrap(), 12, "bind_beats should pick up every beat track_beats found");
}

#[test] fn bind_beats_pulses_a_property_on_explicit_beats_and_decays() {
    let (mut s, t) = session(30.0);
    let clip = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1.0, 1.0, 1.0], "start": 0.0, "duration": 2.0})), "clip_id");
    c(&mut s, "set_property", json!({"clip_id": clip, "property": "scale", "value": 1.0}));
    c(&mut s, "bind_beats", json!({"clip_id": clip, "property": "scale", "beats": [0.5, 1.0, 1.5], "amount": 0.3, "decay": 0.2, "curve": "pulse"}));
    let scale = |t: f64| s.project.find_clip(&clip).unwrap().props.get("scale").unwrap().at(t);
    assert!((scale(0.0) - 1.0).abs() < 0.02, "before any beat, scale should sit at base 1.0, got {}", scale(0.0));
    // the keyframe grid lands near, not exactly on, each beat (one fps step late at worst), so read just
    // after 0.5 rather than at it, and expect a clear rise rather than the exact un-decayed peak value.
    assert!(scale(0.54) > 1.1, "just past a beat the pulse should be clearly elevated (base 1.0 + up to amount 0.3), got {}", scale(0.54));
    assert!(scale(0.54) > scale(0.9), "a pulse decays after the beat: scale just after the beat should be higher than just before the next one, got {} vs {}", scale(0.54), scale(0.9));
}
