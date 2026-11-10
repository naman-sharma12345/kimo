use kimo_core::color::idt::*;
use kimo_core::{color::managed::mv, effects::{Frame, Params}, keyframe::Prop};
fn rng(s: &mut u64) -> f64 { *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((*s >> 33) as f64) / (1u64 << 31) as f64 }
const M: [[f64; 3]; 3] = [[1.1, 0.05, -0.02], [-0.03, 0.95, 0.08], [0.01, -0.06, 1.2]];
fn chart(g: [f64; 3]) -> Vec<([f64; 3], [f64; 3])> { let mut s = 7u64; (0..24).map(|_| { let m = [0.02 + 0.9 * rng(&mut s), 0.02 + 0.9 * rng(&mut s), 0.02 + 0.9 * rng(&mut s)]; let x = [m[0].powf(g[0]), m[1].powf(g[1]), m[2].powf(g[2])]; (m, mv(M, x)) }).collect() }
#[test] fn fit_accuracy_matrix() { let f = fit(&chart([1.; 3]), false).unwrap(); assert!(f.max_err < 1e-6 && !f.curve_fitted, "{:?}", f); for i in 0..3 { for j in 0..3 { assert!((f.profile.matrix[i][j] - M[i][j]).abs() < 1e-6) } } }
#[test] fn fit_deterministic() { let a = fit(&chart([1.; 3]), true).unwrap(); let b = fit(&chart([1.; 3]), true).unwrap(); assert_eq!(a.profile, b.profile); }
#[test] fn fit_curve_on_neutral_ramp() {
    let g = [2.2, 2.0, 2.4]; let gain = [1.0, 0.9, 0.8]; let d = |v: f64| [v, v, v];
    let mut p: Vec<_> = (1..=8).map(|i| { let v = i as f64 / 9.; (std::array::from_fn(|c| (v / gain[c]).powf(1. / g[c])), d(v)) }).collect();
    let mut s = 3u64; for _ in 0..12 { let m: [f64; 3] = std::array::from_fn(|_| 0.05 + 0.9 * rng(&mut s)); p.push((m, std::array::from_fn(|c| gain[c] * m[c].powf(g[c])))) }
    let f = fit(&p, true).unwrap(); assert!(f.curve_fitted && f.max_err < 1e-5, "{:?}", f); for c in 0..3 { assert!((f.profile.gamma[c] - g[c]).abs() < 1e-6) } }
#[test] fn fit_hostile() { assert!(fit(&[], false).is_err()); let mut c = chart([1.; 3]); assert!(fit(&c[..5], false).is_err()); c[0].0[1] = f64::NAN; assert!(fit(&c, false).is_err());
    let flat: Vec<_> = (0..10).map(|_| ([0.5; 3], [0.5; 3])).collect(); assert!(fit(&flat, false).is_err()); let z: Vec<_> = (0..10).map(|_| ([0.; 3], [0.; 3])).collect(); assert!(fit(&z, true).is_err()); }
#[test] fn profile_roundtrip_and_hostile() { let f = fit(&chart([1.; 3]), false).unwrap(); let r = from_text(&to_text(&f.profile)).unwrap(); assert_eq!(r, f.profile); assert_eq!(from_text("  ").unwrap(), Profile::default());
    for bad in ["x 1", "kimo-idt-v1 1 1 1", "kimo-idt-v1 0 1 1 1 0 0 0 1 0 0 0 1", "kimo-idt-v1 1 1 1 1 0 0 0 1 0 0 0 NaN", "kimo-idt-v1 1 1 1 1e300 0 0 0 1 0 0 0 1"] { assert!(from_text(bad).is_err(), "{bad}") } }
fn frame() -> Frame { Frame { w: 2, h: 1, d: vec![0.2, 0.4, 0.6, 1., 0.1, 0.9, 0.3, 0.5] } }
#[test] fn identity_cases() { let mut f = frame(); let b = f.d.clone(); apply(&Params::new(), &mut f, 0.).unwrap(); assert_eq!(f.d, b);
    let pr = Profile { matrix: M, gamma: [1.2; 3] }; apply_profile(&pr, &mut f, 0.).unwrap(); assert_eq!(f.d, b); }
#[test] fn applies_alpha_untouched_deterministic() { let pr = Profile { matrix: M, gamma: [1.2, 1., 0.9] }; let (mut a, mut b) = (frame(), frame()); apply_profile(&pr, &mut a, 1.).unwrap(); apply_profile(&pr, &mut b, 1.).unwrap(); assert_eq!(a.d, b.d); assert_eq!(a.d[3], 1.); assert_eq!(a.d[7], 0.5); assert_ne!(a.d[0], 0.2);
    let mut h = frame(); apply_profile(&pr, &mut h, 0.5).unwrap(); let e = (0.2 + (a.d[0] - 0.2) * 0.5) as f32; assert!((h.d[0] - e).abs() < 1e-6); }
#[test] fn hostile_params_no_mutation() { for (k, v) in [("amount", 2.), ("amount", f64::NAN), ("m00", f64::INFINITY), ("m11", 1e9), ("gamma_r", 0.), ("gamma_g", 50.), ("gamma_b", f64::NAN)] { let mut p = Params::new(); p.insert(k.into(), Prop::new(v)); let mut f = frame(); let b = f.d.clone(); assert!(apply(&p, &mut f, 0.).is_err(), "{k}"); assert_eq!(f.d, b) }
    let mut f = Frame { w: 1, h: 1, d: vec![f32::NAN, 0., 0., 1.] }; assert!(apply(&Params::new(), &mut f, 0.).is_err()); }
fn logframe(lo: f32) -> Frame { Frame { w: 64, h: 32, d: (0..2048).flat_map(|i| { let v = lo + (i as f32 / 2048.) * (0.9 - lo); [v, v, v, 1.] }).collect() } }
#[test] fn picker_conservative() { let ok = logframe(0.1); assert_eq!(pick_encoding(&ok, "LogC3"), Pick::Select(4)); assert_eq!(pick_encoding(&ok, "S-Log3"), Pick::Select(5)); assert_eq!(pick_encoding(&ok, "ACEScct"), Pick::Select(3));
    for h in ["", "log", "unknown", "sRGB"] { assert!(matches!(pick_encoding(&ok, h), Pick::Defer(_))) }
    assert!(matches!(pick_encoding(&logframe(0.0), "LogC3"), Pick::Defer(_))); let small = Frame { w: 2, h: 2, d: vec![0.3; 16] }; assert!(matches!(pick_encoding(&small, "LogC3"), Pick::Defer(_)));
    let mut bad = logframe(0.1); bad.d[0] = 1.5; assert!(matches!(pick_encoding(&bad, "LogC3"), Pick::Defer(_))); let mut n = logframe(0.1); n.d[0] = f32::NAN; assert!(matches!(pick_encoding(&n, "LogC3"), Pick::Defer(_))); }
