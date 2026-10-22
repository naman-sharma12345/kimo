//! v6 eyes: saliency_map. Where the eye is likely to go in one composited frame, as raw numbers: the spectral residual
//! saliency of Hou & Zhang 2007 ("Saliency Detection: A Spectral Residual Approach", CVPR) on Rec.709 luma of the output
//! frame, area-averaged to `work_px` on its long side (default 64, as in the paper): log amplitude spectrum minus its 3x3
//! box average (edges replicated), back with the original phase, squared magnitude, Gaussian blur (`blur_px` work
//! pixels, default 2.5), min-max scaled to 0..1 (the reference code's mat2gray). Optional motion term: the absolute luma change to the frame one project frame later, on the same
//! grid and blur, scaled to max 1, mixed in as S = (1 - motion) * spectral + motion * change. Numbers: the peak (canvas
//! px), the mass centroid and spread, the share of the frame area that holds half the mass, a grid x grid table of mass,
//! and per rect (e.g. a title box) its area share, mass share, their ratio, its max, and the share of the top 10% cells
//! inside it. Own code; FFT by rustfft (already a dependency). Raw numbers, no verdict.
use super::kit::*;
use crate::perception::analysis::Image;
use rustfft::{num_complex::Complex, FftPlanner};

pub const MAX_RECTS: usize = 16;
fn r4(x: f64) -> f64 { (x * 1e4).round() / 1e4 }

#[derive(Clone, Copy, Debug)]
pub struct SParams { pub work_px: usize, pub blur_px: f64, pub motion: f64, pub grid: usize }
impl Default for SParams { fn default() -> Self { SParams { work_px: 64, blur_px: 2.5, motion: 0.0, grid: 3 } } }

/// Area-average a w x h luma plane down to ww x wh (each output cell the mean of the source pixels it covers, by area).
pub fn area_down(l: &[f64], w: usize, h: usize, ww: usize, wh: usize) -> Vec<f64> {
    let (sx, sy) = (w as f64 / ww as f64, h as f64 / wh as f64); let mut out = vec![0.0; ww * wh];
    for oy in 0..wh { let (y0, y1) = (oy as f64 * sy, (oy + 1) as f64 * sy);
        for ox in 0..ww { let (x0, x1) = (ox as f64 * sx, (ox + 1) as f64 * sx); let (mut s, mut a) = (0.0, 0.0);
            for y in (y0.floor() as usize)..((y1.ceil() as usize).min(h)) { let fy = (y1.min(y as f64 + 1.0) - y0.max(y as f64)).max(0.0);
                for x in (x0.floor() as usize)..((x1.ceil() as usize).min(w)) { let f = fy * (x1.min(x as f64 + 1.0) - x0.max(x as f64)).max(0.0); s += f * l[y * w + x]; a += f; } }
            out[oy * ww + ox] = if a > 0.0 { s / a } else { 0.0 }; } }
    out
}

fn fft2(buf: &mut [Complex<f64>], w: usize, h: usize, inverse: bool) {
    let mut pl = FftPlanner::<f64>::new();
    let (fr, fc) = if inverse { (pl.plan_fft_inverse(w), pl.plan_fft_inverse(h)) } else { (pl.plan_fft_forward(w), pl.plan_fft_forward(h)) };
    for row in buf.chunks_mut(w) { fr.process(row); }
    let mut col = vec![Complex::new(0.0, 0.0); h];
    for x in 0..w { for y in 0..h { col[y] = buf[y * w + x]; } fc.process(&mut col); for y in 0..h { buf[y * w + x] = col[y]; } }
}

/// Separable Gaussian blur (edges clamped); sigma in grid cells, 0 = none.
pub fn blur(m: &[f64], w: usize, h: usize, sigma: f64) -> Vec<f64> {
    if sigma <= 0.0 { return m.to_vec(); }
    let r = (sigma * 3.0).ceil() as i64; let k: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect(); let ks: f64 = k.iter().sum();
    let mut t = vec![0.0; w * h]; let mut o = vec![0.0; w * h];
    for y in 0..h { for x in 0..w { let mut s = 0.0; for (j, kv) in k.iter().enumerate() { let xx = (x as i64 + j as i64 - r).clamp(0, w as i64 - 1) as usize; s += kv * m[y * w + xx]; } t[y * w + x] = s / ks; } }
    for y in 0..h { for x in 0..w { let mut s = 0.0; for (j, kv) in k.iter().enumerate() { let yy = (y as i64 + j as i64 - r).clamp(0, h as i64 - 1) as usize; s += kv * t[yy * w + x]; } o[y * w + x] = s / ks; } }
    o
}

fn scale_max(v: &mut [f64]) -> f64 { let mx = v.iter().cloned().fold(0.0f64, f64::max); if mx > 0.0 { for x in v.iter_mut() { *x /= mx; } } mx }

/// Spectral residual saliency of a ww x wh luma grid, blurred, min-max scaled to 0..1 (all zero for a flat grid).
pub fn spectral_residual(g: &[f64], ww: usize, wh: usize, blur_px: f64) -> Vec<f64> {
    let n = ww * wh; let mut buf: Vec<Complex<f64>> = g.iter().map(|v| Complex::new(*v, 0.0)).collect(); fft2(&mut buf, ww, wh, false);
    let la: Vec<f64> = buf.iter().map(|c| (c.norm() + 1e-12).ln()).collect();
    let mut res = vec![0.0; n];
    for y in 0..wh { for x in 0..ww { let mut s = 0.0; for dy in -1i64..=1 { for dx in -1i64..=1 {
        let xx = (x as i64 + dx).clamp(0, ww as i64 - 1) as usize; let yy = (y as i64 + dy).clamp(0, wh as i64 - 1) as usize; s += la[yy * ww + xx]; } }
        res[y * ww + x] = la[y * ww + x] - s / 9.0; } }
    for i in 0..n { let ph = buf[i].arg(); buf[i] = Complex::from_polar(res[i].exp(), ph); }
    fft2(&mut buf, ww, wh, true);
    let flat = { let (mn, mx) = g.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v))); mx - mn < 1e-9 };
    if flat { return vec![0.0; n]; }
    let mut s: Vec<f64> = buf.iter().map(|c| c.norm_sqr() / (n as f64 * n as f64)).collect(); s = blur(&s, ww, wh, blur_px);
    // min-max scaling (mat2gray in the paper's reference code): the constant floor the DC term leaves goes to 0
    minmax_scale(&mut s); s
}

/// Min-max scale to 0..1 in place (mat2gray). A constant map (max == min) becomes all zeros: no divide by zero, no NaN.
pub fn minmax_scale(v: &mut [f64]) {
    let (mn, mx) = v.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
    if mx > mn { for x in v.iter_mut() { *x = (*x - mn) / (mx - mn); } } else { v.iter_mut().for_each(|x| *x = 0.0); }
}

/// Numbers for a saliency grid `s` (ww x wh, max 1) on a w x h canvas, with rects [x,y,w,h] in canvas px.
pub fn stats(s: &[f64], ww: usize, wh: usize, w: usize, h: usize, rects: &[[f64; 4]], grid: usize) -> Value {
    let n = ww * wh; let (cw, ch) = (w as f64 / ww as f64, h as f64 / wh as f64); let total: f64 = s.iter().sum();
    let cx = |i: usize| ((i % ww) as f64 + 0.5) * cw; let cy = |i: usize| ((i / ww) as f64 + 0.5) * ch;
    if total <= 0.0 { return json!({"empty": true, "peak": Value::Null, "centroid": Value::Null, "rects": rects.iter().map(|r| json!({"rect": r, "area_frac": r4(r[2] * r[3] / (w * h) as f64), "mass_frac": Value::Null})).collect::<Vec<_>>()}); }
    let (mut ip, mut vp) = (0usize, -1.0); for (i, v) in s.iter().enumerate() { if *v > vp { vp = *v; ip = i; } }
    let (mx, my) = (s.iter().enumerate().map(|(i, v)| v * cx(i)).sum::<f64>() / total, s.iter().enumerate().map(|(i, v)| v * cy(i)).sum::<f64>() / total);
    let (sx, sy) = ((s.iter().enumerate().map(|(i, v)| v * (cx(i) - mx).powi(2)).sum::<f64>() / total).sqrt(), (s.iter().enumerate().map(|(i, v)| v * (cy(i) - my).powi(2)).sum::<f64>() / total).sqrt());
    let mut order: Vec<usize> = (0..n).collect(); order.sort_by(|a, b| s[*b].total_cmp(&s[*a]).then(a.cmp(b)));
    let (mut acc, mut k) = (0.0, 0usize); while k < n && acc < total * 0.5 { acc += s[order[k]]; k += 1; }
    let top_n = ((n as f64) * 0.1).ceil().max(1.0) as usize; let top: Vec<usize> = order[..top_n.min(n)].to_vec();
    // cover[i]: share of work cell i inside the rect (exact box overlap in canvas px)
    let cover = |r: &[f64; 4], i: usize| { let (x0, y0) = ((i % ww) as f64 * cw, (i / ww) as f64 * ch);
        let ox = ((x0 + cw).min(r[0] + r[2]) - x0.max(r[0])).max(0.0); let oy = ((y0 + ch).min(r[1] + r[3]) - y0.max(r[1])).max(0.0); ox * oy / (cw * ch) };
    let rr: Vec<Value> = rects.iter().map(|r| {
        let area = r[2] * r[3] / (w * h) as f64; let (mut m, mut mxi, mut tp) = (0.0, 0.0f64, 0.0);
        for i in 0..n { let c = cover(r, i); if c > 0.0 { m += c * s[i]; mxi = mxi.max(s[i]); } }
        for i in &top { tp += cover(r, *i); }
        let mf = m / total;
        json!({"rect": r, "area_frac": r4(area), "mass_frac": r4(mf), "mass_per_area": if area > 0.0 { json!(r4(mf / area)) } else { Value::Null }, "max": r4(mxi), "top10_frac": r4(tp / top.len() as f64)})
    }).collect();
    let mut cells = vec![]; for gy in 0..grid { for gx in 0..grid {
        let r = [gx as f64 * w as f64 / grid as f64, gy as f64 * h as f64 / grid as f64, w as f64 / grid as f64, h as f64 / grid as f64];
        let m: f64 = (0..n).map(|i| cover(&r, i) * s[i]).sum(); cells.push(r4(m / total)); } }
    json!({
        "peak": {"x": r4(cx(ip)), "y": r4(cy(ip)), "value": r4(vp)}, "centroid": [r4(mx), r4(my)], "spread": [r4(sx), r4(sy)],
        "mean": r4(total / n as f64), "half_mass_area_frac": r4(k as f64 / n as f64),
        "grid": {"cols": grid, "rows": grid, "mass": cells, "cell": "share of the total saliency mass, row-major from the top left"},
        "rects": rr,
    })
}

fn rect4(v: &Value, i: usize, w: usize, h: usize) -> Result<[f64; 4], String> {
    let bad = || format!("out_of_range: rects[{i}] must be [x,y,w,h] in canvas px with w,h > 0 inside the {w}x{h} frame");
    let a = v.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?; let r: Vec<f64> = a.iter().filter_map(|x| x.as_f64()).filter(|x| x.is_finite()).collect();
    if r.len() != 4 || r[2] <= 0.0 || r[3] <= 0.0 || r[0] < 0.0 || r[1] < 0.0 || r[0] + r[2] > w as f64 + 1e-9 || r[1] + r[3] > h as f64 + 1e-9 { return Err(bad()); }
    Ok([r[0], r[1], r[2], r[3]])
}

fn luma_of(f: &crate::effects::Frame) -> Vec<f64> { let im = Image::from_frame(f); (0..im.w * im.h).map(|i| im.luma(i)).collect() }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t = a.get("time").and_then(|v| v.as_f64()).filter(|t| t.is_finite() && *t >= 0.0).ok_or("out_of_range: time must be a number of timeline seconds >= 0")?;
    let mut q = SParams::default();
    if let Some(v) = a.get("work_px").filter(|v| !v.is_null()) { q.work_px = v.as_u64().filter(|v| (16..=512).contains(v)).ok_or("out_of_range: work_px must be a whole number 16..512 (long side of the saliency grid)")? as usize; }
    if let Some(v) = a.get("blur_px").filter(|v| !v.is_null()) { q.blur_px = v.as_f64().filter(|v| v.is_finite() && (0.0..=32.0).contains(v)).ok_or("out_of_range: blur_px must be a number 0..32 (Gaussian sigma in grid cells)")?; }
    if let Some(v) = a.get("motion").filter(|v| !v.is_null()) { q.motion = v.as_f64().filter(|v| v.is_finite() && (0.0..=1.0).contains(v)).ok_or("out_of_range: motion must be a number 0..1 (weight of the frame-change term)")?; }
    if let Some(v) = a.get("grid").filter(|v| !v.is_null()) { q.grid = v.as_u64().filter(|v| (1..=8).contains(v)).ok_or("out_of_range: grid must be a whole number 1..8")? as usize; }
    let (w, h) = (ss.project.width as usize, ss.project.height as usize);
    let rects: Vec<[f64; 4]> = match a.get("rects") { None | Some(Value::Null) => vec![], Some(Value::Array(v)) => { if v.len() > MAX_RECTS { return Err(format!("out_of_range: rects holds at most {MAX_RECTS} rects")); } v.iter().enumerate().map(|(i, r)| rect4(r, i, w, h)).collect::<Result<_, _>>()? } _ => return Err("out_of_range: rects must be an array of [x,y,w,h]".into()) };
    let path = match a.get("path") { None | Some(Value::Null) => None, Some(Value::String(s)) if !s.is_empty() => Some(s.clone()), _ => return Err("out_of_range: path must be a non-empty string (png)".into()) };
    let long = w.max(h).max(1); let (ww, wh) = (((w * q.work_px) as f64 / long as f64).round().max(1.0) as usize, ((h * q.work_px) as f64 / long as f64).round().max(1.0) as usize);
    crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t))?;   // media decoded into the session cache first (an empty cache draws media black)
    let f0 = crate::render::render_frame(&ss.project, t, &ss.cache)?; let l0 = luma_of(&f0);
    let g0 = area_down(&l0, w, h, ww, wh); let spec = spectral_residual(&g0, ww, wh, q.blur_px);
    let mut out_motion = Value::Null; let mut s = spec.clone();
    if q.motion > 0.0 {
        let t1 = t + 1.0 / ss.project.fps.max(1e-6);
        crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t1))?; let f1 = crate::render::render_frame(&ss.project, t1, &ss.cache)?; let l1 = luma_of(&f1);
        let d: Vec<f64> = l0.iter().zip(&l1).map(|(a, b)| (a - b).abs()).collect(); let mean_delta = d.iter().sum::<f64>() / d.len().max(1) as f64; let max_delta = d.iter().cloned().fold(0.0, f64::max);
        let mut m = blur(&area_down(&d, w, h, ww, wh), ww, wh, q.blur_px); let mmax = scale_max(&mut m);
        for i in 0..s.len() { s[i] = (1.0 - q.motion) * spec[i] + q.motion * m[i]; } scale_max(&mut s);
        out_motion = json!({"weight": q.motion, "next_time": r4(t1), "mean_delta": r4(mean_delta), "max_delta": r4(max_delta), "grid_max_delta": r4(mmax)});
    }
    let mut out = stats(&s, ww, wh, w, h, &rects, q.grid);
    out["time"] = json!(t); out["canvas"] = json!([w, h]); out["work"] = json!([ww, wh]);
    out["params"] = json!({"work_px": q.work_px, "blur_px": q.blur_px, "motion": q.motion, "grid": q.grid}); out["motion"] = out_motion;
    if let Some(p) = path {
        let mut fr = crate::effects::Frame::new(w, h);
        for y in 0..h { for x in 0..w { let v = s[((y * wh) / h).min(wh - 1) * ww + ((x * ww) / w).min(ww - 1)] as f32; let i = (y * w + x) * 4; fr.d[i] = v; fr.d[i + 1] = v; fr.d[i + 2] = v; fr.d[i + 3] = 1.0; } }
        crate::render::write_image(&fr, &p)?; out["path"] = json!(p);
    }
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![Command { name: "saliency_map", mutates: false,
    description: "Saliency of one output frame as raw numbers: where the eye is likely to go, and how much of it lands in given rects. Spectral residual (Hou & Zhang 2007) on luma at work_px (default 64) on the long side, Gaussian blur_px (default 2.5 grid cells), min-max scaled to 0..1; motion (0..1, default 0) mixes in the luma change to the next project frame. Returns the peak and mass centroid / spread in canvas px, half_mass_area_frac, a grid x grid mass table, and per rect [x,y,w,h] (up to 16, canvas px) area_frac, mass_frac, mass_per_area, max and top10_frac. path writes the map as a png. No verdict.",
    schema: obj(vec![
        ("time", n("timeline seconds")),
        ("rects", json!({"type": "array", "maxItems": MAX_RECTS, "items": {"type": "array", "items": {"type": "number"}, "minItems": 4, "maxItems": 4}, "description": "rects [x,y,w,h] in canvas px, e.g. a title box"})),
        ("work_px", json!({"type": "integer", "minimum": 16, "maximum": 512, "description": "long side of the saliency grid, default 64"})),
        ("blur_px", json!({"type": "number", "minimum": 0, "maximum": 32, "description": "Gaussian sigma in grid cells, default 2.5"})),
        ("motion", json!({"type": "number", "minimum": 0, "maximum": 1, "description": "weight of the change to the next frame, default 0"})),
        ("grid", json!({"type": "integer", "minimum": 1, "maximum": 8, "description": "mass table size, default 3"})),
        ("path", s("optional output png of the map")),
    ], &["time"]), run }] }
