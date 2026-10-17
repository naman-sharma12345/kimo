//! v6 "eyes" item #1: full-reference picture metrics on luma, our own code from the published formulas, plus the
//! compare_ranges tool (A/B over time). Raw numbers only: no verdict, every threshold is the caller's.
//! - Luma: Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' (BT.709 coefficients) on the display-referred R'G'B' values kimo renders
//!   (u8/255), times 255, not re-quantised. Data range L = 255.
//! - PSNR = 10 log10(L^2 / MSE) on Y'. MSE exactly 0 => psnr_db null + identical true (JSON has no infinity);
//!   psnr_cap_db returns min(psnr, cap), and the cap when identical.
//! - SSIM: Wang, Bovik, Sheikh, Simoncelli 2004 (IEEE TIP 13(4)): 11x11 circular-symmetric Gaussian, sigma 1.5, normalised to
//!   sum 1; K1 = 0.01, K2 = 0.03, C1 = (K1 L)^2, C2 = (K2 L)^2; local statistics on 'valid' windows only (no padding, the
//!   map is (w-10) x (h-10)); the score is the mean of the SSIM map. No automatic downsampling.
//! - MS-SSIM: Wang, Simoncelli, Bovik 2003 (Asilomar): M = 5 scales, exponents 0.0448 0.2856 0.3001 0.2363 0.1333; at each
//!   scale the mean contrast-structure term cs_j = mean((2 s_xy + C2)/(s_x^2 + s_y^2 + C2)), the luminance term l only at
//!   the coarsest scale; between scales a 2x2 mean then keep every second sample (an odd last row/column is dropped).
//!   MS-SSIM = l_M^w_M * prod_j cs_j^w_j. A negative cs (or l) has no real fractional power: ms_ssim is null with a note,
//!   never NaN and never clamped.
use super::kit::*;
use crate::perception::analysis::Image;

pub const QUALITY: &[&str] = &["psnr", "ssim", "ms_ssim"];
pub const MS_WEIGHTS: [f64; 5] = [0.0448, 0.2856, 0.3001, 0.2363, 0.1333];
pub const L: f64 = 255.0;
pub const WIN: usize = 11;
/// Smallest short side MS-SSIM accepts: an 11-pixel window must fit at the 5th scale (11 * 2^4).
pub const MS_MIN: usize = 176;
/// Most sample pairs compare_ranges renders without max_points.
pub const RANGE_CAP: usize = 2000;
pub const LUMA_TEXT: &str = "Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' (BT.709) on display-referred R'G'B' u8/255, times 255, not re-quantised";

/// A plain luma plane, row-major.
pub struct Plane { pub w: usize, pub h: usize, pub v: Vec<f64> }
pub fn luma_plane(im: &Image) -> Plane { Plane { w: im.w, h: im.h, v: (0..im.w * im.h).map(|i| (0.2126 * im.rgb[0][i] + 0.7152 * im.rgb[1][i] + 0.0722 * im.rgb[2][i]) * L).collect() } }
pub fn gauss_kernel() -> [f64; WIN] { let mut k = [0.; WIN]; let mut s = 0.; for (i, x) in k.iter_mut().enumerate() { let d = i as f64 - 5.; *x = (-d * d / (2. * 1.5 * 1.5)).exp(); s += *x; } for x in &mut k { *x /= s; } k }
/// Separable 11x11 Gaussian filter, valid region only: out is (w-10) x (h-10). The 2D kernel g(x)g(y) is the normalised
/// circular-symmetric Gaussian of the paper (separable, so the result is the same as the 2D sum up to rounding).
fn filt(v: &[f64], w: usize, h: usize, k: &[f64; WIN]) -> Vec<f64> {
    let (ow, oh) = (w - (WIN - 1), h - (WIN - 1));
    let mut tmp = vec![0.; ow * h];
    for y in 0..h { if y % 64 == 0 { crate::guard::check(); } let r = &v[y * w..(y + 1) * w]; for x in 0..ow { let mut s = 0.; for j in 0..WIN { s += k[j] * r[x + j]; } tmp[y * ow + x] = s; } }
    let mut out = vec![0.; ow * oh];
    for y in 0..oh { for x in 0..ow { let mut s = 0.; for j in 0..WIN { s += k[j] * tmp[(y + j) * ow + x]; } out[y * ow + x] = s; } }
    out
}
/// (mean SSIM map, mean cs map, mean l map) over the valid windows. Needs w, h >= 11.
pub fn ssim_terms(a: &Plane, b: &Plane) -> (f64, f64, f64) {
    let (w, h) = (a.w, a.h); let k = gauss_kernel();
    let (c1, c2) = ((0.01 * L).powi(2), (0.03 * L).powi(2));
    let ma = filt(&a.v, w, h, &k); let mb = filt(&b.v, w, h, &k);
    let sq = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(x, y)| x * y).collect::<Vec<f64>>();
    let aa = filt(&sq(&a.v, &a.v), w, h, &k); let bb = filt(&sq(&b.v, &b.v), w, h, &k); let ab = filt(&sq(&a.v, &b.v), w, h, &k);
    let n = ma.len() as f64; let (mut ss, mut cs, mut ll) = (0., 0., 0.);
    for i in 0..ma.len() {
        let (mx, my) = (ma[i], mb[i]); let (vx, vy, cxy) = (aa[i] - mx * mx, bb[i] - my * my, ab[i] - mx * my);
        let l = (2. * mx * my + c1) / (mx * mx + my * my + c1); let c = (2. * cxy + c2) / (vx + vy + c2);
        ss += l * c; cs += c; ll += l;
    }
    (ss / n, cs / n, ll / n)
}
pub fn down2(p: &Plane) -> Plane { let (w, h) = (p.w / 2, p.h / 2); let mut v = vec![0.; w * h]; for y in 0..h { for x in 0..w { let i = 2 * y * p.w + 2 * x; v[y * w + x] = (p.v[i] + p.v[i + 1] + p.v[i + p.w] + p.v[i + p.w + 1]) / 4.; } } Plane { w, h, v } }
/// MS-SSIM: (score or None when a term is negative, per-scale rows). Needs min(w, h) >= 176.
pub fn ms_ssim(a: &Plane, b: &Plane) -> (Option<f64>, Vec<Value>) {
    let (mut pa, mut pb) = (Plane { w: a.w, h: a.h, v: a.v.clone() }, Plane { w: b.w, h: b.h, v: b.v.clone() });
    let (mut rows, mut score, mut ok) = (vec![], 1f64, true);
    for (j, wj) in MS_WEIGHTS.iter().enumerate() {
        let (_, cs, l) = ssim_terms(&pa, &pb); let last = j == MS_WEIGHTS.len() - 1;
        let mut r = json!({"scale": j + 1, "w": pa.w, "h": pa.h, "weight": wj, "cs": cs}); if last { r["l"] = json!(l); }
        rows.push(r);
        let t = if last { cs * l } else { cs };
        // l_M^w * cs_M^w == (l_M cs_M)^w for non-negative terms; a negative term or product leaves the score undefined
        if cs < 0. || (last && l < 0.) { ok = false; } else { score *= t.powf(*wj); }
        if !last { pa = down2(&pa); pb = down2(&pb); }
    }
    (if ok && score.is_finite() { Some(score) } else { None }, rows)
}
pub struct Opts { pub psnr: bool, pub ssim: bool, pub ms: bool, pub cap: Option<f64> }
/// Read `quality` (+ `psnr_cap_db`); Ok(None) when quality is absent (psnr_cap_db alone is an error).
pub fn parse_opts(a: &Value, default_all: bool) -> Result<Option<Opts>, String> {
    let names: Vec<String> = match a.get("quality") {
        None if default_all => QUALITY.iter().map(|s| s.to_string()).collect(),
        None => { if a.get("psnr_cap_db").is_some() { return Err("out_of_range: psnr_cap_db needs quality with \"psnr\"".into()); } return Ok(None) }
        Some(v) => { let arr = v.as_array().ok_or("invalid_metric: quality must be an array of psnr, ssim, ms_ssim")?; if arr.is_empty() { return Err("invalid_metric: quality must name at least one of psnr, ssim, ms_ssim".into()); }
            let mut out: Vec<String> = vec![]; for x in arr { let s = x.as_str().ok_or("invalid_metric: quality entries must be strings")?; if !QUALITY.contains(&s) { return Err(format!("invalid_metric: unknown quality metric \"{s}\"; Valid: psnr, ssim, ms_ssim")); } if out.iter().any(|o| o == s) { return Err(format!("invalid_metric: quality lists \"{s}\" twice")); } out.push(s.into()); } out }
    };
    let has = |k: &str| names.iter().any(|n| n == k);
    let cap = match a.get("psnr_cap_db") { None => None, Some(v) => { let c = v.as_f64().filter(|c| c.is_finite() && *c > 0. && *c <= 1000.).ok_or("out_of_range: psnr_cap_db must be a number > 0 and <= 1000")?; if !has("psnr") { return Err("out_of_range: psnr_cap_db needs quality with \"psnr\"".into()); } Some(c) } };
    Ok(Some(Opts { psnr: has("psnr"), ssim: has("ssim"), ms: has("ms_ssim"), cap }))
}
/// Size checks before any work: SSIM needs an 11x11 window, MS-SSIM 176 px on the short side.
pub fn check_size(o: &Opts, w: usize, h: usize) -> Result<(), String> {
    if o.ms && w.min(h) < MS_MIN { return Err(format!("out_of_range: ms_ssim needs at least {MS_MIN} px on the short side of the compared image (got {w}x{h}); pass a larger scale (1 = full resolution) or a larger region")); }
    if o.ssim && w.min(h) < WIN { return Err(format!("out_of_range: ssim needs at least {WIN} px on each side of the compared image (got {w}x{h})")); }
    Ok(())
}
pub struct Scores { pub mse: f64, pub identical: bool, pub psnr: Option<f64>, pub ssim: Option<f64>, pub ms: Option<Option<f64>>, pub ms_rows: Vec<Value> }
pub fn psnr_of(mse: f64, cap: Option<f64>) -> Option<f64> { if mse == 0. { cap } else { let p = 10. * (L * L / mse).log10(); Some(cap.map_or(p, |c| p.min(c))) } }
pub fn score(o: &Opts, a: &Image, b: &Image) -> Scores {
    let (pa, pb) = (luma_plane(a), luma_plane(b));
    let mut mse = 0.; for i in 0..pa.v.len() { if i % 65536 == 0 { crate::guard::check(); } let d = pa.v[i] - pb.v[i]; mse += d * d; } mse /= pa.v.len() as f64;
    let ssim = if o.ssim { Some(ssim_terms(&pa, &pb).0) } else { None };
    let (ms, ms_rows) = if o.ms { let (s, r) = ms_ssim(&pa, &pb); (Some(s), r) } else { (None, vec![]) };
    Scores { mse, identical: mse == 0., psnr: psnr_of(mse, o.cap), ssim, ms, ms_rows }
}
pub fn method(o: &Opts) -> Value {
    let mut m = serde_json::Map::new();
    if o.psnr { m.insert("psnr".into(), json!(format!("10 log10(255^2 / MSE) on Y'; MSE 0 gives null + identical{}", if o.cap.is_some() { "; psnr_cap_db returns min(psnr, cap) and the cap when identical" } else { "" }))); }
    if o.ssim { m.insert("ssim".into(), json!("Wang et al. 2004: 11x11 Gaussian sigma 1.5 (normalised), K1 0.01, K2 0.03, L 255, valid windows only (map (w-10)x(h-10)), mean of the map, no automatic downsampling")); }
    if o.ms { m.insert("ms_ssim".into(), json!("Wang et al. 2003: 5 scales, weights 0.0448 0.2856 0.3001 0.2363 0.1333, cs at every scale, l at the coarsest only, 2x2 mean downsample between scales (odd last row/column dropped); a negative term gives null")); }
    Value::Object(m)
}
/// The diff_frames `quality` object.
pub fn quality_object(o: &Opts, a: &Image, b: &Image) -> Value {
    let s = score(o, a, b);
    let mut q = json!({"luma": LUMA_TEXT, "data_range": L, "w": a.w, "h": a.h, "identical": s.identical});
    if o.psnr { q["mse"] = json!(s.mse); q["psnr_db"] = json!(s.psnr); if let Some(c) = o.cap { q["psnr_cap_db"] = json!(c); } }
    if o.ssim { q["ssim"] = json!(s.ssim); }
    if let Some(ms) = s.ms { q["ms_ssim"] = json!(ms); q["ms_ssim_scales"] = json!(s.ms_rows); if ms.is_none() { q["ms_ssim_note"] = json!("a contrast-structure (or coarsest luminance) term is negative, so the weighted product has no real value"); } }
    q["method"] = method(o);
    q
}
/// 5th percentile and friends of the non-null values: p5 by linear interpolation between order statistics (numpy default).
pub fn summary(t: &[f64], v: &[Option<f64>]) -> Value {
    let mut xs: Vec<(f64, f64)> = t.iter().zip(v).filter_map(|(t, v)| v.map(|v| (*t, v))).collect();
    if xs.is_empty() { return json!({"n": 0, "mean": null, "min": null, "p5": null, "max": null, "min_at_s": null}); }
    let n = xs.len(); let mean = xs.iter().map(|x| x.1).sum::<f64>() / n as f64;
    let (mut min_at, mut min) = (xs[0].0, xs[0].1); for x in &xs { if x.1 < min { min = x.1; min_at = x.0; } }
    xs.sort_by(|a, b| a.1.total_cmp(&b.1)); let max = xs[n - 1].1;
    let pos = 0.05 * (n - 1) as f64; let lo = pos.floor() as usize; let hi = (lo + 1).min(n - 1); let p5 = xs[lo].1 + (pos - lo as f64) * (xs[hi].1 - xs[lo].1);
    json!({"n": n, "mean": mean, "min": min, "p5": p5, "max": max, "min_at_s": min_at})
}

fn side_obj<'a>(a: &'a Value, k: &str) -> Result<&'a Value, String> { let v = a.get(k).ok_or_else(|| format!("missing object '{k}'"))?; if !v.is_object() { return Err(format!("out_of_range: {k} must be an object {{source, start}}")); } Ok(v) }
fn start_of(v: &Value, k: &str) -> Result<f64, String> { match v.get("start") { None => Ok(0.), Some(x) => x.as_f64().filter(|t| t.is_finite() && *t >= 0.).ok_or_else(|| format!("out_of_range: {k}.start must be a number >= 0 seconds")) } }
fn run_ranges(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (av, bv) = (side_obj(a, "a")?, side_obj(a, "b")?);
    for (k, v) in [("a", av), ("b", bv)] { for key in v.as_object().unwrap().keys() { if key != "source" && key != "start" { return Err(format!("out_of_range: {k} takes source and start only (got {key}); duration and hop are shared")); } } }
    let (sa, sb) = (start_of(av, "a")?, start_of(bv, "b")?);
    let dur = a.get("duration").ok_or("missing number 'duration' (seconds > 0)")?.as_f64().filter(|d| d.is_finite() && *d > 0. && *d <= 86400.).ok_or("out_of_range: duration must be a number > 0 and <= 86400 seconds")?;
    let fps = ss.project.fps;
    if a.get("hop_s").is_some() && a.get("step_frames").is_some() { return Err("out_of_range: choose hop_s or step_frames, not both".into()); }
    // the sampling unit: 1 ms for hop_s, one project frame for step_frames; the hop is a whole number of units
    let (unit, req, by_frames) = if let Some(v) = a.get("step_frames") {
        let n = v.as_u64().filter(|n| (1..=1_000_000).contains(n)).ok_or("out_of_range: step_frames must be an integer 1..1000000")?; (1. / fps, n as usize, true)
    } else {
        let h = match a.get("hop_s") { None => 1.0, Some(v) => v.as_f64().filter(|h| h.is_finite() && (0.001..=3600.).contains(h)).ok_or("out_of_range: hop_s must be a number 0.001..3600 seconds")? };
        let ms = (h * 1000.).round(); if (h * 1000. - ms).abs() > 1e-6 { return Err("out_of_range: hop_s must be a whole number of milliseconds".into()); } (0.001, ms as usize, false)
    };
    let max_points = match a.get("max_points") { None => None, Some(v) => Some(v.as_u64().filter(|n| (2..=RANGE_CAP as u64).contains(n)).ok_or(format!("out_of_range: max_points must be an integer 2..{RANGE_CAP}"))? as usize) };
    let span = (dur / unit - 1e-9).ceil().max(1.) as usize; // units in [0, duration)
    // max_points raises the hop (in whole units) so the curve holds at most max_points samples, the way measure_loudness's curve does
    let hop = match max_points { Some(m) => req.max(span.div_ceil(m)), None => req };
    let count = span.div_ceil(hop);
    if count > RANGE_CAP { return Err(format!("out_of_range: {count} sample pairs (cap {RANGE_CAP}); raise hop_s / step_frames or pass max_points")); }
    let o = parse_opts(a, true)?.unwrap();
    // the whole range [start, start + duration) is checked up front for timeline and clip sources, so no partial curve is ever
    // returned (file and snapshot sources are checked by their own render, before any output)
    for (k, v, st) in [("a", av, sa), ("b", bv, sb)] {
        let src = v.get("source").and_then(|x| x.as_str()).unwrap_or("composite"); let end_k = st + dur;
        if src == "composite" { let end = ss.project.duration(); if end_k > end + 1e-9 { return Err(format!("out_of_range: {k} covers {st:.3}..{end_k:.3} s but the timeline ends at {end:.3} s; shorten duration or move {k}.start")); } }
        if let Some(id) = src.strip_prefix("clip:") { let c = ss.project.find_clip(id).ok_or_else(|| format!("invalid_source: {k} clip not found"))?; if st < c.start - 1e-9 || end_k > c.start + c.duration + 1e-9 { return Err(format!("out_of_range: {k} covers {st:.3}..{end_k:.3} s; it must lie inside the clip's timeline span {:.3}..{:.3} s", c.start, c.start + c.duration)); } }
    }
    let scale = match a.get("scale") { None => 480., Some(v) => v.as_f64().filter(|s| s.is_finite() && *s > 0. && *s <= 8192.).ok_or("out_of_range: scale must be >0 and <=8192 (1 = full resolution)")? };
    let region = match a.get("region") { None => None, Some(v) => { let r = v.as_array().filter(|v| v.len() == 4).ok_or("out_of_range: region must be [x,y,w,h] pixels")?.iter().map(|x| x.as_u64().map(|x| x as usize).ok_or("out_of_range: region integer pixels required")).collect::<Result<Vec<_>, _>>()?; Some([r[0], r[1], r[2], r[3]]) } };
    let (mut ts, mut ps, mut s1, mut s3) = (vec![], vec![], vec![], vec![]); let mut identical = 0usize; let mut dims = (0, 0, 0, 0, 1.0, [0usize; 4]);
    for k in 0..count {
        let t = (k * hop) as f64 * unit; let t = (t * 1e9).round() / 1e9;
        let side = |v: &Value, st: f64| { let mut s = json!({"time": st + t}); if let Some(src) = v.get("source") { s["source"] = src.clone(); } s };
        let mut call = json!({"a": side(av, sa), "b": side(bv, sb)}); if let Some(r) = region { call["region"] = json!(r); }
        let p = super::diff_frames::pair(ss, &call, scale)?;
        if k == 0 { check_size(&o, p.a.w, p.a.h)?; dims = (p.source_w, p.source_h, p.a.w, p.a.h, p.ratio, p.region); }
        let s = score(&o, &p.a, &p.b);
        if s.identical { identical += 1; }
        ts.push(t); ps.push(s.psnr); s1.push(s.ssim); s3.push(s.ms.flatten());
    }
    let mut out = json!({"a": {"source": av.get("source").cloned().unwrap_or(json!("composite")), "start": sa}, "b": {"source": bv.get("source").cloned().unwrap_or(json!("composite")), "start": sb},
        "duration": dur, "points": count, "max_points": max_points, "luma": LUMA_TEXT, "data_range": L,
        "compared": {"source_w": dims.0, "source_h": dims.1, "w": dims.2, "h": dims.3, "scale": dims.4, "region": dims.5},
        "t": "sample k compares a.start + t_s[k] with b.start + t_s[k] (seconds); samples cover [0, duration)", "t_s": ts, "identical_count": identical});
    if by_frames { out["requested_step_frames"] = json!(req); out["step_frames"] = json!(hop); out["hop_s"] = json!(hop as f64 / fps); }
    else { out["requested_hop_s"] = json!(req as f64 / 1000.); out["hop_s"] = json!(hop as f64 / 1000.); }
    let mut sm = serde_json::Map::new();
    if o.psnr { out["psnr_db"] = json!(ps); sm.insert("psnr_db".into(), summary(&ts, &ps)); if let Some(c) = o.cap { out["psnr_cap_db"] = json!(c); } }
    if o.ssim { out["ssim"] = json!(s1); sm.insert("ssim".into(), summary(&ts, &s1)); }
    if o.ms { out["ms_ssim"] = json!(s3); sm.insert("ms_ssim".into(), summary(&ts, &s3)); }
    out["summary"] = Value::Object(sm);
    out["method"] = method(&o);
    if let Some(m) = out["method"].as_object_mut() { m.insert("summary".into(), json!("over non-null samples: arithmetic mean (of dB values for PSNR), min with its time, p5 by linear interpolation between order statistics, max; identical samples are null for PSNR unless psnr_cap_db is given")); }
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![Command { name: "compare_ranges", mutates: false,
    description: "A/B picture quality over time: PSNR, SSIM and MS-SSIM on BT.709 luma between two sources or time ranges, as per-sample curves plus mean/min/p5. a/b = {source: composite|clip:<id>|file:<path>|snapshot:<rev>, start seconds}; duration is shared; samples every hop_s (default 1, whole ms) or step_frames, over [0, duration). max_points (2..2000) raises the hop so the curve holds at most that many samples and both requested and used hop are returned; more than 2000 samples without it is an error. quality picks metrics (default all three); scale (default 480 max side, 1 = full res) and region crop as diff_frames. PSNR on identical frames is null with identical counted, or psnr_cap_db. SSIM per Wang 2004 (11x11 Gaussian sigma 1.5, K1 .01, K2 .03), MS-SSIM per Wang 2003 (5 scales). Raw numbers only, no verdict; bad arguments are errors, never clamped.",
    schema: obj(vec![("a", json!({"type":"object","properties":{"source":{"type":"string"},"start":{"type":"number","minimum":0}}})), ("b", json!({"type":"object","properties":{"source":{"type":"string"},"start":{"type":"number","minimum":0}}})),
        ("duration", json!({"type":"number","exclusiveMinimum":0,"maximum":86400,"description":"seconds compared on both sides"})),
        ("hop_s", json!({"type":"number","minimum":0.001,"maximum":3600,"description":"sample spacing seconds, whole ms, default 1"})), ("step_frames", json!({"type":"integer","minimum":1,"maximum":1000000,"description":"sample every N project frames instead of hop_s"})),
        ("max_points", json!({"type":"integer","minimum":2,"maximum":2000,"description":"cap samples; raises the hop to fit"})),
        ("quality", json!({"type":"array","items":{"type":"string","enum":["psnr","ssim","ms_ssim"]},"description":"default all three"})), ("psnr_cap_db", json!({"type":"number","exclusiveMinimum":0,"maximum":1000,"description":"PSNR value for identical frames; also caps the rest"})),
        ("scale", n("max analysis side, default 480; 1 means full res")), ("region", json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4}))], &["a", "b", "duration"]),
    run: run_ranges }] }
