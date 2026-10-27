use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Ease { Linear, In, Out, InOut, Hold, OutQuart, OutExpo, OutBack, InOutQuint, InExpo,
    /// AE/easings.net overshoot and spring curves; values leave 0..1 mid-segment (bounded props are clamped where read, see `clamp_prop`), endpoints are exact.
    InBack, InOutBack, InElastic, OutElastic, InOutElastic, InBounce, OutBounce, InOutBounce }

/// Every easing name, in schema order: the ONE list every keyframe tool (clip, effect, mask, camera, audio volume, speed ramp, text animator,
/// motion path) and recipes take, and transitions add to their own. Same names serde reads into `Ease`.
pub const EASE_NAMES: &[&str] = &["linear", "in", "out", "in_out", "hold", "out_quart", "out_expo", "out_back", "in_out_quint", "in_expo",
    "in_back", "in_out_back", "in_elastic", "out_elastic", "in_out_elastic", "in_bounce", "out_bounce", "in_out_bounce"];

fn out_bounce(u: f64) -> f64 { let (n, d) = (7.5625, 2.75); if u < 1.0 / d { n * u * u } else if u < 2.0 / d { let u = u - 1.5 / d; n * u * u + 0.75 } else if u < 2.5 / d { let u = u - 2.25 / d; n * u * u + 0.9375 } else { let u = u - 2.625 / d; n * u * u + 0.984375 } }

impl Ease {
    /// The shared easing primitive: progress u in 0..1 -> eased progress. The first ten arms are the curves kimo always drew (unchanged, bit for bit).
    pub fn apply(self, u: f64) -> f64 {
        use std::f64::consts::PI;
        let (c1, c4, c5) = (1.70158, 2.0 * PI / 3.0, 2.0 * PI / 4.5); let c2 = c1 * 1.525;
        let new_curve = !matches!(self, Ease::Linear | Ease::Hold | Ease::In | Ease::Out | Ease::OutQuart | Ease::OutExpo | Ease::InExpo | Ease::OutBack | Ease::InOutQuint | Ease::InOut);
        if new_curve { if u <= 0.0 { return 0.0; } if u >= 1.0 { return 1.0; } }
        match self {
            Ease::Linear | Ease::Hold => u,
            Ease::In => u * u * u,
            Ease::Out => 1.0 - (1.0 - u).powi(3),
            Ease::OutQuart => 1.0 - (1.0 - u).powi(4),
            Ease::OutExpo => if u >= 1.0 { 1.0 } else { 1.0 - 2f64.powf(-10.0 * u) },
            Ease::InExpo => if u <= 0.0 { 0.0 } else { 2f64.powf(10.0 * u - 10.0) },
            Ease::OutBack => { let (c1, c3) = (1.2, 2.2); 1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2) }
            Ease::InOutQuint => if u < 0.5 { 16.0 * u.powi(5) } else { 1.0 - (-2.0 * u + 2.0).powi(5) / 2.0 },
            Ease::InOut => if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) / 2.0 },
            Ease::InBack => (c1 + 1.0) * u * u * u - c1 * u * u,
            Ease::InOutBack => if u < 0.5 { (2.0 * u).powi(2) * ((c2 + 1.0) * 2.0 * u - c2) / 2.0 } else { ((2.0 * u - 2.0).powi(2) * ((c2 + 1.0) * (u * 2.0 - 2.0) + c2) + 2.0) / 2.0 },
            Ease::InElastic => -(2f64.powf(10.0 * u - 10.0)) * ((u * 10.0 - 10.75) * c4).sin(),
            Ease::OutElastic => 2f64.powf(-10.0 * u) * ((u * 10.0 - 0.75) * c4).sin() + 1.0,
            Ease::InOutElastic => if u < 0.5 { -(2f64.powf(20.0 * u - 10.0) * ((20.0 * u - 11.125) * c5).sin()) / 2.0 } else { 2f64.powf(-20.0 * u + 10.0) * ((20.0 * u - 11.125) * c5).sin() / 2.0 + 1.0 },
            Ease::InBounce => 1.0 - out_bounce(1.0 - u),
            Ease::OutBounce => out_bounce(u),
            Ease::InOutBounce => if u < 0.5 { (1.0 - out_bounce(1.0 - 2.0 * u)) / 2.0 } else { (1.0 + out_bounce(2.0 * u - 1.0)) / 2.0 },
        }
    }
    /// The easing a name means (any of EASE_NAMES), or None.
    pub fn parse(name: &str) -> Option<Ease> { serde_json::from_value(serde_json::Value::String(name.to_string())).ok() }
}

/// The per-property clamp flag: properties whose value range is bounded (an elastic/back overshoot must not push opacity past 1 or volume
/// below 0). Position, scale, rotation and every other property are left unclamped, so overshoot reads as motion.
pub const BOUNDED_PROPS: &[(&str, f64, f64)] = &[("opacity", 0.0, 1.0), ("fill_alpha", 0.0, 1.0), ("stroke_alpha", 0.0, 1.0), ("reveal", 0.0, 1.0), ("volume", 0.0, f64::INFINITY), ("pan", -1.0, 1.0)];
/// `v` clamped to the property's range when it is bounded, else unchanged.
pub fn clamp_prop(name: &str, v: f64) -> f64 { match BOUNDED_PROPS.iter().find(|(n, _, _)| *n == name) { Some((_, lo, hi)) => v.clamp(*lo, *hi), None => v } }

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Key { pub t: f64, pub v: f64, #[serde(default = "d_ease")] pub ease: Ease, /// Optional cubic-bezier timing curve [x1,y1,x2,y2] for the segment leaving this key (overrides `ease`; y may overshoot).
    #[serde(default)] pub bez: Option<[f64; 4]>,
    /// Optional tangent handle for the segment LEAVING this key: control point (t+dt, v+dv). Wins over `bez`.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub out: Option<Handle>,
    /// Optional tangent handle for the segment ARRIVING at this key: control point (t-dt, v-dv), so a mirrored pair has equal in/out values.
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")] pub inn: Option<Handle> }
/// A tangent handle as an offset from its key: `dt` seconds along the time axis (clamped to the segment, so time stays monotone), `dv` in property units (any sign, unclamped: overshoot is allowed).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handle { pub dt: f64, pub dv: f64 }
fn d_ease() -> Ease { Ease::Linear }

/// A property that is either constant or keyframed over clip-local time (seconds).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prop { pub base: f64, #[serde(default)] pub keys: Vec<Key>, /// Expression ops applied in order (AE-style wiggle, loopOut, posterizeTime, inertia...).
    #[serde(default)] pub expr: Vec<Op> }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Smooth fractal noise added to the value. freq Hz, amp in property units.
    Wiggle { freq: f64, amp: f64, #[serde(default = "d_oct")] octaves: u32, #[serde(default)] seed: u64, /// seconds added to t (set when a split moves the clip's clock)
        #[serde(default, skip_serializing_if = "is_zero_shift")] shift: f64 },
    /// After the last key: cycle | pingpong | offset | continue.
    LoopOut { mode: String },
    /// Before the first key: cycle | pingpong | offset | continue.
    LoopIn { mode: String },
    /// value += amp * sin(2 pi (freq t + phase))
    Sin { freq: f64, amp: f64, #[serde(default)] phase: f64 },
    /// Overshoot spring after each key (AE inertia): amp scaled by key velocity, freq Hz, decay 1/s.
    Inertia { amp: f64, freq: f64, decay: f64 },
    /// Evaluate at quantized time (posterize time), fps frames/s.
    PosterizeTime { fps: f64, #[serde(default, skip_serializing_if = "is_zero_shift")] shift: f64 },
    Clamp { min: f64, max: f64 },
    /// Quantize the value to multiples of `step`.
    Round { step: f64 },
    /// Rebound after the LAST key: value springs back toward the previous value, |sin| bounces with exponential decay. amp ~0.3 of the last segment's delta.
    Bounce { amp: f64, freq: f64, decay: f64 },
    /// Spring overshoot past EVERY key (proper damped spring on the segment delta): amp as a fraction of the delta.
    Overshoot { amp: f64, freq: f64, decay: f64 },
    /// Closed-form damped spring from `from` to `to` starting at t0 (see `spring_response`). combine: set (default, replaces the value) | add | mul.
    Spring { stiffness: f64, damping: f64, mass: f64, from: f64, to: f64, #[serde(default)] t0: f64, #[serde(default)] combine: String },
    /// amp * exp(-decay * (t - t0)) * fractal noise(freq * (t - t0)); deterministic in (seed, t). Zero before t0. combine: add (default) | set | mul.
    NoiseImpulse { seed: u64, freq: f64, amp: f64, decay: f64, #[serde(default = "d_oct")] octaves: u32, #[serde(default)] t0: f64, #[serde(default)] combine: String },
    /// from + (to - from) * cubic_bezier(x1,y1,x2,y2)((t - t0) / duration), x clamped to [0,1] like CSS. combine: set (default) | add | mul.
    Bezier { x1: f64, y1: f64, x2: f64, y2: f64, from: f64, to: f64, #[serde(default)] t0: f64, duration: f64, #[serde(default)] combine: String,
        /// Set by a tool that layers its own op on top of the property (add_punch_ins: "punch"); the tool finds and replaces its ops by
        /// this tag, and a tagged op runs on the clip's own time (the property's posterize / loop time ops do not move it).
        #[serde(default, skip_serializing_if = "String::is_empty")] tag: String },
}
/// Normalised spring response e(tau) for m x'' + c x' + k x = 0 with x(0)=1, x'(0)=0: x(t) = to + (from - to) * e(t - t0). Underdamped, critically damped and overdamped closed forms.
pub fn spring_response(stiffness: f64, damping: f64, mass: f64, tau: f64) -> f64 {
    if tau <= 0.0 { return 1.0; } let (k, c, m) = (stiffness, damping, mass.max(1e-12)); let w0 = (k.max(0.0) / m).sqrt(); if w0 < 1e-12 { return 1.0; }
    let z = c / (2.0 * (k.max(1e-300) * m).sqrt());
    if (z - 1.0).abs() < 1e-9 { (-w0 * tau).exp() * (1.0 + w0 * tau) }
    else if z < 1.0 { let (sg, wd) = (z * w0, w0 * (1.0 - z * z).sqrt()); (-sg * tau).exp() * ((wd * tau).cos() + sg / wd * (wd * tau).sin()) }
    else { let r = w0 * (z * z - 1.0).sqrt(); let (r1, r2) = (-z * w0 + r, -z * w0 - r); (r2 * (r1 * tau).exp() - r1 * (r2 * tau).exp()) / (r2 - r1) }
}
fn combine(v: f64, g: f64, mode: &str, default_set: bool) -> f64 { match (mode, default_set) { ("add", _) | ("", false) => v + g, ("mul", _) => v * g, _ => g } }
fn d_oct() -> u32 { 1 }
fn hash(mut x: u64) -> f64 { x = x.wrapping_add(0x9E3779B97F4A7C15); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
/// 1D smooth value noise in [-1, 1].
pub fn noise(seed: u64, x: f64) -> f64 { let i = x.floor(); let f = x - i; let u = f * f * (3.0 - 2.0 * f); let a = hash(seed.wrapping_mul(0x9E37).wrapping_add(i as i64 as u64)) * 2.0 - 1.0; let b = hash(seed.wrapping_mul(0x9E37).wrapping_add((i + 1.0) as i64 as u64)) * 2.0 - 1.0; a + (b - a) * u }
/// CSS-style cubic-bezier timing function y(x).
pub fn bezier_ease(b: [f64; 4], x: f64) -> f64 { bez_solve(b[0], b[1], b[2], b[3], 1.0, x) }
/// The one cubic solver behind `bez` and handles: x control points are clamped to [0,1] (monotone time), y control points are free.
fn bez_solve(x1: f64, y1: f64, x2: f64, y2: f64, ye: f64, x: f64) -> f64 { let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0)); let cx = |t: f64| 3.0 * x1 * t * (1.0 - t).powi(2) + 3.0 * x2 * t * t * (1.0 - t) + t.powi(3); let cy = |t: f64| 3.0 * y1 * t * (1.0 - t).powi(2) + 3.0 * y2 * t * t * (1.0 - t) + ye * t.powi(3);
    let (mut lo, mut hi) = (0.0, 1.0); for _ in 0..40 { let m = 0.5 * (lo + hi); if cx(m) < x { lo = m; } else { hi = m; } } cy(0.5 * (lo + hi)) }

impl Prop {
    pub fn new(v: f64) -> Self { Prop { base: v, keys: vec![], expr: vec![] } }
    /// Evaluate with expressions: time ops (posterize, loops) first, then keyframes, then value ops (wiggle, sin, inertia, clamp).
    pub fn at(&self, t: f64) -> f64 {
        if self.expr.is_empty() { return self.raw(t); }
        let (mut tt, mut add) = (t, 0.0);
        for op in &self.expr { match op {
            Op::PosterizeTime { fps, shift } => { if *fps > 0.0 { tt = ((tt + shift) * fps + 1e-9).floor() / fps - shift; } }
            Op::LoopOut { mode } if self.keys.len() >= 2 => { let (t0, t1) = (self.keys[0].t, self.keys[self.keys.len() - 1].t); if tt > t1 && t1 > t0 { let d = t1 - t0; let n = ((tt - t0) / d).floor(); let ph = (tt - t0) - n * d;
                match mode.as_str() { "pingpong" => { tt = if (n as i64) % 2 == 0 { t0 + ph } else { t1 - ph }; } "offset" => { tt = t0 + ph; add += n * (self.raw(t1) - self.raw(t0)); } "continue" => { let v = (self.raw(t1) - self.raw(t1 - 1e-3)) / 1e-3; add += v * (tt - t1); tt = t1; } _ => { tt = t0 + ph; } } } }
            Op::LoopIn { mode } if self.keys.len() >= 2 => { let (t0, t1) = (self.keys[0].t, self.keys[self.keys.len() - 1].t); if tt < t0 && t1 > t0 { let d = t1 - t0; let n = ((t1 - tt) / d).floor(); let ph = (t1 - tt) - n * d;
                match mode.as_str() { "pingpong" => { tt = if (n as i64) % 2 == 0 { t1 - ph } else { t0 + ph }; } "offset" => { tt = t1 - ph; add -= (n + 1.0) * (self.raw(t1) - self.raw(t0)); add += 0.0; } "continue" => { let v = (self.raw(t0 + 1e-3) - self.raw(t0)) / 1e-3; add += v * (tt - t0); tt = t0; } _ => { tt = t1 - ph; } } } }
            _ => {} } }
        let mut v = self.raw(tt) + add;
        for op in &self.expr { match op {
            Op::Wiggle { freq, amp, octaves, seed, shift } => { let tt = tt + shift; let mut s = 0.0; for o in 0..(*octaves).clamp(1, 8) { let f = freq * 2f64.powi(o as i32); s += amp * 0.5f64.powi(o as i32) * noise(seed.wrapping_add((o as u64).wrapping_mul(7919)), tt * f); } v += s; }
            Op::Sin { freq, amp, phase } => v += amp * (std::f64::consts::TAU * (freq * tt + phase)).sin(),
            Op::Inertia { amp, freq, decay } => { for w in self.keys.windows(2).map(|w| (w[0].t, w[0].v, w[1].t, w[1].v)).chain(std::iter::empty()) { let (_, _, tb, vb) = w; if tt > tb { let _ = vb; } }
                // spring after each key based on the velocity of the segment arriving at it
                for i in 1..self.keys.len() { let (ta, va, tb, vb) = (self.keys[i - 1].t, self.keys[i - 1].v, self.keys[i].t, self.keys[i].v); if tt > tb { let vel = (vb - va) / (tb - ta).max(1e-6); let dt = tt - tb; v += amp * vel * 0.1 * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin() / (std::f64::consts::TAU * freq).max(1e-6) * 10.0; } } }
            Op::Bounce { amp, freq, decay } => { let n = self.keys.len(); if n >= 2 { let (ta, va, tb, vb) = (self.keys[n - 2].t, self.keys[n - 2].v, self.keys[n - 1].t, self.keys[n - 1].v); let _ = ta; if tt > tb { let dt = tt - tb; v -= (vb - va) * amp * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin().abs(); } } }
            Op::Overshoot { amp, freq, decay } => { for i in 1..self.keys.len() { let (va, tb, vb) = (self.keys[i - 1].v, self.keys[i].t, self.keys[i].v); if tt > tb { let dt = tt - tb; v += (vb - va) * amp * (-decay * dt).exp() * (std::f64::consts::TAU * freq * dt).sin(); } } }
            Op::Spring { stiffness, damping, mass, from, to, t0, combine: cm } => { let g = to + (from - to) * spring_response(*stiffness, *damping, *mass, tt - t0); v = combine(v, g, cm, true); }
            Op::NoiseImpulse { seed, freq, amp, decay, octaves, t0, combine: cm } => { let tau = tt - t0; let g = if tau < 0.0 { 0.0 } else { let mut sm = 0.0; for o in 0..(*octaves).clamp(1, 8) { let f = freq * 2f64.powi(o as i32); sm += 0.5f64.powi(o as i32) * noise(seed.wrapping_add(o as u64 * 7919), tau * f); } amp * (-decay * tau).exp() * sm }; v = combine(v, g, cm, false); }
            Op::Bezier { x1, y1, x2, y2, from, to, t0, duration, combine: cm, tag } => { let tq = if tag.is_empty() { tt } else { t }; let u = if *duration > 0.0 { ((tq - t0) / duration).clamp(0.0, 1.0) } else { 1.0 }; let g = if u <= 0.0 { *from } else if u >= 1.0 { *to } else { from + (to - from) * bezier_ease([*x1, *y1, *x2, *y2], u) }; v = combine(v, g, cm, true); }
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
                if w[0].out.is_some() || w[1].inn.is_some() { return handle_segment(&w[0], &w[1], u); }
                if let Some(b) = w[0].bez { let y = bezier_ease(b, u); return w[0].v + (w[1].v - w[0].v) * y; }
                let u = w[0].ease.apply(u);
                return w[0].v + (w[1].v - w[0].v) * u;
            }
        }
        self.base
    }
    pub fn set_key(&mut self, t: f64, v: f64, ease: Ease) {
        self.keys.retain(|k| (k.t - t).abs() > 1e-6);
        self.keys.push(Key { t, v, ease, bez: None, out: None, inn: None });
        self.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    }
}

/// Value of the segment between keys `a` and `b` at normalized time `u` when either carries a handle. A missing handle defaults to
/// a third of the segment (dt) and a third of the value change (dv), i.e. a straight line. Same solver as `bez`, with y in value units so a flat segment can still overshoot.
pub fn handle_segment(a: &Key, b: &Key, u: f64) -> f64 {
    let (dt, dv) = ((b.t - a.t).max(1e-9), b.v - a.v);
    let o = a.out.unwrap_or(Handle { dt: dt / 3.0, dv: dv / 3.0 }); let i = b.inn.unwrap_or(Handle { dt: dt / 3.0, dv: dv / 3.0 });
    a.v + bez_solve(o.dt / dt, o.dv, 1.0 - i.dt / dt, dv - i.dv, dv, u)
}
/// The `bez` quad equivalent to a handle pair when the segment value change is not zero (round-trip helper; flat segments have no `bez` form).
pub fn handles_to_bez(a: &Key, b: &Key) -> Option<[f64; 4]> { let (dt, dv) = (b.t - a.t, b.v - a.v); if dt <= 0.0 || dv == 0.0 { return None; } let o = a.out?; let i = b.inn?; Some([o.dt / dt, o.dv / dv, 1.0 - i.dt / dt, 1.0 - i.dv / dv]) }

fn is_zero_shift(v: &f64) -> bool { *v == 0.0 }

impl Prop {
    /// The same animation started `d` seconds later on its own clock: `p.shifted(d).at(t) == p.at(t + d)` (keys move by -d and keep
    /// their handles; keys that fall before 0 stay so the segment crossing 0 interpolates exactly; time-anchored ops shift with them).
    pub fn shifted(&self, d: f64) -> Prop {
        let mut p = self.clone(); if d == 0.0 { return p; }
        for k in p.keys.iter_mut() { k.t -= d; }
        for op in p.expr.iter_mut() { match op {
            Op::Sin { freq, phase, .. } => { *phase += *freq * d; }
            Op::Spring { t0, .. } | Op::NoiseImpulse { t0, .. } | Op::Bezier { t0, .. } => { *t0 -= d; }
            Op::Wiggle { shift, .. } | Op::PosterizeTime { shift, .. } => { *shift += d; }
            _ => {} } }
        p
    }
}

/// After an edit that moves a clip's head along its content without changing what plays on the timeline (split, hold from here,
/// roll, slide), the clip's own clock starts `d` seconds later: every clip-local animation (transform and other props, effect
/// params, speed ramp, mask paths, audio automation, grade nodes, text animators) shifts by -d so it continues where it was.
/// The source (its internal clock is the in point), the time remap (its `offset` carries the shift) and the source-seconds reframe
/// keys are left alone.
pub fn shift_clip_clock(c: &mut crate::model::Clip, d: f64) {
    if d == 0.0 || !d.is_finite() { return; }
    fn walk(x: &mut serde_json::Value, d: f64) {
        use serde_json::Value;
        if let Value::Object(o) = x { if o.get("base").is_some_and(|b| b.is_number()) && o.get("keys").is_some_and(|k| k.is_array()) {
            if let Ok(p) = serde_json::from_value::<Prop>(x.clone()) { if let Ok(v) = serde_json::to_value(p.shifted(d)) { *x = v; } } return; } }
        match x {
            Value::Object(o) => { for v in o.values_mut() { walk(v, d); } }
            Value::Array(a) => {
                let path_keys = !a.is_empty() && a.iter().all(|e| e.get("t").is_some_and(|t| t.is_number()) && e.get("points").is_some_and(|p| p.is_array()));
                if path_keys { for e in a.iter_mut() { if let Some(t) = e.get("t").and_then(|t| t.as_f64()) { e["t"] = serde_json::json!(t - d); } } }
                for v in a.iter_mut() { walk(v, d); } }
            _ => {} }
    }
    let Ok(mut v) = serde_json::to_value(&*c) else { return };
    if let Some(o) = v.as_object_mut() { for (k, x) in o.iter_mut() { match k.as_str() {
        "source" | "time_remap" | "id" => {}
        "props" => { if let Some(po) = x.as_object_mut() { for (pk, pv) in po.iter_mut() { if !crate::reframe::source_keyed(pk) { walk(pv, d); } } } }
        _ => walk(x, d) } } }
    if let Ok(n) = serde_json::from_value(v) { *c = n; }
}
