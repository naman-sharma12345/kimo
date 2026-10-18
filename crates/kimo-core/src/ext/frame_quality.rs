//! v6 eyes: frame quality numbers for lint_frame (optional `quality`). Per sampled frame, on Rec.709 luma of the output
//! values (0..1): mean luma, the fraction of pixels at or under crush_below (crushed) and at or over clip_above (clipped),
//! the fraction with any RGB channel at or over clip_above, the distinct 8-bit luma codes, banding numbers on 8-bit luma
//! (horizontal neighbour steps of 1..band_step codes per row, and the mean length of the flat runs those steps bound) and
//! the change from the frame one project frame earlier (mean |dY|, max |d| over RGB, identical). Raw numbers, no verdict.
use super::kit::*;
use crate::perception::analysis::Image;

#[derive(Clone, Copy, Debug)]
pub struct QParams { pub crush_below: f64, pub clip_above: f64, pub band_step: u32 }
impl Default for QParams { fn default() -> Self { QParams { crush_below: 0.02, clip_above: 0.98, band_step: 2 } } }
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }
fn luma(im: &Image, i: usize) -> f64 { 0.2126 * im.rgb[0][i] + 0.7152 * im.rgb[1][i] + 0.0722 * im.rgb[2][i] }

/// Parse lint_frame's `quality` argument: true, or an object with crush_below / clip_above (0..1) and band_step (1..32).
pub fn params(v: &Value) -> Result<Option<QParams>, String> {
    match v {
        Value::Null | Value::Bool(false) => Ok(None),
        Value::Bool(true) => Ok(Some(QParams::default())),
        Value::Object(o) => {
            let mut q = QParams::default();
            for (k, x) in o {
                match k.as_str() {
                    "crush_below" => q.crush_below = x.as_f64().filter(|v| v.is_finite() && (0.0..=1.0).contains(v)).ok_or("out_of_range: quality.crush_below must be a number in [0, 1]")?,
                    "clip_above" => q.clip_above = x.as_f64().filter(|v| v.is_finite() && (0.0..=1.0).contains(v)).ok_or("out_of_range: quality.clip_above must be a number in [0, 1]")?,
                    "band_step" => q.band_step = x.as_u64().filter(|v| (1..=32).contains(v)).ok_or("out_of_range: quality.band_step must be an integer 1..32 (8-bit luma codes)")? as u32,
                    _ => return Err(format!("out_of_range: unknown quality.{k}; allowed: crush_below, clip_above, band_step")),
                }
            }
            if q.crush_below >= q.clip_above { return Err("out_of_range: quality.crush_below must be below quality.clip_above".into()); }
            Ok(Some(q))
        }
        _ => Err("out_of_range: quality must be true or an object {crush_below, clip_above, band_step}".into()),
    }
}

/// The numbers for one frame; `prev` is the frame one project frame earlier (None at t = 0).
pub fn quality(im: &Image, prev: Option<&Image>, q: &QParams) -> Value {
    let n = im.w * im.h; let nf = n.max(1) as f64;
    let (mut sum, mut crushed, mut clipped, mut chclip) = (0.0, 0usize, 0usize, 0usize);
    let mut codes = vec![0u8; n]; let mut seen = [false; 256];
    for i in 0..n {
        let y = luma(im, i); sum += y;
        if y <= q.crush_below { crushed += 1; } if y >= q.clip_above { clipped += 1; }
        if im.rgb[0][i] >= q.clip_above || im.rgb[1][i] >= q.clip_above || im.rgb[2][i] >= q.clip_above { chclip += 1; }
        let c = (y.clamp(0.0, 1.0) * 255.0).round() as u8; codes[i] = c; seen[c as usize] = true;
    }
    // banding on 8-bit luma, row by row: small steps (1..=band_step codes) and the flat runs between two small steps
    let (mut steps, mut runs, mut run_px) = (0usize, 0usize, 0usize);
    for y in 0..im.h {
        crate::guard::check();
        let row = &codes[y * im.w..(y + 1) * im.w];
        let mut run_start = 0usize; let mut left_small = false;
        for x in 1..im.w {
            let d = (row[x] as i32 - row[x - 1] as i32).unsigned_abs();
            if d == 0 { continue; }
            let small = d <= q.band_step;
            if small { steps += 1; }
            if small && left_small { runs += 1; run_px += x - run_start; }
            left_small = small; run_start = x;
        }
    }
    let (pd, pmax, ident) = match prev {
        Some(p) if p.w == im.w && p.h == im.h => {
            let (mut s, mut m) = (0.0f64, 0.0f64);
            for i in 0..n { s += (luma(im, i) - luma(p, i)).abs(); for ch in 0..3 { m = m.max((im.rgb[ch][i] - p.rgb[ch][i]).abs()); } }
            (json!(r6(s / nf)), json!(r6(m)), json!(m == 0.0))
        }
        _ => (Value::Null, Value::Null, Value::Null),
    };
    json!({"mean_luma": r6(sum / nf), "crushed_frac": r6(crushed as f64 / nf), "clipped_frac": r6(clipped as f64 / nf), "channel_clipped_frac": r6(chclip as f64 / nf),
        "luma_levels": seen.iter().filter(|s| **s).count(), "band_steps_per_row": r6(steps as f64 / im.h.max(1) as f64),
        "band_plateau_px": if runs > 0 { json!(r6(run_px as f64 / runs as f64)) } else { Value::Null }, "band_plateaus": runs,
        "prev_mean_delta": pd, "prev_max_delta": pmax, "prev_identical": ident})
}

pub fn commands() -> Vec<Command> { vec![] }
