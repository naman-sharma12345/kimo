//! V4-0b dsp module: resample (R1-R4, R6), fft (R5), provenance and zero coupling (R8). Fixtures are generated in the test.
use kimo_core::dsp::fft::{Fft, FRAME};
use kimo_core::dsp::resample::{resample, true_peak};
use std::f32::consts::PI;
fn sine(f: f32, sr: f32, n: usize, ph: f32) -> Vec<f32> { (0..n).map(|i| (2.0 * PI * f * i as f32 / sr + ph).sin()).collect() }
fn tone_power(x: &[f32], f: f64, sr: f64) -> f64 {
    let n = x.len(); let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, v) in x.iter().enumerate() { let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos(); let ph = 2.0 * std::f64::consts::PI * f * i as f64 / sr; re += f64::from(*v) * w * ph.cos(); im += f64::from(*v) * w * ph.sin(); }
    (re * re + im * im) / (n as f64 * n as f64)
}
fn bits(v: &[f32]) -> Vec<u32> { v.iter().map(|x| x.to_bits()).collect() }
#[test] fn r1_round_trip_length_peak_and_error_floor_and_thread_equality() {
    let x = sine(1000.0, 48000.0, 48000, 0.0);
    let y = resample(&x, 48000, 44100).unwrap(); assert_eq!(y.len(), 44100);
    let pk = y[1000..43000].iter().fold(0.0f32, |m, v| m.max(v.abs())); assert!((pk - 1.0).abs() < 0.01, "peak {pk}");
    let z = resample(&y, 44100, 48000).unwrap(); assert_eq!(z.len(), 48000);
    let (a, b) = (2000, 46000); let e: f64 = (a..b).map(|i| { let d = f64::from(z[i]) - f64::from(x[i]); d * d }).sum::<f64>() / (b - a) as f64; let s: f64 = (a..b).map(|i| f64::from(x[i]).powi(2)).sum::<f64>() / (b - a) as f64;
    let db = 10.0 * (e / s).log10(); println!("R1 round-trip error {db:.2} dB RMS"); assert!(db < -80.0, "{db}");
    let hs: Vec<Vec<u32>> = (0..4).map(|_| { let x = x.clone(); std::thread::spawn(move || bits(&resample(&x, 48000, 44100).unwrap())) }).collect::<Vec<_>>().into_iter().map(|h| h.join().unwrap()).collect();
    for h in &hs { assert_eq!(h, &bits(&y), "4 concurrent threads are bit-equal to the single-thread result"); }
}
#[test] fn r2_alias_rejection_against_linear_interpolation() {
    let (sr, to) = (48000.0f32, 22050u32); let x = sine(15000.0, sr, 48000, 0.0); let y = resample(&x, 48000, to).unwrap();
    let ratio = f64::from(to) / 48000.0; let lin: Vec<f32> = (0..y.len()).map(|i| { let t = i as f64 / ratio; let k = t as usize; let f = (t - k as f64) as f32; let a = x[k.min(x.len() - 1)]; let b = x[(k + 1).min(x.len() - 1)]; a + (b - a) * f }).collect();
    let alias = 22050.0 - 15000.0; let (pl, ps) = (tone_power(&lin[500..20000], alias, 22050.0), tone_power(&y[500..20000], alias, 22050.0));
    println!("R2 alias power linear {pl:.3e} sinc {ps:.3e}"); assert!(pl > 1e-3, "linear reference must alias: {pl}"); assert!(ps < pl * 1e-3, "sinc {ps} vs linear {pl}");
}
#[test] fn r3_identity_is_bit_identical_and_zero_rate_is_a_typed_error() {
    let x = sine(440.0, 48000.0, 1000, 0.3); assert_eq!(bits(&resample(&x, 48000, 48000).unwrap()), bits(&x));
    for (f, t) in [(0, 44100), (44100, 0), (0, 0)] { let e = resample(&x, f, t).unwrap_err(); assert!(e.starts_with("out_of_range:"), "{e}"); }
}
#[test] fn r4_true_peak_finds_the_intersample_over_and_agrees_with_the_editorial_meter() {
    let y: Vec<f32> = (0..8192).map(|i| (2.0 * PI * 0.25 * i as f32 + PI / 4.0).sin()).collect();
    let sp = y.iter().fold(0.0f32, |m, v| m.max(v.abs())); let tp = true_peak(&y); println!("R4 sample peak {sp:.4} true peak {tp:.4}");
    assert!(sp < 0.72 && tp > 0.97, "{sp} {tp}");
    let st: Vec<f32> = y.iter().flat_map(|v| [*v, *v]).collect(); let ed_db = kimo_core::editorial::true_peak_4x(&st).unwrap().unwrap(); let ed = 10f64.powf(ed_db / 20.0); println!("R4 editorial {ed_db:.4} dBTP = {ed:.4} linear");
    assert!((f64::from(tp) - ed).abs() < 0.01, "dsp {tp} vs editorial {ed}");
}
#[test] fn r5_fft_matches_a_naive_dft_and_parseval_holds() {
    let x: Vec<f64> = (0..FRAME).map(|i| ((i * 37 % 101) as f64 / 50.0 - 1.0) + (i as f64 * 0.01).sin()).collect();
    let mut f = Fft::new(); f.forward_real(&x);
    let mut worst = 0.0f64; for k in [0usize, 1, 2, 5, 100, 511, 512, 700, 1023] { let (mut re, mut im) = (0.0, 0.0); for (n, v) in x.iter().enumerate() { let a = -2.0 * std::f64::consts::PI * (k * n % FRAME) as f64 / FRAME as f64; re += v * a.cos(); im += v * a.sin(); } worst = worst.max((re - f.re[k]).abs()).max((im - f.im[k]).abs()); }
    println!("R5 max |FFT - DFT| {worst:.3e}"); assert!(worst < 1e-9, "{worst}");
    let te: f64 = x.iter().map(|v| v * v).sum(); let fe: f64 = (0..FRAME).map(|k| f.re[k] * f.re[k] + f.im[k] * f.im[k]).sum::<f64>() / FRAME as f64; assert!((te - fe).abs() / te < 1e-12, "{te} {fe}");
}
#[test] fn r6_guards_and_a_malformed_fuzz_never_panic() {
    let ok = sine(1000.0, 48000.0, 480, 0.0);
    assert!(resample(&[], 48000, 44100).unwrap_err().starts_with("invalid_samples:")); assert!(resample(&[0.5], 48000, 44100).unwrap_err().starts_with("invalid_samples:"));
    assert!(resample(&ok, 1, 4_000_000_000).unwrap_err().starts_with("out_of_range:")); assert!(resample(&ok, 4_000_000_000, 1).unwrap_err().starts_with("out_of_range:"));
    assert!(resample(&[0.0, f32::NAN, 0.0], 48000, 44100).unwrap_err().starts_with("invalid_samples:"));
    let cap = kimo_core::guard::effective_limits().max_samples as usize; let big = vec![0.0f32; 1000]; let mut ratio_ok = 0; let _ = (cap, &big, &mut ratio_ok);
    let mut s = 0x9E3779B97F4A7C15u64; let mut r = || { s ^= s << 13; s ^= s >> 7; s ^= s << 17; s };
    let mut errs = 0; for _ in 0..1000 { let n = (r() % 40) as usize; let x: Vec<f32> = (0..n).map(|_| match r() % 7 { 0 => f32::NAN, 1 => f32::INFINITY, 2 => 1e30, _ => (r() % 2000) as f32 / 1000.0 - 1.0 }).collect();
        let pick = |v: u64| [0u32, 1, 8000, 44100, 48000, 96000, u32::MAX][(v % 7) as usize]; if resample(&x, pick(r()), pick(r())).is_err() { errs += 1; } }
    assert!(errs > 100, "fuzz exercised the error paths: {errs}");
}
fn tree(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) { for e in std::fs::read_dir(d).unwrap().flatten() { let p = e.path(); let n = p.file_name().unwrap().to_string_lossy().to_string(); if p.is_dir() { if n != "target" && n != ".git" && n != "node_modules" { tree(&p, out) } } else { out.push(p) } } }
#[test] fn r8_every_dsp_file_has_a_provenance_header_and_is_in_the_notices_and_nothing_else_names_the_origin() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap(); let notices = std::fs::read_to_string(root.join("THIRD_PARTY_NOTICES.md")).unwrap();
    let dsp = root.join("crates/kimo-core/src/dsp"); let mut n = 0;
    for e in std::fs::read_dir(&dsp).unwrap().flatten() { let name = e.file_name().to_string_lossy().to_string(); let txt = std::fs::read_to_string(e.path()).unwrap(); n += 1;
        assert!(txt.lines().next().unwrap().starts_with("//! Provenance:"), "{name}: first line must be the provenance header");
        assert!(notices.contains(&format!("crates/kimo-core/src/dsp/{name}")), "{name} missing from THIRD_PARTY_NOTICES.md"); }
    assert!(n >= 3); assert!(notices.contains("Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors") && notices.contains("eac0edd") && notices.contains("907fb0a"));
    let word = ["beat", "box"].concat(); let mut files = vec![]; tree(&root, &mut files); let mut bad = vec![];
    for f in files { if f.file_name().unwrap() == "THIRD_PARTY_NOTICES.md" { continue; } let Ok(txt) = std::fs::read_to_string(&f) else { continue };
        let in_dsp = f.starts_with(&dsp); for (i, l) in txt.lines().enumerate() { if l.to_lowercase().contains(&word) { let header = in_dsp && l.starts_with("//!") && i < 8; if !header { bad.push(format!("{}:{}", f.strip_prefix(&root).unwrap().display(), i + 1)); } } } }
    assert!(bad.is_empty(), "origin name outside the notices and provenance headers: {bad:?}");
}
#[test] fn r7_wire_up_read_samples_rate_and_import_samples_native_rate() {
    use kimo_core::commands::{dispatch, Session};
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("kimo-dsp-r7-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::new(); s.asset_dir = Some(dir.clone());
    let tone: Vec<f32> = sine(1000.0, 44100.0, 2205, 0.0);
    let a = dispatch(&mut s, "import_samples", &json!({"rate":44100,"channels":1,"data":[tone.clone()]})).unwrap();
    assert_eq!(a["rate"], 48000); assert_eq!(a["source_info"]["native_rate"], 44100); assert_eq!(a["n_samples"], 2400);
    let t = format!("file:{}", a["path"].as_str().unwrap());
    let r1 = dispatch(&mut s, "read_samples", &json!({"target": t, "rate": 44100})).unwrap(); assert_eq!(r1["rate"], 44100);
    let n = r1["n_samples"].as_u64().unwrap(); assert!((2204..=2206).contains(&n), "round trip length {n}");
    for threads in [1, 4, 1] { kimo_core::par::set_threads(threads); let r = dispatch(&mut s, "read_samples", &json!({"target": t, "rate": 44100})).unwrap(); assert_eq!(r["hash"], r1["hash"], "stable at {threads} threads"); }
    let _ = std::fs::remove_dir_all(dir);
}
