//! Linear chronological event matching. Canonical composition is documented in docs/PERCEPTION.md.
//! ```
//! use kimo_core::commands::{dispatch, Session};
//! use serde_json::json;
//! let mut s = Session::new();
//! // audio_transients beat times -> a; detect_cuts cut times -> b; diff_frames verifies pixels.
//! let offsets = dispatch(&mut s, "match_events", &json!({
//!     "a": [{"t": 1.0}, {"t": 2.0}], "b": [{"t": 1.01}, {"t": 2.0}],
//!     "tolerance_s": 0.02
//! })).unwrap();
//! assert_eq!(offsets["stats"]["within_tol_ratio"], 1.0);
//! ```
use super::kit::*;
fn events(a: &Value, k: &str) -> Result<(Vec<(usize, f64)>, bool), String> {
    let v = a
        .get(k)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing array '{k}'"))?;
    if v.len() > 1000000 {
        return Err("out_of_range: max 1000000 events per input".into());
    }
    let mut times = Vec::with_capacity(v.len());
    for (i, v) in v.iter().enumerate() {
        let t = if v.is_number() {
            v.as_f64()
        } else {
            v.get("t").and_then(Value::as_f64)
        }
        .filter(|t| t.is_finite())
        .ok_or_else(|| format!("invalid_event: '{k}' events need finite f64 timestamp or t"))?;
        times.push((i, t));
    }
    let sorted = times.windows(2).all(|w| w[0].1 <= w[1].1);
    if !sorted {
        times.sort_by(|x, y| x.1.total_cmp(&y.1).then(x.0.cmp(&y.0)));
    }
    Ok((times, !sorted))
}
pub fn sweep(
    a: &[(usize, f64)],
    b: &[(usize, f64)],
    tol: f64,
) -> (Vec<(usize, usize, f64)>, Vec<usize>, Vec<usize>) {
    let mut pairs = Vec::with_capacity(a.len().min(b.len()));
    let mut ua = Vec::with_capacity(a.len());
    let mut ub = Vec::with_capacity(b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if (i + j) % 16384 == 0 {
            crate::guard::check();
        }
        let off = b[j].1 - a[i].1;
        if off.abs() <= tol {
            pairs.push((a[i].0, b[j].0, off));
            i += 1;
            j += 1;
        } else if off < -tol {
            ub.push(b[j].0);
            j += 1;
        } else {
            ua.push(a[i].0);
            i += 1;
        }
    }
    ua.extend(a[i..].iter().map(|x| x.0));
    ub.extend(b[j..].iter().map(|x| x.0));
    (pairs, ua, ub)
}
pub fn nearest(
    a: &[(usize, f64)],
    b: &[(usize, f64)],
    _tol: f64,
    cap: f64,
) -> (Vec<(usize, usize, f64)>, Vec<usize>, Vec<usize>) {
    let mut pairs = Vec::with_capacity(a.len());
    let mut ua = Vec::with_capacity(a.len());
    let mut ub = Vec::with_capacity(b.len());
    let mut used = vec![false; b.len()];
    let mut j = 0;
    let mut next = 1;
    while next < b.len() && b[next].1 == b[0].1 {
        next += 1;
    }
    for &(ai, t) in a {
        if pairs.len() % 16384 == 0 {
            crate::guard::check();
        }
        while next < b.len() && (b[next].1 - t).abs() < (b[j].1 - t).abs() {
            j = next;
            next += 1;
            while next < b.len() && b[next].1 == b[j].1 {
                next += 1;
            }
        }
        if b.is_empty() || (b[j].1 - t).abs() > cap {
            ua.push(ai);
        } else {
            pairs.push((ai, b[j].0, b[j].1 - t));
            used[j] = true;
        }
    }
    for (i, &(bi, _)) in b.iter().enumerate() {
        if !used[i] {
            ub.push(bi);
        }
    }
    (pairs, ua, ub)
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (aa, sa) = events(a, "a")?;
    let (bb, sb) = events(a, "b")?;
    let tol = a
        .get("tolerance_s")
        .map(|v| v.as_f64().ok_or("out_of_range: tolerance_s numeric"))
        .transpose()?
        .unwrap_or(1. / ss.project.fps);
    if !tol.is_finite() || tol < 0. {
        return Err("out_of_range: tolerance_s must be finite >=0".into());
    }
    let mode = a.get("mode").and_then(Value::as_str).unwrap_or("nearest");
    if !["nearest", "one_to_one"].contains(&mode) {
        return Err("unknown mode; valid: nearest, one_to_one".into());
    }
    let cap = a
        .get("max_offset_s")
        .map(|v| v.as_f64().ok_or("out_of_range: max_offset_s numeric"))
        .transpose()?
        .unwrap_or(f64::INFINITY);
    if cap.is_nan() || cap < 0. {
        return Err("out_of_range: max_offset_s must be >=0".into());
    }
    let (pairs, ua, ub) = if mode == "nearest" {
        nearest(&aa, &bb, tol, cap)
    } else {
        sweep(&aa, &bb, tol.min(cap))
    };
    let within = pairs.iter().filter(|p| p.2.abs() <= tol).count();
    let n = pairs.len();
    let mut offsets = Vec::with_capacity(n);
    let mut abs = Vec::with_capacity(n);
    let (mut sum, mut max) = (0., 0f64);
    for p in &pairs {
        offsets.push(p.2);
        abs.push(p.2.abs());
        sum += p.2;
        max = max.max(p.2.abs());
    }
    let mean = if n == 0 { 0. } else { sum / n as f64 };
    let jitter = if n == 0 {
        0.
    } else {
        (offsets.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64).sqrt()
    };
    let median = if n == 0 {
        0.
    } else {
        let k = n / 2;
        let (_, mid, hi) = offsets.select_nth_unstable_by(k, f64::total_cmp);
        let m = *mid;
        if n % 2 == 0 {
            let low = offsets[..k]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            (low + m) / 2.
        } else {
            let _ = hi;
            m
        }
    };
    let p90 = if n == 0 {
        0.
    } else {
        let k = ((n as f64 * 0.9).ceil() as usize).saturating_sub(1);
        *abs.select_nth_unstable_by(k, f64::total_cmp).1
    };
    let mut vals = Vec::with_capacity(n);
    for (i, j, off) in pairs {
        vals.push(json!({"a_index":i,"b_index":j,"offset_s":off,"within_tol":off.abs()<=tol}));
    }
    let mut notes = vec![];
    if sa || sb {
        notes.push("Unsorted inputs stable-sorted on a copy; original indices preserved.");
    }
    Ok(
        json!({"pairs":vals,"unmatched_a":ua,"unmatched_b":ub,"stats":{"matched":n,"count_a":aa.len(),"count_b":bb.len(),"mean_offset_s":mean,"median_offset_s":median,"max_abs_offset_s":max,"p90_abs_offset_s":p90,"jitter_s":jitter,"within_tol_ratio":if n==0{0.}else{within as f64/n as f64}},"params_used":{"tolerance_s":tol,"mode":mode,"algorithm":if mode=="nearest"{"linear neighbor walk, many-to-one, tie earlier b"}else{"chronological one-to-one O(N+M) sweep, earliest b within inclusive tolerance, never crossing"},"offset_convention":"b.t - a.t"},"notes":notes}),
    )
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"match_events",mutates:false,description:"Match timestamp vectors or {t,...} arrays. mode nearest (default): linear neighbour walk, many-to-one, equal-distance tie earlier b. mode one_to_one: chronological two-pointer O(N+M) tolerance sweep. Earliest b within inclusive tolerance_s wins; offset=b.t-a.t; out-of-window events are unmatched dropouts on both sides. No crossing pairs. Unsorted inputs stable-sort a copy and note it; finite timestamps required. Default tolerance_s=1/project fps. Preallocated output buffers do not reallocate during sweep. Matched-pair mean/median drift, max error, p90 absolute error and population-standard-deviation jitter; empty stats zero. Advisory only. Cap1000000 events per input.",schema:obj(vec![("a",json!({"type":"array"})),("b",json!({"type":"array"})),("tolerance_s",n("inclusive finite >=0 seconds")),("mode",json!({"type":"string","enum":["nearest","one_to_one"]})),("max_offset_s",n("max pair distance >=0 seconds"))],&["a","b"]),run}]
}
