use kimo_core::ext::undistort::undistort_frame; use kimo_core::sfm::{norm_obs, Intr};
#[test] fn undistort_inverts_the_solver_lens_model() {
    let (w, h, f, k1, k2) = (320usize, 180usize, 300.0, -0.18, 0.05); let ki = Intr { f, cx: w as f64 / 2.0, cy: h as f64 / 2.0, k1, k2 };
    let ideal = |x: f64, y: f64| -> f32 { (0.5 + 0.25 * (x * 0.09).sin() + 0.25 * (y * 0.07 + x * 0.03).cos()) as f32 };
    // distorted image: pixel p shows the ideal scene at the undistorted position of p
    let mut dist = vec![0f32; w * h * 4]; for y in 0..h { for x in 0..w { let (ux, uy) = { let n = norm_obs((x as f64 + 0.5, y as f64 + 0.5), &ki); (n.0 * f + ki.cx, n.1 * f + ki.cy) }; let v = ideal(ux, uy); for c in 0..3 { dist[(y * w + x) * 4 + c] = v; } dist[(y * w + x) * 4 + 3] = 1.0; } }
    let out = undistort_frame(&dist, w, h, f, ki.cx, ki.cy, k1, k2); let (mut e, mut n, mut cov) = (0.0f64, 0usize, 0usize);
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; if out[i + 3] > 0.99 { cov += 1; if x > 4 && y > 4 && x < w - 5 && y < h - 5 { e += (out[i] - ideal(x as f64 + 0.5, y as f64 + 0.5)).abs() as f64; n += 1; } } } }
    let mae = e / n.max(1) as f64; eprintln!("undistort round trip: mean abs error {mae:.5} over {n} px, coverage {:.1}%", cov as f64 / (w * h) as f64 * 100.0); assert!(mae < 0.01 && cov > w * h * 8 / 10);
}
