//! Keylight-class keyer: screen matte (gain/balance), clip black/white, core shrink + edge soften, spill suppression with luma replace,
//! edge color unmixing (removes key color contamination from semi-transparent pixels), matte view.
use crate::effects::{box_blur3, p, Frame, Params};
use crate::color::Entry;

pub fn catalog4() -> Vec<Entry> { vec![
    ("keyer", "Pro green/blue-screen keyer (screen matte with gain+balance, clip black/white, core shrink, edge soften, spill suppression with luma replace, edge color unmixing). Set r,g,b to the screen color (see pick_key_color). view: 0 result, 1 matte, 2 despilled color only.",
     Box::leak(vec![("r", 0.0, "screen R"), ("g", 0.75, "screen G"), ("b", 0.1, "screen B"), ("screen_gain", 1.0, "0..3 higher = more removed"), ("screen_balance", 0.5, "0..1 weight of max vs min of the other channels"),
        ("clip_black", 0.05, "0..1 matte below this becomes fully transparent"), ("clip_white", 0.95, "0..1 matte above this becomes fully opaque"),
        ("core_shrink", 0.0, "px, erodes the matte (positive) or grows (negative)"), ("edge_soften", 1.5, "px blur of the matte"),
        ("despill", 1.0, "0..1 spill suppression strength"), ("spill_replace", 0.3, "0..1 add luma back where spill was removed"), ("edge_unmix", 1.0, "0..1 remove key color from semi-transparent edge pixels"), ("view", 0.0, "0 result,1 matte,2 despilled color")].into_boxed_slice())),
] }

fn erode(a: &mut [f32], w: usize, h: usize, r: i32, grow: bool) {
    if r <= 0 { return; }
    let src = a.to_vec(); let mut tmp = src.clone();
    for y in 0..h { for x in 0..w { let mut v = src[y * w + x]; for d in -r..=r { let xx = (x as i32 + d).clamp(0, w as i32 - 1) as usize; let s = src[y * w + xx]; v = if grow { v.max(s) } else { v.min(s) }; } tmp[y * w + x] = v; } }
    for y in 0..h { for x in 0..w { let mut v = tmp[y * w + x]; for d in -r..=r { let yy = (y as i32 + d).clamp(0, h as i32 - 1) as usize; let s = tmp[yy * w + x]; v = if grow { v.max(s) } else { v.min(s) }; } a[y * w + x] = v; } }
}

pub fn apply4(kind: &str, pr: &Params, f: &mut Frame, t: f64, _seed: u64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t);
    match kind {
        "keyer" => {
            let (w, h) = (f.w, f.h); let n = w * h; let key = [g("r", 0.0), g("g", 0.75), g("b", 0.1)];
            let m = if key[1] >= key[0] && key[1] >= key[2] { 1 } else if key[2] >= key[0] && key[2] >= key[1] { 2 } else { 0 }; let (o1, o2) = ((m + 1) % 3, (m + 2) % 3);
            let (gain, bal, cb, cw, shrink, soft) = (g("screen_gain", 1.0), g("screen_balance", 0.5), g("clip_black", 0.05), g("clip_white", 0.95), g("core_shrink", 0.0), g("edge_soften", 1.5));
            let (dsp, rep, unm, view) = (g("despill", 1.0), g("spill_replace", 0.3), g("edge_unmix", 1.0), g("view", 0.0) as u32);
            let rf = |c: [f32; 3]| bal * c[o1].max(c[o2]) + (1.0 - bal) * c[o1].min(c[o2]); let sd = (key[m] - rf(key)).max(0.05);
            let mut a: Vec<f32> = (0..n).map(|i| { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let v = 1.0 - gain * ((c[m] - rf(c)).max(0.0) / c[m].max(0.25 * key[m])) / (sd / key[m]); ((v - cb) / (cw - cb).max(0.01)).clamp(0.0, 1.0) }).collect();
            if shrink > 0.0 { erode(&mut a, w, h, shrink.round() as i32, false); } else if shrink < 0.0 { erode(&mut a, w, h, (-shrink).round() as i32, true); }
            if soft > 0.0 { let mut b = Frame { w, h, d: vec![1.0; n * 4] }; for i in 0..n { b.d[i * 4] = a[i]; } box_blur3(&mut b, (soft * 2.0) as usize); for i in 0..n { a[i] = b.d[i * 4]; } }
            for i in 0..n { let o = i * 4; let mut c = [f.d[o], f.d[o + 1], f.d[o + 2]]; let al = a[i];
                // spill suppression: limit key channel to the (average) of the other two
                let lim = (c[o1] + c[o2]) * 0.5; if c[m] > lim { let sp = (c[m] - lim) * dsp; c[m] -= sp; c[o1] += sp * rep * 0.5; c[o2] += sp * rep * 0.5; }
                // edge unmix: C = a*F + (1-a)*K  ->  F
                if unm > 0.0 && al > 0.01 && al < 0.999 { for j in 0..3 { let fgc = ((c[j] - (1.0 - al) * key[j]) / al).clamp(0.0, 1.5); c[j] += (fgc - c[j]) * unm; } }
                match view { 1 => { c = [al, al, al]; f.d[o + 3] = 1.0; } 2 => { f.d[o + 3] = 1.0; } _ => { f.d[o + 3] *= al; } }
                f.d[o] = c[0]; f.d[o + 1] = c[1]; f.d[o + 2] = c[2]; }
        }
        other => return Err(format!("unknown effect '{other}'")),
    }
    Ok(())
}
