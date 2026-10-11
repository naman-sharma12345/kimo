use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Ease { Linear, In, Out, InOut, Hold, OutQuart, OutExpo, OutBack, InOutQuint, InExpo }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Key { pub t: f64, pub v: f64, #[serde(default = "d_ease")] pub ease: Ease, /// Optional cubic-bezier timing curve [x1,y1,x2,y2] for the segment leaving this key (overrides `ease`; y may overshoot).
    #[serde(default)] pub bez: Option<[f64; 4]> }
fn d_ease() -> Ease { Ease::Linear }

/// A property that is either constant or keyframed over clip-local time (seconds).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prop { pub base: f64, #[serde(default)] pub keys: Vec<Key>, /// Expression ops applied in order (AE-style wiggle, loopOut, posterizeTime, inertia...).
    #[serde(default)] pub expr: Vec<Op> }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Smooth fractal noise added to the value. freq Hz, amp in property units.
    Wiggle { freq: f64, amp: f64, #[serde(default = "d_oct")] octaves: u32, #[serde(default)] seed: u64 },
    /// After the last key: cycle | pingpong | offset | continue.
    LoopOut { mode: String },
    /// Before the first key: cycle | pingpong | offset | continue.
    LoopIn { mode: String },
    /// value += amp * sin(2 pi (freq t + phase))
    Sin { freq: f64, amp: f64, #[serde(default)] phase: f64 },
    /// Overshoot spring after each key (AE inertia): amp scaled by key velocity, freq Hz, decay 1/s.
    Inertia { amp: f64, freq: f64, decay: f64 },
    /// Evaluate at quantized time (posterize time), fps frames/s.
    PosterizeTime { fps: f64 },
    Clamp { min: f64, max: f64 },
    /// Quantize the value to multiples of `step`.
    Round { step: f64 },
    /// Rebound after the LAST key: value springs back toward the previous value, |sin| bounces with exponential decay. amp ~0.3 of the last segment's delta.
    Bounce { amp: f64, freq: f64, decay: f64 },
    /// Spring overshoot past EVERY key (proper damped spring on the segment delta): amp as a fraction of the delta.
    Overshoot { amp: f64, freq: f64, decay: f64 },
}
fn d_oct() -> u32 { 1 }
fn hash(mut x: u64) -> f64 { x = x.wrapping_add(0x9E3779B97F4A7C15); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
/// 1D smooth value noise in [-1, 1].
pub fn noise(seed: u64, x: f64) -> f64 { let i = x.floor(); let f = x - i; let u = f * f * (3.0 - 2.0 * f); let a = hash(seed.wrapping_mul(0x9E37).wrapping_add(i as i64 as u64)) * 2.0 - 1.0; let b = hash(seed.wrapping_mul(0x9E37).wrapping_add((i + 1.0) as i64 as u64)) * 2.0 - 1.0; a + (b - a) * u }
/// CSS-style cubic-bezier timing function y(x).
pub fn bezier_ease(b: [f64; 4], x: f64) -> f64 { let (x1, y1, x2, y2) = (b[0].clamp(0.0, 1.0), b[1], b[2].clamp(0.0, 1.0), b[3]); let cx = |t: f64| 3.0 * x1 * t * (1.0 - t).powi(2) + 3.0 * x2 * t * t * (1.0 - t) + t.powi(3); let cy = |t: f64| 3.0 * y1 * t * (1.0 - t).powi(2) + 3.0 * y2 * t * t * (1.0 - t) + t.powi(3);
    let (mut lo, mut hi) = (0.0, 1.0); for _ in 0..40 { let m = 0.5 * (lo + hi); if cx(m) < x { lo = m; } else { hi = m; } } cy(0.5 * (lo + hi)) }

impl Prop {
    pub fn new(v: f64) -> Self { Prop { base: v, keys: vec![], expr: vec![] } }
    /// Evaluate with expressions: time ops (posterize, loops) first, then keyframes, then value ops (wiggle, sin, inertia, clamp).
    pub fn at(&self, t: f64) -> f64 {
        if self.expr.is_empty() { return self.raw(t); }
        let (mut tt, mut add) = (t, 0.0);
        for op in &self.expr { match op {
            Op::PosterizeTime { fps } => { if *fps > 0.0 { tt = (tt * fps + 1e-9).floor() / fps; } }
            Op::LoopOut { mode } if self.keys.len() >= 2 => { let (t0, t1) = (self.keys[0].t, self.keys[self.keys.len() - 1].t); if tt > t1 && t1 > t0 { let d = t1 - t0; let n = ((tt - t0) / d).floor(); let ph = (tt - t0) - n * d;
                match mode.as_str() { "pingpong" => { tt = if (n as i64) % 2 == 0 { t0 + ph } else { t1 - ph }; } "offset" => { tt = t0 + ph; add += n * (self.raw(t1) - self.raw(t0)); } "continue" => { let v = (self.raw(t1) - self.raw(t1 - 1e-3)) / 1e-3; add += v * (tt - t1); tt = t1; } _ => { tt = t0 + ph; } } } }
            Op::LoopIn { mode } if self.keys.len() >= 2 => { let (t0, t1) = (self.keys[0].t, self.keys[self.keys.len() - 1].t); if tt < t0 && t1 > t0 { let d = t1 - t0; let n = ((t1 - tt) / d).floor(); let ph = (t1 - tt) - n * d;
                match mode.as_str() { "pingpong" => { tt = if (n as i64) % 2 == 0 { t1 - ph } else { t0 + ph }; } "offset" => { tt = t1 - ph; add -= (n + 1.0) * (self.raw(t1) - self.raw(t0)); add += 0.0; } "continue" => { let v = (self.raw(t0 + 1e-3) - self.raw(t0)) / 1e-3; add += v * (tt - t0); tt = t0; } _ => { tt = t1 - ph; } } } }
            _ => {} } }
        let mut v = self.raw(tt) + add;
        for op in &self.expr { match op {
            Op::Wiggle { freq, amp, octaves, seed } => { let mut s = 0.0; for o in 0..(*octaves).clamp(1, 8) { let f = freq * 2f64.powi(o as i32); s += amp * 0.5f64.powi(o as i32) * noise(seed.wrapping_add(o as u64 * 7919), tt * f); } v += s; }
            Op::Sin { freq, amp, phase } => v += amp * (std::f64::consts::TAU * (freq * tt + phase)).sin(),
            Op::Inertia { amp, freq, decay } => { for w in self.keys.windows(2).map(|w| (w[0].t, w[0].v, w[1].t, w[1].v)).chain(std::iter::empty()) { let (_, _, tb, vb) = w; if tt > tb { let _ = vb; } }
                // spring after each key based on the velocity of the segment arriving at it
                for i in 1..self.keys.len() { let (ta, va, tb, vb) = (self.keys[i - 1].t, self.keys[i - 1].v, self.keys[i].t, self.keys[i].v); if tt > tb { let vel = (vb - va) / (tb - ta).max(1e-6); let dt = tt - tb; v += amp * vel * 0.1 * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin() / (std::f64::consts::TAU * freq).max(1e-6) * 10.0; } } }
            Op::Bounce { amp, freq, decay } => { let n = self.keys.len(); if n >= 2 { let (ta, va, tb, vb) = (self.keys[n - 2].t, self.keys[n - 2].v, self.keys[n - 1].t, self.keys[n - 1].v); let _ = ta; if tt > tb { let dt = tt - tb; v -= (vb - va) * amp * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin().abs(); } } }
            Op::Overshoot { amp, freq, decay } => { for i in 1..self.keys.len() { let (va, tb, vb) = (self.keys[i - 1].v, self.keys[i].t, self.keys[i].v); if tt > tb { let dt = tt - tb; v += (vb - va) * amp * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin(); } } }
            Op::Clamp { min, max } => v = v.clamp(*min, *max),
            Op::Round { step } => if *step > 0.0 { v = (v / step).round() * step },
            _ => {} } }
        v
    }
    pub fn raw(&self, t: f64) -> f64 {
        if self.keys.is_empty() { return self.base; }
        let k = &self.keys;
        if t <= k[0].t { return k[0].v; }
        if t >= k[k.len() - 1].t { return k[k.len() - 1].v; }
        for w in k.windows(2) {
            if t >= w[0].t && t <= w[1].t {
                if w[0].ease == Ease::Hold { return w[0].v; }
                let u = (t - w[0].t) / (w[1].t - w[0].t).max(1e-9);
                if let Some(b) = w[0].bez { let y = bezier_ease(b, u); return w[0].v + (w[1].v - w[0].v) * y; }
                let u = match w[0].ease {
                    Ease::Linear | Ease::Hold => u,
                    Ease::In => u * u * u,
                    Ease::Out => 1.0 - (1.0 - u).powi(3),
                    Ease::OutQuart => 1.0 - (1.0 - u).powi(4),
                    Ease::OutExpo => if u >= 1.0 { 1.0 } else { 1.0 - 2f64.powf(-10.0 * u) },
                    Ease::InExpo => if u <= 0.0 { 0.0 } else { 2f64.powf(10.0 * u - 10.0) },
                    Ease::OutBack => { let (c1, c3) = (1.2, 2.2); 1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2) }
                    Ease::InOutQuint => if u < 0.5 { 16.0 * u.powi(5) } else { 1.0 - (-2.0 * u + 2.0).powi(5) / 2.0 },
                    Ease::InOut => if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) / 2.0 },
                };
                return w[0].v + (w[1].v - w[0].v) * u;
            }
        }
        self.base
    }
    pub fn set_key(&mut self, t: f64, v: f64, ease: Ease) {
        self.keys.retain(|k| (k.t - t).abs() > 1e-6);
        self.keys.push(Key { t, v, ease, bez: None });
        self.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    }
}
