//! v6 eyes item #3: numbers for get_scopes. Optional arguments only: with none of them get_scopes answers exactly its v5
//! bytes. Luma is the v5 luma (display-referred R'G'B' clamped to 0..1, Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' in f32);
//! Cb = (B' - Y') / 1.8556 and Cr = (R' - Y') / 1.5748 (BT.709), chroma = sqrt(Cb^2 + Cr^2), the vectorscope radius.
//! Percentiles are numpy's default (linear interpolation between order statistics). Raw numbers, no verdict.
use crate::commands::Command;
use serde_json::{json, Map, Value};

pub const ARGS: [&str; 5] = ["percentiles", "waveform_columns", "zones", "crush_below", "clip_above"];
pub const BASIS: &str = "Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' on display-referred R'G'B' clamped to 0..1 (the v5 luma); Cb = (B'-Y')/1.8556, Cr = (R'-Y')/1.5748 (BT.709); chroma = sqrt(Cb^2+Cr^2); percentiles by linear interpolation between order statistics (numpy default); zones split width and height as floor(i*w/cols); crushed = Y' < crush_below, clipped = Y' > clip_above";

#[derive(Debug, Clone)]
pub struct Opts { pub pcts: Option<Vec<f64>>, pub cols: Option<usize>, pub zones: Option<(usize, usize)>, pub crush_below: f64, pub clip_above: f64 }

fn given<'a>(a: &'a Value, k: &str) -> Option<&'a Value> { a.get(k).filter(|v| !v.is_null()) }
fn int_in(v: &Value, k: &str, lo: u64, hi: u64) -> Result<usize, String> {
    v.as_f64().filter(|x| x.is_finite() && x.fract() == 0.0 && *x >= lo as f64 && *x <= hi as f64).map(|x| x as usize).ok_or(format!("out_of_range: {k} must be a whole number {lo}..{hi}"))
}
/// None when no numeric argument is given (the v5 path). Arguments are checked before anything renders; never clamped.
pub fn parse(a: &Value) -> Result<Option<Opts>, String> {
    if !ARGS.iter().any(|k| given(a, k).is_some()) { return Ok(None); }
    let pcts = match given(a, "percentiles") { None => None, Some(v) => {
        let arr = v.as_array().ok_or("out_of_range: percentiles must be an array of 1..32 numbers in 0..100, strictly ascending")?;
        if arr.is_empty() || arr.len() > 32 { return Err(format!("out_of_range: percentiles must hold 1..32 numbers (got {})", arr.len())); }
        let mut out = Vec::with_capacity(arr.len());
        for (i, x) in arr.iter().enumerate() {
            let p = x.as_f64().filter(|p| p.is_finite() && (0.0..=100.0).contains(p)).ok_or(format!("out_of_range: percentiles[{i}] must be a number in 0..100"))?;
            if let Some(&prev) = out.last() { if p <= prev { return Err(format!("out_of_range: percentiles must be strictly ascending (percentiles[{i}] = {p} after {prev})")); } }
            out.push(p);
        }
        Some(out) } };
    let cols = match given(a, "waveform_columns") { None => None, Some(v) => { if pcts.is_none() { return Err("out_of_range: waveform_columns needs percentiles".into()); } Some(int_in(v, "waveform_columns", 1, 256)?) } };
    let zones = match given(a, "zones") { None => None, Some(v) => {
        let z = v.as_array().filter(|z| z.len() == 2).ok_or("out_of_range: zones must be [cols, rows], whole numbers 1..32")?;
        Some((int_in(&z[0], "zones[0] (cols)", 1, 32)?, int_in(&z[1], "zones[1] (rows)", 1, 32)?)) } };
    let thr = |k: &str, d: f64| -> Result<f64, String> { match given(a, k) { None => Ok(d), Some(v) => { if zones.is_none() { return Err(format!("out_of_range: {k} applies to zones; give zones [cols, rows]")); } v.as_f64().filter(|x| x.is_finite() && (0.0..=1.0).contains(x)).ok_or(format!("out_of_range: {k} must be a number in 0..1")) } } };
    let (crush_below, clip_above) = (thr("crush_below", 0.02)?, thr("clip_above", 0.98)?);
    Ok(Some(Opts { pcts, cols, zones, crush_below, clip_above }))
}

/// numpy.percentile(method="linear") on an ascending slice, including numpy's lerp form (b - d*(1-t) when t >= 0.5).
pub fn percentile_sorted(s: &[f64], p: f64) -> f64 {
    let n = s.len(); if n == 1 { return s[0]; }
    let idx = p / 100.0 * (n - 1) as f64; let lo = (idx.floor() as usize).min(n - 1); let hi = (lo + 1).min(n - 1); let t = idx - lo as f64;
    let (a, b) = (s[lo], s[hi]); let d = b - a; if t >= 0.5 { b - d * (1.0 - t) } else { a + d * t }
}
fn pcts_of(mut v: Vec<f64>, ps: &[f64]) -> Vec<f64> { v.sort_by(|a, b| a.total_cmp(b)); ps.iter().map(|&p| percentile_sorted(&v, p)).collect() }
/// The v5 luma of one RGBA pixel (same f32 order of operations as get_scopes).
pub fn luma(px: &[f32]) -> f32 { let mut l = 0f32; for j in 0..3 { l += px[j].clamp(0.0, 1.0) * [0.2126f32, 0.7152, 0.0722][j]; } l }

/// Adds the numeric keys to get_scopes' object. `d` is RGBA f32, w x h.
pub fn extend(o: &Opts, d: &[f32], w: usize, h: usize, out: &mut Value) -> Result<(), String> {
    let n = w * h; if n == 0 { return Err("out_of_range: the frame is empty".into()); }
    if let (Some(_), Some(c)) = (&o.pcts, o.cols) { if c > w { return Err(format!("out_of_range: waveform_columns {c} exceeds the frame width {w}")); } }
    // given columns are never clamped; the default is 16, or the frame width when the frame is narrower (echoed as waveform_columns)
    let cols = o.cols.unwrap_or(16.min(w));
    if let Some((c, r)) = o.zones { if c > w || r > h { return Err(format!("out_of_range: zones [{c}, {r}] exceed the frame {w}x{h}")); } }
    let lum: Vec<f32> = (0..n).map(|i| luma(&d[i * 4..i * 4 + 4])).collect();
    let m: &mut Map<String, Value> = out.as_object_mut().ok_or("internal: scopes result is not an object")?;
    if let Some(ps) = &o.pcts {
        let (mut cbs, mut crs, mut chs) = (0f64, 0f64, 0f64); let mut chroma = Vec::with_capacity(n);
        for i in 0..n { let y = lum[i] as f64; let r = d[i * 4].clamp(0.0, 1.0) as f64; let b = d[i * 4 + 2].clamp(0.0, 1.0) as f64;
            let cb = (b - y) / 1.8556; let cr = (r - y) / 1.5748; let c = (cb * cb + cr * cr).sqrt(); cbs += cb; crs += cr; chs += c; chroma.push(c); }
        m.insert("percentiles".into(), json!(ps));
        m.insert("luma_percentiles".into(), json!(pcts_of(lum.iter().map(|&x| x as f64).collect(), ps)));
        m.insert("chroma_percentiles".into(), json!(pcts_of(chroma, ps)));
        m.insert("cb_mean".into(), json!(cbs / n as f64)); m.insert("cr_mean".into(), json!(crs / n as f64)); m.insert("chroma_mean".into(), json!(chs / n as f64));
        let wf: Vec<Vec<f64>> = (0..cols).map(|cx| { let (x0, x1) = (cx * w / cols, (cx + 1) * w / cols); let mut v = Vec::with_capacity((x1 - x0) * h);
            for y in 0..h { for x in x0..x1 { v.push(lum[y * w + x] as f64); } } pcts_of(v, ps) }).collect();
        m.insert("waveform_columns".into(), json!(cols)); m.insert("waveform_percentiles".into(), json!(wf));
    }
    if let Some((c, r)) = o.zones {
        let (lo, hi) = (o.crush_below as f32, o.clip_above as f32); let (mut zc, mut zw) = (vec![], vec![]);
        for zy in 0..r { for zx in 0..c { let (x0, x1, y0, y1) = (zx * w / c, (zx + 1) * w / c, zy * h / r, (zy + 1) * h / r); let (mut a, mut b) = (0u64, 0u64);
            for y in y0..y1 { for x in x0..x1 { let l = lum[y * w + x]; if l < lo { a += 1; } if l > hi { b += 1; } } }
            let area = ((x1 - x0) * (y1 - y0)) as f64; zc.push(a as f64 * 100.0 / area); zw.push(b as f64 * 100.0 / area); } }
        let mx = |v: &[f64]| v.iter().cloned().fold(0f64, f64::max);
        m.insert("zone_grid".into(), json!([c, r])); m.insert("crush_below".into(), json!(o.crush_below)); m.insert("clip_above".into(), json!(o.clip_above));
        m.insert("zone_crushed_blacks_max_pct".into(), json!(mx(&zc))); m.insert("zone_clipped_whites_max_pct".into(), json!(mx(&zw)));
        m.insert("zone_crushed_blacks_pct".into(), json!(zc)); m.insert("zone_clipped_whites_pct".into(), json!(zw));
    }
    m.insert("basis".into(), json!(BASIS));
    Ok(())
}
pub fn commands() -> Vec<Command> { vec![] }
