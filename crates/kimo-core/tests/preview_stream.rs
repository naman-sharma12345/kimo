//! Real-time preview (crate::preview_stream) on real media. The quick test runs everywhere ffmpeg is; the benchmark is the
//! Phase 1 bar (a 1080p, 3-track timeline with sound plays at 25 fps or more once proxies exist; a scrub frame in under
//! 150 ms) and needs a real machine: `cargo test --profile gate -p kimo-core --test preview_stream -- --ignored --nocapture`.
use kimo_core::commands::{dispatch, Session};
use kimo_core::preview_stream as ps;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn call(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(tag: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("kimo-ps-{tag}-{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn media(out: &Path, w: u32, h: u32, secs: u32, hz: u32, pattern: &str) -> bool {
    std::process::Command::new("ffmpeg").args(["-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i", &format!("{pattern}=s={w}x{h}:r=25:d={secs}"), "-f", "lavfi", "-i", &format!("sine=frequency={hz}:sample_rate=48000:duration={secs}"),
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-g", "250", "-c:a", "aac", "-shortest"]).arg(out).status().map(|s| s.success()).unwrap_or(false)
}
/// A w×h 25 fps timeline of `tracks` overlapping media clips with sound (the upper ones half-transparent, so every layer composites).
fn timeline(dir: &Path, w: u32, h: u32, secs: u32, tracks: usize, proxies: bool) -> Option<Session> {
    let mut s = Session::new();
    call(&mut s, "project_set", json!({"width": w, "height": h, "fps": 25}));
    for k in 0..tracks {
        let f = dir.join(format!("m{k}.mp4"));
        if !media(&f, w, h, secs, 220 + 110 * k as u32, ["testsrc2", "smptehdbars", "testsrc"][k % 3]) { return None; }
        let a = call(&mut s, "import_media", json!({"path": f}))["asset_id"].clone();
        if proxies { call(&mut s, "proxy_generate", json!({"asset_id": a, "path": dir.join(format!("p{k}.mp4")), "max_side": (w.max(h) / 2)})); }
        let tr = call(&mut s, "add_track", json!({}))["track_id"].clone();
        let c = call(&mut s, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": a, "start": 0.0, "duration": secs as f64}))["clip_id"].clone();
        if k > 0 { call(&mut s, "set_property", json!({"clip_id": c, "property": "opacity", "value": 0.5})); }
    }
    if proxies { call(&mut s, "proxy_set_enabled", json!({"enabled": true})); }
    Some(s)
}
/// Play from `t` the way the app does: start, then poll a few times a second until `n` frames are ready ahead. Returns frames per second.
fn fill(s: &Session, dir: &Path, n: usize, deadline: Duration) -> (f64, Value) {
    let t0 = Instant::now();
    ps::start(&s.project, dir, 0.5, 0.0, 1.0, true).unwrap();
    loop {
        let r = ps::update(&s.project, 0.0, None, Some(true), false).unwrap();
        assert!(r["error"].is_null(), "stream error: {}", r["error"]);
        if r["ready_ahead"].as_u64().unwrap_or(0) as usize >= n || t0.elapsed() > deadline { return (r["ready_ahead"].as_u64().unwrap() as f64 / t0.elapsed().as_secs_f64(), r); }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[test]
fn a_stream_renders_ahead_with_sound_and_cleans_up() {
    let dir = tmp("quick");
    let Some(s) = timeline(&dir, 320, 180, 3, 2, false) else { eprintln!("ffmpeg missing: skipped"); return };
    let out = dir.join("stream");
    let (_, r) = fill(&s, &out, 30, Duration::from_secs(60));
    assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(160), Some(90)), "half size");
    let frames = r["frames"].as_array().unwrap();
    assert!(frames.len() >= 30, "{} frames", frames.len());
    for (i, f) in frames.iter().take(30).enumerate() {
        assert_eq!(f["index"].as_i64(), Some(i as i64));
        let b = std::fs::read(out.join(f["name"].as_str().unwrap())).unwrap();
        assert!(b.starts_with(b"BM") && b.len() == 54 + 160 * 90 * 4);
    }
    // the first audio chunk arrives and holds the mix of both tracks
    let t0 = Instant::now();
    let chunk = loop {
        let r = ps::update(&s.project, 0.0, None, Some(true), false).unwrap();
        if let Some(c) = r["audio"].as_array().and_then(|a| a.first()).cloned() { break c; }
        assert!(t0.elapsed() < Duration::from_secs(30), "no audio chunk");
        std::thread::sleep(Duration::from_millis(40));
    };
    assert_eq!((chunk["chunk"].as_u64(), chunk["start"].as_f64(), chunk["end"].as_f64(), chunk["audible"].as_bool()), (Some(0), Some(0.0), Some(1.0), Some(true)));
    let wav = std::fs::read(out.join(chunk["name"].as_str().unwrap())).unwrap();
    assert!(wav.starts_with(b"RIFF") && wav.len() == 44 + 48000 * 4, "1 s of 48 kHz stereo 16-bit");
    // the playhead moves: frames behind it are freed (their files deleted), frames ahead keep coming
    let first = out.join(frames[0]["name"].as_str().unwrap());
    let r = ps::update(&s.project, 1.0, None, Some(true), false).unwrap();
    assert!(!first.exists(), "frame 0 fell out of the ring");
    assert!(r["frames"].as_array().unwrap().iter().all(|f| f["index"].as_i64().unwrap() >= 25 - ps::BEHIND as i64));
    // a scrub frame now, then the same frame again from the ring or the scrub cache
    let f = ps::frame(&s.project, 2.0).unwrap();
    assert_eq!(f["index"].as_i64(), Some(50));
    assert!(out.join(f["name"].as_str().unwrap()).is_file());
    assert_eq!(ps::frame(&s.project, 2.0).unwrap()["cached"], json!(true));
    // an edit restarts the stream: new generation, the old files gone
    let gen = r["gen"].as_u64().unwrap();
    let mut s2 = Session::new(); s2.project = s.project.clone();
    let clip = s2.project.tracks[0].clips[0].id.clone();
    call(&mut s2, "set_property", json!({"clip_id": clip, "property": "opacity", "value": 0.25}));
    let r2 = ps::update(&s2.project, 1.0, None, Some(true), false).unwrap();
    assert!(r2["gen"].as_u64().unwrap() > gen && r2["rev"] != r["rev"]);
    // fast forward: every 2nd frame, no sound
    let r3 = ps::update(&s2.project, 1.0, Some(2.0), Some(true), true).unwrap();
    assert_eq!((r3["run"]["dir"].as_i64(), r3["run"]["stride"].as_i64(), r3["audio_on"].as_bool()), (Some(1), Some(2), Some(false)));
    assert!(ps::stop());
    let left: Vec<_> = std::fs::read_dir(&out).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(left.is_empty(), "stop deletes every stream file: {left:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "benchmark: 1080p media, run on a real machine with --ignored --nocapture"]
fn bench_1080p_three_tracks_plays_at_25_fps_and_scrubs_under_150_ms() {
    let dir = tmp("bench");
    let s = timeline(&dir, 1920, 1080, 8, 3, true).expect("ffmpeg makes the 1080p test media");
    let out = dir.join("stream");
    {   // where a frame's time goes, one thread
        let p = &s.project; let mut c = kimo_core::render::Cache::new(); let _nb = kimo_core::analysis_proxy::NonBlocking::new();
        let t = Instant::now(); kimo_core::render::prepare_stream_range(p, &mut c, 1.0, 1.36).unwrap(); let prep = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now(); let f = kimo_core::render::export_frame_rgba8(p, 1.2, &c).unwrap(); let comp = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now(); let d = kimo_core::ext::preview_range::box_downscale(&f, 1920, 1080, 960, 540); let down = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now(); let b = ps::bmp(&d, 960, 540); let enc = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!("preview_stream: one frame: decode window of 10 frames x3 clips {prep:.0} ms, compose {comp:.0} ms, downscale {down:.0} ms, bmp {enc:.1} ms ({} bytes)", b.len());
    }
    let _ = fill(&s, &out, 6, Duration::from_secs(60)); // warm the decoders
    ps::stop();
    let (fps, r) = fill(&s, &out, 80, Duration::from_secs(60));
    eprintln!("preview_stream: 1080p x3 tracks -> {}x{} draft at {fps:.1} fps (render_fps {:.1}, first frame {} ms, proxies {})", r["width"], r["height"], r["render_fps"].as_f64().unwrap_or(0.0), r["first_frame_ms"], r["proxies"]);
    let mut ms = vec![];
    for k in 0..12 { let t = 0.37 + k as f64 * 0.61; let t0 = Instant::now(); let f = ps::frame(&s.project, t).unwrap(); if f["cached"] == json!(true) { continue; } ms.push(t0.elapsed().as_secs_f64() * 1000.0); }
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    eprintln!("preview_stream: scrub frame median {:.0} ms, worst {:.0} ms", ms[ms.len() / 2], ms[ms.len() - 1]);
    ps::stop();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(fps >= 25.0, "renders {fps:.1} fps ahead; 25 needed for real time");
    assert!(ms[ms.len() / 2] < 150.0, "scrub median {:.0} ms", ms[ms.len() / 2]);
}
