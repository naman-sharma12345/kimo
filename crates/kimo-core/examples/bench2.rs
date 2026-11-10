use kimo_core::effects::Frame; use std::time::Instant;
fn mk(w: usize, h: usize, sh: usize) -> Frame { let mut f = Frame::new(w, h); for y in 0..h { for x in 0..w { let xx = (x + sh) as f32; let v = 0.5 + 0.25 * (xx * 0.07).sin() * (y as f32 * 0.05).cos() + if ((x + sh) / 40 + y / 40) % 2 == 0 { 0.2 } else { 0.0 };
  let o = (y * w + x) * 4; f.d[o] = v; f.d[o+1] = v * 0.9; f.d[o+2] = v * 0.8; f.d[o+3] = 1.0; } } f }
fn hash(d: &[f32]) -> u64 { let mut h = 1469598103934665603u64; for v in d { h ^= v.to_bits() as u64; h = h.wrapping_mul(1099511628211); } h }
fn main() {
    let (w, h) = (1280, 720); let a = mk(w, h, 0); let b = mk(w, h, 6);
    let t = Instant::now(); let (u, v, _, _) = kimo_core::flow::flow(&a, &b, 5); let tf = t.elapsed().as_secs_f64();
    println!("flow 720p q5: {tf:.3}s hash {:016x}", hash(&u) ^ hash(&v));
    let t = Instant::now(); let o = kimo_core::flow::flow_blur(&a, &b, 0.5, 5); println!("flow_blur 720p: {:.3}s hash {:016x}", t.elapsed().as_secs_f64(), hash(&o.d));
    let mut c = vec![0f32; w * h]; for i in 0..w*h { c[i] = if (i % w > 300 && i % w < 900 && i / w > 200 && i / w < 500) { 1.0 } else { 0.0 }; }
    let t = Instant::now(); kimo_core::masks::blur_cov(&mut c, w, h, 12.0); println!("blur_cov s12: {:.3}s hash {:016x}", t.elapsed().as_secs_f64(), hash(&c));
    let mut p = kimo_core::color::denoise::DenoiseParams::default(); p.amount = 1.0; let mut n = a.d.clone(); let mut s = 7u32; for x in n.iter_mut() { s = s.wrapping_mul(1664525).wrapping_add(1013904223); *x += ((s >> 8) as f32 / 16777216.0 - 0.5) * 0.1; }
    let n2: Vec<f32> = n.iter().map(|x| x * 0.99).collect(); let t = Instant::now(); let o = kimo_core::color::denoise::denoise_frame(&n, &[&n2], w, h, &p); println!("denoise 720p: {:.3}s hash {:016x}", t.elapsed().as_secs_f64(), hash(&o));
}
