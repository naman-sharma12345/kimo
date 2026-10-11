//! Motion presets: named, parameterised animation recipes (the "pop in", "whip", "bounce" vocabulary of CapCut / After Effects presets)
//! baked to dense per-frame keyframes on x, y, scale, rotation, opacity. A prompt matcher maps plain-language descriptions to presets.
use std::f64::consts::PI;
/// Per-property delta at normalised time u in [0,1] over `d` seconds. Returned as (dx px, dy px, scale multiplier, rotation deg added, opacity multiplier).
pub type Sample = (f64, f64, f64, f64, f64);
fn out_expo(u: f64) -> f64 { if u >= 1.0 { 1.0 } else { 1.0 - 2f64.powf(-10.0 * u) } }
fn out_back(u: f64, s: f64) -> f64 { let c = s + 1.0; 1.0 + c * (u - 1.0).powi(3) + s * (u - 1.0).powi(2) }
fn spring(u: f64, freq: f64, damp: f64) -> f64 { if u <= 0.0 { 0.0 } else if u >= 1.0 { 1.0 } else { 1.0 - (-damp * u).exp() * (2.0 * PI * freq * u).cos() } }
fn bounce(u: f64, e: f64) -> f64 { // fall from 1 to 0 height with restitution e: returns height (1 at start, 0 at rest)
    let g: f64 = 2.0; let t_fall = (2.0 / g).sqrt(); let mut t = u * 3.0 * t_fall; if t < t_fall { return 1.0 - 0.5 * g * t * t; } t -= t_fall; let mut v = g * t_fall * e; let mut tb = 2.0 * v / g;
    for _ in 0..6 { if t < tb { return v * t - 0.5 * g * t * t; } t -= tb; v *= e; tb = 2.0 * v / g; } 0.0 }
fn noise(i: f64, s: f64) -> f64 { ((i * 12.9898 + s * 78.233).sin() * 43758.5453).fract() * 2.0 - 1.0 }
pub const NAMES: &[&str] = &["pop_in", "bounce_in", "slide_in_left", "slide_in_right", "slide_in_up", "slide_in_down", "whip_in", "spin_in", "zoom_punch", "shake", "wobble", "float", "pulse", "heartbeat", "elastic_in", "fade_out_down", "fade_in", "fade_out", "drift_zoom", "glitch_jitter", "drop_out"];
/// Sample preset `name` at time `s` (seconds from preset start) with duration `d`, intensity `k`, distance `dist` px, rate `bpm`.
pub fn sample(name: &str, s: f64, d: f64, k: f64, dist: f64, bpm: f64) -> Option<Sample> { let u = (s / d).clamp(0.0, 1.0); let one = (0.0, 0.0, 1.0, 0.0, 1.0);
    Some(match name {
        "pop_in" => (0.0, 0.0, (out_back(u, 1.9 * k)).max(0.0), 0.0, (u * 6.0).min(1.0)),
        "elastic_in" => (0.0, 0.0, spring(u, 3.0 + k, 5.0 / k.max(0.3)), 0.0, (u * 8.0).min(1.0)),
        "bounce_in" => (0.0, -dist * bounce(u, 0.45 + 0.1 * k).max(0.0), 1.0, 0.0, (u * 10.0).min(1.0)),
        "slide_in_left" => (-dist * (1.0 - out_expo(u)), 0.0, 1.0, 0.0, (u * 5.0).min(1.0)),
        "slide_in_right" => (dist * (1.0 - out_expo(u)), 0.0, 1.0, 0.0, (u * 5.0).min(1.0)),
        "slide_in_up" => (0.0, dist * (1.0 - out_expo(u)), 1.0, 0.0, (u * 5.0).min(1.0)),
        "slide_in_down" => (0.0, -dist * (1.0 - out_expo(u)), 1.0, 0.0, (u * 5.0).min(1.0)),
        "whip_in" => (-dist * 2.0 * (1.0 - out_expo(u)).powf(1.4), 0.0, 1.0 + 0.12 * k * (1.0 - out_expo(u)), 0.0, (u * 8.0).min(1.0)),
        "spin_in" => (0.0, 0.0, out_back(u, 1.2).max(0.0), -360.0 * k * (1.0 - out_expo(u)), (u * 6.0).min(1.0)),
        "zoom_punch" => { let e = if u < 0.25 { out_expo(u / 0.25) } else { 1.0 - out_expo((u - 0.25) / 0.75) }; (0.0, 0.0, 1.0 + 0.3 * k * e, 0.0, 1.0) }
        "shake" => { let a = (1.0 - u).powi(2) * dist * 0.05 * k; let i = (s * 60.0).floor(); (noise(i, 1.0) * a, noise(i, 2.0) * a, 1.0, noise(i, 3.0) * a * 0.1, 1.0) }
        "glitch_jitter" => { let i = (s * 25.0).floor(); let on = noise(i, 9.0) > 0.35; let a = if on { dist * 0.08 * k } else { 0.0 }; (noise(i, 4.0) * a, noise(i, 5.0) * a * 0.3, 1.0, 0.0, if on && noise(i, 6.0) > 0.7 { 0.6 } else { 1.0 }) }
        "wobble" => (0.0, 0.0, 1.0, 12.0 * k * (-4.0 * u).exp() * (2.0 * PI * 3.0 * u).sin(), 1.0),
        "float" => (0.0, -dist * 0.04 * k * (2.0 * PI * s / 2.5).sin(), 1.0, 1.5 * k * (2.0 * PI * s / 3.1).sin(), 1.0),
        "pulse" => { let ph = (s * bpm / 60.0).fract(); (0.0, 0.0, 1.0 + 0.12 * k * (-5.0 * ph).exp(), 0.0, 1.0) }
        "heartbeat" => { let ph = (s * 72.0 / 60.0).fract(); let b = (-30.0 * (ph - 0.08).powi(2)).exp() + 0.7 * (-30.0 * (ph - 0.28).powi(2)).exp(); (0.0, 0.0, 1.0 + 0.1 * k * b, 0.0, 1.0) }
        "fade_out_down" => (0.0, dist * 0.3 * u * u, 1.0, 0.0, 1.0 - u * u),
        "drop_out" => (0.0, dist * 2.0 * u * u, 1.0, 25.0 * k * u * u, 1.0 - (u * u).min(1.0)),
        "fade_in" => (0.0, 0.0, 1.0, 0.0, u),
        "fade_out" => (0.0, 0.0, 1.0, 0.0, 1.0 - u),
        "drift_zoom" => (0.0, 0.0, 1.0 + 0.15 * k * u, 0.0, 1.0),
        _ => { let _ = one; return None; }
    }) }
/// Map a plain-language description to presets, ordered by where each is mentioned; at most one entrance preset (the first mentioned) is kept so "bounce in from above" does not also slide.
pub fn from_prompt(prompt: &str) -> Vec<(&'static str, f64)> { let p = prompt.to_lowercase();
    const T: &[(&str, &[&str], bool)] = &[("heartbeat", &["heartbeat", "heart beat"], false), ("pop_in", &["pop", "appear", "burst in"], true), ("bounce_in", &["bounce", "drop in", "fall in", "land"], true), ("elastic_in", &["elastic", "spring", "rubber", "jelly"], true),
        ("slide_in_left", &["from the left", "slide left", "slide in from left"], true), ("slide_in_right", &["from the right", "slide right", "slide in from right"], true), ("slide_in_up", &["from below", "from the bottom", "slide up", "rise"], true), ("slide_in_down", &["from above", "from the top", "slide down"], true),
        ("whip_in", &["whip", "swoosh", "fast slide", "snap in"], true), ("spin_in", &["spin", "twirl", "rotate in"], true), ("zoom_punch", &["punch", "zoom hit", "impact", "slam", "beat hit"], false), ("shake", &["shake", "rumble", "earthquake"], false),
        ("glitch_jitter", &["glitch", "jitter", "digital"], false), ("wobble", &["wobble", "wiggle", "sway"], false), ("float", &["float", "hover", "levitate", "bob"], false), ("pulse", &["pulse", "throb", "bpm"], false),
        ("drop_out", &["fall out", "drop out", "fall away", "crash down"], false), ("fade_in", &["fade in"], true), ("fade_out_down", &["fade out", "disappear", "vanish"], false), ("drift_zoom", &["slow zoom", "ken burns", "push in", "drift"], false)];
    let mut hits: Vec<(usize, &'static str, bool)> = vec![]; for (n, kws, ent) in T { if let Some(pos) = kws.iter().filter_map(|k| p.find(k)).min() { hits.push((pos, n, *ent)); } }
    hits.sort_by_key(|h| h.0); let mut seen_entrance = false; let mut r = vec![]; for (_, n, ent) in hits { if ent { if seen_entrance { continue; } seen_entrance = true; } r.push((n, 2.0)); } r }
