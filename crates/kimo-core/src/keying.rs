//! Keylight-class keyer: screen matte (gain/balance), clip black/white, core shrink + edge soften, spill suppression with luma replace,
//! edge color unmixing (removes key color contamination from semi-transparent pixels), matte view.
//! matte_model 1: linear colour-difference screen matte against a per-pixel screen colour (clean plate), trimap from clip black/white,
//! edge alpha by projection onto the nearest interior foreground colour (A. R. Smith, J. F. Blinn, "Blue Screen Matting", SIGGRAPH 1996).
//! Despill modes / bias / replace colour and the status view follow the Keylight documentation (ideas only, no code ported).
use crate::effects::{box_blur3, lum, p, Frame, Params};
use crate::color::Entry;

pub fn catalog4() -> Vec<Entry> { vec![
    ("keyer", "Pro green/blue-screen keyer (screen matte gain+balance, clip black/white, core shrink, edge soften, despill average/double/limit/min with bias and replace colour, edge unmix). matte_model 1: linear colour difference + projection edges, clean plate (strings.ref_clip/aux_path), garbage/core mattes (strings.garbage_mask/core_mask). Set r,g,b to the screen color (see pick_key_color). view: 0 result, 1 matte, 2 despilled color, 3 status, 4 raw screen matte.",
     Box::leak(vec![("r", 0.0, "screen R"), ("g", 0.75, "screen G"), ("b", 0.1, "screen B"), ("screen_gain", 1.0, "0..3 higher = more removed"), ("screen_balance", 0.5, "0..1 weight of max vs min of the other channels"),
        ("clip_black", 0.05, "0..1 matte below this becomes fully transparent"), ("clip_white", 0.95, "0..1 matte above this becomes fully opaque"),
        ("core_shrink", 0.0, "px, erodes the matte (positive) or grows (negative)"), ("edge_soften", 1.5, "px blur of the matte"),
        ("despill", 1.0, "0..1 spill suppression strength"), ("spill_replace", 0.3, "0..1 add luma back where spill was removed"), ("edge_unmix", 1.0, "0..1 remove key color from semi-transparent edge pixels (model 0)"), ("view", 0.0, "0 result,1 matte,2 despilled color,3 status,4 raw screen matte"),
        ("matte_model", 0.0, "0 legacy, 1 linear screen difference + edge refinement (clean plate, trimap, projection)"),
        ("despill_mode", 0.0, "key channel limited to 0 average (o1+o2)/2, 1 double (2*o1+o2)/3, 2 limit max(o1,o2), 3 min(o1,o2); o1=(m+1)%3: red for green, green for blue"),
        ("despill_bias_r", 0.5, "despill bias R (colour kept neutral)"), ("despill_bias_g", 0.5, "despill bias G"), ("despill_bias_b", 0.5, "despill bias B"),
        ("replace_mode", 0.0, "0 removed spill added half to each other channel, 1 replaced by replace_r/g/b"), ("replace_r", 0.5, "replace colour R"), ("replace_g", 0.5, "replace colour G"), ("replace_b", 0.5, "replace colour B")].into_boxed_slice())),
] }

/// Per-pixel inputs of the keyer: clean plate (screen colour per pixel, resampled to the frame), garbage matte (forces transparent)
/// and core matte (forces opaque), both w*h coverages.
#[derive(Default, Clone, Copy)]
pub struct KeyerAux<'a> { pub clean_plate: Option<&'a Frame>, pub garbage: Option<&'a [f32]>, pub core: Option<&'a [f32]> }
/// Owned aux the renderer hands to apply4("keyer") for one effect call (render.rs apply_effects_masked).
pub(crate) struct KeyerAuxOwned { pub clean_plate: Option<Frame>, pub garbage: Option<Vec<f32>>, pub core: Option<Vec<f32>> }
impl KeyerAuxOwned { pub(crate) fn view(&self) -> KeyerAux<'_> { KeyerAux { clean_plate: self.clean_plate.as_ref(), garbage: self.garbage.as_deref(), core: self.core.as_deref() } } }

fn erode(a: &mut [f32], w: usize, h: usize, r: i32, grow: bool) {
    if r <= 0 { return; }
    let src = a.to_vec(); let mut tmp = src.clone();
    for y in 0..h { for x in 0..w { let mut v = src[y * w + x]; for d in -r..=r { let xx = (x as i32 + d).clamp(0, w as i32 - 1) as usize; let s = src[y * w + xx]; v = if grow { v.max(s) } else { v.min(s) }; } tmp[y * w + x] = v; } }
    for y in 0..h { for x in 0..w { let mut v = tmp[y * w + x]; for d in -r..=r { let yy = (y as i32 + d).clamp(0, h as i32 - 1) as usize; let s = tmp[yy * w + x]; v = if grow { v.max(s) } else { v.min(s) }; } a[y * w + x] = v; } }
}

/// Separable Gaussian of a matte, sigma px, radius ceil(3 sigma) capped at max(w,h), clamped edges.
fn gauss(a: &mut [f32], w: usize, h: usize, s: f32) {
    let r = ((3.0 * s).ceil() as usize).clamp(1, w.max(h)); let mut k = vec![1f32; r + 1]; for i in 1..=r { k[i] = (-((i * i) as f32) / (2.0 * s * s)).exp(); } let sum = k[0] + 2.0 * k[1..].iter().sum::<f32>();
    let mut t = vec![0f32; w * h];
    for y in 0..h { let row = &a[y * w..(y + 1) * w]; for x in 0..w { let mut v = row[x]; for i in 1..=r { v += k[i] * (row[(x + i).min(w - 1)] + row[x.saturating_sub(i)]); } t[y * w + x] = v / sum; } }
    for y in 0..h { for x in 0..w { let mut v = t[y * w + x]; for i in 1..=r { v += k[i] * (t[(y + i).min(h - 1) * w + x] + t[y.saturating_sub(i) * w + x]); } a[y * w + x] = v / sum; } }
}

/// Bilinear sample of an rgb grid at pixel-centre coordinates (x, y), clamped.
fn bil3(g: &[[f32; 3]], gw: usize, gh: usize, x: f32, y: f32) -> [f32; 3] {
    let (x, y) = (x.clamp(0.0, (gw - 1) as f32), y.clamp(0.0, (gh - 1) as f32)); let (x0, y0) = (x as usize, y as usize); let (x1, y1) = ((x0 + 1).min(gw - 1), (y0 + 1).min(gh - 1)); let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (g[y0 * gw + x0], g[y0 * gw + x1], g[y1 * gw + x0], g[y1 * gw + x1]); let mut o = [0f32; 3];
    for j in 0..3 { o[j] = (a[j] * (1.0 - fx) + b[j] * fx) * (1.0 - fy) + (c[j] * (1.0 - fx) + d[j] * fx) * fy; } o
}

/// Foreground colour estimate: every pixel takes the (3x3 sample-averaged) colour of its nearest foreground sample, found by
/// jump flooding (Rong & Tan 2006) plus a final 1-step pass. None when there is no sample.
fn nearest_fill(c: &[[f32; 3]], samp: &[bool], w: usize, h: usize) -> Option<Vec<[f32; 3]>> {
    if !samp.iter().any(|&s| s) { return None; }
    // sample colours averaged over the samples in their 3x3 neighbourhood (noise robustness; a lone sample keeps its own colour)
    let sm: Vec<[f32; 3]> = (0..w * h).map(|i| { if !samp[i] { return [0.0; 3]; } let (x, y) = (i % w, i / w); let mut a = [0f32; 4];
        for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) { for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) { let j = yy * w + xx; if samp[j] { for k in 0..3 { a[k] += c[j][k]; } a[3] += 1.0; } } }
        [a[0] / a[3], a[1] / a[3], a[2] / a[3]] }).collect();
    let mut seed: Vec<u32> = (0..w * h).map(|i| if samp[i] { i as u32 } else { u32::MAX }).collect();
    let d2 = |i: usize, s: u32| { let (dx, dy) = ((i % w) as i64 - (s as usize % w) as i64, (i / w) as i64 - (s as usize / w) as i64); dx * dx + dy * dy };
    let mut step = (w.max(h).next_power_of_two() / 2).max(1) as i64; let mut steps = vec![]; while step >= 1 { steps.push(step); step /= 2; } steps.push(1);
    for st in steps { let prev = seed.clone(); for i in 0..w * h { let (x, y) = ((i % w) as i64, (i / w) as i64); let mut best = prev[i]; let mut bd = if best == u32::MAX { i64::MAX } else { d2(i, best) };
        for dy in [-st, 0, st] { for dx in [-st, 0, st] { let (xx, yy) = (x + dx, y + dy); if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 { continue; } let s = prev[yy as usize * w + xx as usize]; if s == u32::MAX { continue; } let dd = d2(i, s); if dd < bd { bd = dd; best = s; } } }
        seed[i] = best; } }
    Some(seed.iter().map(|&s| sm[s as usize]).collect())
}

/// Despill rules: mode (0 average, 1 double, 2 limit, 3 min), strength, replace amount, bias scale (None when the bias is grey), replace colour.
struct Spill { mode: u32, dsp: f32, rep: f32, scale: Option<[f32; 3]>, rc: Option<[f32; 3]> }
/// Limits the key channel; returns the amount removed from it (in the input's units). Defaults run the legacy expressions unchanged.
fn despill(c: &mut [f32; 3], m: usize, o1: usize, o2: usize, s: &Spill) -> f32 {
    let mut x = *c; if let Some(k) = s.scale { for j in 0..3 { x[j] *= k[j]; } }
    let lim = match s.mode { 1 => (2.0 * x[o1] + x[o2]) / 3.0, 2 => x[o1].max(x[o2]), 3 => x[o1].min(x[o2]), _ => (x[o1] + x[o2]) * 0.5 };
    let mut sp = 0.0; if x[m] > lim { sp = (x[m] - lim) * s.dsp; x[m] -= sp; match s.rc { Some(rc) => { for j in 0..3 { x[j] += sp * s.rep * rc[j]; } } None => { x[o1] += sp * s.rep * 0.5; x[o2] += sp * s.rep * 0.5; } } }
    if let Some(k) = s.scale { for j in 0..3 { x[j] /= k[j]; } sp /= k[m]; } *c = x; sp
}

/// Writes one output pixel for the view; non-finite colour is written as 0.
fn put(d: &mut [f32], o: usize, mut c: [f32; 3], al: f32, view: u32, sp: f32, raw: f32) {
    match view { 1 => { c = [al, al, al]; d[o + 3] = 1.0; } 2 => { d[o + 3] = 1.0; }
        3 => { c = if al <= 0.0 { [0.0; 3] } else if al < 1.0 { [0.5; 3] } else if sp > 0.01 { [1.0, 1.0, 0.0] } else { [1.0; 3] }; d[o + 3] = 1.0; }
        4 => { let r = raw.clamp(0.0, 1.0); c = [r, r, r]; d[o + 3] = 1.0; } _ => { d[o + 3] *= al; } }
    if !(c[0].is_finite() && c[1].is_finite() && c[2].is_finite()) { c = [0.0; 3]; }
    d[o] = c[0]; d[o + 1] = c[1]; d[o + 2] = c[2];
}

pub fn apply4(kind: &str, pr: &Params, f: &mut Frame, t: f64, _seed: u64) -> Result<(), String> {
    match kind {
        "keyer" => apply_keyer(pr, f, t, &KeyerAux::default()),
        other => Err(format!("unknown effect '{other}'")),
    }
}

pub fn apply_keyer(pr: &Params, f: &mut Frame, t: f64, aux: &KeyerAux) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t);
    let (w, h) = (f.w, f.h); let n = w * h; let key = [g("r", 0.0), g("g", 0.75), g("b", 0.1)];
    for (nm, v) in [("garbage", aux.garbage), ("core", aux.core)] { if let Some(v) = v { if v.len() != n { return Err(format!("keyer: {nm} matte has {} values, expected {n} ({w}x{h})", v.len())); } } }
    if let Some(cp) = aux.clean_plate { if cp.w == 0 || cp.h == 0 || cp.d.len() < cp.w * cp.h * 4 { return Err("keyer: empty or malformed clean plate".into()); } }
    if n == 0 { return Ok(()); }
    let m = if key[1] >= key[0] && key[1] >= key[2] { 1 } else if key[2] >= key[0] && key[2] >= key[1] { 2 } else { 0 }; let (o1, o2) = ((m + 1) % 3, (m + 2) % 3);
    let (gain, bal, cb, cw, shrink, soft) = (g("screen_gain", 1.0), g("screen_balance", 0.5), g("clip_black", 0.05), g("clip_white", 0.95), g("core_shrink", 0.0), g("edge_soften", 1.5));
    let (dsp, rep, unm, view) = (g("despill", 1.0), g("spill_replace", 0.3), g("edge_unmix", 1.0), g("view", 0.0) as u32);
    let bias = [g("despill_bias_r", 0.5), g("despill_bias_g", 0.5), g("despill_bias_b", 0.5)];
    let scale = if bias[0] == bias[1] && bias[1] == bias[2] { None } else { let yb = lum(bias).max(1e-3); Some([0usize, 1, 2].map(|j| (yb / bias[j].max(1e-3)).clamp(1e-6, 1e6))) };
    let rc = if g("replace_mode", 0.0) as u32 == 1 { Some([g("replace_r", 0.5), g("replace_g", 0.5), g("replace_b", 0.5)]) } else { None };
    let s = Spill { mode: g("despill_mode", 0.0) as u32, dsp, rep, scale, rc };
    let rf = |c: [f32; 3]| bal * c[o1].max(c[o2]) + (1.0 - bal) * c[o1].min(c[o2]);
    if g("matte_model", 0.0) as u32 == 1 { model1(f, aux, key, (m, o1, o2), rf, (gain, cb, cw, shrink, soft), &s, view); return Ok(()); }
    let sd = (key[m] - rf(key)).max(0.05); let mut raw = vec![0f32; if view == 4 { n } else { 0 }];
    let mut a: Vec<f32> = (0..n).map(|i| { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let v = 1.0 - gain * ((c[m] - rf(c)).max(0.0) / c[m].max(0.25 * key[m])) / (sd / key[m]); if view == 4 { raw[i] = v; }
        let a = ((v - cb) / (cw - cb).max(0.01)).clamp(0.0, 1.0); if a.is_nan() { 0.0 } else { a } }).collect();
    if let Some(gb) = aux.garbage { for i in 0..n { if gb[i] > 0.0 { a[i] *= 1.0 - gb[i].min(1.0); } } }
    if let Some(co) = aux.core { for i in 0..n { a[i] = a[i].max(co[i].min(1.0)); } }
    // a radius past the frame is the same min/max as the frame-bound radius (the box blur is O(n) for any radius already)
    let mx = w.max(h) as i32; if shrink > 0.0 { erode(&mut a, w, h, (shrink.round() as i32).min(mx), false); } else if shrink < 0.0 { erode(&mut a, w, h, ((-shrink).round() as i32).min(mx), true); }
    if soft > 0.0 { let mut b = Frame { w, h, d: vec![1.0; n * 4] }; for i in 0..n { b.d[i * 4] = a[i]; } box_blur3(&mut b, (soft * 2.0) as usize); for i in 0..n { a[i] = b.d[i * 4]; } }
    for i in 0..n { let o = i * 4; let mut c = [f.d[o], f.d[o + 1], f.d[o + 2]]; let al = a[i];
        // spill suppression: limit key channel to the (average) of the other two (despill_mode / bias / replace_mode)
        let sp = despill(&mut c, m, o1, o2, &s);
        // edge unmix: C = a*F + (1-a)*K  ->  F
        if unm > 0.0 && al > 0.01 && al < 0.999 { for j in 0..3 { let fgc = ((c[j] - (1.0 - al) * key[j]) / al).clamp(0.0, 1.5); c[j] += (fgc - c[j]) * unm; } }
        put(&mut f.d, o, c, al, view, sp, raw.get(i).copied().unwrap_or(0.0)); }
    Ok(())
}

/// matte_model 1 (Smith & Blinn): a_s = 1 - gain*d(C)/d(K(x)) with d(c) = c[m] - rf(c), trimap from clip black/white (+ garbage/core),
/// unknown alpha = projection of C-K onto Fh-K (Fh = colour of the nearest interior foreground sample), F = K + (C-K)/a, despill.
fn model1(f: &mut Frame, aux: &KeyerAux, key: [f32; 3], (m, o1, o2): (usize, usize, usize), rf: impl Fn([f32; 3]) -> f32, (gain, cb, cw, shrink, soft): (f32, f32, f32, f32, f32), s: &Spill, view: u32) {
    let (w, h) = (f.w, f.h); let n = w * h; let dd = |c: [f32; 3]| c[m] - rf(c);
    let kx: Option<Vec<[f32; 3]>> = aux.clean_plate.map(|cp| { let g: Vec<[f32; 3]> = (0..cp.w * cp.h).map(|i| [cp.d[i * 4], cp.d[i * 4 + 1], cp.d[i * 4 + 2]]).collect(); if cp.w == w && cp.h == h { return g; }
        let (sx, sy) = (cp.w as f32 / w as f32, cp.h as f32 / h as f32); (0..n).map(|i| bil3(&g, cp.w, cp.h, ((i % w) as f32 + 0.5) * sx - 0.5, ((i / w) as f32 + 0.5) * sy - 0.5)).collect() });
    let kat = |i: usize| kx.as_ref().map_or(key, |k| k[i]); let dk = dd(key).max(0.05);
    let px = |d: &[f32], i: usize| [d[i * 4], d[i * 4 + 1], d[i * 4 + 2]];
    let raw: Vec<f32> = (0..n).map(|i| { let v = 1.0 - gain * dd(px(&f.d, i)) / kx.as_ref().map_or(dk, |k| dd(k[i]).max(0.05)); if v.is_nan() { 0.0 } else { v.clamp(-1e6, 1e6) } }).collect();
    // foreground samples: screen-matte sure-fg (raw >= clip_white) whose whole 5x5 neighbourhood is sure-fg too, so the anti-aliased rim
    // (raw >= clip_white already at alpha ~0.75 for a colour with d(F) < 0) is solved by projection, not forced opaque. A core-forced pixel
    // may be screen coloured (a hole in the subject) and is no foreground sample.
    let fgr: Vec<bool> = (0..n).map(|i| raw[i] >= cw && px(&f.d, i).iter().all(|v| v.is_finite())).collect();
    let samp: Vec<bool> = (0..n).map(|i| { if !fgr[i] { return false; } let (x, y) = (i % w, i / w); (y.saturating_sub(2)..=(y + 2).min(h - 1)).all(|yy| (x.saturating_sub(2)..=(x + 2).min(w - 1)).all(|xx| fgr[yy * w + xx])) }).collect();
    // trimap 0 bg / 1 unknown / 2 fg; the core matte wins over the garbage matte (as in a *= 1-garbage; a = max(a, core))
    let tri: Vec<u8> = (0..n).map(|i| if aux.core.map_or(false, |c| c[i] >= 0.5) || samp[i] { 2 } else if aux.garbage.map_or(false, |g| g[i] >= 0.5) || raw[i] <= cb { 0 } else { 1 }).collect();
    let cols: Vec<[f32; 3]> = (0..n).map(|i| px(&f.d, i)).collect(); let fh = nearest_fill(&cols, &samp, w, h);
    let span = (cw - cb).max(0.01); let clip = |v: f32| { let a = ((v - cb) / span).clamp(0.0, 1.0); if a.is_nan() { 0.0 } else { a } };
    let mut a: Vec<f32> = (0..n).map(|i| match tri[i] { 0 => 0.0, 2 => 1.0, _ => match &fh { Some(fh) => { let (c, k) = (px(&f.d, i), kat(i)); let fk = [fh[i][0] - k[0], fh[i][1] - k[1], fh[i][2] - k[2]]; let den = fk[0] * fk[0] + fk[1] * fk[1] + fk[2] * fk[2];
        if den > 1e-4 { let a = (((c[0] - k[0]) * fk[0] + (c[1] - k[1]) * fk[1] + (c[2] - k[2]) * fk[2]) / den).clamp(0.0, 1.0); if a.is_nan() { 0.0 } else { a } } else { clip(raw[i]) } } None => clip(raw[i]) } }).collect();
    if let Some(gb) = aux.garbage { for i in 0..n { if gb[i] > 0.0 { a[i] *= 1.0 - gb[i].min(1.0); } } }
    if let Some(co) = aux.core { for i in 0..n { a[i] = a[i].max(co[i].min(1.0)); } }
    if shrink.is_finite() && shrink != 0.0 { let mut v = a; let lim = (w + h) as f32; crate::masks::morph(&mut v, w, h, (-shrink).clamp(-lim, lim)); a = v; }
    if soft > 0.0 { gauss(&mut a, w, h, soft); }
    for i in 0..n { let o = i * 4; let c = px(&f.d, i); let (al, k) = (a[i], kat(i));
        let mut fc = if tri[i] == 2 && al == 1.0 { c } else if al > 1e-3 { [0usize, 1, 2].map(|j| (k[j] + (c[j] - k[j]) / al).clamp(0.0, 1.5)) } else { fh.as_ref().map_or(c, |fh| fh[i]) };
        let sp = despill(&mut fc, m, o1, o2, s);
        put(&mut f.d, o, fc, al, view, sp, raw[i]); }
}
