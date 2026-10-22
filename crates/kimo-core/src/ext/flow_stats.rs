//! v6 eyes: render_flow numbers (optional `stats`). The flow is render_flow's own dense flow (coarse-to-fine Lucas-Kanade
//! at half resolution, crate::flow::flow, quality 5) between the clip's displayed frame and the next one; the numbers are
//! that field scaled to canvas pixels per frame: mean and median vector, speed percentiles and maximum (with where),
//! the fraction of the field moving faster than moving_px, the mean vector's direction, and a grid x grid table of
//! mean vectors. Image axes: +u right, +v down; direction in degrees, 0 = right, 90 = down. Raw numbers, no verdict.
use super::kit::*;

#[derive(Clone, Copy, Debug)]
pub struct FParams { pub grid: usize, pub moving_px: f64 }
impl Default for FParams { fn default() -> Self { FParams { grid: 4, moving_px: 0.5 } } }
fn r4(x: f64) -> f64 { (x * 1e4).round() / 1e4 }

/// Parse render_flow's `stats` argument: true, or an object with grid (1..16) and moving_px (0..1000).
pub fn params(v: &Value) -> Result<Option<FParams>, String> {
    match v {
        Value::Null | Value::Bool(false) => Ok(None),
        Value::Bool(true) => Ok(Some(FParams::default())),
        Value::Object(o) => {
            let mut q = FParams::default();
            for (k, x) in o {
                match k.as_str() {
                    "grid" => q.grid = x.as_u64().filter(|v| (1..=16).contains(v)).ok_or("out_of_range: stats.grid must be an integer 1..16")? as usize,
                    "moving_px" => q.moving_px = x.as_f64().filter(|v| v.is_finite() && (0.0..=1000.0).contains(v)).ok_or("out_of_range: stats.moving_px must be a number in [0, 1000] (canvas px per frame)")?,
                    _ => return Err(format!("out_of_range: unknown stats.{k}; allowed: grid, moving_px")),
                }
            }
            Ok(Some(q))
        }
        _ => Err("out_of_range: stats must be true or an object {grid, moving_px}".into()),
    }
}

fn pct(sorted: &[f64], p: f64) -> f64 { if sorted.is_empty() { return 0.0; } let i = ((sorted.len() - 1) as f64 * p / 100.0).round() as usize; sorted[i.min(sorted.len() - 1)] }
fn median(mut v: Vec<f64>) -> f64 { v.sort_by(f64::total_cmp); pct(&v, 50.0) }

/// Numbers for one flow field `u`, `v` (fw x fh grid) on a canvas of `w` x `h` pixels.
pub fn stats(u: &[f32], v: &[f32], fw: usize, fh: usize, w: usize, h: usize, p: &FParams) -> Value {
    let n = fw * fh; if n == 0 || u.len() < n || v.len() < n { return json!({"error": "empty flow field"}); }
    let (sx, sy) = (w as f64 / fw as f64, h as f64 / fh as f64);
    let us: Vec<f64> = u[..n].iter().map(|x| *x as f64 * sx).collect(); let vs: Vec<f64> = v[..n].iter().map(|x| *x as f64 * sy).collect();
    let mags: Vec<f64> = us.iter().zip(&vs).map(|(a, b)| (a * a + b * b).sqrt()).collect();
    let (mu, mv) = (us.iter().sum::<f64>() / n as f64, vs.iter().sum::<f64>() / n as f64);
    let (mut imax, mut mmax) = (0usize, -1.0f64); for (i, m) in mags.iter().enumerate() { if *m > mmax { mmax = *m; imax = i; } }
    let moving = mags.iter().filter(|m| **m > p.moving_px).count() as f64 / n as f64;
    let mut sorted = mags.clone(); sorted.sort_by(f64::total_cmp);
    let mean_mag = mags.iter().sum::<f64>() / n as f64;
    let mean_len = (mu * mu + mv * mv).sqrt();
    let dir = if mean_len > 1e-6 { json!(r4(mv.atan2(mu).to_degrees())) } else { Value::Null };
    let g = p.grid; let mut cells = vec![];
    for gy in 0..g { for gx in 0..g {
        let (x0, x1) = (gx * fw / g, ((gx + 1) * fw / g).max(gx * fw / g + 1).min(fw)); let (y0, y1) = (gy * fh / g, ((gy + 1) * fh / g).max(gy * fh / g + 1).min(fh));
        let (mut su, mut sv, mut sm, mut c) = (0.0, 0.0, 0.0, 0usize);
        for y in y0..y1 { for x in x0..x1 { let i = y * fw + x; su += us[i]; sv += vs[i]; sm += mags[i]; c += 1; } }
        let c = c.max(1) as f64; cells.push(json!([r4(su / c), r4(sv / c), r4(sm / c)]));
    } }
    json!({
        "units": "canvas px per frame", "field": [fw, fh],
        "mean_u": r4(mu), "mean_v": r4(mv), "median_u": r4(median(us.clone())), "median_v": r4(median(vs.clone())),
        "mean_dir_deg": dir, "mean_speed": r4(mean_mag),
        "speed_p50": r4(pct(&sorted, 50.0)), "speed_p90": r4(pct(&sorted, 90.0)), "speed_p99": r4(pct(&sorted, 99.0)),
        "speed_max": r4(mmax.max(0.0)), "speed_max_at": [r4(((imax % fw) as f64 + 0.5) * sx), r4(((imax / fw) as f64 + 0.5) * sy)],
        "moving_frac": r4(moving),
        "grid": {"cols": g, "rows": g, "cells": cells, "cell": "[mean_u, mean_v, mean_speed], row-major from the top left"},
        "params": {"grid": g, "moving_px": p.moving_px},
    })
}

pub fn commands() -> Vec<Command> { vec![] }
