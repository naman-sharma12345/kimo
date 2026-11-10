use kimo_core::matting::*; use kimo_core::sfm::Rng;
fn noise_tex(w: usize, h: usize, seed: u64, base: [f32; 3]) -> Vec<f32> { let mut r = Rng(seed); let mut o = vec![0f32; w * h * 3]; let mut grid = vec![[0f32; 3]; 40 * 40]; for g in grid.iter_mut() { for c in 0..3 { g[c] = (r.next() % 1000) as f32 / 1000.0; } }
    for y in 0..h { for x in 0..w { let (fx, fy) = (x as f32 / 8.0, y as f32 / 8.0); let (gx, gy) = (fx as usize, fy as usize); let (ax, ay) = (fx - gx as f32, fy - gy as f32); for c in 0..3 { let g = |a: usize, b: usize| grid[(b % 40) * 40 + a % 40][c]; let v = g(gx, gy) * (1.0 - ax) * (1.0 - ay) + g(gx + 1, gy) * ax * (1.0 - ay) + g(gx, gy + 1) * (1.0 - ax) * ay + g(gx + 1, gy + 1) * ax * ay; o[(y * w + x) * 3 + c] = (base[c] + 0.35 * (v - 0.5)).clamp(0.0, 1.0); } } } o }
#[test] fn refines_soft_edges_and_hair_better_than_rough_seed() {
    let (w, h) = (256usize, 192usize); let fgt = noise_tex(w, h, 1, [0.75, 0.35, 0.3]); let bgt = noise_tex(w, h, 2, [0.25, 0.45, 0.7]);
    let mut gt = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let (dx, dy) = ((x as f32 - 128.0) / 70.0, (y as f32 - 96.0) / 55.0); let d = (dx * dx + dy * dy).sqrt(); gt[y * w + x] = ((1.0 - d) * 70.0 / 2.5 + 0.5).clamp(0.0, 1.0); } }
    for s in 0..40 { let th = s as f32 / 40.0 * 6.2832; for t in 0..14 { let r = 1.0 + t as f32 * 0.012; let (x, y) = ((128.0 + 70.0 * r * th.cos()) as usize, (96.0 + 55.0 * r * th.sin()) as usize); if x < w && y < h { gt[y * w + x] = gt[y * w + x].max(0.55 - t as f32 * 0.03); } } }
    let mut img = vec![0f32; w * h * 3]; for i in 0..w * h { for c in 0..3 { img[i * 3 + c] = gt[i] * fgt[i * 3 + c] + (1.0 - gt[i]) * bgt[i * 3 + c]; } }
    let mut seed = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let (dx, dy) = ((x as f32 - 130.0) / 69.0, (y as f32 - 95.0) / 56.0); seed[y * w + x] = if dx * dx + dy * dy < 1.0 { 1.0 } else { 0.0 }; } }
    let tri = trimap_from_alpha(&seed, w, h, 8); let m = matte(&img, &tri, w, h, &Opts::default());
    let err = |a: &[f32]| -> (f32, f32) { let (mut s, mut sb, mut nb) = (0.0, 0.0, 0usize); for i in 0..w * h { let e = (a[i] - gt[i]).abs(); s += e; if gt[i] > 0.02 && gt[i] < 0.98 { sb += e; nb += 1; } } (s / (w * h) as f32 * 1000.0, sb / nb.max(1) as f32) };
    let (mae_seed, edge_seed) = err(&seed); let (mae_m, edge_m) = err(&m.alpha);
    eprintln!("unknown px {}; MAE x1000: seed {:.2} -> matte {:.2}; mean error on partial-alpha px: seed {:.3} -> matte {:.3}", m.unknown, mae_seed, mae_m, edge_seed, edge_m);
    assert!(mae_m < mae_seed * 0.5 && edge_m < edge_seed * 0.6);
}
