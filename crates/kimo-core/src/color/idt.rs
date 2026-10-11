//! Chart-calibrated input transform (IDT): optional per-channel power curve + 3x3 matrix into the working
//! space, fit from measured chart patches, saved as a reusable text profile. Also a conservative source-encoding
//! picker that defers unless a declared hint is uniquely identified and not contradicted by the pixels.
//! Input to `apply` is assumed to be the camera's native RGB (curve is a plain per-channel power). Alpha untouched.
use super::managed::{mv, Mat};
use crate::effects::{p, Frame, Params};
pub const IDENTITY: Mat = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
#[derive(Clone, Debug, PartialEq)]
pub struct Profile { pub matrix: Mat, pub gamma: [f64; 3] }
impl Default for Profile { fn default() -> Self { Profile { matrix: IDENTITY, gamma: [1.; 3] } } }
#[derive(Clone, Debug)]
pub struct Fit { pub profile: Profile, pub rms: f64, pub max_err: f64, pub curve_fitted: bool }
fn det(a: &Mat) -> f64 { a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]) }
fn inv(a: &Mat) -> Option<Mat> {
    let d = det(a); if !d.is_finite() || d.abs() < 1e-300 { return None }
    let c = |i: usize, j: usize| { let (r0, r1) = ((i + 1) % 3, (i + 2) % 3); let (c0, c1) = ((j + 1) % 3, (j + 2) % 3); a[r0][c0] * a[r1][c1] - a[r0][c1] * a[r1][c0] };
    Some(std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / d)))
}
fn pw(v: f64, g: f64) -> f64 { v.abs().powf(g).copysign(v) }
fn lin(m: &Profile, c: [f64; 3]) -> [f64; 3] { mv(m.matrix, std::array::from_fn(|i| pw(c[i], m.gamma[i]))) }
/// Fit from (measured camera RGB, reference working-space linear RGB) patches. `fit_curve` estimates per-channel
/// gamma from neutral reference patches (needs >=4 with a real tonal spread), otherwise gamma stays 1.
pub fn fit(patches: &[([f64; 3], [f64; 3])], fit_curve: bool) -> Result<Fit, String> {
    if patches.len() < 6 { return Err("need at least 6 chart patches".into()) }
    if patches.len() > 100_000 { return Err("too many patches".into()) }
    for (m, r) in patches { if m.iter().chain(r.iter()).any(|v| !v.is_finite() || v.abs() > 1e6) { return Err("non-finite or absurd patch value".into()) } }
    let mut gamma = [1.; 3]; let mut curve_fitted = false;
    if fit_curve {
        let neutral: Vec<_> = patches.iter().filter(|(m, r)| r[0] > 1e-6 && (r[0] - r[1]).abs() <= 1e-6 * r[0].max(1.) && (r[0] - r[2]).abs() <= 1e-6 * r[0].max(1.) && m.iter().all(|v| *v > 1e-6)).collect();
        if neutral.len() >= 4 {
            let ys: Vec<f64> = neutral.iter().map(|(_, r)| r[0].ln()).collect(); let ym = ys.iter().sum::<f64>() / ys.len() as f64;
            let mut ok = true; let mut g = [1.; 3];
            for c in 0..3 { let xs: Vec<f64> = neutral.iter().map(|(m, _)| m[c].ln()).collect(); let xm = xs.iter().sum::<f64>() / xs.len() as f64;
                let sxx: f64 = xs.iter().map(|x| (x - xm) * (x - xm)).sum(); let sxy: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - xm) * (y - ym)).sum();
                if sxx < 1e-3 { ok = false; break } let s = sxy / sxx; if !s.is_finite() || !(0.2..=5.).contains(&s) { ok = false; break } g[c] = s }
            if ok { gamma = g; curve_fitted = true }
        }
    }
    // normal equations: M = (sum r x^T)(sum x x^T + lambda I)^-1
    let mut a: Mat = [[0.; 3]; 3]; let mut b: Mat = [[0.; 3]; 3];
    for (m, r) in patches { let x: [f64; 3] = std::array::from_fn(|i| pw(m[i], gamma[i])); for i in 0..3 { for j in 0..3 { a[i][j] += x[i] * x[j]; b[i][j] += r[i] * x[j] } } }
    let tr = a[0][0] + a[1][1] + a[2][2]; if !(tr > 1e-12) { return Err("degenerate patch data".into()) }
    let sc = tr / 3.; let an: Mat = std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] / sc + if i == j { 1e-9 } else { 0. }));
    if det(&an) < 1e-6 { return Err("patches do not span color space (rank deficient)".into()) }
    let ai = inv(&an).ok_or("singular patch matrix")?;
    let matrix: Mat = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| b[i][k] / sc * ai[k][j]).sum()));
    if matrix.iter().flatten().any(|v| !v.is_finite() || v.abs() > 100.) { return Err("unstable fit".into()) }
    let profile = Profile { matrix, gamma };
    let (mut s2, mut mx) = (0., 0f64);
    for (m, r) in patches { let o = lin(&profile, *m); for j in 0..3 { let e = (o[j] - r[j]).abs(); s2 += e * e; mx = mx.max(e) } }
    Ok(Fit { profile, rms: (s2 / (3 * patches.len()) as f64).sqrt(), max_err: mx, curve_fitted })
}
/// Reusable profile text: `kimo-idt-v1 gamma(3) matrix(9)`. Empty / whitespace-only text is the identity profile.
pub fn to_text(pr: &Profile) -> String {
    let mut s = String::from("kimo-idt-v1"); for g in pr.gamma { s += &format!(" {:e}", g) } for r in pr.matrix { for v in r { s += &format!(" {:e}", v) } } s
}
pub fn from_text(t: &str) -> Result<Profile, String> {
    let t = t.trim(); if t.is_empty() { return Ok(Profile::default()) }
    if t.len() > 4096 { return Err("profile too large".into()) }
    let mut it = t.split_whitespace(); if !matches!(it.next(), Some("kimo-idt-v1") | Some("kino-idt-v1")) { // legacy header from before the rename is still accepted
         return Err("not a kimo-idt-v1 profile".into()) }
    let v: Vec<f64> = it.map(|s| s.parse::<f64>().map_err(|_| "bad number in profile".to_string())).collect::<Result<_, _>>()?;
    if v.len() != 12 { return Err("profile needs 3 gamma + 9 matrix values".into()) }
    let pr = Profile { gamma: [v[0], v[1], v[2]], matrix: std::array::from_fn(|i| std::array::from_fn(|j| v[3 + i * 3 + j])) };
    check(&pr)?; Ok(pr)
}
pub fn check(pr: &Profile) -> Result<(), String> {
    if pr.matrix.iter().flatten().any(|v| !v.is_finite() || v.abs() > 100.) { return Err("invalid IDT matrix".into()) }
    if pr.gamma.iter().any(|g| !g.is_finite() || !(0.1..=10.).contains(g)) { return Err("invalid IDT gamma (0.1..10)".into()) }
    Ok(())
}
pub fn apply_profile(pr: &Profile, f: &mut Frame, amount: f64) -> Result<(), String> {
    super::frame::validate(f)?; check(pr)?; if !amount.is_finite() || !(0. ..=1.).contains(&amount) { return Err("amount outside 0..1".into()) }
    if amount == 0. || *pr == Profile::default() { return Ok(()) }
    let mut out = f.d.clone();
    for (i, px) in out.chunks_exact_mut(4).enumerate() { if i % 65536 == 0 { crate::guard::check() }
        let c = lin(pr, [px[0] as f64, px[1] as f64, px[2] as f64]);
        for j in 0..3 { let v = px[j] as f64 + (c[j] - px[j] as f64) * amount; if !v.is_finite() || v.abs() > f32::MAX as f64 { return Err("IDT overflow".into()) } px[j] = v as f32 } }
    f.d = out; Ok(())
}
pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t) as f64;
    let prof = Profile { matrix: std::array::from_fn(|i| std::array::from_fn(|j| g(&format!("m{i}{j}"), if i == j { 1. } else { 0. }))), gamma: [g("gamma_r", 1.), g("gamma_g", 1.), g("gamma_b", 1.)] };
    apply_profile(&prof, f, g("amount", 1.))
}
/// Result of the conservative source-encoding picker. `Select(id)` uses `color_managed` transfer IDs.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick { Select(usize), Defer(String) }
/// Selects only when `hint` (e.g. container/camera metadata string) names exactly one supported log encoding AND the
/// frame does not contradict it (>=1024 opaque samples, all in 0..1, none below the encoding's code-value floor).
/// Pixel statistics alone never select. Anything else defers to the explicit transform.
pub fn pick_encoding(f: &Frame, hint: &str) -> Pick {
    let h: String = hint.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    // (transfer id, black code value floor)
    let cand = match h.as_str() { "acescct" => (3, 0.0729), "logc3" | "logc" | "arrilogc3" => (4, 0.0928), "slog3" | "sonyslog3" => (5, 0.0929), _ => return Pick::Defer("no unambiguous encoding hint".into()) };
    if f.w.checked_mul(f.h).and_then(|n| n.checked_mul(4)) != Some(f.d.len()) { return Pick::Defer("invalid frame".into()) }
    let mut n = 0usize; let mut mn = f64::MAX;
    for px in f.d.chunks_exact(4) { if px[3] < 0.99 { continue } if px[..3].iter().any(|v| !v.is_finite() || *v < 0. || *v > 1.) { return Pick::Defer("values outside 0..1 contradict a log encoding".into()) } n += 1; for v in &px[..3] { mn = mn.min(*v as f64) } }
    if n < 1024 { return Pick::Defer("too few opaque samples".into()) }
    if mn < cand.1 - 0.02 { return Pick::Defer("pixels below encoding black floor contradict hint".into()) }
    Pick::Select(cand.0)
}
pub fn catalog() -> Vec<super::Entry> { vec![("camera_idt", "Camera input transform from a chart-calibrated profile: per-channel power gamma then 3x3 matrix, blended by amount. Identity at amount 0 or default profile. Fit offline with idt::fit; input is camera-native RGB, output working-space linear (no gamut/white adaptation beyond the fitted matrix).", &[("amount", 1., "0..1"), ("gamma_r", 1., "0.1..10"), ("gamma_g", 1., ""), ("gamma_b", 1., ""), ("m00", 1., "matrix row-major"), ("m01", 0., ""), ("m02", 0., ""), ("m10", 0., ""), ("m11", 1., ""), ("m12", 0., ""), ("m20", 0., ""), ("m21", 0., ""), ("m22", 1., "")])] }
