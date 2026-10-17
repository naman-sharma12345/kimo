//! v6 eyes audio unit B: audio_spectrum against known references (bin-centred and off-bin sines with the Hann scalloping
//! loss computed analytically, white +3.01 dB/octave and pink flat band slopes, Parseval total vs time-domain RMS, silence as
//! JSON null), argument refusals (never clamped), the max_points hop raise, and one engine call through dispatch.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::audio_spectrum::{analyze, cfg_from_args};
use serde_json::{json, Value};
use std::f64::consts::TAU;

fn tone(hz: f64, amp: f64, secs: f64) -> Vec<f32> { (0..(secs * 48000.) as usize).flat_map(|i| { let v = (amp * (TAU * hz * i as f64 / 48000.).sin()) as f32; [v, v] }).collect() }
/// Deterministic white noise, uniform in [-a, a], independent L and R (xorshift64*).
fn white(secs: f64, a: f64, seed: u64) -> Vec<f32> { let mut s = seed; (0..(secs * 48000.) as usize * 2).map(|_| { s ^= s >> 12; s ^= s << 25; s ^= s >> 27; let u = (s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64; ((2. * u - 1.) * a) as f32 }).collect() }
/// Pink noise from white through Paul Kellet's refined filter (documented within 0.05 dB of -3 dB/octave above 9.2 Hz at 44.1 kHz).
fn pink(secs: f64, seed: u64) -> Vec<f32> { let w = white(secs, 1., seed); let mut b = [[0f64; 7]; 2]; w.iter().enumerate().map(|(i, &x)| { let x = x as f64; let b = &mut b[i % 2];
    b[0] = 0.99886 * b[0] + x * 0.0555179; b[1] = 0.99332 * b[1] + x * 0.0750759; b[2] = 0.96900 * b[2] + x * 0.1538520; b[3] = 0.86650 * b[3] + x * 0.3104856; b[4] = 0.55000 * b[4] + x * 0.5329522; b[5] = -0.7616 * b[5] - x * 0.0168980;
    let y = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + x * 0.5362; b[6] = x * 0.115926; (y * 0.05) as f32 }).collect() }
fn run(pcm: &[f32], args: Value) -> Result<Value, String> { analyze(pcm, &cfg_from_args(&args)?) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }
fn rms_db(pcm: &[f32]) -> f64 { 10. * (pcm.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / pcm.len() as f64).log10() }

#[test] fn sine_1khz_peaks_in_its_bin_at_the_hann_scalloped_level() {
    // 1 kHz at -6 dBFS, fft 4096: bin 85 (996.09 Hz) is 1/3 bin off; Hann gain there = sinc(1/3)/(1-1/9) = -0.627 dB
    let r = run(&tone(1000., 10f64.powf(-6. / 20.), 2.), json!({})).unwrap();
    let d = 1. / 3.; let sc = 20. * ((std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d) / (1. - d * d)).log10();
    eprintln!("1 kHz -6 dBFS: peak {} Hz {} dBFS (expected {:.4})", r["peak_hz"], r["peak_dbfs"], -6. + sc);
    assert_eq!(f(&r["peak_hz"]), 85. * 48000. / 4096.);
    assert!((f(&r["peak_dbfs"]) - (-6. + sc)).abs() < 0.02, "{}", r["peak_dbfs"]);
    assert!((f(&r["total_dbfs"]) - (-6. - 3.0103)).abs() < 0.01, "{}", r["total_dbfs"]);
    assert_eq!(r["loudest_band"]["lo_hz"], json!(707.946)); assert_eq!(r["bands"].as_array().unwrap().len(), 10);
}
#[test] fn bin_centred_full_scale_sine_reads_0_dbfs_peak_and_minus_3_01_total() {
    let r = run(&tone(1500., 1., 1.), json!({"fft_size": 4096})).unwrap();   // 1500 Hz = bin 128 exactly
    eprintln!("1500 Hz full scale: peak {} dBFS, total {} dBFS", r["peak_dbfs"], r["total_dbfs"]);
    assert_eq!(f(&r["peak_hz"]), 1500.); assert!(f(&r["peak_dbfs"]).abs() < 0.01); assert!((f(&r["total_dbfs"]) + 3.0103).abs() < 0.01);
}
#[test] fn parseval_total_matches_time_domain_rms() {
    for (name, pcm) in [("white", white(3., 0.5, 7)), ("pink", pink(3., 11)), ("tone", tone(440., 0.3, 3.))] {
        let r = run(&pcm, json!({"bands": [0, 24000]})).unwrap();
        let t = rms_db(&pcm); eprintln!("{name}: total {} dBFS, one band 0..24k {} dBFS, time-domain RMS {t:.4} dBFS", r["total_dbfs"], r["bands"][0]["dbfs"]);
        assert!((f(&r["total_dbfs"]) - t).abs() < 0.05); assert!((f(&r["bands"][0]["dbfs"]) - f(&r["total_dbfs"])).abs() < 1e-9);
    }
}
#[test] fn white_noise_rises_3_db_per_octave_and_pink_is_flat() {
    let w = run(&white(20., 0.5, 3), json!({})).unwrap(); let p = run(&pink(20., 5), json!({})).unwrap();
    let band = |r: &Value, i: usize| f(&r["bands"][i]["dbfs"]);
    for i in 2..9 { // 125 Hz .. 8 kHz octave pairs
        let (dw, dp) = (band(&w, i + 1) - band(&w, i), band(&p, i + 1) - band(&p, i));
        eprintln!("octave {} -> {}: white {dw:+.3} dB, pink {dp:+.3} dB", w["bands"][i]["centre_hz"], w["bands"][i + 1]["centre_hz"]);
        assert!((dw - 3.0103).abs() < 0.3, "white {dw}"); assert!(dp.abs() < 0.5, "pink {dp}");
    }
}
#[test] fn silence_is_json_null_not_inf() {
    let r = run(&vec![0f32; 48000 * 2], json!({"hop_s": 0.25, "bands": "third_octave"})).unwrap();
    for k in ["peak_hz", "peak_dbfs", "total_dbfs", "loudest_band"] { assert!(r[k].is_null(), "{k}"); }
    assert!(r["bands"].as_array().unwrap().iter().all(|b| b["dbfs"].is_null())); assert!(r["spectrum"]["dbfs"].as_array().unwrap().iter().all(|v| v.is_null()));
    assert!(r["band_series"]["dbfs"].as_array().unwrap().iter().flat_map(|b| b.as_array().unwrap()).all(|v| v.is_null()));
    let s = serde_json::to_string(&r).unwrap(); assert!(!s.contains("inf") && !s.contains("NaN"));
}
#[test] fn out_of_range_arguments_are_errors_never_clamped() {
    let pcm = tone(440., 0.5, 2.);
    for a in [json!({"fft_size": 1000}), json!({"fft_size": 128}), json!({"fft_size": 131072}), json!({"fft_size": 4096.5}), json!({"hop_s": 0.005}), json!({"hop_s": 0.015}), json!({"hop_s": 61}), json!({"hop_s": 0.01}),
        json!({"max_points": 50}), json!({"hop_s": 0.1, "max_points": 5}), json!({"hop_s": 0.1, "max_points": 100001}), json!({"bands": [100, 50]}), json!({"bands": [100]}), json!({"bands": [0, 30000]}), json!({"bands": "fifth"}),
        json!({"spectrum_points": 65538}), json!({"spectrum_points": -1}), json!({"fft_size": 65536, "hop_s": 0.5})] {
        let e = run(&pcm, a.clone()); eprintln!("{a} -> {e:?}", e = e.as_ref().err()); assert!(e.is_err(), "{a} should be refused");
    }
    // a range shorter than fft_size is refused with the fix named
    assert!(run(&tone(440., 0.5, 0.05), json!({})).unwrap_err().contains("fewer than fft_size"));
}
#[test] fn max_points_raises_hop_like_measure_loudness_and_reports_both() {
    let r = run(&white(10., 0.3, 9), json!({"hop_s": 0.05, "max_points": 20})).unwrap(); let s = &r["band_series"];
    eprintln!("max_points 20: requested {} s, hop {} s, {} readings", s["requested_hop_s"], s["hop_s"], s["readings"]);
    assert_eq!(f(&s["requested_hop_s"]), 0.05); assert!(f(&s["hop_s"]) > 0.05); assert!(s["readings"].as_u64().unwrap() <= 20);
    assert_eq!(s["dbfs"].as_array().unwrap().len(), 10); assert_eq!(s["dbfs"][0].as_array().unwrap().len() as u64, s["readings"].as_u64().unwrap());
    // without max_points the hop is kept as asked
    let r = run(&white(10., 0.3, 9), json!({"hop_s": 0.5})).unwrap(); assert_eq!(f(&r["band_series"]["hop_s"]), 0.5); assert_eq!(r["band_series"]["readings"], json!(20));
}
#[test] fn band_series_follows_a_tone_that_moves_from_250_hz_to_4_khz() {
    let mut pcm = tone(250., 0.5, 1.); pcm.extend(tone(4000., 0.5, 1.));
    let r = run(&pcm, json!({"hop_s": 0.5, "fft_size": 2048})).unwrap(); let d = &r["band_series"]["dbfs"];
    let (b250, b4k) = (3, 7); // octave bands centred 251 Hz and 3981 Hz
    eprintln!("250 Hz band {:?}; 4 kHz band {:?}", d[b250], d[b4k]);
    // readings 0 and 3 lie wholly inside one tone; 1 and 2 hold frames that straddle the switch at 1.0 s (window leakage), so a smaller margin
    assert!(f(&d[b250][0]) > f(&d[b4k][0]) + 40.); assert!(f(&d[b4k][3]) > f(&d[b250][3]) + 40.);
    assert!(f(&d[b250][1]) > f(&d[b4k][1]) + 15.); assert!(f(&d[b4k][2]) > f(&d[b250][2]) + 15.);
    assert!((f(&d[b250][0]) + 9.031).abs() < 0.01 && (f(&d[b4k][3]) + 9.031).abs() < 0.01);   // a -6.02 dBFS sine reads -9.03 dBFS RMS
}
#[test] fn third_octave_and_spectrum_shape() {
    let r = run(&white(1., 0.3, 1), json!({"bands": "third_octave", "spectrum_points": 64})).unwrap(); let b = r["bands"].as_array().unwrap();
    assert_eq!(b.len(), 30); assert!((f(&b[0]["centre_hz"]) - 25.119).abs() < 0.001); assert!((f(&b[29]["centre_hz"]) - 19952.623).abs() < 0.001);
    let n = r["spectrum"]["points"].as_u64().unwrap(); assert!(n <= 64 && n > 40, "{n}"); assert_eq!(r["spectrum"]["hz"].as_array().unwrap().len() as u64, n);
    let all = run(&white(1., 0.3, 1), json!({"fft_size": 256, "spectrum_points": 200})).unwrap(); assert_eq!(all["spectrum"]["points"], json!(128));
    let none = run(&white(1., 0.3, 1), json!({"spectrum_points": 0})).unwrap(); assert!(none.get("spectrum").is_none());
}

fn wav(path: &str, frames: &[(i16, i16)]) {
    let data = (frames.len() * 4) as u32; let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes()); b.extend((48000u32 * 4).to_le_bytes()); b.extend(4u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
    for (l, r) in frames { b.extend(l.to_le_bytes()); b.extend(r.to_le_bytes()); }
    std::fs::write(path, b).unwrap();
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

#[test] fn engine_clip_and_mix_through_dispatch() {
    // 2 s stereo 1500 Hz (bin 128 at fft 4096) at -6.02 dBFS as a 16-bit WAV on a real track
    let path = format!("{}/kimo-spectrum-{}.wav", std::env::temp_dir().display(), std::process::id());
    let fr: Vec<(i16, i16)> = (0..96000).map(|i| { let v = (16384. * (TAU * 1500. * i as f64 / 48000.).sin()).round() as i16; (v, v) }).collect(); wav(&path, &fr);
    let mut s = Session::new(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let a = c(&mut s, "import_media", json!({"path": path}))["asset_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2}))["clip_id"].clone();
    let clip = c(&mut s, "audio_spectrum", json!({"clip_id": id, "hop_s": 0.5}));
    let mix = c(&mut s, "audio_spectrum", json!({"start": 0.5, "end": 1.5}));
    eprintln!("clip: peak {} Hz {} dBFS total {}; mix 0.5-1.5 s: peak {} dBFS range {}", clip["peak_hz"], clip["peak_dbfs"], clip["total_dbfs"], mix["peak_dbfs"], mix["range"]);
    let want = 20. * (16384f64 / 32768.).log10();
    for r in [&clip, &mix] { assert_eq!(f(&r["peak_hz"]), 1500.); assert!((f(&r["peak_dbfs"]) - want).abs() < 0.02, "{}", r["peak_dbfs"]); }
    assert_eq!(clip["band_series"]["readings"], json!(4)); assert_eq!(mix["range"], json!({"start": 0.5, "end": 1.5}));
    for a in [json!({"start": -1}), json!({"start": 1, "end": 1}), json!({"end": 2.5}), json!({"clip_id": "nope"}), json!({"start": 1.99})] {
        let e = dispatch(&mut s, "audio_spectrum", &a); eprintln!("{a} -> {:?}", e.as_ref().err()); assert!(e.is_err(), "{a}");
    }
    let cmd = kimo_core::commands::registry_ref().iter().find(|c| c.name == "audio_spectrum").expect("registered"); assert!(!cmd.mutates);
    assert_eq!(kimo_core::toolgroups::group_of("audio_spectrum"), "audio");
    let first = cmd.description.split(". ").next().unwrap(); assert!(first.len() < 150, "{}", first.len());
    std::fs::remove_file(path).ok();
}
