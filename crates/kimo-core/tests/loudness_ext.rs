//! v6 eyes: measure_loudness extension (momentary / short-term curve, loudness range, clip count) against EBU Tech 3341 /
//! 3342-style synthetic signals, BS.1770 true peak cases, and a byte-for-byte pin of the v5 default output.
use kimo_core::commands::{dispatch, Session};
use kimo_core::editorial::{count_clipped, k_energy, k_energy_chunks, loudness, loudness_range, loudness_series, true_peak_4x};
use serde_json::{json, Value};
use std::f64::consts::TAU;

/// Stereo 1 kHz sine, phase-continuous across segments of (peak dBFS, seconds); a peak of L dBFS reads L LUFS (BS.1770 at 1 kHz).
fn tones(segs: &[(f64, f64)]) -> Vec<f32> {
    let mut pcm = vec![]; let mut i = 0usize;
    for &(db, secs) in segs { let a = 10f64.powf(db / 20.); for _ in 0..(secs * 48000.).round() as usize { let v = (a * (TAU * 1000. * i as f64 / 48000.).sin()) as f32; pcm.extend([v, v]); i += 1; } }
    pcm
}
fn chunks(pcm: &[f32]) -> Vec<f64> { k_energy_chunks(&k_energy(pcm).unwrap()) }
fn wav(path: &str, frames: &[(i16, i16)]) {
    let data = (frames.len() * 4) as u32; let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes()); b.extend((48000u32 * 4).to_le_bytes()); b.extend(4u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
    for (l, r) in frames { b.extend(l.to_le_bytes()); b.extend(r.to_le_bytes()); }
    std::fs::write(path, b).unwrap();
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// A session holding one 2 s media clip of `path` on one track; returns the clip id.
fn session_with(path: &str, secs: f64) -> (Session, String) {
    let mut s = Session::new(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let a = c(&mut s, "import_media", json!({"path": path}))["asset_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": secs}))["clip_id"].as_str().unwrap().to_string();
    (s, id)
}
fn tmp(name: &str) -> String { format!("{}/kimo-loudext-{}-{name}.wav", std::env::temp_dir().display(), std::process::id()) }

#[test] fn tech3341_integrated_cases_within_0_1_lu() {
    // EBU Tech 3341 cases 1-5 (stereo 1 kHz): expected -23.0 LUFS, case 2 -33.0; tolerance +/-0.1 LU
    let cases: [(&str, Vec<(f64, f64)>, f64); 5] = [
        ("case 1: -23 dBFS 20 s", vec![(-23., 20.)], -23.), ("case 2: -33 dBFS 20 s", vec![(-33., 20.)], -33.),
        ("case 3: -36/-23/-36 dBFS 10/60/10 s", vec![(-36., 10.), (-23., 60.), (-36., 10.)], -23.),
        ("case 4: -72/-36/-23/-36/-72 dBFS 10/10/60/10/10 s", vec![(-72., 10.), (-36., 10.), (-23., 60.), (-36., 10.), (-72., 10.)], -23.),
        ("case 5: -26/-20/-26 dBFS 20/20.1/20 s", vec![(-26., 20.), (-20., 20.1), (-26., 20.)], -23.)];
    for (name, segs, want) in cases { let l = loudness(&tones(&segs)).unwrap(); eprintln!("{name}: {l:.4} LUFS"); assert!((l - want).abs() <= 0.1, "{name}: {l}"); }
}
#[test] fn momentary_and_short_term_on_3341_tones() {
    for lvl in [-23., -33.] {
        let ch = chunks(&tones(&[(lvl, 20.)]));
        let m = loudness_series(&ch, 40, 10); let s = loudness_series(&ch, 300, 10);
        assert_eq!(m.len(), (2000 - 40) / 10 + 1); assert_eq!(s.len(), (2000 - 300) / 10 + 1);
        // every reading after the K-filter settles (window ends >= 0.5 s): momentary and short-term within 0.1 LU of the tone level
        for (k, v) in m.iter().enumerate().skip(1) { assert!((v.unwrap() - lvl).abs() <= 0.1, "{lvl} momentary #{k}: {v:?}"); }
        for (k, v) in s.iter().enumerate() { assert!((v.unwrap() - lvl).abs() <= 0.1, "{lvl} short-term #{k}: {v:?}"); }
    }
    // case 5 levels: the loudest 400 ms and 3 s windows sit inside the -20 dBFS segment, so both maxima read -20.0
    let ch = chunks(&tones(&[(-26., 20.), (-20., 20.1), (-26., 20.)]));
    let mx = |v: Vec<Option<f64>>| v.into_iter().flatten().fold(f64::MIN, f64::max);
    let (mm, ms) = (mx(loudness_series(&ch, 40, 10)), mx(loudness_series(&ch, 300, 10)));
    eprintln!("case 5 max momentary {mm:.4}, max short-term {ms:.4}");
    assert!((mm + 20.).abs() <= 0.1 && (ms + 20.).abs() <= 0.1);
    // silence reads null, never a drift value
    assert!(loudness_series(&chunks(&vec![0.; 96000]), 40, 10).iter().all(|v| v.is_none()));
    assert!(loudness_series(&chunks(&vec![0.; 9000]), 40, 10).is_empty(), "under 400 ms gives no reading");
}
#[test] fn tech3342_style_loudness_range() {
    // EBU Tech 3342 cases 1-4 (stereo 1 kHz, 20 s per level): LRA 10, 5, 20 and 15 LU. 3342's own tolerance is +/-1 LU;
    // these steady tones are asserted within 0.1 LU, at the default 100 ms hop and at a 1 s hop
    let cases: [(Vec<(f64, f64)>, f64); 4] = [(vec![(-20., 20.), (-30., 20.)], 10.), (vec![(-20., 20.), (-15., 20.)], 5.), (vec![(-40., 20.), (-20., 20.)], 20.),
        (vec![(-50., 20.), (-35., 20.), (-20., 20.), (-35., 20.), (-50., 20.)], 15.)];
    for (segs, want) in cases {
        let ch = chunks(&tones(&segs));
        for hop in [10, 100] {
            let (lra, lo, hi, n) = loudness_range(&loudness_series(&ch, 300, hop)).unwrap();
            eprintln!("{segs:?} hop {hop}0 ms: LRA {lra:.4} ({lo:.3}..{hi:.3}, {n} readings)");
            assert!((lra - want).abs() <= 0.1, "{segs:?} hop {hop}: {lra}");
        }
    }
    // case 4: the -50 dBFS segments fall under the relative gate (-20 LU below the gated mean), so low = -35
    let (_, lo, hi, _) = loudness_range(&loudness_series(&chunks(&tones(&[(-50., 20.), (-35., 20.), (-20., 20.), (-35., 20.), (-50., 20.)])), 300, 10)).unwrap();
    assert!((lo + 35.).abs() <= 0.1 && (hi + 20.).abs() <= 0.1, "{lo} {hi}");
    assert_eq!(loudness_range(&[None, Some(-80.)]), None, "nothing passes the absolute gate");
}
#[test] fn true_peak_intersample_over_and_bs1770_scale() {
    // fs/4 sine phased by pi/4: every sample sits at 0.7071 of the peak, so sample peak = A - 3.01 dB and true peak = A
    let a = 0.5f64; let pcm: Vec<f32> = (0..48000).flat_map(|i| { let fade = (i as f64 / 1000.).min((48000 - i) as f64 / 1000.).min(1.); let v = (a * fade * (TAU * 12000. * i as f64 / 48000. + std::f64::consts::FRAC_PI_4).sin()) as f32; [v, v] }).collect();
    let sp = 20. * pcm.iter().map(|v| v.abs() as f64).fold(0., f64::max).log10(); let tp = true_peak_4x(&pcm).unwrap().unwrap();
    eprintln!("fs/4 sine: sample peak {sp:.4} dBFS, true peak {tp:.4} dBTP");
    assert!((sp - (20. * a.log10() - 3.0103)).abs() < 0.01); assert!((tp - 20. * a.log10()).abs() < 0.05); assert!(tp - sp > 2.9);
    // a 1 kHz tone: 48 samples per cycle, the sample grid already holds the peak, so the two agree
    let t = tones(&[(-23., 1.)]); let sp1 = 20. * t.iter().map(|v| v.abs() as f64).fold(0., f64::max).log10();
    assert!((true_peak_4x(&t).unwrap().unwrap() - sp1).abs() < 0.01);
}
#[test] fn clipped_sample_count_fixture() {
    // 1 s 440 Hz at 0.5 with 37 left samples at +32767 and 12 right samples at -32768, all in different frames
    let mut fr: Vec<(i16, i16)> = (0..48000).map(|i| { let v = (0.5 * 32767. * (TAU * 440. * i as f64 / 48000.).sin()).round() as i16; (v, v) }).collect();
    for k in 0..37 { fr[100 + 1000 * k].0 = 32767; } for k in 0..12 { fr[40000 + 500 * k].1 = -32768; }
    let p = tmp("clip"); wav(&p, &fr); let (mut s, id) = session_with(&p, 1.);
    let r = c(&mut s, "measure_loudness", json!({"clip_id": id, "clip_threshold_dbfs": -0.001}));
    assert_eq!((r["clipping"]["clipped_samples"].as_u64(), r["clipping"]["clipped_frames"].as_u64()), (Some(49), Some(49)), "{}", r["clipping"]);
    assert_eq!(r["clipping"]["samples_total"], 96000);
    // at exactly 0 dBFS only the -32768 samples (decoded -1.0) count: +32767 decodes to 32767/32768
    let r0 = c(&mut s, "measure_loudness", json!({"clip_id": id, "clip_threshold_dbfs": 0}));
    assert_eq!(r0["clipping"]["clipped_samples"], 12);
    assert_eq!(count_clipped(&[1.0, 0.0, -1.0, 1.0, 0.5, 0.5], 1.0), (3, 2));
    // isolated full-scale samples do not overshoot on reconstruction (a lone spike interpolates below itself), so the true peak
    // equals the sample peak here; it never reads below it (the intersample case is true_peak_intersample_over_and_bs1770_scale)
    let t = c(&mut s, "measure_loudness", json!({"clip_id": id, "true_peak": true}));
    assert!(t["true_peak_dbfs"].as_f64().unwrap() >= t["sample_peak_dbfs"].as_f64().unwrap());
    std::fs::remove_file(p).unwrap();
}
#[test] fn tool_curve_and_lra_through_dispatch() {
    // 20 s at -20 dBFS then 20 s at -30 dBFS, written as 16-bit and decoded by the engine, measured through the tool
    let pcm = tones(&[(-20., 20.), (-30., 20.)]);
    let fr: Vec<(i16, i16)> = pcm.chunks_exact(2).map(|f| ((f[0] as f64 * 32767.).round() as i16, (f[1] as f64 * 32767.).round() as i16)).collect();
    let p = tmp("curve"); wav(&p, &fr); let (mut s, id) = session_with(&p, 40.);
    let r = c(&mut s, "measure_loudness", json!({"clip_id": id, "curve": "both", "lra": true}));
    let m = &r["loudness_curve"]["momentary"]; let st = &r["loudness_curve"]["short_term"]; let lr = &r["loudness_range"];
    assert_eq!(m["lufs"].as_array().unwrap().len(), (4000 - 40) / 10 + 1); assert_eq!(st["lufs"].as_array().unwrap().len(), (4000 - 300) / 10 + 1);
    assert_eq!((m["window_s"].as_f64(), st["window_s"].as_f64(), m["hop_s"].as_f64()), (Some(0.4), Some(3.0), Some(0.1)));
    assert!((m["max_lufs"].as_f64().unwrap() + 20.).abs() < 0.1 && (st["max_lufs"].as_f64().unwrap() + 20.).abs() < 0.1);
    let last = st["lufs"].as_array().unwrap().last().unwrap().as_f64().unwrap(); assert!((last + 30.).abs() < 0.1, "{last}");
    assert!((lr["lra_lu"].as_f64().unwrap() - 10.).abs() < 0.1, "{lr}");
    let i = r["integrated_lufs"].as_f64().unwrap(); eprintln!("tool: I {i:.3}, LRA {}, max M {}, max S {}", lr["lra_lu"], m["max_lufs"], st["max_lufs"]);
    // hop and a single curve: 1 s hop, short-term only
    let r2 = c(&mut s, "measure_loudness", json!({"clip_id": id, "curve": "short_term", "hop_s": 1.0}));
    assert!(r2["loudness_curve"].get("momentary").is_none()); assert_eq!(r2["loudness_curve"]["short_term"]["lufs"].as_array().unwrap().len(), 38);
    // payload cap: max_points raises the hop in 10 ms steps so each series holds at most that many readings
    let r3 = c(&mut s, "measure_loudness", json!({"clip_id": id, "curve": "both", "max_points": 50}));
    let m3 = r3["loudness_curve"]["momentary"]["lufs"].as_array().unwrap().len(); assert!(m3 <= 50 && m3 >= 45, "{m3}");
    assert_eq!(r3["loudness_curve"]["momentary"]["hop_s"].as_f64(), Some(0.81)); assert_eq!(r3["loudness_curve"]["requested_hop_s"].as_f64(), Some(0.1));
    assert!(r3.to_string().len() < 6000, "capped payload {} bytes", r3.to_string().len());
    for bad in [json!({"curve": "fast"}), json!({"curve": "both", "hop_s": 0.05}), json!({"curve": "both", "hop_s": 0.105}), json!({"hop_s": 0.1}), json!({"max_points": 50}), json!({"curve": "both", "max_points": 5}), json!({"lra": "yes"}), json!({"clip_threshold_dbfs": 3}), json!({"lra_hop_s": 0.1})] {
        let mut a = bad.clone(); a["clip_id"] = json!(id); assert!(dispatch(&mut s, "measure_loudness", &a).is_err(), "{bad}");
    }
    // out-of-range hop is a clear error, never clamped: below 0.1 s, above 3 s, zero, and the same for lra_hop_s
    for (bad, key) in [(json!({"curve": "both", "hop_s": 3.5}), "hop_s"), (json!({"curve": "momentary", "hop_s": 0.0}), "hop_s"), (json!({"curve": "both", "hop_s": 0.09}), "hop_s"), (json!({"lra": true, "lra_hop_s": 5.0}), "lra_hop_s")] {
        let mut a = bad.clone(); a["clip_id"] = json!(id);
        let e = dispatch(&mut s, "measure_loudness", &a).err().unwrap_or_else(|| panic!("{bad} was accepted"));
        assert!(e.contains(key) && e.contains("0.1..3 s"), "{bad}: unclear error {e:?}");
    }
    let r4 = c(&mut s, "measure_loudness", json!({"clip_id": id, "curve": "short_term", "hop_s": 3.0}));
    assert_eq!(r4["loudness_curve"]["short_term"]["hop_s"].as_f64(), Some(3.0), "3 s is the top of the range and is taken as given");
    let txt = r.to_string(); for k in ["pass", "fail", "compliant", "verdict", "ok"] { assert!(!txt.contains(&format!("\"{k}")), "no verdict field {k}"); }
    std::fs::remove_file(p).unwrap();
}
/// The v5.0.0 (5ad355c) measure_loudness output for the fixture below, captured byte for byte from a v5 kimo-mcp over MCP
/// (tools/call text). With none of the v6 arguments the tool must return exactly these bytes.
const V5_DEFAULT: &str = r#"{"integrated_lufs":-11.613243946662957,"method":"EBU R128 stereo 48k; clip measurement post filters, before volume/pan; timeline post mix/limiter","minimum_seconds":0.4,"sample_peak_dbfs":-12.247198959355487,"true_peak_dbfs":null,"true_peak_method":null}"#;
const V5_TRUE_PEAK: &str = r#"{"integrated_lufs":-11.613243946662957,"method":"EBU R128 stereo 48k; clip measurement post filters, before volume/pan; timeline post mix/limiter","minimum_seconds":0.4,"sample_peak_dbfs":-12.247198959355487,"true_peak_dbfs":-6.580630543345794,"true_peak_method":"4x 64-tap Blackman-windowed sinc estimate, not certified"}"#;
#[test] fn default_output_is_byte_identical_to_v5() {
    // fixture: 2 s, left = sawtooth ((i*37) % 2000 - 1000) * 8, right = 31-bit LCG noise in -8000..8000, 16-bit stereo
    let mut st: u64 = 0x2545F491; let fr: Vec<(i16, i16)> = (0..96000u64).map(|i| { st = (st * 1103515245 + 12345) & 0x7fff_ffff; ((((i * 37) % 2000) as i64 - 1000) as i16 * 8, (((st >> 16) % 16001) as i64 - 8000) as i16) }).collect();
    let p = tmp("v5pin"); wav(&p, &fr); let (mut s, id) = session_with(&p, 2.);
    for (args, want) in [(json!({}), V5_DEFAULT), (json!({"clip_id": id}), V5_DEFAULT), (json!({"true_peak": true}), V5_TRUE_PEAK), (json!({"clip_id": id, "true_peak": true}), V5_TRUE_PEAK), (json!({"true_peak": false}), V5_DEFAULT)] {
        assert_eq!(serde_json::to_string(&c(&mut s, "measure_loudness", args.clone())).unwrap(), want, "{args}");
    }
    std::fs::remove_file(p).unwrap();
}
