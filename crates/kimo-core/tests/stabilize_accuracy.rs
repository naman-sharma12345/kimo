//! Warp stabilizer accuracy on synthetic shaky plates with exact ground truth (pure in-memory, no ffmpeg).
//!
//! World texture: an analytic band-limited function (60 random-direction sinusoids, wavelengths 6..48 px, plus 150 Gaussian blobs
//! of radius 2..6 px, values around 0..255), so every sub-pixel sample is exact. Camera T_t maps world coordinates to frame-t
//! pixel-index coordinates and frame_t(u, v) = tex(T_t^-1(u, v)). T_t = J_t G_t, both about the frame centre: G_t is the true smooth
//! path (pan (0.9, 0.35) px/frame, roll 0.01 deg/frame, zoom 1 + 0.0004 t), J_t the injected 2..8 Hz jitter (4 sinusoids with random
//! phases per parameter: translation 3 px RMS (vector), roll 0.35 deg RMS, scale 0.4 % RMS; the perspective plate adds keystone
//! h20, h21 of 2e-5 RMS). 384x216, 150 frames, 30 fps, seeded xorshift.
//! Variants: clean; noise (N(0, 2) per pixel); blur (8 sub-frame positions across a 180 degree shutter); outliers (3 independently
//! moving textured discs, 30% of the frame together, 3..6 px/frame, bouncing inside the frame).
//!
//! Metrics, on a 7x5 grid of world points x that land in the central 60% of frame 0, with q_t(x) = B_t(T_t(x)) and B from
//! frame_corrections(corrections(path, smooth_path(path)), framing):
//! (i) drift (no_motion): max over t and x of |q_t(x) - q_0(x)|;
//! (ii) path error (smooth_motion): fit ONE similarity K (least squares over all t and x) mapping G_t(x) to q_t(x);
//!      e_t(x) = q_t(x) - K(G_t(x)); RMS and worst |e|;
//! (iii) residual jitter: the high-pass e_t - Gauss_{sigma = 5 frames}(e)_t (odd reflection about the end samples); RMS over all t
//!      and x, and the worst frame (max over t of the RMS over x).
//! The input jitter is the same metric with B = identity (q_t = T_t x), so the reduction is visible. Smooth cases use the default
//! framing (crop_auto_scale, so the errors include the zoom). Drift uses stabilize_only: locking a 135 px pan leaves a crop of about
//! 60%, and auto-scale would multiply the drift by that zoom.
//! Run with an optimised profile (`cargo test --profile gate --test stabilize_accuracy -- --nocapture`); each case prints one line.
use kimo_core::robust_est::{self, Model};
use kimo_core::stabilize::*;
use kimo_core::trackmath::{self, H3, IDENT, P};
use std::f64::consts::TAU;

const W: usize = 384; const H: usize = 216; const N: usize = 150; const FPS: f64 = 30.0;
const CX: f64 = W as f64 * 0.5; const CY: f64 = H as f64 * 0.5;

struct Rng(u64);
impl Rng {
    fn new(s: u64) -> Rng { let mut r = Rng(s.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1); for _ in 0..8 { r.next(); } r }
    fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x }
    fn f(&mut self) -> f64 { (self.next() >> 11) as f64 / (1u64 << 53) as f64 }
    fn range(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.f() }
    fn gauss(&mut self) -> f64 { let (u, v) = (self.f().max(1e-300), self.f()); (-2.0 * u.ln()).sqrt() * (TAU * v).cos() }
}

/// Analytic texture: sinusoids [kx, ky, phase, amp] + Gaussian blobs [x, y, r, amp] bucketed on a 32 px grid.
struct Tex { waves: Vec<[f64; 4]>, blobs: Vec<[f64; 4]>, cells: Vec<Vec<usize>>, x0: f64, y0: f64, gw: usize, gh: usize }
const CELL: f64 = 32.0;
impl Tex {
    fn new(seed: u64, region: [f64; 4]) -> Tex {
        let mut r = Rng::new(seed);
        let waves: Vec<[f64; 4]> = (0..60).map(|_| { let lam = 6.0 * 8f64.powf(r.f()); let k = TAU / lam; let a = r.range(0.0, TAU); [k * a.cos(), k * a.sin(), r.range(0.0, TAU), 5.5] }).collect();
        let blobs: Vec<[f64; 4]> = (0..150).map(|_| { let (x, y, rr) = (r.range(region[0], region[2]), r.range(region[1], region[3]), r.range(2.0, 6.0)); let a = r.range(40.0, 80.0); [x, y, rr, if r.f() < 0.5 { -a } else { a }] }).collect();
        let (gw, gh) = (((region[2] - region[0]) / CELL).ceil() as usize + 1, ((region[3] - region[1]) / CELL).ceil() as usize + 1);
        let mut cells = vec![vec![]; gw * gh]; for (i, b) in blobs.iter().enumerate() { let (cx, cy) = (((b[0] - region[0]) / CELL) as usize, ((b[1] - region[1]) / CELL) as usize); cells[cy * gw + cx].push(i); }
        Tex { waves, blobs, cells, x0: region[0], y0: region[1], gw, gh }
    }
    fn blob(&self, x: f64, y: f64) -> f64 {
        let (cx, cy) = (((x - self.x0) / CELL).floor() as i64, ((y - self.y0) / CELL).floor() as i64); let mut s = 0.0;
        for j in cy - 1..=cy + 1 { for i in cx - 1..=cx + 1 { if i < 0 || j < 0 || i >= self.gw as i64 || j >= self.gh as i64 { continue; }
            for &k in &self.cells[j as usize * self.gw + i as usize] { let b = &self.blobs[k]; let d2 = (x - b[0]).powi(2) + (y - b[1]).powi(2); s += b[3] * (-d2 / (2.0 * b[2] * b[2])).exp(); } } }
        s
    }
    fn at(&self, x: f64, y: f64) -> f64 { let mut s = 128.0 + self.blob(x, y); for w in &self.waves { s += w[3] * (w[0] * x + w[1] * y + w[2]).sin(); } s }
    /// out[i] = tex(x0 + i dx, y0 + i dy), sinusoids by an exact rotation recurrence along the row.
    fn row(&self, x0: f64, y0: f64, dx: f64, dy: f64, out: &mut [f64]) {
        for v in out.iter_mut() { *v = 128.0; }
        for w in &self.waves { let (mut s, mut c) = (w[0] * x0 + w[1] * y0 + w[2]).sin_cos(); let (ds, dc) = (w[0] * dx + w[1] * dy).sin_cos();
            for v in out.iter_mut() { *v += w[3] * s; let ns = s * dc + c * ds; c = c * dc - s * ds; s = ns; } }
        for (i, v) in out.iter_mut().enumerate() { *v += self.blob(x0 + i as f64 * dx, y0 + i as f64 * dy); }
    }
}

fn tr(x: f64, y: f64) -> H3 { [[1.0, 0.0, x], [0.0, 1.0, y], [0.0, 0.0, 1.0]] }
/// T_c M T_-c: M acts about the frame centre.
fn about_c(m: H3) -> H3 { trackmath::mul(&trackmath::mul(&tr(CX, CY), &m), &tr(-CX, -CY)) }
fn sim_c(ls: f64, th: f64, tx: f64, ty: f64) -> H3 { let s = ls.exp(); about_c([[s * th.cos(), -s * th.sin(), tx], [s * th.sin(), s * th.cos(), ty], [0.0, 0.0, 1.0]]) }
fn ap(h: &H3, p: P) -> P { trackmath::apply(h, p).unwrap() }

#[derive(Clone, Copy, Default)]
struct Cfg { seed: u64, noise: f64, blur: bool, discs: bool, persp: bool, translation_only: bool }
struct Disc { p0: P, v: P, r: f64, off: P }
struct Plate { cfg: Cfg, tex: Tex, dtex: Tex, jit: Vec<Vec<(f64, f64, f64)>>, discs: Vec<Disc> }
impl Plate {
    fn new(cfg: Cfg) -> Plate {
        let region = [-120.0, -120.0, 720.0, 420.0]; let mut r = Rng::new(cfg.seed ^ 0xA5A5);
        let s2 = 2f64.sqrt(); let rms = [3.0 / s2, 3.0 / s2, if cfg.translation_only { 0.0 } else { 0.35f64.to_radians() }, if cfg.translation_only { 0.0 } else { 0.004 }, if cfg.persp { 2e-5 } else { 0.0 }, if cfg.persp { 2e-5 } else { 0.0 }];
        // 4 sinusoids of amplitude rms / sqrt(2) each: RMS of the sum = rms; frequencies 2..8 Hz
        let mut jit: Vec<Vec<(f64, f64, f64)>> = vec![]; for &q in &rms { jit.push((0..4).map(|_| (q / s2, r.range(2.0, 8.0) / FPS, r.range(0.0, TAU))).collect::<Vec<_>>()); }
        let discs: Vec<Disc> = if cfg.discs { let rr = (0.10 * (W * H) as f64 / std::f64::consts::PI).sqrt(); (0..3).map(|_| { let (sp, a) = (r.range(3.0, 6.0), r.range(0.0, TAU)); Disc { p0: (r.range(rr, W as f64 - rr), r.range(rr, H as f64 - rr)), v: (sp * a.cos(), sp * a.sin()), r: rr, off: (r.range(0.0, 400.0), r.range(0.0, 200.0)) } }).collect() } else { vec![] };
        Plate { cfg, tex: Tex::new(cfg.seed, region), dtex: Tex::new(cfg.seed ^ 0xD15C, region), jit, discs }
    }
    fn jv(&self, k: usize, tau: f64) -> f64 { self.jit[k].iter().map(|&(a, f, ph)| a * (TAU * f * tau + ph).sin()).sum() }
    fn g(&self, tau: f64) -> H3 { sim_c((1.0 + 0.0004 * tau).ln(), (0.01 * tau).to_radians(), -0.9 * tau, -0.35 * tau) }
    fn j(&self, tau: f64) -> H3 { let (s, th) = (self.jv(3, tau).exp(), self.jv(2, tau)); about_c([[s * th.cos(), -s * th.sin(), self.jv(0, tau)], [s * th.sin(), s * th.cos(), self.jv(1, tau)], [self.jv(4, tau), self.jv(5, tau), 1.0]]) }
    /// T_t: world -> frame t.
    fn cam(&self, tau: f64) -> H3 { trackmath::norm_h(&trackmath::mul(&self.j(tau), &self.g(tau))).unwrap() }
    fn disc_at(&self, d: &Disc, tau: f64) -> P {
        let b = |p: f64, v: f64, lo: f64, hi: f64| { let span = hi - lo; let mut x = (p - lo + v * tau).rem_euclid(2.0 * span); if x > span { x = 2.0 * span - x; } lo + x };
        (b(d.p0.0, d.v.0, d.r, W as f64 - d.r), b(d.p0.1, d.v.1, d.r, H as f64 - d.r))
    }
    fn render(&self, t: usize) -> Vec<f32> {
        let taus: Vec<f64> = if self.cfg.blur { (0..8).map(|k| t as f64 + ((k as f64 + 0.5) / 8.0 - 0.5) * 0.5).collect() } else { vec![t as f64] };
        let mut acc = vec![0f64; W * H]; let mut row = vec![0f64; W];
        for &tau in &taus { let ti = trackmath::norm_h(&trackmath::inv3(&self.cam(tau)).unwrap()).unwrap(); let affine = ti[2][0] == 0.0 && ti[2][1] == 0.0;
            let dp: Vec<P> = self.discs.iter().map(|d| self.disc_at(d, tau)).collect();
            for v in 0..H { let vf = v as f64;
                if affine { self.tex.row(ti[0][1] * vf + ti[0][2], ti[1][1] * vf + ti[1][2], ti[0][0], ti[1][0], &mut row); } else { for u in 0..W { let q = ap(&ti, (u as f64, vf)); row[u] = self.tex.at(q.0, q.1); } }
                for (k, d) in self.discs.iter().enumerate() { let (px, py) = dp[k]; if (vf - py).abs() >= d.r { continue; }
                    for u in 0..W { let uf = u as f64; if (uf - px).powi(2) + (vf - py).powi(2) < d.r * d.r { row[u] = self.dtex.at(uf - px + d.off.0, vf - py + d.off.1); } } }
                for u in 0..W { acc[v * W + u] += row[u]; } } }
        let k = 1.0 / taus.len() as f64; let mut rn = Rng::new(self.cfg.seed.wrapping_mul(31).wrapping_add(t as u64 * 7919 + 1));
        acc.iter().map(|v| { let n = if self.cfg.noise > 0.0 { self.cfg.noise * rn.gauss() } else { 0.0 }; (v * k + n).clamp(0.0, 255.0) as f32 }).collect()
    }
    /// The 7x5 world points that land in the central 60% of frame 0.
    fn grid(&self) -> Vec<P> { let ti = trackmath::inv3(&self.cam(0.0)).unwrap(); let mut v = vec![]; for j in 0..5 { for i in 0..7 { v.push(ap(&ti, (W as f64 * (0.2 + 0.6 * i as f64 / 6.0), H as f64 * (0.2 + 0.6 * j as f64 / 4.0)))); } } v }
}

/// Gaussian low-pass, kernel truncated at 3 sigma, odd reflection about the end samples (x[-k] = 2 x[0] - x[k]).
fn lowpass(x: &[f64], sigma: f64) -> Vec<f64> {
    let n = x.len() as i64; let r = (3.0 * sigma).ceil() as i64; let ws: Vec<f64> = (-r..=r).map(|k| (-((k * k) as f64) / (2.0 * sigma * sigma)).exp()).collect(); let wsum: f64 = ws.iter().sum();
    let at = |i: i64| -> f64 { if i < 0 { 2.0 * x[0] - x[(-i).min(n - 1) as usize] } else if i >= n { 2.0 * x[(n - 1) as usize] - x[(2 * (n - 1) - i).max(0) as usize] } else { x[i as usize] } };
    (0..n).map(|i| (-r..=r).map(|k| ws[(k + r) as usize] * at(i + k)).sum::<f64>() / wsum).collect()
}
#[derive(Debug, Clone, Copy)]
struct Metrics { jitter_rms: f64, jitter_worst: f64, path_rms: f64, path_worst: f64, drift: f64 }
fn metrics(p: &Plate, b: &[H3]) -> Metrics {
    let xs = p.grid(); let n = b.len(); let m = xs.len();
    let q: Vec<Vec<P>> = (0..n).map(|t| { let cam = p.cam(t as f64); xs.iter().map(|x| ap(&b[t], ap(&cam, *x))).collect() }).collect();
    let g: Vec<Vec<P>> = (0..n).map(|t| { let gt = p.g(t as f64); xs.iter().map(|x| ap(&gt, *x)).collect() }).collect();
    let drift = (0..n).flat_map(|t| (0..m).map(move |k| (t, k))).map(|(t, k)| trackmath::dist(q[t][k], q[0][k])).fold(0.0, f64::max);
    let (gs, qs): (Vec<P>, Vec<P>) = (0..n).flat_map(|t| (0..m).map(move |k| (t, k))).map(|(t, k)| (g[t][k], q[t][k])).unzip();
    let kk = robust_est::refit(Model::Similarity, &gs, &qs).unwrap();
    let e: Vec<Vec<P>> = (0..n).map(|t| (0..m).map(|k| { let f = ap(&kk, g[t][k]); (q[t][k].0 - f.0, q[t][k].1 - f.1) }).collect()).collect();
    let path_rms = (e.iter().flatten().map(|v| v.0 * v.0 + v.1 * v.1).sum::<f64>() / (n * m) as f64).sqrt(); let path_worst = e.iter().flatten().map(|v| v.0.hypot(v.1)).fold(0.0, f64::max);
    let mut hp2 = vec![0.0; n];
    for k in 0..m { for c in 0..2 { let s: Vec<f64> = (0..n).map(|t| if c == 0 { e[t][k].0 } else { e[t][k].1 }).collect(); let lp = lowpass(&s, 5.0); for t in 0..n { hp2[t] += (s[t] - lp[t]).powi(2); } } }
    let jitter_rms = (hp2.iter().sum::<f64>() / (n * m) as f64).sqrt(); let jitter_worst = hp2.iter().map(|v| (v / m as f64).sqrt()).fold(0.0, f64::max);
    Metrics { jitter_rms, jitter_worst, path_rms, path_worst, drift }
}

fn run(name: &str, cfg: Cfg, o: &Opts) -> Metrics {
    let p = Plate::new(cfg); let mut render_s = 0.0; let t0 = std::time::Instant::now();
    let est = { let mut frames = |i: usize| -> Result<Vec<f32>, String> { let s = std::time::Instant::now(); let f = p.render(i); render_s += s.elapsed().as_secs_f64(); Ok(f) }; estimate_path(&mut frames, N, W, H, o.method, o).unwrap_or_else(|e| panic!("{name}: {e}")) };
    let sm = smooth_path(&est.path, W, H, o.method, o); let fr = frame_corrections(&corrections(&est.path, &sm), W, H, o.framing);
    let ms = ((t0.elapsed().as_secs_f64() - render_s) * 1000.0 / N as f64).max(0.0);
    let m = metrics(&p, &fr.corr); let inp = metrics(&p, &vec![IDENT; N]);
    println!("case={name} method={} smoothing={} jitter_rms={:.4} jitter_worst={:.4} path_rms={:.4} path_worst={:.4} drift={:.4} input_jitter_rms={:.4} reanchors={} ms_per_frame={:.1}",
        o.method.name(), o.smoothing.name(), m.jitter_rms, m.jitter_worst, m.path_rms, m.path_worst, m.drift, inp.jitter_rms, est.reanchors, ms);
    assert!(inp.jitter_rms > 1.0, "{name}: the plate is not shaky ({})", inp.jitter_rms);
    m
}
fn smooth_opts() -> Opts { Opts { fps: FPS, ..Opts::default() } }
fn locked_opts() -> Opts { Opts { fps: FPS, mode: Mode::Locked, framing: Framing::StabilizeOnly, ..Opts::default() } }

#[test] fn clean_similarity_gaussian() { let m = run("clean", Cfg { seed: 11, ..Default::default() }, &smooth_opts());
    assert!(m.jitter_rms <= 0.05 && m.jitter_worst <= 0.20, "{m:?}"); assert!(m.path_rms <= 0.50 && m.path_worst <= 1.50, "{m:?}"); }
#[test] fn noise_similarity_gaussian() { let m = run("noise", Cfg { seed: 12, noise: 2.0, ..Default::default() }, &smooth_opts()); assert!(m.jitter_rms <= 0.10 && m.jitter_worst <= 0.40, "{m:?}"); }
#[test] fn blur_similarity_gaussian() { let m = run("blur", Cfg { seed: 13, blur: true, ..Default::default() }, &smooth_opts()); assert!(m.jitter_rms <= 0.10 && m.jitter_worst <= 0.40, "{m:?}"); }
#[test] fn outliers_similarity_gaussian() { let m = run("outliers", Cfg { seed: 14, discs: true, ..Default::default() }, &smooth_opts()); assert!(m.jitter_rms <= 0.08 && m.jitter_worst <= 0.30, "{m:?}"); }
#[test] fn locked_clean_drift() { let m = run("locked_clean", Cfg { seed: 11, ..Default::default() }, &locked_opts()); assert!(m.drift <= 0.10, "{m:?}");
    // reported, not gated: the same lock with crop + auto-scale framing (the drift is multiplied by the auto-scale zoom)
    let _ = run("locked_clean_crop_auto_scale", Cfg { seed: 11, ..Default::default() }, &Opts { framing: Framing::CropAutoScale, ..locked_opts() }); }
#[test] fn locked_noise_drift() { let m = run("locked_noise", Cfg { seed: 12, noise: 2.0, ..Default::default() }, &locked_opts()); assert!(m.drift <= 0.30, "{m:?}"); }
#[test] fn perspective_plate_perspective_method() { let o = Opts { method: Method::Perspective, ..smooth_opts() }; let m = run("perspective", Cfg { seed: 15, persp: true, ..Default::default() }, &o); assert!(m.jitter_rms <= 0.10 && m.jitter_worst <= 0.40, "{m:?}"); }
#[test] fn l1_clean() { let o = Opts { smoothing: Smoothing::L1, ..smooth_opts() }; let m = run("l1_clean", Cfg { seed: 11, ..Default::default() }, &o); assert!(m.jitter_rms <= 0.08 && m.path_rms <= 1.00, "{m:?}"); }
#[test] fn position_translation_only() { let o = Opts { method: Method::Position, ..smooth_opts() }; let m = run("position", Cfg { seed: 16, translation_only: true, ..Default::default() }, &o); assert!(m.jitter_rms <= 0.05, "{m:?}"); }

// ---------- unit tests ----------
#[test] fn gaussian_odd_reflection_reproduces_a_ramp() {
    let x: Vec<f64> = (0..80).map(|i| 3.0 - 0.7 * i as f64).collect();
    for s in [0.5, 2.0, 15.0, 60.0] { let y = gauss_odd(&x, s); let e = x.iter().zip(&y).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max); assert!(e <= 1e-9, "sigma {s}: {e}"); }
}
#[test] fn l1_preserves_a_monotone_pan() {
    // an exact constant-velocity pan comes back unchanged (the plain w1*|D1| objective would slow it by the box width)
    let ramp: Vec<f64> = (0..150).map(|i| 0.9 * i as f64 + 3.0).collect(); let p = l1_smooth(&ramp, 5.0);
    let e0 = p.iter().zip(&ramp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    // the same pan with 2..8 Hz shake (3 px RMS): error of the smoothed path against the true pan
    let mut r = Rng::new(21); let ph: Vec<f64> = (0..4).map(|_| r.f() * std::f64::consts::TAU).collect(); let fr = [2.0, 3.3, 5.1, 7.7];
    let shaky: Vec<f64> = (0..150).map(|i| { let t = i as f64 / FPS; ramp[i] + 3.0 * std::f64::consts::SQRT_2 / 2.0 * (0..4).map(|k| (std::f64::consts::TAU * fr[k] * t + ph[k]).sin()).sum::<f64>() }).collect();
    // box m: the crop budget smoothness 50 gives on these 384x216 plates (0.5 * 0.10 * 216 = 10.8 px). The L1 path can never leave
    // its box, so a box smaller than the shake's peak amplitude forces it to follow the shake: m = 5 px is printed for that reason.
    let err_of = |m: f64| { let q = l1_smooth(&shaky, m); let err: Vec<f64> = q.iter().zip(&ramp).map(|(a, b)| a - b).collect();
        ((err.iter().map(|e| e * e).sum::<f64>() / err.len() as f64).sqrt(), err.iter().fold(0.0f64, |m, e| m.max(e.abs()))) };
    let peak = shaky.iter().zip(&ramp).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
    let (rms, worst) = err_of(10.8); let (rms5, worst5) = err_of(5.0);
    println!("case=l1_monotone_pan exact_pan_max_err={e0:.3e} shake_peak={peak:.3} box=10.8 shaky_pan_path_rms={rms:.4} shaky_pan_path_worst={worst:.4} | box=5 path_rms={rms5:.4} path_worst={worst5:.4}");
    assert!(e0 <= 1e-6, "exact pan changed by {e0}"); assert!(rms <= 1.00, "shaky pan path rms {rms}");
}
#[test] fn l1_solver_stays_in_the_box_and_removes_curvature() {
    let mut r = Rng::new(5); let c: Vec<f64> = (0..120).map(|i| 0.5 * i as f64 + 2.0 * r.gauss()).collect(); let m = 5.0; let p = l1_smooth(&c, m);
    let d2 = |v: &[f64]| v.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).sum::<f64>();
    let worst = p.iter().zip(&c).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max); assert!(worst <= m + 1e-9, "outside the box by {}", worst - m);
    assert!(d2(&p) < 0.1 * d2(&c), "|D2 p| {} vs |D2 c| {}", d2(&p), d2(&c));
}
fn inside_tol(q: &[P; 4], p: P) -> bool { let mut sg = 0.0; for i in 0..4 { let (a, b) = (q[i], q[(i + 1) % 4]); let cr = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0); if cr.abs() < 1e-6 { continue; } if sg == 0.0 { sg = cr.signum(); } else if cr * sg < 0.0 { return false; } } true }
fn quad(b: &H3, r: [f64; 4]) -> [P; 4] { [ap(b, (r[0], r[1])), ap(b, (r[2], r[1])), ap(b, (r[2], r[3])), ap(b, (r[0], r[3]))] }
#[test] fn crop_is_inside_every_frame_and_auto_scale_fills_it() {
    let mut r = Rng::new(9); let corr: Vec<H3> = (0..40).map(|_| sim_c(r.range(-0.01, 0.01), r.range(-1.0, 1.0).to_radians(), r.range(-10.0, 10.0), r.range(-6.0, 6.0))).collect();
    let full = [0.0, 0.0, W as f64, H as f64];
    let c = frame_corrections(&corr, W, H, Framing::Crop); let rect = c.crop.expect("crop rect"); assert!(c.zoom == 1.0 && rect[0] > 0.0 && rect[2] < W as f64);
    assert!(((rect[2] - rect[0]) / (rect[3] - rect[1]) - W as f64 / H as f64).abs() < 1e-9, "aspect");
    for b in &corr { let q = quad(b, full); for p in [(rect[0], rect[1]), (rect[2], rect[1]), (rect[2], rect[3]), (rect[0], rect[3])] { assert!(inside_tol(&q, p), "crop corner {p:?} outside B(frame)"); } }
    let a = frame_corrections(&corr, W, H, Framing::CropAutoScale); assert!(a.crop.is_none() && a.zoom > 1.0 && (a.zoom - W as f64 / (rect[2] - rect[0])).abs() < 1e-9);
    for b in &a.corr { let q = quad(b, full); for p in [(0.0, 0.0), (W as f64, 0.0), (W as f64, H as f64), (0.0, H as f64)] { assert!(inside_tol(&q, p), "frame corner {p:?} not covered"); } }
    let s = frame_corrections(&corr, W, H, Framing::StabilizeOnly); assert!(s.zoom == 1.0 && s.crop.is_none() && s.corr == corr);
}
#[test] fn similarity_to_props_round_trips_through_the_transform_bb_convention() {
    for (ls, th, tx, ty) in [(0.03, 0.4, 12.5, -7.25), (-0.1, -2.9, -3.0, 40.0), (0.0, 0.0, 1.5, 2.5)] {
        let b = sim_c(ls, th, tx, ty); let (x, y, s, rd) = similarity_to_props(&b, W, H); let (co, si) = (rd.to_radians().cos(), rd.to_radians().sin());
        for p in [(0.0, 0.0), (W as f64, 0.0), (W as f64, H as f64), (0.0, H as f64)] { let (ux, uy) = ((p.0 - CX) * s, (p.1 - CY) * s); let o = (ux * co - uy * si + CX + x, ux * si + uy * co + CY + y); let want = ap(&b, p);
            assert!(trackmath::dist(o, want) < 1e-9, "{o:?} vs {want:?}"); } }
    let (_, _, s, r) = similarity_to_props(&tr(3.0, -4.0), W, H); assert_eq!((s, r), (1.0, 0.0));
}
#[test] fn enum_parsing_refuses_unknown_values() {
    assert_eq!(Method::parse("similarity").unwrap(), Method::Similarity); assert_eq!(Method::parse("position_scale_rotation").unwrap(), Method::Similarity); assert_eq!(Method::parse("perspective").unwrap(), Method::Perspective); assert_eq!(Method::parse("position").unwrap(), Method::Position);
    assert_eq!(Mode::parse("no_motion").unwrap(), Mode::Locked); assert_eq!(Smoothing::parse("l1").unwrap(), Smoothing::L1); assert_eq!(Framing::parse("crop").unwrap(), Framing::Crop);
    for bad in ["bogus", "", "Similarity", "subspace_warp"] { assert!(Method::parse(bad).is_err(), "{bad}"); }
    assert!(Mode::parse("locked").is_err()); assert!(Smoothing::parse("l2").is_err()); assert!(Framing::parse("synthesize").is_err());
    let e = Method::parse("bogus").unwrap_err(); for v in METHODS { assert!(e.contains(v), "{e}"); }
}
#[test] fn synthesize_edges_parses_and_keeps_the_corrections() {
    assert_eq!(Framing::parse("synthesize_edges").unwrap(), Framing::SynthesizeEdges); assert_eq!(Framing::SynthesizeEdges.name(), "synthesize_edges");
    assert!(FRAMINGS.contains(&"synthesize_edges")); for f in FRAMINGS { assert_eq!(Framing::parse(f).unwrap().name(), f); }
    let e = Framing::parse("bogus").unwrap_err(); assert!(e.contains("synthesize_edges"), "{e}");
    // corrections unchanged, no zoom, no crop (like stabilize_only)
    let corr = vec![IDENT, trackmath::mul(&tr(5.0, -3.0), &IDENT)]; let fr = frame_corrections(&corr, W, H, Framing::SynthesizeEdges);
    assert_eq!(fr.corr, corr); assert_eq!(fr.zoom, 1.0); assert!(fr.crop.is_none());
    // synthesis range in frames: round(secs * fps) clamped 1..30
    assert_eq!(synth_frames_for(0.3, 24.0), 7); assert_eq!(synth_frames_for(0.3, 30.0), 9); assert_eq!(synth_frames_for(0.03, 24.0), 1); assert_eq!(synth_frames_for(2.0, 60.0), 30); assert_eq!(Opts::default().synth_frames, 9);
}
