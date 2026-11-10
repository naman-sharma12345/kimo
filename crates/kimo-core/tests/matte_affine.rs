use kimo_core::matte::{aff_step, aff_warp};
fn tex(x: f64, y: f64) -> f32 { (0.5 + 0.25 * (x * 0.21).sin() * (y * 0.17).cos() + 0.15 * ((x + 2.0 * y) * 0.09).sin() + 0.1 * (x * 0.5 + y * 0.43).sin()) as f32 }
#[test]
fn affine_step_recovers_known_motion() {
    let (w, h) = (320usize, 180usize); let (a, th, s) = (0.0f64, 3.0f64.to_radians(), 1.02f64); let (tx, ty) = (7.0, -3.0); let (cx, cy) = (160.0, 90.0);
    let m = [s * th.cos(), -s * th.sin(), s * th.sin(), s * th.cos(), cx + tx - (s * th.cos() * cx - s * th.sin() * cy), cy + ty - (s * th.sin() * cx + s * th.cos() * cy)]; let _ = a;
    let det = m[0] * m[3] - m[1] * m[2]; let mut gp = vec![0f32; w * h]; let mut gc = vec![0f32; w * h]; let mut al = vec![0f32; w * h];
    for y in 0..h { for x in 0..w { let (fx, fy) = (x as f64, y as f64); gp[y * w + x] = tex(fx, fy); let (dx, dy) = (fx - m[4], fy - m[5]); let (sx, sy) = ((m[3] * dx - m[1] * dy) / det, (-m[2] * dx + m[0] * dy) / det); gc[y * w + x] = tex(sx, sy);
        if (fx - cx).powi(2) + (fy - cy).powi(2) < 60.0 * 60.0 { al[y * w + x] = 1.0; } } }
    let e = aff_step(&gp, &gc, &al, w, h, None); eprintln!("est {:?}\ntrue {:?}", e, m);
    let wa = aff_warp(&al, w, h, &e); let wt = aff_warp(&al, w, h, &m); let (mut i, mut u) = (0.0, 0.0); for k in 0..w * h { i += wa[k].min(wt[k]) as f64; u += wa[k].max(wt[k]) as f64; }
    assert!(i / u > 0.97, "iou {}", i / u);
}
