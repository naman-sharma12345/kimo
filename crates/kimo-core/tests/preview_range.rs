//! RV-9 preview_range: same composition path as export (hash-equal at scale 1 / divisor 1), exact box downscale, divisor, audio PCM, bounds, read-only,
//! never inside a job directory. The ms-to-first-preview measurement is an #[ignore]d test (run with --ignored --nocapture in the optimised profile).
use kimo_core::commands::{dispatch, Session};
use kimo_core::deps::sha256_hex;
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_rv9_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn scene(w: u32, h: u32) -> Session {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let s = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": w / 4, "height": h / 4, "start": 0, "duration": 3}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_keyframes", json!({"clip_id": s, "property": "x", "keys": [{"t": 0, "v": -(w as f64) / 4.0}, {"t": 3, "v": w as f64 / 4.0}]}));
    c(&mut ss, "add_text", json!({"track_id": t, "text": "Preview", "start": 0, "duration": 3, "size": h / 6})); ss
}
/// the export loop's per-frame steps, written out independently of the implementation
fn export_frame(ss: &Session, i: usize) -> Vec<u8> {
    let p = &ss.project; let mut cache = kimo_core::render::Cache::new_export(); kimo_core::render::prepare_export_range(p, &mut cache, i as f64 / p.fps, i as f64 / p.fps).unwrap();
    let mut f = kimo_core::render::render_frame_opts(p, i as f64 / p.fps, &cache, false).unwrap(); kimo_core::color::pipeline::output(p, &mut f).unwrap(); kimo_core::render::to_rgba8(&f)
}
fn box2(src: &[u8], w: usize, h: usize) -> Vec<u8> { let (nw, nh) = (w / 2, h / 2); let mut o = vec![0u8; nw * nh * 4]; for y in 0..nh { for x in 0..nw { for ch in 0..4 { let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|(dx, dy)| src[((2 * y + dy) * w + 2 * x + dx) * 4 + ch] as u32).sum(); o[(y * nw + x) * 4 + ch] = ((s + 2) / 4) as u8; } } } o }
fn probe(p: &str, entries: &str, sel: &str) -> String { let o = std::process::Command::new("ffprobe").args(["-v", "error", "-count_frames", "-select_streams", sel, "-show_entries", entries, "-of", "csv=p=0"]).arg(p).output().unwrap(); String::from_utf8_lossy(&o.stdout).trim().to_string() }

#[test] fn full_size_every_frame_preview_is_hash_equal_to_the_export_frames() {
    let d = tmp("full"); let mut ss = scene(160, 90); let r = c(&mut ss, "preview_range", json!({"scale": 1, "hashes": true, "path": d.join("p.mp4").to_str().unwrap()}));
    assert_eq!(r["frames"], 75); let hs = r["frame_hashes"].as_array().unwrap(); assert_eq!(hs.len(), 75);
    for i in [0usize, 1, 17, 40, 74] { assert_eq!(hs[i], json!(sha256_hex(&export_frame(&ss, i))), "frame {i} equals the export frame"); }
    assert_ne!(hs[0], hs[40], "the scene moves, so the frames differ");
    assert_eq!(probe(r["path"].as_str().unwrap(), "stream=nb_read_frames,width,height", "v:0"), "160,90,75"); std::fs::remove_dir_all(&d).ok();
}
#[test] fn half_scale_is_exactly_the_box_downscale_of_the_full_frame() {
    let d = tmp("half"); let mut ss = scene(160, 90); let r = c(&mut ss, "preview_range", json!({"scale": 0.5, "hashes": true, "path": d.join("p.mp4").to_str().unwrap()}));
    assert_eq!((r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap()), (80, 46), "even-rounded size"); // 90/2 = 45 -> 46: not an exact 2x factor, so use a size that is
    std::fs::remove_dir_all(&d).ok();
    let mut ss = scene(160, 88); let r = c(&mut ss, "preview_range", json!({"scale": 0.5, "hashes": true, "fps_divisor": 5}));
    assert_eq!((r["width"].as_u64().unwrap(), r["height"].as_u64().unwrap()), (80, 44)); let hs = r["frame_hashes"].as_array().unwrap(); assert_eq!(r["frames"], 15); assert_eq!(r["fps"], 5.0);
    for i in [0usize, 5, 10, 70].iter() { let k = *i / 5; let want = sha256_hex(&box2(&export_frame(&ss, *i), 160, 88)); assert_eq!(hs[k], json!(want), "frame {i} == 2x2 box downscale of the full-size export frame"); }
}
#[test] fn audio_pcm_equals_the_export_slice_and_the_preview_carries_an_aac_stream() {
    let d = tmp("aud"); let wav = d.join("t.wav"); assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:sample_rate=48000:duration=3"]).arg(&wav).status().unwrap().success());
    let mut ss = scene(160, 90); let a = c(&mut ss, "import_media", json!({"path": wav.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string(); c(&mut ss, "add_audio_clip", json!({"asset_id": a, "start": 0, "duration": 3}));
    let r = c(&mut ss, "preview_range", json!({"scale": 0.5, "audio": true, "start": 0.5, "end": 2.0, "path": d.join("p.mp4").to_str().unwrap()}));
    let sr = kimo_core::audio::SR as f64; let (f0, f1) = (13usize, 50usize); let (s0, s1) = ((f0 as f64 / 25.0 * sr) as usize * 2, (f1 as f64 / 25.0 * sr) as usize * 2); let full = kimo_core::audio::mix(&ss.project); let want: Vec<u8> = full[s0..s1].iter().flat_map(|v| v.to_le_bytes()).collect();
    assert_eq!(r["audio"]["pcm_sha256"], json!(sha256_hex(&want)), "{r}"); assert_eq!(r["audio"]["included"], true); assert_eq!(probe(r["path"].as_str().unwrap(), "stream=codec_name", "a:0"), "aac");
    let off = c(&mut ss, "preview_range", json!({"scale": 0.5, "path": d.join("q.mp4").to_str().unwrap()})); assert_eq!(off["audio"], json!({"included": false})); std::fs::remove_dir_all(&d).ok();
}
#[test] fn bounded_read_only_and_never_inside_a_job_directory() {
    let d = tmp("bounds"); let mut ss = scene(160, 90); let before = serde_json::to_string(&ss.project).unwrap(); let u = ss.undo.len();
    let m = e(&mut ss, "preview_range", json!({"start": 0, "end": 31, "path": d.join("p.mp4").to_str().unwrap()})); assert!(m.contains("bounded to 30"), "{m}");
    let m = e(&mut ss, "preview_range", json!({"start": 2, "end": 1})); assert!(m.contains("end must be greater"), "{m}"); let m = e(&mut ss, "preview_range", json!({"scale": 2})); assert!(m.contains("scale must be between"), "{m}");
    let m = e(&mut ss, "preview_range", json!({"fps_divisor": 2.5})); assert!(m.contains("whole number"), "{m}");
    let job = d.join("out.mp4.chunks"); std::fs::create_dir_all(&job).unwrap(); let m = e(&mut ss, "preview_range", json!({"path": job.join("p.mp4").to_str().unwrap()})); assert!(m.contains("export job's chunk directory"), "{m}"); assert!(!job.join("p.mp4").exists());
    std::fs::create_dir_all(d.join("live")).unwrap(); std::fs::write(d.join("live/job.lock"), "{}").unwrap(); let m = e(&mut ss, "preview_range", json!({"path": d.join("live/p.mp4").to_str().unwrap()})); assert!(m.contains("chunk directory"), "{m}");
    let ok = c(&mut ss, "preview_range", json!({"start": 0, "end": 40, "max_seconds": 60})); assert_eq!(ok["end"], 3.0, "capped at the timeline end"); assert!(std::path::Path::new(ok["path"].as_str().unwrap()).is_file());
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "project untouched"); assert_eq!(ss.undo.len(), u, "no undo entry"); std::fs::remove_dir_all(&d).ok();
}
/// cargo test --profile gate --test preview_range -- --ignored --nocapture
#[test] #[ignore] fn measure_time_to_first_preview() {
    for (w, h, scale) in [(1920u32, 1080u32, 0.5), (1920, 1080, 1.0)] {
        let mut ss = scene(w, h); let _ = c(&mut ss, "preview_range", json!({"scale": scale, "end": 1})); // warm
        let r = c(&mut ss, "preview_range", json!({"scale": scale, "end": 3})); println!("MEASURE {w}x{h} scale {scale}: out {}x{}, {} frames, first_frame {:.0} ms, total {:.0} ms", r["width"], r["height"], r["frames"], r["timing_ms"]["first_frame"], r["timing_ms"]["total"]);
    }
}
