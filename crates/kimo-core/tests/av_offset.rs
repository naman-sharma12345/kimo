//! v6 eyes: av_offset, the lag between sound and picture. Known references: a 64x36 24 fps timeline whose picture steps
//! black / white / black / white at exactly 1, 2 and 3 s (one change per step) and a WAV whose 20 ms clicks sit at
//! 1, 2, 3 s + offset, for offsets 0, +80 and -120 ms: best_lag_ms must be the offset to the millisecond (positive = the
//! sound after the picture). On the 1 ms grid the train of three picture steps against three 20 ms onset bumps gives
//! r 0.383 at the true lag (worked out the same way in numpy), about 0 at lag 0 for the shifted ones, and nothing near
//! the peak more than a frame away. Pearson on sparse samples alone ties every lag inside the onset ramp (the first
//! draft picked 71 for 80); the grid is what makes the peak exact. The onset envelope and
//! the picture change are checked on their own (a step gives one change; a click's onset peaks on its first sample);
//! silent audio gives no lag (null, never 0); refused arguments last.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::av_offset::{corr_at, onset_envelope, picture_change};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// 4 s of stereo at 48 kHz, a 20 ms 1 kHz burst (amplitude 0.5) starting at each time in ms; silent when `clicks` is empty.
fn wav(dir: &std::path::Path, name: &str, clicks_ms: &[i64]) -> String {
    let n = 4 * 48000; let mut m = vec![0f32; n];
    for &c in clicks_ms { let s0 = (c * 48) as usize; for i in 0..960 { m[s0 + i] = 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin() + if i == 0 { 0.25 } else { 0.0 }; } }
    let mut inter = Vec::with_capacity(2 * n); for v in m { inter.extend([v, v]); }
    let p = dir.join(name); kimo_core::audio::write_wav(p.to_str().unwrap(), &inter).unwrap(); p.to_str().unwrap().to_string()
}
fn timeline(wav_path: &str) -> (Session, Value) { timeline_at(wav_path, 0.0, 0.0) }
/// The same picture with the WAV placed at `start` on the timeline and read from `in_point` into the file (4 s in all).
fn timeline_at(wav_path: &str, start: f64, in_point: f64) -> (Session, Value) { timeline_with(&[[0., 0., 0.], [1., 1., 1.], [0., 0., 0.], [1., 1., 1.]], wav_path, start, in_point) }
/// One 1 s solid per colour, then the WAV as above.
fn timeline_with(picture: &[[f64; 3]], wav_path: &str, start: f64, in_point: f64) -> (Session, Value) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for (k, col) in picture.iter().enumerate() { c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": k as f64, "duration": 1.0})); }
    let a = c(&mut s, "import_media", json!({"path": wav_path}))["asset_id"].clone();
    let clip = c(&mut s, "add_audio_clip", json!({"asset_id": a, "start": start, "in_point": in_point, "duration": 4.0 - start - in_point}))["clip_id"].clone();
    (s, clip)
}
fn tmp() -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_av_offset_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }

#[test] fn known_offsets_to_the_millisecond() {
    let d = tmp();
    for off in [0i64, 80, -120] {
        let p = wav(&d, &format!("clicks_{off}.wav"), &[1000 + off, 2000 + off, 3000 + off]);
        let (mut s, _) = timeline(&p);
        let r = c(&mut s, "av_offset", json!({"start": 0.5, "end": 3.5}));
        assert_eq!(r["best_lag_ms"], off, "offset {off}: {}", serde_json::to_string(&r).unwrap().chars().take(600).collect::<String>());
        assert!((f(&r["best_r"]) - 0.383008).abs() < 1e-3, "offset {off}: best_r {}", r["best_r"]);
        if off != 0 { assert!(f(&r["r_at_zero"]).abs() < 0.02, "offset {off}: r at 0 = {}", r["r_at_zero"]); }
        assert_eq!(r["frames_sampled"], 72); assert_eq!(r["max_lag_ms"], 500); assert_eq!(r["audio"], "mix");
        // the three steps are the three biggest picture changes, at exactly 1, 2, 3 s
        let mut top: Vec<f64> = r["top_picture_changes"].as_array().unwrap()[..3].iter().map(|x| f(&x["t"])).collect(); top.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(top, vec![1.0, 2.0, 3.0]);
        // the curve: every 10 ms from -500 to +500 (101 points), and its max sits on the best lag's 10 ms neighbourhood
        let cv = r["curve"].as_array().unwrap(); assert_eq!(cv.len(), 101); assert_eq!(cv[0][0], -500); assert_eq!(cv[100][0], 500);
        let at = cv.iter().find(|p| p[0] == off).unwrap(); assert!((f(&at[1]) - f(&r["best_r"])).abs() < 1e-9);
        assert!(f(&r["peak_margin"]) > 0.35, "offset {off}: margin {}", r["peak_margin"]);
    }
    // the audio clip as target, and a narrower search
    let p = wav(&d, "clicks_c.wav", &[1080, 2080, 3080]); let (mut s, clip) = timeline(&p);
    let r = c(&mut s, "av_offset", json!({"start": 0.5, "end": 3.5, "audio": {"clip_id": clip}, "max_lag_ms": 200, "curve_step_ms": 50}));
    assert_eq!(r["best_lag_ms"], 80); assert_eq!(r["curve"].as_array().unwrap().len(), 9);
}

#[test] fn signals_on_their_own() {
    // picture: a step gives one change, the frame after is quiet
    let frames = vec![vec![0.0; 4], vec![0.0; 4], vec![1.0; 4], vec![1.0; 4]];
    assert_eq!(picture_change(&frames), vec![0.0, 0.0, 1.0, 0.0]);
    // audio: a burst starting at 100 ms peaks the onset envelope at exactly k = 100
    let mut m = vec![0f32; 48 * 300]; for i in 0..960 { m[100 * 48 + i] = 0.5; }
    let o = onset_envelope(&m, 10); let k = (0..o.len()).max_by(|a, b| o[*a].partial_cmp(&o[*b]).unwrap().then(b.cmp(a))).unwrap();
    assert_eq!(k, 100); assert!((o[100] - 0.5).abs() < 1e-6);
    // correlation: an impulse train against itself shifted by 7 ms
    let v = vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]; let base: Vec<i64> = (0..8).map(|i| i * 40).collect();
    let mut env = vec![0.0; 400]; for i in [1, 4, 7] { env[i * 40 + 7] = 1.0; }
    assert!((corr_at(&v, &base, &env, 7).unwrap() - 1.0).abs() < 1e-12); assert!(corr_at(&v, &base, &env, -33).unwrap() < 0.0); assert!(corr_at(&v, &base, &env, 0).unwrap() < 0.0, "the onsets fall between the picture events at lag 0");
    assert_eq!(corr_at(&v, &base, &vec![0.0; 400], 0), None);
}

#[test] fn silent_audio_gives_no_lag() {
    let d = tmp(); let p = wav(&d, "silent.wav", &[]); let (mut s, _) = timeline(&p);
    let r = c(&mut s, "av_offset", json!({"start": 0.5, "end": 3.5}));
    assert_eq!(r["best_lag_ms"], Value::Null); assert_eq!(r["best_r"], Value::Null); assert_eq!(r["r_at_zero"], Value::Null); assert_eq!(r["onset_max"], 0.0);
}

#[test] fn refused_arguments() {
    let d = tmp(); let p = wav(&d, "clicks_r.wav", &[1000]); let (mut s, _) = timeline(&p);
    let bad = [json!({"source": "file:/x.mp4"}), json!({"source": "clip:nope"}), json!({"start": 3.0, "end": 2.0}), json!({"start": 0.0, "end": 9.0}),
        json!({"start": 1.0, "end": 1.1}), json!({"max_lag_ms": 0}), json!({"max_lag_ms": 2001}), json!({"curve_step_ms": 0}), json!({"onset_window_ms": 1}),
        json!({"scale": 8}), json!({"audio": "nope"}), json!({"onset_window_ms": 101}), json!({"max_lag_ms": 1.5})];
    for a in &bad { assert!(dispatch(&mut s, "av_offset", a).is_err(), "accepted {a}"); }
}

/// A known shift of one clip's sound: the in-sync WAV (clicks on the picture steps at 1, 2, 3 s) moved 120 ms late (placed at
/// 0.12 s) and 80 ms early (read from 0.08 s into the file). At 24 fps one frame is 41.667 ms; best_lag_ms must land within one
/// frame of the shift, and the in-sync placement within one frame of 0.
#[test] fn a_clips_sound_shifted_by_known_ms_reads_within_one_frame() {
    let d = tmp(); let p = wav(&d, "clicks_sync.wav", &[1000, 2000, 3000]); let frame_ms = 1000.0 / 24.0;
    for (shift, start, in_point) in [(0i64, 0.0, 0.0), (120, 0.12, 0.0), (-80, 0.0, 0.08)] {
        let (mut s, _) = timeline_at(&p, start, in_point);
        let r = c(&mut s, "av_offset", json!({"start": 0.5, "end": 3.5}));
        let lag = r["best_lag_ms"].as_i64().unwrap_or_else(|| panic!("shift {shift}: no lag {}", r["best_lag_ms"]));
        eprintln!("shift {shift} ms: best_lag_ms {lag}, best_r {}, r_at_zero {}, tolerance {frame_ms:.3} ms", r["best_r"], r["r_at_zero"]);
        assert!(((lag - shift).abs() as f64) <= frame_ms, "shift {shift}: read {lag}, more than one frame ({frame_ms:.3} ms) off");
        if shift == 0 { assert_eq!(lag, 0, "the zero-offset control must read exactly 0 ms"); }
        assert_eq!(r["fps"], 24.0); assert!((f(&r["frame_ms"]) - 41.666667).abs() < 1e-6);
    }
}

/// The same shifts (and the in-sync control) through a real render: the timeline is exported by the engine (its own encode), the file is imported into a
/// fresh session as one media clip, and av_offset reads that clip's picture against its own sound (decode only). The lag must
/// land within one frame (41.667 ms at 24 fps) of the shift that was rendered in.
#[test] fn a_rendered_file_with_a_known_shift_reads_within_one_frame() {
    let d = tmp(); let p = wav(&d, "clicks_sync_r.wav", &[1000, 2000, 3000]); let frame_ms = 1000.0 / 24.0;
    for (shift, start, in_point) in [(0i64, 0.0, 0.0), (120, 0.12, 0.0), (-80, 0.0, 0.08)] {
        let (mut s, _) = timeline_at(&p, start, in_point);
        let mp4 = d.join(format!("rendered_{}.mp4", if shift < 0 { format!("m{}", -shift) } else { format!("p{shift}") }));
        c(&mut s, "export", json!({"path": mp4.to_str().unwrap()}));
        let mut b = Session::new(); c(&mut b, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
        let a = c(&mut b, "import_media", json!({"path": mp4.to_str().unwrap()}))["asset_id"].clone();
        let t = c(&mut b, "add_track", json!({}))["track_id"].clone();
        let clip = c(&mut b, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
        let r = c(&mut b, "av_offset", json!({"source": format!("clip:{clip}"), "start": 0.5, "end": 3.5}));
        let lag = r["best_lag_ms"].as_i64().unwrap_or_else(|| panic!("rendered shift {shift}: no lag {}", serde_json::to_string(&r).unwrap().chars().take(600).collect::<String>()));
        eprintln!("rendered shift {shift} ms: best_lag_ms {lag}, best_r {}, r_at_zero {}, tolerance {frame_ms:.3} ms", r["best_r"], r["r_at_zero"]);
        assert!(((lag - shift).abs() as f64) <= frame_ms, "rendered shift {shift}: read {lag}, more than one frame ({frame_ms:.3} ms) off");
    }
}

/// A clip that starts with a 2 s quiet head: black picture and silence until the first step / click at 2 s (+ shift), a second
/// pair at 3 s, and the whole 0..4 s span searched, so the quiet head sits inside the window. The read must still land within
/// one frame (41.667 ms at 24 fps) of the planted shift (+120 late, -80 early), and the in-sync control must read exactly 0.
#[test] fn a_quiet_head_does_not_pull_the_read() {
    let d = tmp(); let p = wav(&d, "clicks_quiet_head.wav", &[2000, 3000]); let frame_ms = 1000.0 / 24.0;
    let picture = [[0., 0., 0.], [0., 0., 0.], [1., 1., 1.], [0., 0., 0.]];
    for (shift, start, in_point) in [(0i64, 0.0, 0.0), (120, 0.12, 0.0), (-80, 0.0, 0.08)] {
        let (mut s, _) = timeline_with(&picture, &p, start, in_point);
        let r = c(&mut s, "av_offset", json!({}));
        assert_eq!(r["start"], 0.0); assert_eq!(r["end"], 4.0); assert_eq!(r["frames_sampled"], 96);
        let lag = r["best_lag_ms"].as_i64().unwrap_or_else(|| panic!("quiet head, shift {shift}: no lag {}", r["best_lag_ms"]));
        eprintln!("quiet head, shift {shift} ms: best_lag_ms {lag}, best_r {}, r_at_zero {}, runner-up {} r {}, tolerance {frame_ms:.3} ms", r["best_r"], r["r_at_zero"], r["runner_up_lag_ms"], r["runner_up_r"]);
        assert!(((lag - shift).abs() as f64) <= frame_ms, "quiet head, shift {shift}: read {lag}, more than one frame ({frame_ms:.3} ms) off");
        if shift == 0 { assert_eq!(lag, 0, "quiet head: the zero-offset control must read exactly 0 ms"); }
        // the two biggest picture changes are the two steps, nothing in the quiet head
        let mut top: Vec<f64> = r["top_picture_changes"].as_array().unwrap()[..2].iter().map(|x| f(&x["t"])).collect(); top.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(top, vec![2.0, 3.0]);
    }
}

/// 16-bit PCM stereo WAV at `rate` Hz, 4 s, the same 20 ms 1 kHz clicks (written here, no resampler in between).
fn wav_at_rate(dir: &std::path::Path, name: &str, clicks_ms: &[i64], rate: u32) -> String {
    let n = 4 * rate as usize; let burst = (rate / 50) as usize; let mut m = vec![0f32; n];
    for &c in clicks_ms { let s0 = (c as usize * rate as usize) / 1000; for i in 0..burst { m[s0 + i] = 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin() + if i == 0 { 0.25 } else { 0.0 }; } }
    let data_len = (n * 4) as u32; let mut b: Vec<u8> = Vec::with_capacity(44 + data_len as usize);
    b.extend(b"RIFF"); b.extend((36 + data_len).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(rate.to_le_bytes()); b.extend((rate * 4).to_le_bytes()); b.extend(4u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data_len.to_le_bytes());
    for v in m { let q = (v.clamp(-1.0, 1.0) * 32767.0).round() as i16; b.extend(q.to_le_bytes()); b.extend(q.to_le_bytes()); }
    let p = dir.join(name); std::fs::write(&p, b).unwrap(); p.to_str().unwrap().to_string()
}

/// The first audio stream's sample rate as ffprobe reads it (a probe, no decode of the samples).
fn rate_of(path: &str) -> u64 {
    let o = std::process::Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=sample_rate", "-of", "default=nw=1:nk=1", path]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().parse().unwrap_or_else(|_| panic!("no sample rate for {path}: {}", String::from_utf8_lossy(&o.stderr)))
}

/// Sample-rate change through a render: the source sound is a 44100 Hz WAV (clicks on the picture steps at 1, 2, 3 s), shifted
/// +120 ms late and -80 ms early on the timeline, exported by the engine, and the rendered file read back as one clip (decode
/// only). The two rates are read with ffprobe (source 44100, rendered file at the export rate, which must differ), so a
/// frame-vs-sample rounding slip would show; the read must land within one frame (41.667 ms at 24 fps) of the planted shift.
#[test] fn a_rendered_file_at_another_sample_rate_reads_within_one_frame() {
    let d = tmp(); let p = wav_at_rate(&d, "clicks_44k.wav", &[1000, 2000, 3000], 44100); let frame_ms = 1000.0 / 24.0;
    for (shift, start, in_point) in [(120i64, 0.12, 0.0), (-80, 0.0, 0.08)] {
        let (mut s, _) = timeline_at(&p, start, in_point);
        let src_rate = rate_of(&p);
        let mp4 = d.join(format!("rendered_44k_{}.mp4", if shift < 0 { format!("m{}", -shift) } else { format!("p{shift}") }));
        c(&mut s, "export", json!({"path": mp4.to_str().unwrap()}));
        let mut b = Session::new(); c(&mut b, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
        let a = c(&mut b, "import_media", json!({"path": mp4.to_str().unwrap()}))["asset_id"].clone();
        let out_rate = rate_of(mp4.to_str().unwrap());
        let t = c(&mut b, "add_track", json!({}))["track_id"].clone();
        let clip = c(&mut b, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
        let r = c(&mut b, "av_offset", json!({"source": format!("clip:{clip}"), "start": 0.5, "end": 3.5}));
        let lag = r["best_lag_ms"].as_i64().unwrap_or_else(|| panic!("44.1k rendered shift {shift}: no lag {}", serde_json::to_string(&r).unwrap().chars().take(600).collect::<String>()));
        eprintln!("44.1k source, rendered shift {shift} ms: source rate {src_rate}, rendered rate {out_rate}, fps 24, best_lag_ms {lag}, best_r {}, tolerance {frame_ms:.3} ms", r["best_r"]);
        assert_eq!(src_rate, 44100, "the source WAV must be 44100 Hz");
        assert_ne!(out_rate, 44100, "the rendered file must be at another rate");
        assert!(((lag - shift).abs() as f64) <= frame_ms, "44.1k rendered shift {shift}: read {lag}, more than one frame ({frame_ms:.3} ms) off");
    }
}
