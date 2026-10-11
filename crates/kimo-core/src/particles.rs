//! Deterministic particle emitter: closed-form birth schedule + per-particle integration (forces, turbulence, attractor, floor bounce),
//! life curves (size, color, alpha), trails, sprite shapes. Stateless per frame, so scrubbing and parallel render are exact.
use crate::effects::Frame;
use serde::{Deserialize, Serialize};

fn d1() -> f64 { 1.0 } fn d_half() -> f64 { 0.5 } fn d_rate() -> f64 { 60.0 } fn d_life() -> f64 { 2.0 } fn d_speed() -> f64 { 120.0 } fn d_sz() -> f64 { 6.0 }
fn d_shape() -> String { "point".into() } fn d_sprite() -> String { "soft".into() } fn d_c0() -> [f64; 3] { [1.0, 0.8, 0.4] } fn d_c1() -> [f64; 3] { [1.0, 0.2, 0.05] } fn d_up() -> f64 { 270.0 } fn d_spread() -> f64 { 30.0 } fn d_seed() -> u64 { 1 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Emitter {
    #[serde(default = "d_shape")] pub shape: String,            // point | line | circle | rect | ring
    #[serde(default = "d_half")] pub x: f64, #[serde(default = "d_half")] pub y: f64,          // emitter center, normalized 0..1
    #[serde(default)] pub width: f64, #[serde(default)] pub height: f64,                       // emitter size, normalized to frame
    #[serde(default = "d_rate")] pub rate: f64, #[serde(default = "d_life")] pub life: f64, #[serde(default)] pub life_var: f64,
    #[serde(default = "d_speed")] pub speed: f64, #[serde(default)] pub speed_var: f64,        // px/s
    #[serde(default = "d_up")] pub angle: f64, #[serde(default = "d_spread")] pub spread: f64,  // degrees, 270 = up (screen coords)
    #[serde(default)] pub gravity: f64, #[serde(default)] pub wind: f64, #[serde(default)] pub drag: f64,
    #[serde(default)] pub turbulence: f64, #[serde(default = "d1")] pub turb_scale: f64,       // px/s^2, field size in frame fractions
    #[serde(default)] pub attractor_x: f64, #[serde(default)] pub attractor_y: f64, #[serde(default)] pub attractor: f64, // strength px/s^2 (0 = off), pos normalized
    #[serde(default)] pub floor: bool, #[serde(default = "d_half")] pub bounce: f64,
    #[serde(default = "d_sz")] pub size0: f64, #[serde(default = "d_sz")] pub size1: f64, #[serde(default)] pub size_var: f64,
    #[serde(default = "d_c0")] pub color0: [f64; 3], #[serde(default = "d_c1")] pub color1: [f64; 3],
    #[serde(default = "d1")] pub alpha0: f64, #[serde(default)] pub alpha1: f64,
    #[serde(default)] pub fade_in: f64, #[serde(default)] pub trail: u32, #[serde(default = "d_sprite")] pub sprite: String,        // soft | hard | streak | ring | star | spark
    #[serde(default)] pub shutter_deg: f64, #[serde(default)] pub blur_samples: u32, #[serde(default)] pub prewarm: bool, #[serde(default = "d_seed")] pub seed: u64, #[serde(default)] pub additive: bool,
}
impl Default for Emitter { fn default() -> Self { serde_json::from_str("{}").unwrap() } }

fn h(seed: u64, i: i64, k: u64) -> f64 { let mut x = seed ^ (i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ k.wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
fn lerp(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }

pub fn render(e: &Emitter, f: &mut Frame, t: f64) { render_fps(e, f, t, 25.0) }
/// Motion blur: when `shutter_deg` > 0 the particle system is integrated at `blur_samples` instants spread over the shutter interval
/// centred on `t` (shutter_deg/360 of a frame) and the premultiplied accumulation buffers are averaged, which is the temporal integral of what the
/// camera sees (exact for every sprite, speed and force, cost = samples x one render). Auto samples = fastest-particle travel over the shutter divided by 0.6 of the smallest sprite size, clamped 4..32 (so no visible stepping). 0 degrees = off, pixel-identical to before.
pub fn render_fps(e: &Emitter, f: &mut Frame, t: f64, fps: f64) {
    let buf = if e.shutter_deg > 0.0 { let sh = e.shutter_deg.clamp(0.0, 720.0) / 360.0 / fps.max(1.0); let n = if e.blur_samples == 0 { let sc = f.h as f64 / 1080.0; let travel = (e.speed + e.speed_var.abs()) * sc * sh; ((travel / (0.6 * e.size0.min(e.size1).max(1.0) * sc).max(1.0)).ceil() as usize).clamp(4, 32) } else { e.blur_samples.clamp(2, 64) as usize };
        let mut acc = vec![0f32; f.w * f.h * 4]; for k in 0..n { let tk = t + sh * ((k as f64 + 0.5) / n as f64 - 0.5); let b = accumulate(e, f.w, f.h, tk); for i in 0..acc.len() { acc[i] += b[i]; } } let inv = 1.0 / n as f32; for v in acc.iter_mut() { *v *= inv; } acc }
        else { accumulate(e, f.w, f.h, t) };
    composite(e, f, &buf);
}
fn accumulate(e: &Emitter, fw: usize, fh: usize, t: f64) -> Vec<f32> {
    let (w, hh) = (fw as f64, fh as f64); let sc = hh / 1080.0; let rate = e.rate.max(0.01); let lmax = e.life + e.life_var.abs();
    let i_hi = (t * rate).floor() as i64; let i_lo = ((t - lmax) * rate).floor() as i64 + 1; let i_lo = if e.prewarm { i_lo } else { i_lo.max(0) };
    let mut buf = vec![0f32; fw * fh * 4]; // premultiplied accumulation
    for i in i_lo..=i_hi {
        let birth = i as f64 / rate; let life = (e.life + (h(e.seed, i, 9) - 0.5) * 2.0 * e.life_var).max(0.05); let age = t - birth; if age < 0.0 || age > life { continue; }
        // spawn
        let (rx, ry, ra, rs) = (h(e.seed, i, 1), h(e.seed, i, 2), h(e.seed, i, 3), h(e.seed, i, 4));
        let (ew, eh) = (e.width * w, e.height * hh); let (cx, cy) = (e.x * w, e.y * hh);
        let (mut px, mut py) = match e.shape.as_str() { "line" => (cx + (rx - 0.5) * ew, cy + (ry - 0.5) * eh), "rect" => (cx + (rx - 0.5) * ew, cy + (ry - 0.5) * eh),
            "circle" => { let a = rx * std::f64::consts::TAU; let r = ry.sqrt(); (cx + a.cos() * r * ew * 0.5, cy + a.sin() * r * ew * 0.5) }
            "ring" => { let a = rx * std::f64::consts::TAU; (cx + a.cos() * ew * 0.5, cy + a.sin() * ew * 0.5) } _ => (cx, cy) };
        let ang = (e.angle + (ra - 0.5) * 2.0 * e.spread).to_radians(); let spd = (e.speed + (rs - 0.5) * 2.0 * e.speed_var) * sc;
        let (mut vx, mut vy) = (ang.cos() * spd, ang.sin() * spd); if e.shape == "ring" && e.spread >= 179.0 { let d = (px - cx).atan2(py - cy); let _ = d; let a = (py - cy).atan2(px - cx); vx = a.cos() * spd; vy = a.sin() * spd; }
        // integrate
        let steps = ((age * 60.0).ceil() as usize).clamp(1, 400); let dt = age / steps as f64; let mut hist: Vec<(f64, f64)> = Vec::new(); let every = (steps / (e.trail as usize + 1).max(1)).max(1);
        for s in 0..steps { let tt = birth + s as f64 * dt;
            let mut ax = e.wind * sc; let mut ay = e.gravity * sc;
            if e.turbulence != 0.0 { let k = 6.0 / (e.turb_scale.max(0.05) * w.max(hh)); ax += e.turbulence * sc * ((py * k * 0.9 + tt * 1.3 + rx * 6.0).sin() + 0.5 * (py * k * 2.1 - tt * 0.7).cos()); ay += e.turbulence * sc * ((px * k * 1.1 - tt * 1.1 + ry * 6.0).cos() + 0.5 * (px * k * 1.9 + tt * 0.9).sin()); }
            if e.attractor != 0.0 { let (dx, dy) = (e.attractor_x * w - px, e.attractor_y * hh - py); let d = (dx * dx + dy * dy).sqrt().max(20.0 * sc); ax += dx / d * e.attractor * sc; ay += dy / d * e.attractor * sc; }
            let dr = (-e.drag * dt).exp(); vx = (vx + ax * dt) * dr; vy = (vy + ay * dt) * dr; px += vx * dt; py += vy * dt;
            if e.floor && py > hh { py = hh; vy = -vy.abs() * e.bounce; vx *= 0.85; }
            if e.trail > 0 && s % every == 0 { hist.push((px, py)); } }
        let u = age / life; let size = lerp(e.size0, e.size1, u) * sc * (1.0 + (h(e.seed, i, 5) - 0.5) * 2.0 * e.size_var); let mut alpha = lerp(e.alpha0, e.alpha1, u).clamp(0.0, 1.5); if e.fade_in > 0.0 { let t = (u / e.fade_in).clamp(0.0, 1.0); alpha *= t * t * (3.0 - 2.0 * t); }
        let col = [lerp(e.color0[0], e.color1[0], u), lerp(e.color0[1], e.color1[1], u), lerp(e.color0[2], e.color1[2], u)];
        let mut pts: Vec<(f64, f64, f64)> = hist.iter().rev().enumerate().map(|(k, p)| (p.0, p.1, alpha * (1.0 - (k as f64 + 1.0) / (hist.len() as f64 + 1.0)) * 0.6)).collect(); pts.insert(0, (px, py, alpha));
        for (k, (x, y, a)) in pts.iter().enumerate() { let s_ = size * (1.0 - 0.5 * k as f64 / pts.len() as f64).max(0.2); stamp(&mut buf, fw, fh, *x, *y, s_.max(0.4), *a as f32, col, &e.sprite, vx, vy); }
    }
    buf
}
fn composite(e: &Emitter, f: &mut Frame, buf: &[f32]) {
    // composite premultiplied buffer over frame
    for i in 0..f.w * f.h { let (r, g, b, a) = (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]); if a <= 0.0 { continue; } let a = a.min(1.0);
        let o = &mut f.d[i * 4..i * 4 + 4]; if e.additive { for (j, c) in [r, g, b].iter().enumerate() { o[j] = (o[j] * o[3] + c).min(4.0); } o[3] = (o[3] + a).min(1.0); let al = o[3].max(1e-6); for j in 0..3 { o[j] /= al; } }
        else { let na = a + o[3] * (1.0 - a); for (j, c) in [r, g, b].iter().enumerate() { o[j] = (c + o[j] * o[3] * (1.0 - a)) / na.max(1e-6); } o[3] = na; } }
}

fn stamp(buf: &mut [f32], w: usize, hgt: usize, x: f64, y: f64, r: f64, a: f32, c: [f64; 3], sprite: &str, vx: f64, vy: f64) {
    let ext = match sprite { "streak" => r * 3.0 + vx.hypot(vy) * 0.04, "spark" => r * 4.0, _ => r * 2.6 } + 1.0;
    let (x0, x1, y0, y1) = ((x - ext).floor().max(0.0) as usize, ((x + ext).ceil() as usize).min(w.saturating_sub(1)), (y - ext).floor().max(0.0) as usize, ((y + ext).ceil() as usize).min(hgt.saturating_sub(1)));
    let vl = vx.hypot(vy).max(1e-6); let (dx, dy) = (vx / vl, vy / vl); let slen = (vl * 0.035).clamp(r, r * 8.0);
    for yy in y0..=y1 { for xx in x0..=x1 { let (px, py) = (xx as f64 + 0.5 - x, yy as f64 + 0.5 - y); let d = px.hypot(py) / r;
        let k = match sprite { "hard" => (1.0 - (d - 1.0) * 2.0).clamp(0.0, 1.0), "ring" => { (1.0 - ((d - 1.0).abs() * r / (r * 0.18).max(0.7))).clamp(0.0, 1.0) }
            "star" => { let ang = py.atan2(px); let rr = 0.55 + 0.45 * (ang * 4.0).cos().abs().powf(3.0); ((1.0 - d / (1.6 * rr)).clamp(0.0, 1.0)).powf(1.5) }
            "streak" | "spark" => { let along = px * dx + py * dy; let perp = -px * dy + py * dx; let l = if sprite == "spark" { slen * 1.6 } else { slen }; let q = (along / l).abs().min(2.0) + 0.0; let core = (1.0 - (perp.abs() / (r * 0.6)).min(1.0)) * (1.0 - (along.abs() / (l * if along > 0.0 { 0.3 } else { 1.0 })).min(1.0)); let _ = q; core.max(0.0).powf(1.2) }
            "mote" => { let t = ((1.3 - d) / 0.7).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) * 0.9 }
            "puff" => (-(d * d) * 1.1).exp() * (1.0 - d / 2.6).clamp(0.0, 1.0),
            _ => (1.0 - d / 2.6).max(0.0).powi(3) * 1.8 } as f32;
        let kk = (k * a).min(1.0); if kk <= 0.0 { continue; } let o = (yy * w + xx) * 4; buf[o] += c[0] as f32 * kk; buf[o + 1] += c[1] as f32 * kk; buf[o + 2] += c[2] as f32 * kk; buf[o + 3] += kk; } }
}

pub fn preset(name: &str) -> Result<serde_json::Value, String> {
    use serde_json::json;
    Ok(match name {
        "fire" => json!({"shape":"line","x":0.5,"y":0.95,"width":0.25,"height":0.0,"rate":220,"life":1.4,"life_var":0.5,"speed":260,"speed_var":120,"angle":270,"spread":14,"gravity":-120,"turbulence":520,"turb_scale":0.25,"drag":0.6,"size0":38,"size1":6,"size_var":0.5,"color0":[1.0,0.72,0.25],"color1":[0.9,0.1,0.0],"alpha0":0.7,"alpha1":0.0,"sprite":"soft","additive":true,"prewarm":true,"seed":3}),
        "sparks" => json!({"shape":"point","x":0.5,"y":0.8,"rate":170,"life":1.2,"life_var":0.5,"speed":700,"speed_var":420,"angle":270,"spread":55,"gravity":900,"drag":0.4,"size0":3.5,"size1":1.0,"color0":[1.0,0.9,0.6],"color1":[1.0,0.35,0.05],"alpha0":1.0,"alpha1":0.0,"sprite":"streak","trail":3,"floor":true,"bounce":0.4,"additive":true,"prewarm":true,"seed":5}),
        "snow" => json!({"shape":"line","x":0.5,"y":-0.05,"width":1.2,"rate":90,"life":9,"life_var":3,"speed":70,"speed_var":40,"angle":90,"spread":12,"wind":14,"turbulence":38,"turb_scale":0.5,"size0":5,"size1":5,"size_var":0.6,"color0":[1,1,1],"color1":[0.85,0.92,1],"alpha0":0.8,"alpha1":0.8,"fade_in":0.1,"sprite":"soft","prewarm":true,"seed":9}),
        "rain" => json!({"shape":"line","x":0.5,"y":-0.05,"width":1.3,"rate":320,"life":1.1,"speed":1500,"speed_var":300,"angle":100,"spread":2,"size0":1.6,"size1":1.6,"color0":[0.75,0.85,1],"color1":[0.75,0.85,1],"alpha0":0.5,"alpha1":0.5,"sprite":"streak","prewarm":true,"seed":2}),
        "smoke" => json!({"shape":"line","x":0.5,"y":0.95,"width":0.07,"rate":36,"life":8,"life_var":2.5,"speed":75,"speed_var":35,"angle":270,"spread":8,"wind":16,"turbulence":140,"turb_scale":0.28,"drag":0.25,"size0":45,"size1":210,"size_var":0.6,"color0":[0.66,0.66,0.7],"color1":[0.32,0.32,0.36],"alpha0":0.06,"alpha1":0.0,"fade_in":0.15,"sprite":"puff","prewarm":true,"seed":4}),
        "dust" => json!({"shape":"rect","x":0.5,"y":0.5,"width":1.1,"height":1.1,"rate":9,"life":10,"life_var":3,"speed":8,"speed_var":6,"angle":300,"spread":90,"turbulence":10,"turb_scale":0.5,"size0":6,"size1":8,"size_var":1.2,"color0":[1.0,0.93,0.8],"color1":[0.9,0.92,1.0],"alpha0":0.16,"alpha1":0.16,"fade_in":0.25,"sprite":"mote","additive":true,"prewarm":true,"seed":41}),
        "confetti" => json!({"shape":"point","x":0.5,"y":0.9,"rate":150,"life":3.2,"life_var":0.8,"speed":1100,"speed_var":500,"angle":270,"spread":38,"gravity":1100,"drag":1.4,"turbulence":220,"turb_scale":0.3,"size0":9,"size1":9,"size_var":0.5,"color0":[1.0,0.3,0.5],"color1":[0.3,0.7,1.0],"alpha0":1,"alpha1":1,"sprite":"hard","seed":13}),
        "fireflies" => json!({"shape":"rect","x":0.5,"y":0.6,"width":1.0,"height":0.7,"rate":8,"life":6,"life_var":2,"speed":16,"speed_var":10,"angle":270,"spread":180,"turbulence":60,"turb_scale":0.3,"size0":7,"size1":7,"size_var":0.5,"color0":[1.0,0.95,0.55],"color1":[0.7,1.0,0.5],"alpha0":0.9,"alpha1":0.0,"sprite":"soft","additive":true,"prewarm":true,"seed":21}),
        "bokeh" => json!({"shape":"rect","x":0.5,"y":0.5,"width":1.1,"height":1.1,"rate":6,"life":8,"life_var":2,"speed":14,"speed_var":8,"angle":300,"spread":60,"size0":44,"size1":60,"size_var":0.6,"color0":[1.0,0.85,0.6],"color1":[0.6,0.7,1.0],"alpha0":0.0,"alpha1":0.0,"sprite":"ring","additive":true,"prewarm":true,"seed":17}),
        "magic_dust" => json!({"shape":"circle","x":0.5,"y":0.5,"width":0.15,"rate":120,"life":1.8,"life_var":0.6,"speed":120,"speed_var":90,"angle":270,"spread":180,"gravity":-30,"drag":0.9,"turbulence":160,"turb_scale":0.2,"size0":9,"size1":0.5,"color0":[1.0,0.95,0.7],"color1":[0.75,0.55,1.0],"alpha0":1,"alpha1":0,"sprite":"star","additive":true,"prewarm":true,"seed":23}),
        "explosion" => json!({"shape":"point","x":0.5,"y":0.5,"rate":700,"life":1.6,"life_var":0.6,"speed":900,"speed_var":700,"angle":0,"spread":180,"drag":2.2,"gravity":200,"turbulence":300,"size0":18,"size1":3,"size_var":0.4,"color0":[1.0,0.85,0.45],"color1":[0.8,0.1,0.0],"alpha0":0.5,"alpha1":0.0,"sprite":"soft","trail":2,"additive":true,"seed":31}),
        o => return Err(format!("unknown preset {o}; presets: dust fire sparks snow rain smoke confetti fireflies bokeh magic_dust explosion")),
    })
}
