//! E4: the blur effect is a real gaussian with sigma = radius / 2. Before, radius 1, 2 and 3 all quantized to one three-box kernel (sigma 1.158 px, 6.5% L1 off a gaussian).
use kimo_core::effects::{apply, Frame, Params};
fn blurred_frame(radius: f64, size: usize) -> Frame {
    let mut f = Frame { w: size, h: size, d: vec![0.0; size * size * 4] }; let c = size / 2; f.d[(c * size + c) * 4] = 1.0;
    let mut p = Params::new(); p.insert("radius".into(), kimo_core::keyframe::Prop::new(radius)); apply("blur", &p, &Default::default(), &mut f, 0.0, 0).unwrap(); f
}
fn energy(f: &Frame) -> f64 { (0..f.w * f.h).map(|i| f.d[i * 4] as f64).sum() }
fn blurred_row(radius: f64, size: usize) -> Vec<f64> {
    let mut f = Frame { w: size, h: size, d: vec![0.0; size * size * 4] }; let c = size / 2; f.d[(c * size + c) * 4] = 1.0;
    let mut p = Params::new(); p.insert("radius".into(), kimo_core::keyframe::Prop::new(radius)); apply("blur", &p, &Default::default(), &mut f, 0.0, 0).unwrap();
    (0..size).map(|x| f.d[(c * size + x) * 4] as f64).collect()
}
fn sigma_of(row: &[f64]) -> f64 { let c = row.len() / 2; let tot: f64 = row.iter().sum(); (row.iter().enumerate().map(|(i, v)| v * (i as f64 - c as f64).powi(2)).sum::<f64>() / tot).sqrt() }
fn l1_vs_gaussian(row: &[f64], sigma: f64) -> f64 { let c = row.len() / 2; let g: Vec<f64> = (0..row.len()).map(|i| (-(i as f64 - c as f64).powi(2) / (2.0 * sigma * sigma)).exp()).collect(); let gs: f64 = g.iter().sum(); let rs: f64 = row.iter().sum(); row.iter().zip(&g).map(|(a, b)| (a / rs - b / gs).abs()).sum::<f64>() }
#[test] fn sigma_scales_with_radius_and_is_gaussian_at_small_radii() {
    let mut last = 0.0;
    for r in [1.0, 2.0, 3.0, 6.0, 12.0] {
        let row = blurred_row(r, 129); let s = sigma_of(&row); let want = r / 2.0;
        let tol = if r < 2.0 { 0.10 } else { 0.05 }; // a sampled gaussian at sigma 0.5 has sigma 0.461 by construction
        assert!((s - want).abs() / want < tol, "radius {r}: sigma {s} want {want}"); assert!(s > last, "sigma must grow with radius"); last = s;
        let dev = l1_vs_gaussian(&row, want); assert!(dev < 0.02, "radius {r}: L1 deviation from a gaussian {dev}");
        eprintln!("radius {r}: sigma {s:.3} L1 {dev:.4}");
    }
}
#[test] fn large_sigma_box_path_keeps_the_variance_and_energy() {
    let row = blurred_row(40.0, 257); let s = sigma_of(&row); assert!((s - 20.0).abs() / 20.0 < 0.05, "sigma {s}"); let e = energy(&blurred_frame(40.0, 257)); assert!((e - 1.0).abs() < 1e-3, "energy {e}"); let _ = row;
}
#[test] fn zero_radius_is_identity_and_energy_is_conserved_away_from_edges() {
    let row = blurred_row(0.0, 33); assert_eq!(row[16], 1.0); let e = energy(&blurred_frame(5.0, 65)); assert!((e - 1.0).abs() < 1e-4, "energy {e}");
}
