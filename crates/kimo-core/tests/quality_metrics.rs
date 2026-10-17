//! v6 eyes item #1: PSNR / SSIM / MS-SSIM on BT.709 luma (our own code from Wang et al. 2004 / 2003) against known references:
//! identical frames (PSNR null + identical, SSIM 1, MS-SSIM 1), Gaussian noise of known sigma on a flat image (PSNR against
//! 20 log10(255/sigma)), a 1-px shift and a 3x3 box blur cross-checked against an independent numpy implementation written
//! from the papers (2D 11x11 Gaussian via sliding windows, not separable), diff_frames' v5 output pinned byte for byte, the
//! diff_frames `quality` object, compare_ranges curves / summary / max_points hop raise, and argument refusals (never clamped).
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::quality::{luma_plane, ms_ssim, psnr_of, ssim_terms, summary};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

const W: usize = 256; const H: usize = 192;
/// The test pattern, also written in the numpy reference: smooth sinusoids plus a deterministic integer hash texture.
fn pat(x: usize, y: usize) -> f64 { let (xf, yf) = (x as f64, y as f64); 128. + 60. * (xf * 0.11).sin() * (yf * 0.07).cos() + 30. * ((xf + 2. * yf) * 0.31).sin() + ((x * 7919 + y * 104729) % 211) as f64 / 211. * 20. - 10. }
fn gray(w: usize, h: usize, f: impl Fn(usize, usize) -> f64) -> Image { let mut im = Image::new(w, h); for y in 0..h { for x in 0..w { let v = f(x, y) / 255.; for c in 0..3 { im.rgb[c][y * w + x] = v; } } } im }
fn a_img() -> Image { gray(W, H, pat) }
fn shift_img() -> Image { gray(W, H, |x, y| pat((x + 1).min(W - 1), y)) }
fn blur_img() -> Image { gray(W, H, |x, y| { let mut s = 0.; for dy in -1i64..=1 { for dx in -1i64..=1 { let xx = (x as i64 + dx).clamp(0, W as i64 - 1) as usize; let yy = (y as i64 + dy).clamp(0, H as i64 - 1) as usize; s += pat(xx, yy); } } s / 9. }) }
fn scores(a: &Image, b: &Image) -> (f64, f64, Option<f64>) { let (pa, pb) = (luma_plane(a), luma_plane(b)); let mse = pa.v.iter().zip(&pb.v).map(|(x, y)| (x - y).powi(2)).sum::<f64>() / pa.v.len() as f64; (mse, ssim_terms(&pa, &pb).0, ms_ssim(&pa, &pb).0) }

#[test] fn identical_frames_give_null_psnr_ssim_1_ms_ssim_1() {
    let a = a_img(); let (mse, s, ms) = scores(&a, &a_img());
    assert_eq!(mse, 0.); assert_eq!(psnr_of(mse, None), None); assert_eq!(psnr_of(mse, Some(100.)), Some(100.));
    assert!((s - 1.).abs() < 1e-12, "{s}"); assert!((ms.unwrap() - 1.).abs() < 1e-12, "{ms:?}");
}
#[test] fn gaussian_noise_psnr_matches_20_log10_255_over_sigma() {
    // flat Y' 128, 512x512, independent N(0, sigma) per pixel (Box-Muller on xorshift64*), equal in R, G, B so Y' carries it whole.
    // Tolerance 0.05 dB: the sample variance of 262144 draws has a relative std of sqrt(2/N) = 0.28 %, about 0.012 dB.
    for sigma in [2.0f64, 5.0, 10.0] {
        let mut s = 0x9E3779B97F4A7C15u64; let mut u = || { s ^= s >> 12; s ^= s << 25; s ^= s >> 27; ((s.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 + 0.5) / (1u64 << 53) as f64 };
        let n: Vec<f64> = (0..512 * 512).map(|_| (-2. * u().ln()).sqrt() * (std::f64::consts::TAU * u()).cos() * sigma).collect();
        let (mse, _, _) = scores(&gray(512, 512, |_, _| 128.), &gray(512, 512, |x, y| 128. + n[y * 512 + x]));
        let (got, want) = (psnr_of(mse, None).unwrap(), 20. * (255. / sigma).log10());
        eprintln!("sigma {sigma}: psnr {got:.4} dB, 20 log10(255/sigma) = {want:.4} dB, diff {:.4}", got - want);
        assert!((got - want).abs() < 0.05, "sigma {sigma}: {got} vs {want}");
    }
}
#[test] fn shift_and_blur_match_the_independent_numpy_reference() {
    // numpy reference (own code from the papers, 2D Gaussian 11x11 sigma 1.5 by sliding windows, valid region, mean of map;
    // MS-SSIM 5 scales, 2x2 mean downsample), run on the same pattern: tolerance 1e-9 (both are f64 sums in different orders)
    for (name, b, ssim_ref, ms_ref, mse_ref) in [("1-px shift", shift_img(), 0.8217012930753894, 0.9711152567332535, 153.14948020018628), ("3x3 box blur", blur_img(), 0.9359906005602, 0.992065912163567, 43.101088831306924)] {
        let (mse, s, ms) = scores(&a_img(), &b); let ms = ms.unwrap();
        eprintln!("{name}: mse {mse:.6} ssim {s:.10} (numpy {ssim_ref:.10}) ms_ssim {ms:.10} (numpy {ms_ref:.10})");
        assert!((s - ssim_ref).abs() < 1e-9, "{name} ssim {s}"); assert!((ms - ms_ref).abs() < 1e-9, "{name} ms_ssim {ms}"); assert!((mse - mse_ref).abs() < 1e-6, "{name} mse {mse}");
    }
}
#[test] fn summary_mean_min_p5_skip_nulls() {
    let t = [0., 1., 2., 3., 4., 5.]; let v = [Some(30.), None, Some(20.), Some(40.), Some(10.), Some(50.)];
    let s = summary(&t, &v); assert_eq!(s["n"], json!(5)); assert_eq!(s["mean"], json!(30.)); assert_eq!(s["min"], json!(10.)); assert_eq!(s["min_at_s"], json!(4.)); assert_eq!(s["max"], json!(50.));
    assert!((s["p5"].as_f64().unwrap() - 12.).abs() < 1e-12, "{}", s["p5"]); // numpy.percentile([10,20,30,40,50], 5) = 12
    assert_eq!(summary(&t[..1], &[None])["mean"], Value::Null);
}

fn session() -> Session {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": 320, "height": 180, "fps": 24})).unwrap();
    let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "solid", "color": "#336699", "start": 0, "duration": 1})).unwrap();
    dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "gradient", "start": 1, "duration": 1})).unwrap(); s
}
/// diff_frames output from the v5.0.0 (5ad355c) kimo-mcp over MCP (tools/call text) for the session above.
const V5: [(&str, &str); 4] = [
    (r#"{"a":{"time":0.5},"b":{"time":1.5}}"#, r#"{"a":{"h":180,"source":"composite","time":0.5,"w":320},"b":{"h":180,"source":"composite","time":1.5,"w":320},"compared":{"h":180,"method":"fixed-order f64, box downscale, Rec709 luma, SSIM luma 8x8 population variance C1=.01^2 C2=.03^2, 8x8x8 RGB histogram half-L1","region":[0,0,320,180],"scale":1.0,"space":"display-referred RGB u8/255","threshold":0.04,"w":320},"delta":{"bbox":[0,8,320,172],"bit_identical":false,"changed_percent":95.55555555555556,"changed_pixels":55040},"grid":{"cols":4,"mean_abs_delta":[0.1223529411764712,0.1223529411764712,0.1223529411764712,0.1223529411764712,0.3723747276688463,0.3723747276688463,0.3723747276688463,0.3723747276688463,0.6223965141612403,0.6223965141612403,0.6223965141612403,0.6223965141612403,0.8724183006536087,0.8724183006536087,0.8724183006536087,0.8724183006536087],"rows":4},"metrics":{"changed_ratio":0.9555555555555556,"luma_delta":0.49738562091490063,"mse":0.33073689606569745,"psnr_db":4.805173537288108},"verdict_hint":"major"}"#),
    (r#"{"a":{"time":0.5},"b":{"time":0.75}}"#, r#"{"a":{"h":180,"source":"composite","time":0.5,"w":320},"b":{"h":180,"source":"composite","time":0.75,"w":320},"compared":{"h":180,"method":"fixed-order f64, box downscale, Rec709 luma, SSIM luma 8x8 population variance C1=.01^2 C2=.03^2, 8x8x8 RGB histogram half-L1","region":[0,0,320,180],"scale":1.0,"space":"display-referred RGB u8/255","threshold":0.04,"w":320},"delta":{"bbox":null,"bit_identical":true,"changed_percent":0.0,"changed_pixels":0},"grid":{"cols":4,"mean_abs_delta":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],"rows":4},"metrics":{"changed_ratio":0.0,"luma_delta":0.0,"mse":0.0,"psnr_db":99.0},"verdict_hint":"identical"}"#),
    (r#"{"a":{"time":0.5},"b":{"time":1.5},"metrics":["mse","psnr","ssim","max_abs","changed_ratio","luma_delta","hist_distance"],"scale":1,"region":[10,20,200,100],"grid":{"cols":2,"rows":3},"threshold":0.1}"#, r#"{"a":{"h":180,"source":"composite","time":0.5,"w":320},"b":{"h":180,"source":"composite","time":1.5,"w":320},"compared":{"h":100,"method":"fixed-order f64, box downscale, Rec709 luma, SSIM luma 8x8 population variance C1=.01^2 C2=.03^2, 8x8x8 RGB histogram half-L1","region":[10,20,200,100],"scale":1.0,"space":"display-referred RGB u8/255","threshold":0.1,"w":200},"delta":{"bbox":[0,0,200,100],"bit_identical":false,"changed_percent":100.0,"changed_pixels":20000},"grid":{"cols":2,"mean_abs_delta":[0.20288350634370889,0.20288350634370889,0.3890671420083216,0.3890671420083216,0.5724301841948879,0.5724301841948879],"rows":3},"metrics":{"changed_ratio":1.0,"hist_distance":0.97,"luma_delta":0.38627450980404027,"max_abs":0.6627450980392157,"mse":0.17493917723953872,"psnr_db":7.571129203461918,"ssim":0.0011183577119760018},"verdict_hint":"major"}"#),
    (r#"{"a":{"frame":30,"source":"clip:clip3"},"b":{"time":1.5},"scale":64}"#, r#"{"a":{"h":180,"source":"clip:clip3","time":1.25,"w":320},"b":{"h":180,"source":"composite","time":1.5,"w":320},"compared":{"h":36,"method":"fixed-order f64, box downscale, Rec709 luma, SSIM luma 8x8 population variance C1=.01^2 C2=.03^2, 8x8x8 RGB histogram half-L1","region":[0,0,320,180],"scale":0.2,"space":"display-referred RGB u8/255","threshold":0.04,"w":64},"delta":{"bbox":null,"bit_identical":true,"changed_percent":0.0,"changed_pixels":0},"grid":{"cols":4,"mean_abs_delta":[0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0],"rows":4},"metrics":{"changed_ratio":0.0,"luma_delta":0.0,"mse":0.0,"psnr_db":99.0},"verdict_hint":"identical"}"#),
];
#[test] fn diff_frames_without_quality_is_byte_identical_to_v5() {
    let mut s = session();
    for (args, want) in V5 { let a: Value = serde_json::from_str(args).unwrap(); assert_eq!(serde_json::to_string(&dispatch(&mut s, "diff_frames", &a).unwrap()).unwrap(), want, "{args}"); }
}
#[test] fn diff_frames_quality_object() {
    let mut s = session();
    let q = dispatch(&mut s, "diff_frames", &json!({"a": {"time": 0.5}, "b": {"time": 0.75}, "quality": ["psnr", "ssim", "ms_ssim"]})).unwrap()["quality"].clone();
    assert_eq!(q["identical"], json!(true)); assert_eq!(q["psnr_db"], Value::Null); assert_eq!(q["ssim"], json!(1.0)); assert_eq!(q["ms_ssim"], json!(1.0)); assert_eq!(q["data_range"], json!(255.0)); assert_eq!(q["ms_ssim_scales"].as_array().unwrap().len(), 5);
    let q = dispatch(&mut s, "diff_frames", &json!({"a": {"time": 0.5}, "b": {"time": 0.75}, "quality": ["psnr"], "psnr_cap_db": 100})).unwrap()["quality"].clone();
    assert_eq!(q["psnr_db"], json!(100.0)); assert_eq!(q["identical"], json!(true)); assert!(q.get("ssim").is_none());
    let r = dispatch(&mut s, "diff_frames", &json!({"a": {"time": 0.5}, "b": {"time": 1.5}, "quality": ["psnr", "ssim"]})).unwrap(); let q = &r["quality"];
    let p = q["psnr_db"].as_f64().unwrap(); assert!(p.is_finite() && p > 0. && p < 30., "{p}"); assert_eq!(q["identical"], json!(false));
    assert!((p - 10. * (255f64.powi(2) / q["mse"].as_f64().unwrap()).log10()).abs() < 1e-9); assert!(q["ssim"].as_f64().unwrap() < 1.);
    let txt = serde_json::to_string(&r).unwrap(); assert!(!txt.contains("NaN") && !txt.contains("inf"));
    for (args, frag) in [(json!({"quality": "psnr"}), "array"), (json!({"quality": []}), "at least one"), (json!({"quality": ["vmaf"]}), "unknown quality metric"), (json!({"quality": ["psnr", "psnr"]}), "twice"),
        (json!({"psnr_cap_db": 100}), "needs quality"), (json!({"quality": ["ssim"], "psnr_cap_db": 100}), "needs quality"), (json!({"quality": ["psnr"], "psnr_cap_db": 0}), "psnr_cap_db"), (json!({"quality": ["psnr"], "psnr_cap_db": 1001}), "psnr_cap_db"),
        (json!({"quality": ["ms_ssim"], "scale": 64}), "176 px"), (json!({"quality": ["ssim"], "region": [0, 0, 10, 100]}), "11 px")] {
        let mut a = json!({"a": {"time": 0.5}, "b": {"time": 1.5}}); for (k, v) in args.as_object().unwrap() { a[k] = v.clone(); }
        let e = dispatch(&mut s, "diff_frames", &a).unwrap_err(); assert!(e.contains(frag), "{args}: {e}");
    }
}
#[test] fn compare_ranges_curves_summary_and_hop_raise() {
    let mut s = session();
    let r = dispatch(&mut s, "compare_ranges", &json!({"a": {"start": 0}, "b": {"start": 0.5}, "duration": 1, "hop_s": 0.25})).unwrap();
    assert_eq!(r["t_s"], json!([0.0, 0.25, 0.5, 0.75])); assert_eq!(r["points"], json!(4)); assert_eq!(r["identical_count"], json!(2));
    let p = r["psnr_db"].as_array().unwrap(); assert_eq!(p[0], Value::Null); assert_eq!(p[1], Value::Null); assert!(p[2].as_f64().unwrap() < 30.);
    assert_eq!(r["ssim"][0], json!(1.0)); assert_eq!(r["ms_ssim"][1], json!(1.0)); assert_eq!(r["summary"]["psnr_db"]["n"], json!(2)); assert_eq!(r["summary"]["ssim"]["n"], json!(4)); assert_eq!(r["summary"]["ssim"]["min_at_s"], json!(0.5));
    // samples 2 and 3 compare the same solid against the same gradient frame, so they equal one diff_frames call
    let d = dispatch(&mut s, "diff_frames", &json!({"a": {"time": 0.5}, "b": {"time": 1.0}, "quality": ["psnr", "ssim", "ms_ssim"]})).unwrap();
    assert_eq!(r["psnr_db"][2], d["quality"]["psnr_db"]); assert_eq!(r["ssim"][2], d["quality"]["ssim"]); assert_eq!(r["ms_ssim"][2], d["quality"]["ms_ssim"]);
    let c = dispatch(&mut s, "compare_ranges", &json!({"a": {}, "b": {}, "duration": 1, "hop_s": 0.25, "psnr_cap_db": 99, "quality": ["psnr"]})).unwrap();
    assert_eq!(c["psnr_db"], json!([99.0, 99.0, 99.0, 99.0])); assert_eq!(c["identical_count"], json!(4)); assert!(c.get("ssim").is_none());
    // max_points raises the hop (whole ms) so the curve holds at most max_points samples; both hops are returned
    let m = dispatch(&mut s, "compare_ranges", &json!({"a": {}, "b": {"start": 1}, "duration": 1, "hop_s": 0.1, "max_points": 4, "quality": ["psnr"]})).unwrap();
    assert_eq!(m["requested_hop_s"], json!(0.1)); assert_eq!(m["hop_s"], json!(0.25)); assert_eq!(m["points"], json!(4));
    let f = dispatch(&mut s, "compare_ranges", &json!({"a": {}, "b": {}, "duration": 1, "step_frames": 6, "quality": ["ssim"]})).unwrap();
    assert_eq!(f["step_frames"], json!(6)); assert_eq!(f["points"], json!(4)); assert_eq!(f["hop_s"], json!(0.25));
    let src = dispatch(&mut s, "compare_ranges", &json!({"a": {"source": "clip:clip3", "start": 1}, "b": {"start": 1}, "duration": 1, "hop_s": 0.5, "quality": ["psnr"]})).unwrap();
    assert_eq!(src["identical_count"], json!(2));
}
#[test] fn compare_ranges_refuses_bad_arguments() {
    let mut s = session();
    for (args, frag) in [
        (json!({"a": {}, "b": {}}), "duration"), (json!({"a": {}, "b": {}, "duration": 0}), "duration"), (json!({"a": {}, "b": {}, "duration": -1}), "duration"), (json!({"a": {}, "b": {}, "duration": 86401}), "duration"),
        (json!({"a": {}, "duration": 1}), "'b'"), (json!({"a": 1, "b": {}, "duration": 1}), "object"), (json!({"a": {"time": 0}, "b": {}, "duration": 1}), "source and start only"), (json!({"a": {"start": -1}, "b": {}, "duration": 1}), "start"),
        (json!({"a": {}, "b": {}, "duration": 1, "hop_s": 0}), "hop_s"), (json!({"a": {}, "b": {}, "duration": 1, "hop_s": 0.0015}), "whole number of milliseconds"), (json!({"a": {}, "b": {}, "duration": 1, "hop_s": 3601}), "hop_s"),
        (json!({"a": {}, "b": {}, "duration": 1, "hop_s": 0.5, "step_frames": 2}), "not both"), (json!({"a": {}, "b": {}, "duration": 1, "step_frames": 0}), "step_frames"), (json!({"a": {}, "b": {}, "duration": 1, "step_frames": 1.5}), "step_frames"),
        (json!({"a": {}, "b": {}, "duration": 1, "max_points": 1}), "max_points"), (json!({"a": {}, "b": {}, "duration": 1, "max_points": 2001}), "max_points"),
        (json!({"a": {}, "b": {}, "duration": 3, "hop_s": 0.001}), "cap 2000"), (json!({"a": {"start": 1.5}, "b": {}, "duration": 1}), "timeline ends"), (json!({"a": {}, "b": {}, "duration": 2.5}), "timeline ends"),
        (json!({"a": {"source": "clip:clip3"}, "b": {}, "duration": 1}), "clip's timeline span"), (json!({"a": {"source": "clip:nope"}, "b": {}, "duration": 1}), "clip not found"),
        (json!({"a": {}, "b": {}, "duration": 1, "quality": ["ssim", "vmaf"]}), "unknown quality metric"), (json!({"a": {}, "b": {}, "duration": 1, "quality": ["ssim"], "psnr_cap_db": 50}), "needs quality"),
        (json!({"a": {}, "b": {}, "duration": 1, "scale": 64}), "176 px"), (json!({"a": {}, "b": {}, "duration": 1, "scale": 0}), "scale"), (json!({"a": {}, "b": {}, "duration": 1, "region": [0, 0, 400, 10]}), "region")] {
        let e = dispatch(&mut s, "compare_ranges", &args).unwrap_err(); assert!(e.contains(frag), "{args}: {e}");
    }
}
#[test] fn compare_ranges_is_registered_read_only_with_a_short_first_sentence() {
    let c = kimo_core::commands::registry_ref().iter().find(|c| c.name == "compare_ranges").unwrap(); assert!(!c.mutates);
    let first = c.description.split(". ").next().unwrap(); assert!(first.chars().count() < 150, "{} chars", first.chars().count());
    let d = kimo_core::commands::registry_ref().iter().find(|c| c.name == "diff_frames").unwrap(); assert!(d.description.split(". ").next().unwrap().chars().count() < 150);
    assert_eq!(kimo_core::toolgroups::group_of("compare_ranges"), "output");
}
