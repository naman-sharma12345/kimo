//! Convert Audio to Keyframes (After Effects): a clip's loudness becomes keyframes on any animatable property.
//! The envelope is the RMS of the (optionally band-limited) audio in a window around each frame, read in dB (or linear
//! amplitude), normalised between a floor and a ceiling, shaped by an attack/release follower and a moving-average
//! smoothing, then mapped onto [min, max] of the property. One linear key per frame.

/// RBJ biquad, direct form I.
struct Biquad { b: [f64; 3], a: [f64; 2], x: [f64; 2], y: [f64; 2] }
impl Biquad {
    fn new(high: bool, f0: f64, sr: f64) -> Self { let w0 = std::f64::consts::TAU * f0 / sr; let (c, al) = (w0.cos(), w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2)); let a0 = 1.0 + al;
        let b = if high { [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0] } else { [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0] };
        Biquad { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [-2.0 * c / a0, (1.0 - al) / a0], x: [0.0; 2], y: [0.0; 2] } }
    fn run(&mut self, v: f64) -> f64 { let o = self.b[0] * v + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [v, self.x[0]]; self.y = [o, self.y[0]]; o }
}
pub const BANDS: &[&str] = &["full", "low", "mid", "high"];
pub const CHANNELS: &[&str] = &["mix", "left", "right"];
/// Mono signal from interleaved stereo, band-limited: low < 150 Hz, mid 300..3000 Hz, high > 4 kHz (4th order each side).
pub fn band_signal(stereo: &[f32], sr: f64, channel: &str, band: &str) -> Vec<f64> {
    let mono: Vec<f64> = stereo.chunks_exact(2).map(|s| match channel { "left" => s[0] as f64, "right" => s[1] as f64, _ => (s[0] as f64 + s[1] as f64) * 0.5 }).collect();
    let stages: Vec<(bool, f64)> = match band { "low" => vec![(false, 150.0), (false, 150.0)], "mid" => vec![(true, 300.0), (true, 300.0), (false, 3000.0), (false, 3000.0)], "high" => vec![(true, 4000.0), (true, 4000.0)], _ => vec![] };
    let mut fs: Vec<Biquad> = stages.iter().map(|(h, f)| Biquad::new(*h, *f, sr)).collect();
    mono.into_iter().map(|v| fs.iter_mut().fold(v, |acc, f| f.run(acc))).collect()
}
/// RMS of `sig` (sample rate sr) in a window of `window` seconds centred on each time (seconds into the signal); silence outside it.
pub fn rms_at(sig: &[f64], sr: f64, times: &[f64], window: f64) -> Vec<f64> {
    let mut pre = vec![0f64; sig.len() + 1]; for (i, v) in sig.iter().enumerate() { pre[i + 1] = pre[i] + v * v; }
    let half = (window * sr / 2.0).max(0.5);
    times.iter().map(|t| { let c = t * sr; let (a, b) = ((c - half).round(), (c + half).round()); let n = (b - a).max(1.0);
        let (a, b) = (a.clamp(0.0, sig.len() as f64) as usize, b.clamp(0.0, sig.len() as f64) as usize); if b <= a { 0.0 } else { ((pre[b] - pre[a]) / n).max(0.0).sqrt() } }).collect()
}
pub struct Shape { pub db: bool, pub floor_db: f64, pub ceiling: Option<f64>, pub attack: f64, pub release: f64, pub smoothing: f64 }
/// Normalised 0..1 envelope from RMS samples spaced dt seconds apart, plus the ceiling used (dB, or linear amplitude).
pub fn shape(rms: &[f64], dt: f64, s: &Shape) -> (Vec<f64>, f64) {
    let lv: Vec<f64> = rms.iter().map(|r| if s.db { 20.0 * r.max(1e-12).log10() } else { *r }).collect();
    let ceil = s.ceiling.unwrap_or_else(|| lv.iter().cloned().fold(if s.db { s.floor_db } else { 0.0 }, f64::max));
    let lo = if s.db { s.floor_db } else { 0.0 }; let span = (ceil - lo).max(1e-9);
    let n: Vec<f64> = lv.iter().map(|v| ((v - lo) / span).clamp(0.0, 1.0)).collect();
    // attack / release follower: the envelope rises with time constant `attack` and falls with `release` (0 = instant)
    let coef = |tc: f64| if tc <= 0.0 { 1.0 } else { 1.0 - (-dt / tc).exp() };
    let (ka, kr) = (coef(s.attack), coef(s.release)); let mut y = 0.0; let mut f = Vec::with_capacity(n.len());
    for (i, x) in n.iter().enumerate() { if i == 0 { y = if s.attack <= 0.0 { *x } else { x * ka }; } else { y += (x - y) * if *x > y { ka } else { kr }; } f.push(y); }
    // centred moving average over `smoothing` seconds
    let r = ((s.smoothing / dt) / 2.0).round() as isize;
    let out = if r <= 0 { f } else { (0..f.len() as isize).map(|i| { let (a, b) = ((i - r).max(0) as usize, ((i + r) as usize).min(f.len() - 1)); f[a..=b].iter().sum::<f64>() / (b - a + 1) as f64 }).collect() };
    (out, ceil)
}
