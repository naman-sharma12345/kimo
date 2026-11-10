//! Track-analysis toolkit (ported from the showcase film agent's battle-tested scripts into native Rust, no OpenCV):
//! feature tracking with raw track + status preservation, per-frame soft-L1 homography fits with held-out residual validation,
//! Jacobian polar/SVD decomposition, temporal jitter metrics, native-vs-fit corner disagreement, direct-anchor LK residual audit,
//! ECC refinement, and an error-field render. These DIAGNOSE a track; none of them certify it.
//! MCP surface: track_features, track_set_info, fit_track_homography, fit_info, decompose_track_homography, track_jitter_metrics,
//! corner_disagreement, track_feature_residuals, lk_residual_audit, ecc_refine_track, render_error_field.
use super::kit::*;
use crate::sfm;
use crate::trackmath::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

const CAVEAT_HELDOUT: &str = "Held-out residuals are measured on features tracked by the same sequential tracker that seeded the fit. They are NOT ground truth: if a feature drifts or swaps identity (repeated windows, repeating facade texture), the fit can agree with the drift and the residual still looks small, or a correct fit can look bad.";
const CAVEAT_FB: &str = "Forward/back consistency does not eliminate identity drift on repeated windows: a tracker can move consistently forward and back onto the wrong but similar-looking feature.";
const CAVEAT_CORNER: &str = "Corner columns are native-vs-fit DISAGREEMENT between two estimates. Neither is an annotated or ground-truth corner position, so a small value does not mean accurate and a large value does not say which estimate is wrong.";
const CAVEAT_LK: &str = "LK audit features are held-out measurements, not ground truth. Residuals are biased low because LK starts from the fit's own prediction and converges to the nearest local minimum; forward/back consistency does not eliminate identity drift on repeated windows. Use as a diagnostic, not an accuracy score.";

struct TrackSet { clip_id: String, first: usize, fps: f64, w: usize, h: usize, roi: [P; 4], p0: Vec<P>, tr: Vec<Vec<P>>, alive: Vec<Vec<bool>>, hold_every: usize, seconds: f64 }
struct Fit { track_id: String, kind: String, parent: Option<String>, h: Vec<H3>, corners0: [P; 4], native_h: Option<Vec<H3>>, native_quads: Option<Vec<[P; 4]>>, lambda: f64, f_scale: f64, carried: Vec<usize>, ecc: Option<Vec<(f64, f64, bool)>> }
static TRACKS: Mutex<BTreeMap<String, TrackSet>> = Mutex::new(BTreeMap::new());
static FITS: Mutex<BTreeMap<String, Fit>> = Mutex::new(BTreeMap::new());
fn lock_t() -> std::sync::MutexGuard<'static, BTreeMap<String, TrackSet>> { TRACKS.lock().unwrap_or_else(|e| e.into_inner()) }
fn lock_f() -> std::sync::MutexGuard<'static, BTreeMap<String, Fit>> { FITS.lock().unwrap_or_else(|e| e.into_inner()) }

// ---------- loud argument validation ----------
fn keys(a: &Value, allowed: &[&str]) -> Result<(), String> {
    let o = a.as_object().ok_or("arguments must be an object")?;
    for k in o.keys() { if !allowed.contains(&k.as_str()) { return Err(format!("unknown argument '{k}' (allowed: {})", allowed.join(", "))); } } Ok(())
}
fn num(a: &Value, k: &str, def: f64, lo: f64, hi: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(def), Some(v) => { let x = v.as_f64().ok_or(format!("'{k}' must be a number"))?; if !x.is_finite() || x < lo || x > hi { Err(format!("'{k}' = {x} is out of range {lo}..{hi}")) } else { Ok(x) } } }
}
fn int(a: &Value, k: &str, def: usize, lo: usize, hi: usize) -> Result<usize, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(def), Some(v) => { let x = v.as_f64().ok_or(format!("'{k}' must be an integer"))?; if !x.is_finite() || x.fract() != 0.0 || x < lo as f64 || x > hi as f64 { Err(format!("'{k}' = {x} must be an integer in {lo}..{hi}")) } else { Ok(x as usize) } } }
}
fn flag(a: &Value, k: &str, def: bool) -> Result<bool, String> { match a.get(k) { None | Some(Value::Null) => Ok(def), Some(v) => v.as_bool().ok_or(format!("'{k}' must be true or false")) } }
fn choice<'a>(a: &'a Value, k: &str, def: &'a str, opts: &[&str]) -> Result<&'a str, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(def), Some(v) => { let s = v.as_str().ok_or(format!("'{k}' must be a string"))?; if opts.contains(&s) { Ok(s) } else { Err(format!("'{k}' must be one of {}", opts.join(", "))) } } }
}
fn numbers(a: &Value, k: &str, len_mult: usize, lo: f64, hi: f64) -> Result<Option<Vec<f64>>, String> {
    let Some(v) = a.get(k).filter(|v| !v.is_null()) else { return Ok(None) };
    let arr = v.as_array().ok_or(format!("'{k}' must be an array of numbers"))?;
    if arr.is_empty() || arr.len() % len_mult != 0 { return Err(format!("'{k}' needs a non-empty multiple of {len_mult} numbers, got {}", arr.len())); }
    let mut out = Vec::with_capacity(arr.len());
    for (i, x) in arr.iter().enumerate() { let f = x.as_f64().ok_or(format!("'{k}'[{i}] is not a number"))?; if !f.is_finite() || f < lo || f > hi { return Err(format!("'{k}'[{i}] = {f} is outside {lo}..{hi}")); } out.push(f); }
    Ok(Some(out))
}
fn convex_area(q: &[P; 4]) -> Option<f64> {
    let mut sign = 0.0; let mut area = 0.0;
    for i in 0..4 { let (a, b, c) = (q[i], q[(i + 1) % 4], q[(i + 2) % 4]); let cr = (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0); if cr.abs() < 1e-12 { return None; } if sign == 0.0 { sign = cr.signum(); } else if cr.signum() != sign { return None; } area += a.0 * b.1 - b.0 * a.1; }
    Some(area.abs() / 2.0)
}
fn quad_arg(a: &Value, k: &str, w: usize, h: usize, lo: f64, hi: f64) -> Result<Option<[P; 4]>, String> {
    let Some(v) = numbers(a, k, 1, lo, hi)? else { return Ok(None) };
    if v.len() != 8 { return Err(format!("'{k}' needs exactly 8 numbers x0,y0,x1,y1,x2,y2,x3,y3 (normalised, TL TR BR BL), got {}", v.len())); }
    let q = [(v[0] * w as f64, v[1] * h as f64), (v[2] * w as f64, v[3] * h as f64), (v[4] * w as f64, v[5] * h as f64), (v[6] * w as f64, v[7] * h as f64)];
    let ar = convex_area(&q).ok_or(format!("'{k}' must be a convex, non-degenerate quad given in order around the shape"))?;
    if ar < 100.0 { return Err(format!("'{k}' encloses only {ar:.1} px^2; too small to measure")); } Ok(Some(q))
}
fn in_quad(q: &[P; 4], p: P) -> bool { let mut s = 0.0; for i in 0..4 { let (a, b) = (q[i], q[(i + 1) % 4]); let cr = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0); if cr != 0.0 { if s == 0.0 { s = cr.signum(); } else if cr.signum() != s { return false; } } } true }
fn frame_list(a: &Value, k: &str, n: usize) -> Result<Option<Vec<usize>>, String> {
    let Some(v) = a.get(k).filter(|v| !v.is_null()) else { return Ok(None) };
    let arr = v.as_array().ok_or(format!("'{k}' must be an array of frame numbers"))?; if arr.len() > 500 { return Err(format!("'{k}' has {} entries (max 500)", arr.len())); }
    let mut out = vec![]; for (i, x) in arr.iter().enumerate() { let f = x.as_f64().ok_or(format!("'{k}'[{i}] is not a number"))?; if !f.is_finite() || f.fract() != 0.0 || f < 0.0 || f >= n as f64 { return Err(format!("'{k}'[{i}] = {f} is not a frame in 0..{} (frames are relative to the track set's first frame)", n - 1)); } out.push(f as usize); }
    Ok(Some(out))
}
fn media_clip(ss: &Session, id: &str) -> Result<crate::model::Clip, String> {
    let c = ss.project.find_clip(id).ok_or(format!("no clip '{id}'"))?.clone();
    if !matches!(c.source, crate::model::Source::Media { .. }) { return Err("clip must be a media clip".into()); } Ok(c)
}
fn nframes(ss: &Session, c: &crate::model::Clip) -> usize { ((c.duration * ss.project.fps - 1e-3).ceil() as usize).max(1) }
fn qpx(q: &[P; 4]) -> Vec<f64> { q.iter().flat_map(|p| [p.0, p.1]).collect() }
fn latest<T>(m: &BTreeMap<String, T>, a: &Value, k: &str, what: &str) -> Result<String, String> {
    match a.get(k).filter(|v| !v.is_null()) { Some(v) => { let id = v.as_str().ok_or(format!("'{k}' must be a string"))?; if m.contains_key(id) { Ok(id.to_string()) } else { Err(format!("no {what} '{id}'; known: {:?}", m.keys().collect::<Vec<_>>())) } } None => m.keys().last().cloned().ok_or(format!("no {what}s yet; create one first")) }
}
fn stats(v: &[f64]) -> Value { if v.is_empty() { return json!({"n": 0}); } json!({"n": v.len(), "median": percentile(v, 50.0), "p95": percentile(v, 95.0), "max": v.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "mean": v.iter().sum::<f64>() / v.len() as f64}) }
fn med(v: &mut Vec<f64>) -> f64 { percentile(v, 50.0) }

struct FrameRes { n: usize, median: f64, p95: f64, max: f64, drift: f64 }
fn heldout_res(ts: &TrackSet, h: &H3, i: usize) -> FrameRes {
    let (mut norms, mut ex, mut ey) = (vec![], vec![], vec![]);
    for j in 0..ts.p0.len() { if ts.alive[i][j] && j % ts.hold_every == 0 { if let Some(p) = apply(h, ts.p0[j]) { let q = ts.tr[i][j]; norms.push(dist(p, q)); ex.push(p.0 - q.0); ey.push(p.1 - q.1); } } }
    if norms.is_empty() { return FrameRes { n: 0, median: f64::NAN, p95: f64::NAN, max: f64::NAN, drift: f64::NAN }; }
    let (mx, my) = (med(&mut ex), med(&mut ey));
    FrameRes { n: norms.len(), median: percentile(&norms, 50.0), p95: percentile(&norms, 95.0), max: norms.iter().cloned().fold(0.0, f64::max), drift: (mx * mx + my * my).sqrt() }
}
fn corners_at(h: &H3, q0: &[P; 4]) -> Option<[P; 4]> { Some([apply(h, q0[0])?, apply(h, q0[1])?, apply(h, q0[2])?, apply(h, q0[3])?]) }
fn worst_by(rows: &[(usize, f64)], k: usize) -> Vec<Value> { let mut r: Vec<_> = rows.iter().cloned().filter(|x| x.1.is_finite()).collect(); r.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap()); r.truncate(k); r.iter().map(|(f, v)| json!({"frame": f, "value": v})).collect() }

fn px(f: &mut crate::effects::Frame, x: i64, y: i64, c: [f32; 3]) { if x >= 0 && y >= 0 && (x as usize) < f.w && (y as usize) < f.h { let i = (y as usize * f.w + x as usize) * 4; f.d[i] = c[0]; f.d[i + 1] = c[1]; f.d[i + 2] = c[2]; f.d[i + 3] = 1.0; } }
fn line(f: &mut crate::effects::Frame, a: P, b: P, c: [f32; 3]) { let n = (dist(a, b).ceil() as i64).max(1); for s in 0..=n { let t = s as f64 / n as f64; px(f, (a.0 + (b.0 - a.0) * t).round() as i64, (a.1 + (b.1 - a.1) * t).round() as i64, c); } }
fn disc(f: &mut crate::effects::Frame, p: P, r: i64, c: [f32; 3]) { for dy in -r..=r { for dx in -r..=r { if dx * dx + dy * dy <= r * r { px(f, p.0.round() as i64 + dx, p.1.round() as i64 + dy, c); } } } }
fn arrow(f: &mut crate::effects::Frame, a: P, b: P, c: [f32; 3]) {
    line(f, a, b, c); let l = dist(a, b); if l < 3.0 { return; } let (ux, uy) = ((b.0 - a.0) / l, (b.1 - a.1) / l); let hl = (l * 0.2).clamp(3.0, 8.0);
    for s in [-0.45f64, 0.45] { let (cs, sn) = (s.cos(), s.sin()); let (rx, ry) = (-(ux * cs - uy * sn), -(ux * sn + uy * cs)); line(f, b, (b.0 + rx * hl, b.1 + ry * hl), c); }
}
fn base_frame(gray: &[f32], w: usize, h: usize) -> crate::effects::Frame { let mut f = crate::effects::Frame::new(w, h); for i in 0..w * h { let g = (gray[i] / 255.0).clamp(0.0, 1.0); f.d[i * 4] = g; f.d[i * 4 + 1] = g; f.d[i * 4 + 2] = g; f.d[i * 4 + 3] = 1.0; } f }
/// Caption bar; only drawn when a font is installed (the caveat is always also in the sidecar JSON / tool result).
fn banner(f: &mut crate::effects::Frame, text: &str) -> bool {
    let font = crate::text::resolve_font("Roboto"); if !std::path::Path::new(&font).is_file() { return false; }
    let bh = 44usize.min(f.h); let mut b = crate::effects::Frame::new(f.w, bh);
    if crate::text::render_text(&mut b, text, &font, 20.0, [1.0, 1.0, 1.0], 0.0, 0.0, [0.0, 0.0, 0.0], 0.0, "center", 1.0, None, 0.0).is_err() { return false; }
    for i in 0..f.w * bh { let al = b.d[i * 4 + 3].clamp(0.0, 1.0); for c in 0..3 { f.d[i * 4 + c] = f.d[i * 4 + c] * 0.4 * (1.0 - al) + b.d[i * 4 + c] * al + f.d[i * 4 + c] * 0.0; } } true
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "track_features", description: "Track Shi-Tomasi features inside a region through a media clip and store the raw per-frame positions. Track Shi-Tomasi features inside a convex ROI (normalised quad TL,TR,BR,BL at `start`) through a media clip with sequential pyramidal LK plus a forward/back check, and STORE the raw per-frame positions and alive/dead status (read them with track_set_info). Every feature gets a fixed train/held-out role (index % holdout_every == 0 is held out of later fits). The raw tracks are the evidence: callers should validate selected feature identities themselves. Caveat: forward/back consistency does not eliminate identity drift on repeated windows, and sequential tracking accumulates drift; a feature that stays 'alive' is NOT verified to be the same physical point. Coordinates are project pixels (project size = analysis size). Returns a track_id, survivor counts and honest limits.", mutates: false,
        schema: obj(vec![("clip_id", s("media clip")), ("roi", json!({"type":"array","items":{"type":"number"},"description":"8 numbers x0,y0,x1,y1,x2,y2,x3,y3 normalised 0..1, TL TR BR BL, convex; features are detected only inside it in frame `start`"})), ("start", n("first frame index in clip, default 0")), ("end", n("last frame index, default end (at most 2000 frames)")), ("max_features", n("8..2000, default 300")), ("min_distance", n("min spacing between features in px, 2..100, default 7")), ("window", n("LK half window in px, 3..30, default 10 (21px window)")), ("levels", n("pyramid levels 1..5, default 3")), ("fb_threshold_px", n("forward/back round-trip tolerance in px, 0.01..5, default 0.25; features exceeding it die permanently")), ("holdout_every", n("integer 2..20, default 4: every Nth feature is held out of fitting"))], &["clip_id", "roi"]),
        run: |ss, a| {
            keys(a, &["clip_id", "roi", "start", "end", "max_features", "min_distance", "window", "levels", "fb_threshold_px", "holdout_every"])?;
            let t0 = Instant::now(); let cid = str_arg(a, "clip_id")?.to_string(); let c = media_clip(ss, &cid)?; let nf = nframes(ss, &c); let (w, h) = (ss.project.width as usize, ss.project.height as usize);
            let f0 = int(a, "start", 0, 0, nf - 1)?; let f1 = int(a, "end", nf - 1, 0, nf - 1)?; if f1 <= f0 { return Err(format!("end ({f1}) must be greater than start ({f0})")); } if f1 - f0 + 1 > 2000 { return Err("at most 2000 frames per track set; narrow start/end".into()); }
            let roi = quad_arg(a, "roi", w, h, 0.0, 1.0)?.ok_or("missing 'roi' (8 normalised numbers)")?;
            let maxf = int(a, "max_features", 300, 8, 2000)?; let mind = num(a, "min_distance", 7.0, 2.0, 100.0)?; let win = int(a, "window", 10, 3, 30)? as i32; let levels = int(a, "levels", 3, 1, 5)?; let fb = num(a, "fb_threshold_px", 0.25, 0.01, 5.0)?; let hold = int(a, "holdout_every", 4, 2, 20)?;
            crate::guard::reserve((f1 - f0 + 1) * maxf * 17);
            let Session { project, cache, .. } = ss;
            let g0 = crate::tracking::clip_gray_pub(project, cache, &c, f0)?;
            let cand = sfm::detect(&g0, w, h, (maxf * 30).min(20000), mind as f32, &[]);
            let p0: Vec<P> = cand.into_iter().filter(|p| in_quad(&roi, *p)).take(maxf).collect();
            if p0.len() < 8 { return Err(format!("only {} trackable features inside the ROI at frame {f0} (need 8); the region is too flat or too small, or lower min_distance / enlarge the ROI", p0.len())); }
            let n = p0.len(); let mut tr: Vec<Vec<P>> = vec![p0.clone()]; let mut alive: Vec<Vec<bool>> = vec![vec![true; n]];
            let mut prev = sfm::pyramid(&g0, w, h, levels); let (mut cur_pos, mut cur_alive) = (p0.clone(), vec![true; n]);
            for i in f0 + 1..=f1 {
                crate::guard::check();
                let g = crate::tracking::clip_gray_pub(project, cache, &c, i)?; let cp = sfm::pyramid(&g, w, h, levels);
                let idx: Vec<usize> = (0..n).filter(|&j| cur_alive[j]).collect(); let pts: Vec<P> = idx.iter().map(|&j| cur_pos[j]).collect();
                let fwd = sfm::klt(&prev, &cp, &pts, win);
                let ok: Vec<(usize, P)> = idx.iter().zip(&fwd).filter_map(|(&j, f)| f.map(|p| (j, p))).collect(); let okp: Vec<P> = ok.iter().map(|x| x.1).collect();
                let back = if okp.is_empty() { vec![] } else { sfm::klt(&cp, &prev, &okp, win) };
                let mut alive_now = vec![false; n]; for (k, (j, p)) in ok.iter().enumerate() { if let Some(b) = back[k] { if dist(b, cur_pos[*j]) < fb { alive_now[*j] = true; cur_pos[*j] = *p; } } }
                cur_alive = alive_now; tr.push(cur_pos.clone()); alive.push(cur_alive.clone()); prev = cp;
            }
            let counts: Vec<usize> = alive.iter().map(|r| r.iter().filter(|x| **x).count()).collect();
            let hold_n = (0..n).filter(|j| j % hold == 0).count(); let full = (0..n).filter(|&j| alive.last().unwrap()[j]).count();
            let secs = t0.elapsed().as_secs_f64(); let ts = TrackSet { clip_id: cid.clone(), first: f0, fps: project.fps, w, h, roi, p0, tr, alive, hold_every: hold, seconds: secs };
            let mut m = lock_t(); let id = format!("tracks{}", m.len() + 1);
            let out = json!({"track_id": id, "clip_id": cid, "first_frame": f0, "frames": f1 - f0 + 1, "features": n, "train": n - hold_n, "held_out": hold_n, "alive_first": counts[0], "alive_last": counts[counts.len() - 1], "alive_min": counts.iter().min(), "survived_all_frames": full, "alive_per_frame_every10": counts.iter().step_by(10).collect::<Vec<_>>(), "seconds": secs, "roi_px": qpx(&roi),
                "limits": [CAVEAT_FB, "Sequential LK accumulates drift over long takes; compare against an independent measurement before trusting late frames.", "Features come from the first frame's texture only; a repeating facade can match the wrong instance without tripping the round-trip check."]});
            m.insert(id, ts); Ok(out) } },
    Command { name: "track_set_info", description: "Read back the raw tracks of a track_features set so the caller can check feature identities. Read back the RAW tracks of a track_features set so identities can be validated by the caller: per-feature first-frame position, role (train/held-out), the frame it died (null = alive to the end) and, on request, per-frame positions and alive flags. No arguments lists all sets; default track_id is the newest. The positions are measurements, not truth.", mutates: false,
        schema: obj(vec![("track_id", s("default: list all sets when no other argument is given, else newest")), ("frames", json!({"type":"array","items":{"type":"number"},"description":"relative frame indices (0 = first frame) to include positions for"})), ("features", json!({"type":"array","items":{"type":"number"},"description":"feature indices to include; default all (max 400 returned)"})), ("include_status", json!({"type":"boolean","description":"also return alive flags for the selected frames"}))], &[]),
        run: |_, a| { keys(a, &["track_id", "frames", "features", "include_status"])?; let m = lock_t();
            let sumr = |id: &String, t: &TrackSet| json!({"track_id": id, "clip_id": t.clip_id, "first_frame": t.first, "frames": t.tr.len(), "features": t.p0.len(), "holdout_every": t.hold_every, "roi_px": qpx(&t.roi), "alive_last": t.alive.last().map(|r| r.iter().filter(|x| **x).count()), "seconds": t.seconds});
            if a.get("track_id").map(|v| v.is_null()).unwrap_or(true) && a.get("frames").is_none() && a.get("features").is_none() { return Ok(json!({"track_sets": m.iter().map(|(k, t)| sumr(k, t)).collect::<Vec<_>>()})); }
            let id = latest(&m, a, "track_id", "track set")?; let t = &m[&id]; let fr = frame_list(a, "frames", t.tr.len())?.unwrap_or_default();
            let fe: Vec<usize> = match a.get("features").filter(|v| !v.is_null()) { Some(v) => { let arr = v.as_array().ok_or("'features' must be an array")?; let mut o = vec![]; for (i, x) in arr.iter().enumerate() { let f = x.as_f64().ok_or(format!("'features'[{i}] is not a number"))?; if f.fract() != 0.0 || f < 0.0 || f >= t.p0.len() as f64 { return Err(format!("'features'[{i}] = {f} is not a feature in 0..{}", t.p0.len() - 1)); } o.push(f as usize); } o } None => (0..t.p0.len()).collect() };
            if fe.len() > 400 { return Err(format!("{} features selected (max 400); pass 'features' to narrow", fe.len())); } let st = flag(a, "include_status", false)?;
            let feats: Vec<Value> = fe.iter().map(|&j| { let died = (0..t.tr.len()).find(|&i| !t.alive[i][j]); let mut o = json!({"feature": j, "p0": [t.p0[j].0, t.p0[j].1], "role": if j % t.hold_every == 0 { "held_out" } else { "train" }, "died_at_frame": died});
                if !fr.is_empty() { o["positions"] = json!(fr.iter().map(|&i| json!({"frame": i, "xy": [t.tr[i][j].0, t.tr[i][j].1], "alive": if st { Some(t.alive[i][j]) } else { None }})).collect::<Vec<_>>()); } o }).collect();
            let mut o = sumr(&id, t); o["feature_data"] = json!(feats); o["note"] = json!("positions are sequential-LK measurements; a dead feature keeps its last position and is flagged not alive"); Ok(o) } },
    Command { name: "fit_track_homography", description: "Fit a robust per-frame homography to a feature track set, with optional smoothing and held-out validation. Per-frame ROBUST homography fit (soft-L1 loss, scipy-equivalent, RANSAC-seeded) from a track set's TRAIN features, optional second-difference temporal smoothing (frame 0 pinned to identity), then HELD-OUT residual validation on the features that were never fitted. Optionally compares against the host tracker's native quads (e.g. track_planar return_quads) with the same held-out features. Returns a fit_id plus per-frame rows (held-out median/p95/max px, median-vector drift, native equivalents, native-vs-fit corner disagreement) and worst frames. NOT an accuracy score: held-out features are measurements from the same sequential tracker, not ground truth; forward/back consistency does not eliminate identity drift on repeated windows; corner columns are native-vs-fit disagreement, not annotated truth.", mutates: false,
        schema: obj(vec![("track_id", s("track set from track_features (default newest)")), ("corners", json!({"type":"array","items":{"type":"number"},"description":"8 numbers normalised TL TR BR BL: the quad at the track set's first frame whose fitted corners you want (required unless native_quads is given, then defaults to its first quad)"})), ("native_quads", json!({"type":"array","items":{"type":"number"},"description":"optional host-tracker quads, 8 normalised numbers per frame for EVERY frame of the set (track_planar return_quads flattened); enables native-vs-fit columns"})), ("smooth", n("second-difference smoothing weight lambda, 0..1e6, default 2 (0 = none)")), ("f_scale_px", n("soft-L1 scale in px, 0.01..50, default 0.6")), ("ransac_threshold_px", n("seed inlier threshold px, 0.05..20, default 0.6")), ("max_nfev", n("LM iterations per frame, 1..1000, default 150")), ("min_train", n("minimum alive training features per frame, 8..1000, default 8")), ("sparse_frames", json!({"type":"string","enum":["error","carry"],"description":"frame with too few features: error (default) or carry the previous frame's fit and list it in carried_frames"})), ("include_rows", json!({"type":"boolean","description":"default true"}))], &[]),
        run: |_, a| {
            keys(a, &["track_id", "corners", "native_quads", "smooth", "f_scale_px", "ransac_threshold_px", "max_nfev", "min_train", "sparse_frames", "include_rows"])?;
            let t0 = Instant::now(); let m = lock_t(); let tid = latest(&m, a, "track_id", "track set")?; let ts = &m[&tid]; let nfr = ts.tr.len();
            let lam = num(a, "smooth", 2.0, 0.0, 1e6)?; let fs = num(a, "f_scale_px", 0.6, 0.01, 50.0)?; let rt = num(a, "ransac_threshold_px", 0.6, 0.05, 20.0)?; let nfev = int(a, "max_nfev", 150, 1, 1000)?; let mint = int(a, "min_train", 8, 8, 1000)?; let carry = choice(a, "sparse_frames", "error", &["error", "carry"])? == "carry";
            let nq = match numbers(a, "native_quads", 8, -1.0, 2.0)? { Some(v) => { if v.len() != nfr * 8 { return Err(format!("native_quads has {} numbers but the track set has {nfr} frames (need {})", v.len(), nfr * 8)); } Some(v.chunks(8).map(|c| [(c[0] * ts.w as f64, c[1] * ts.h as f64), (c[2] * ts.w as f64, c[3] * ts.h as f64), (c[4] * ts.w as f64, c[5] * ts.h as f64), (c[6] * ts.w as f64, c[7] * ts.h as f64)]).collect::<Vec<[P; 4]>>()) } None => None };
            let corners0 = match quad_arg(a, "corners", ts.w, ts.h, -1.0, 2.0)? { Some(q) => q, None => nq.as_ref().map(|v| v[0]).ok_or("'corners' is required (8 normalised numbers) unless native_quads is supplied")? };
            let train: Vec<usize> = (0..ts.p0.len()).filter(|j| j % ts.hold_every != 0).collect();
            let scale = ts.w.max(ts.h) as f64; let mut params: Vec<[f64; 8]> = vec![]; let mut carried = vec![];
            for i in 0..nfr {
                crate::guard::check();
                let use_: Vec<usize> = train.iter().cloned().filter(|&j| ts.alive[i][j]).collect();
                if use_.len() < mint { if carry && i > 0 { params.push(*params.last().unwrap()); carried.push(ts.first + i); continue; } return Err(format!("frame {} (relative {i}) has only {} alive training features (< min_train {mint}); lower min_train, use sparse_frames=carry, or re-track with a larger ROI / fewer frames", ts.first + i, use_.len())); }
                let src: Vec<P> = use_.iter().map(|&j| ts.p0[j]).collect(); let dst: Vec<P> = use_.iter().map(|&j| ts.tr[i][j]).collect();
                let init = if i == 0 { IDENT } else { ransac(&src, &dst, rt, 4000, 0x9E37_79B9 ^ i as u64).map(|r| r.0).ok_or(format!("frame {}: RANSAC could not seed a homography from {} features", ts.first + i, src.len()))? };
                let (hf, _, _) = robust_fit(&src, &dst, &init, fs, nfev, scale).ok_or(format!("frame {}: robust fit failed (degenerate features)", ts.first + i))?;
                let hn = norm_h(&conj_scale(&hf, scale, false)).ok_or(format!("frame {}: degenerate homography", ts.first + i))?;
                params.push([hn[0][0], hn[0][1], hn[0][2], hn[1][0], hn[1][1], hn[1][2], hn[2][0], hn[2][1]]);
            }
            let mut sm = params.clone(); for k in 0..8 { let col: Vec<f64> = params.iter().map(|p| p[k]).collect(); let z = smooth_series(&col, lam); for i in 0..nfr { sm[i][k] = z[i]; } }
            sm[0] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
            let hs: Vec<H3> = sm.iter().map(|p| conj_scale(&[[p[0], p[1], p[2]], [p[3], p[4], p[5]], [p[6], p[7], 1.0]], scale, true)).collect();
            let (nh, nquads) = match &nq { Some(q) => { let mut v = vec![]; for i in 0..nfr { v.push(h_from_4(&q[0], &q[i]).ok_or(format!("native quad at frame {} is degenerate", ts.first + i))?); } (Some(v), Some(q.clone())) } None => (None, None) };
            let mut rows = vec![]; let mut fitp = vec![]; let (mut fmed, mut nmed) = (vec![], vec![]); let (mut fp95, mut fmaxm, mut np95, mut nmaxm) = (0f64, 0f64, 0f64, 0f64); let mut dr = vec![]; let mut nodata = vec![];
            for i in 0..nfr {
                let r = heldout_res(ts, &hs[i], i); if r.n == 0 { nodata.push(ts.first + i); }
                let mut row = json!({"frame": ts.first + i, "rel_frame": i, "time_s": (ts.first + i) as f64 / ts.fps, "heldout": r.n, "fit_median_px": r.median, "fit_p95_px": r.p95, "fit_max_px": r.max, "median_vector_drift_px": r.drift});
                fmed.push(r.median); fp95 = fp95.max(if r.p95.is_finite() { r.p95 } else { 0.0 }); fmaxm = fmaxm.max(if r.max.is_finite() { r.max } else { 0.0 }); dr.push(r.drift); fitp.push((ts.first + i, r.p95));
                if let Some(nh) = &nh { let rn = heldout_res(ts, &nh[i], i); row["native_median_px"] = json!(rn.median); row["native_p95_px"] = json!(rn.p95); row["native_max_px"] = json!(rn.max); nmed.push(rn.median); np95 = np95.max(if rn.p95.is_finite() { rn.p95 } else { 0.0 }); nmaxm = nmaxm.max(if rn.max.is_finite() { rn.max } else { 0.0 });
                    if let (Some(cf), Some(cn)) = (corners_at(&hs[i], &corners0), corners_at(&nh[i], &corners0)) { for k in 0..4 { row[format!("corner{k}_native_vs_fit_px")] = json!(dist(cf[k], cn[k])); } } }
                rows.push(row);
            }
            let mut fm = fmed.clone(); let mut nm = nmed.clone();
            let mut summary = json!({"frames": nfr, "train_features": train.len(), "held_out_features": ts.p0.len() - train.len(), "frames_without_heldout_data": nodata, "carried_frames": carried, "fit_pooled_frame_median_px": med(&mut fm), "fit_worst_frame_p95_px": fp95, "fit_max_px": fmaxm, "endpoint_median_vector_drift_px": dr.last(), "worst_frames_by_fit_p95": worst_by(&fitp, 5), "smooth_lambda": lam, "f_scale_px": fs});
            if nh.is_some() { summary["native_pooled_frame_median_px"] = json!(med(&mut nm)); summary["native_worst_frame_p95_px"] = json!(np95); summary["native_max_px"] = json!(nmaxm);
                let last = rows.last().unwrap(); summary["corner_disagreement_last_frame_px"] = json!((0..4).map(|k| last[format!("corner{k}_native_vs_fit_px")].clone()).collect::<Vec<_>>()); }
            drop(m);
            let mut fits = lock_f(); let id = format!("fit{}", fits.len() + 1);
            fits.insert(id.clone(), Fit { track_id: tid.clone(), kind: "robust_soft_l1".into(), parent: None, h: hs, corners0, native_h: nh, native_quads: nquads, lambda: lam, f_scale: fs, carried, ecc: None });
            let mut out = json!({"fit_id": id, "track_id": tid, "seconds": t0.elapsed().as_secs_f64(), "summary": summary, "corners0_px": qpx(&corners0), "caveats": [CAVEAT_HELDOUT, CAVEAT_FB, CAVEAT_CORNER]});
            if flag(a, "include_rows", true)? { out["rows"] = json!(rows); } Ok(out) } },
    Command { name: "fit_info", description: "Inspect a stored homography fit: per-frame 3x3 homographies, fitted corners and ECC correlations when present. Inspect a stored homography fit (from fit_track_homography or ecc_refine_track): per-frame 3x3 homographies (pixel coords, map first-frame points to each frame), fitted corners, ECC correlations when present. No arguments lists all fits.", mutates: false,
        schema: obj(vec![("fit_id", s("default: list all fits when no other argument is given, else newest")), ("frames", json!({"type":"array","items":{"type":"number"},"description":"relative frames to return H and corners for (default none)"}))], &[]),
        run: |_, a| { keys(a, &["fit_id", "frames"])?; let m = lock_f();
            let sm = |id: &String, f: &Fit| json!({"fit_id": id, "kind": f.kind, "parent_fit": f.parent, "track_id": f.track_id, "frames": f.h.len(), "has_native": f.native_h.is_some(), "smooth_lambda": f.lambda, "f_scale_px": f.f_scale, "carried_frames": f.carried, "has_ecc": f.ecc.is_some()});
            if a.get("fit_id").map(|v| v.is_null()).unwrap_or(true) && a.get("frames").is_none() { return Ok(json!({"fits": m.iter().map(|(k, f)| sm(k, f)).collect::<Vec<_>>()})); }
            let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let fr = frame_list(a, "frames", f.h.len())?.unwrap_or_default(); let mut o = sm(&id, f); o["corners0_px"] = json!(qpx(&f.corners0));
            o["frames_data"] = json!(fr.iter().map(|&i| json!({"rel_frame": i, "H_px": f.h[i], "corners_px": corners_at(&f.h[i], &f.corners0).map(|c| qpx(&c)), "ecc": f.ecc.as_ref().map(|e| json!({"rho_init": e[i].0, "rho": e[i].1, "converged": e[i].2}))})).collect::<Vec<_>>()); Ok(o) } },
    Command { name: "decompose_track_homography", description: "Decompose a fit's per-frame homography at a point into rotation, scales, shear, determinant and displacement. Jacobian polar/SVD decomposition of a fit's per-frame homography at a chosen point (default: centre of the first-frame quad): rotation in degrees, principal scales (SVD singular values major/minor), shear (off-diagonal of the symmetric polar stretch), determinant, and the point's displacement. Use it to see whether a track's motion looks like a camera move (smooth rotation/scale) or a fit artefact (shear/scale jumps). Pure math on the fit; says nothing about whether the fit is right.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("point", json!({"type":"array","items":{"type":"number"},"description":"2 numbers x,y normalised 0..1 in the first frame; default quad centre"})), ("every", n("return every Nth frame, default 1")), ("include_rows", json!({"type":"boolean"}))], &[]),
        run: |_, a| { keys(a, &["fit_id", "point", "every", "include_rows"])?; let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let (w, hh) = { let t = lock_t(); t.get(&f.track_id).map(|t| (t.w, t.h)).ok_or("the fit's track set no longer exists")? };
            let c = match numbers(a, "point", 2, 0.0, 1.0)? { Some(v) => { if v.len() != 2 { return Err("'point' needs exactly 2 numbers".into()); } (v[0] * w as f64, v[1] * hh as f64) } None => { let q = &f.corners0; (q.iter().map(|p| p.0).sum::<f64>() / 4.0, q.iter().map(|p| p.1).sum::<f64>() / 4.0) } };
            let ev = int(a, "every", 1, 1, 1000)?; let mut rows = vec![]; let (mut rots, mut maj, mut mnr, mut shr) = (vec![], vec![], vec![], vec![]); let mut refl = vec![];
            for (i, h) in f.h.iter().enumerate() { let j = jacobian(h, c).ok_or(format!("rel frame {i}: homography is singular at the chosen point"))?; let d = decompose(&j); let p = apply(h, c).ok_or(format!("rel frame {i}: point maps to infinity"))?;
                rots.push(d.rotation_deg); maj.push(d.scale_major); mnr.push(d.scale_minor); shr.push(d.shear); if d.reflection { refl.push(i); }
                if i % ev == 0 { rows.push(json!({"rel_frame": i, "rotation_deg": d.rotation_deg, "scale_major": d.scale_major, "scale_minor": d.scale_minor, "shear": d.shear, "det": d.det, "center_dx": p.0 - c.0, "center_dy": p.1 - c.1})); } }
            let rng = |v: &Vec<f64>| json!({"min": v.iter().cloned().fold(f64::INFINITY, f64::min), "max": v.iter().cloned().fold(f64::NEG_INFINITY, f64::max), "last": v.last()});
            let mut o = json!({"fit_id": id, "point_px": [c.0, c.1], "summary": {"rotation_deg": rng(&rots), "scale_major": rng(&maj), "scale_minor": rng(&mnr), "shear": rng(&shr), "orientation_flip_frames": refl}, "note": "rotation is the polar-factor angle; scale_major/minor are the Jacobian's singular values; these describe the fitted homography only"});
            if flag(a, "include_rows", true)? { o["rows"] = json!(rows); } Ok(o) } },
    Command { name: "track_jitter_metrics", description: "Temporal jitter of a fit's corner paths: p95 and max per-frame step and acceleration, per corner and overall. Temporal jitter of a fit's corner trajectories: p95 and max of the first difference (per-frame step, px) and second difference (acceleration, px/frame^2) across all four corners, per corner, plus the worst frames; computed for the robust fit and, when native quads were supplied, for the native track so you can compare smoothness. Smoothness is not accuracy: a perfectly smooth track can be wrong and a jittery one right.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("source", json!({"type":"string","enum":["fit","native","both"],"description":"default both (native only if available)"})), ("include_rows", json!({"type":"boolean","description":"per-frame step/acceleration per corner, default false"}))], &[]),
        run: |_, a| { keys(a, &["fit_id", "source", "include_rows"])?; let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let src = choice(a, "source", "both", &["fit", "native", "both"])?; let rows_on = flag(a, "include_rows", false)?;
            if f.h.len() < 3 { return Err("need at least 3 frames for second differences".into()); } let first = { lock_t().get(&f.track_id).map(|t| t.first).unwrap_or(0) };
            let acc = |cs: &Vec<[P; 4]>, i: usize, k: usize| ((cs[i][k].0 - 2.0 * cs[i - 1][k].0 + cs[i - 2][k].0).powi(2) + (cs[i][k].1 - 2.0 * cs[i - 1][k].1 + cs[i - 2][k].1).powi(2)).sqrt();
            let analyse = |hs: &Vec<H3>| -> Result<Value, String> { let cs: Vec<[P; 4]> = hs.iter().enumerate().map(|(i, h)| corners_at(h, &f.corners0).ok_or(format!("rel frame {i}: corner maps to infinity"))).collect::<Result<_, _>>()?;
                let (mut v1, mut v2) = (vec![], vec![]); let (mut pk1, mut pk2) = (vec![vec![]; 4], vec![vec![]; 4]); let mut acc_by_frame = vec![];
                for i in 1..cs.len() { for k in 0..4 { let d = dist(cs[i][k], cs[i - 1][k]); v1.push(d); pk1[k].push(d); } }
                for i in 2..cs.len() { let mut worst = 0f64; for k in 0..4 { let d = acc(&cs, i, k); v2.push(d); pk2[k].push(d); worst = worst.max(d); } acc_by_frame.push((first + i, worst)); }
                let mut o = json!({"step_px": stats(&v1), "accel_px": stats(&v2), "per_corner": (0..4).map(|k| json!({"corner": k, "step_p95": percentile(&pk1[k], 95.0), "step_max": pk1[k].iter().cloned().fold(0.0, f64::max), "accel_p95": percentile(&pk2[k], 95.0), "accel_max": pk2[k].iter().cloned().fold(0.0, f64::max)})).collect::<Vec<_>>(), "worst_frames_by_accel": worst_by(&acc_by_frame, 5)});
                if rows_on { o["rows"] = json!((1..cs.len()).map(|i| json!({"rel_frame": i, "step_px": (0..4).map(|k| dist(cs[i][k], cs[i - 1][k])).collect::<Vec<_>>(), "accel_px": if i >= 2 { Some((0..4).map(|k| acc(&cs, i, k)).collect::<Vec<_>>()) } else { None }})).collect::<Vec<_>>()); } Ok(o) };
            let mut o = json!({"fit_id": id, "note": "first difference = corner step per frame; second difference = frame-to-frame change of that step. Smoothness is not accuracy."});
            if src != "native" { o["fit"] = analyse(&f.h)?; } if src != "fit" { match &f.native_h { Some(nh) => o["native"] = analyse(nh)?, None => if src == "native" { return Err("this fit has no native quads; pass native_quads to fit_track_homography".into()); } } } Ok(o) } },
    Command { name: "corner_disagreement", description: "Per-frame distance in pixels between the native tracker's corners and a homography fit's corners. Per-corner distance in px between the NATIVE tracker's corners and this fit's corners at every frame (requires native_quads given to fit_track_homography). This is disagreement between two estimates, not error against annotated truth: neither is ground truth and the tool cannot say which one is wrong.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("every", n("return every Nth frame, default 1")), ("include_rows", json!({"type":"boolean"}))], &[]),
        run: |_, a| { keys(a, &["fit_id", "every", "include_rows"])?; let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let nh = f.native_h.as_ref().ok_or("this fit has no native quads; pass native_quads to fit_track_homography")?; let ev = int(a, "every", 1, 1, 1000)?; let first = { lock_t().get(&f.track_id).map(|t| t.first).unwrap_or(0) };
            let mut per = vec![vec![]; 4]; let mut rows = vec![]; let mut maxr = vec![];
            for i in 0..f.h.len() { let (cf, cn) = (corners_at(&f.h[i], &f.corners0).ok_or("fit corner at infinity")?, corners_at(&nh[i], &f.corners0).ok_or("native corner at infinity")?); let d: Vec<f64> = (0..4).map(|k| dist(cf[k], cn[k])).collect(); for k in 0..4 { per[k].push(d[k]); } maxr.push((first + i, d.iter().cloned().fold(0.0, f64::max))); if i % ev == 0 { rows.push(json!({"frame": first + i, "rel_frame": i, "corner_native_vs_fit_px": d})); } }
            let mut o = json!({"fit_id": id, "per_corner": (0..4).map(|k| json!({"corner": k, "median": percentile(&per[k], 50.0), "p95": percentile(&per[k], 95.0), "max": per[k].iter().cloned().fold(0.0, f64::max), "last": per[k].last()})).collect::<Vec<_>>(), "worst_frames": worst_by(&maxr, 5), "caveat": CAVEAT_CORNER});
            if flag(a, "include_rows", true)? { o["rows"] = json!(rows); } Ok(o) } },
    Command { name: "track_feature_residuals", description: "Per-feature residuals for one frame of a fit: which tracked features disagree with it and by how much. Per-feature residual vectors for one frame of a fit: which tracked features disagree with the fit and by how much (predicted vs tracked position, residual vector and length, train/held-out role). The way to find individual features that likely drifted or changed identity. A large residual flags a feature to inspect, it does not prove the feature is wrong rather than the fit.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("frame", n("relative frame index (0 = first frame of the track set)")), ("set", json!({"type":"string","enum":["heldout","train","all"],"description":"default heldout"})), ("top", n("return the N largest residuals, 1..400, default 25"))], &["frame"]),
        run: |_, a| { keys(a, &["fit_id", "frame", "set", "top"])?; if a.get("frame").map(|v| v.is_null()).unwrap_or(true) { return Err("missing 'frame'".into()); } let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let t = lock_t(); let ts = t.get(&f.track_id).ok_or("the fit's track set no longer exists")?; let i = int(a, "frame", 0, 0, ts.tr.len() - 1)?;
            let set = choice(a, "set", "heldout", &["heldout", "train", "all"])?; let top = int(a, "top", 25, 1, 400)?; let mut v: Vec<(f64, Value)> = vec![];
            for j in 0..ts.p0.len() { let hold = j % ts.hold_every == 0; if (set == "heldout" && !hold) || (set == "train" && hold) { continue; } if !ts.alive[i][j] { continue; } let Some(p) = apply(&f.h[i], ts.p0[j]) else { continue }; let q = ts.tr[i][j]; let r = dist(p, q);
                v.push((r, json!({"feature": j, "role": if hold { "held_out" } else { "train" }, "p0": [ts.p0[j].0, ts.p0[j].1], "predicted": [p.0, p.1], "tracked": [q.0, q.1], "residual": [q.0 - p.0, q.1 - p.1], "length_px": r}))); }
            v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap()); let lens: Vec<f64> = v.iter().map(|x| x.0).collect(); let n_all = v.len(); v.truncate(top);
            Ok(json!({"fit_id": id, "frame": ts.first + i, "rel_frame": i, "set": set, "compared": n_all, "length_stats_px": stats(&lens), "largest": v.into_iter().map(|x| x.1).collect::<Vec<_>>(), "caveat": CAVEAT_HELDOUT})) } },
    Command { name: "lk_residual_audit", description: "Audit a homography fit with direct-anchor LK: fresh first-frame features predicted into every frame and measured. Direct-anchor LK residual audit: detects fresh features inside the ROI in the FIRST frame, predicts each one into every frame with the fit's homography, runs single-level LK from frame 0 straight to frame i (no sequential chaining) starting at the prediction, back-checks it to frame 0, and reports per-frame residual |prediction - LK result| for features passing the check: valid count, median/p95/max and count over `over_px`. Optional annotated PNGs (green cross = predicted, red circle = LK). NOT ground truth: held-out LK features are not ground truth; residuals are biased low because LK starts at the fit's own prediction; forward/back consistency does not eliminate identity drift on repeated windows. Reads every clip frame once, so time scales with frame count.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("roi", json!({"type":"array","items":{"type":"number"},"description":"8 numbers normalised convex quad in the first frame; default the track set's ROI"})), ("max_features", n("8..1000, default 120")), ("min_distance", n("px, 2..100, default 15")), ("window", n("LK half window px, 2..30, default 8 (17px window)")), ("fb_threshold_px", n("back-check tolerance px, 0.01..5, default 0.35")), ("over_px", n("count residuals above this, 0.05..50, default 1")), ("frames", json!({"type":"array","items":{"type":"number"},"description":"relative frames to render annotated PNGs for"})), ("render_dir", s("directory for the PNGs (created); required with frames"))], &[]),
        run: |ss, a| { keys(a, &["fit_id", "roi", "max_features", "min_distance", "window", "fb_threshold_px", "over_px", "frames", "render_dir"])?;
            let t0 = Instant::now(); let (fit_id, hs, ts_clip, first, roi_d, w, h, nfr) = { let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; let t = lock_t(); let ts = t.get(&f.track_id).ok_or("the fit's track set no longer exists")?; (id.clone(), f.h.clone(), ts.clip_id.clone(), ts.first, ts.roi, ts.w, ts.h, f.h.len()) };
            let c = media_clip(ss, &ts_clip)?; if (ss.project.width as usize, ss.project.height as usize) != (w, h) { return Err("project size changed since tracking; re-run track_features".into()); }
            let roi = quad_arg(a, "roi", w, h, 0.0, 1.0)?.unwrap_or(roi_d); let maxf = int(a, "max_features", 120, 8, 1000)?; let mind = num(a, "min_distance", 15.0, 2.0, 100.0)?; let win = int(a, "window", 8, 2, 30)? as i32; let fb = num(a, "fb_threshold_px", 0.35, 0.01, 5.0)?; let over = num(a, "over_px", 1.0, 0.05, 50.0)?;
            let rf = frame_list(a, "frames", nfr)?.unwrap_or_default(); let rdir = match a.get("render_dir").filter(|v| !v.is_null()) { Some(v) => Some(v.as_str().ok_or("'render_dir' must be a string")?.trim_end_matches('/').to_string()), None => None }; if !rf.is_empty() && rdir.is_none() { return Err("'render_dir' is required with 'frames'".into()); } if let Some(d) = &rdir { if d.is_empty() { return Err("'render_dir' is empty".into()); } std::fs::create_dir_all(d).map_err(|e| format!("cannot create render_dir: {e}"))?; }
            let Session { project, cache, .. } = ss; let g0 = crate::tracking::clip_gray_pub(project, cache, &c, first)?;
            let p: Vec<P> = sfm::detect(&g0, w, h, (maxf * 30).min(20000), mind as f32, &[]).into_iter().filter(|p| in_quad(&roi, *p)).take(maxf).collect(); if p.len() < 8 { return Err(format!("only {} features in the ROI in the first frame (need 8)", p.len())); }
            let pyr0 = sfm::pyramid(&g0, w, h, 1); let mut rows = vec![]; let mut worst = vec![]; let mut files = vec![];
            for i in 0..nfr {
                crate::guard::check(); let g = if i == 0 { g0.clone() } else { crate::tracking::clip_gray_pub(project, cache, &c, first + i)? }; let pyr = sfm::pyramid(&g, w, h, 1);
                let mut res = vec![]; let mut marks: Vec<(P, P)> = vec![];
                for q in &p { let Some(pred) = apply(&hs[i], *q) else { continue }; let guess = ((pred.0 - q.0) as f32, (pred.1 - q.1) as f32);
                    let Some((gx, gy, _)) = sfm::lk_level(&pyr0.lv[0], &pyr.lv[0], (q.0 as f32, q.1 as f32), guess, win) else { continue }; let found = (q.0 + gx as f64, q.1 + gy as f64);
                    if found.0 < 4.0 || found.1 < 4.0 || found.0 > w as f64 - 5.0 || found.1 > h as f64 - 5.0 { continue; }
                    let Some((bx, by, _)) = sfm::lk_level(&pyr.lv[0], &pyr0.lv[0], (found.0 as f32, found.1 as f32), ((q.0 - found.0) as f32, (q.1 - found.1) as f32), win) else { continue };
                    if dist((found.0 + bx as f64, found.1 + by as f64), *q) >= fb { continue; } res.push(dist(pred, found)); marks.push((pred, found)); }
                let r = json!({"frame": first + i, "rel_frame": i, "valid": res.len(), "total": p.len(), "median": percentile(&res, 50.0), "p95": percentile(&res, 95.0), "max": res.iter().cloned().fold(f64::NAN, f64::max), "over_px_count": res.iter().filter(|x| **x > over).count()});
                worst.push((first + i, percentile(&res, 95.0))); rows.push(r);
                if rf.contains(&i) { let mut fr = base_frame(&g, w, h); for (pr, fo) in &marks { line(&mut fr, (pr.0 - 3.0, pr.1), (pr.0 + 3.0, pr.1), [0.0, 1.0, 0.0]); line(&mut fr, (pr.0, pr.1 - 3.0), (pr.0, pr.1 + 3.0), [0.0, 1.0, 0.0]); for k in 0..16 { let t = k as f64 / 16.0 * std::f64::consts::TAU; px(&mut fr, (fo.0 + 2.5 * t.cos()).round() as i64, (fo.1 + 2.5 * t.sin()).round() as i64, [1.0, 0.0, 0.0]); } }
                    banner(&mut fr, "LK audit: green = fit prediction, red = LK (not ground truth)"); let path = format!("{}/residual-{}.png", rdir.as_ref().unwrap(), first + i); crate::render::write_image(&fr, &path)?; files.push(path); }
            }
            let valids: Vec<usize> = rows.iter().map(|r| r["valid"].as_u64().unwrap() as usize).collect(); let meds: Vec<f64> = rows.iter().map(|r| r["median"].as_f64().unwrap_or(f64::NAN)).collect(); let p95s: Vec<f64> = rows.iter().map(|r| r["p95"].as_f64().unwrap_or(f64::NAN)).collect(); let mx = rows.iter().map(|r| r["max"].as_f64().unwrap_or(0.0)).fold(0.0, f64::max);
            Ok(json!({"fit_id": fit_id, "features_detected": p.len(), "valid_floor": valids.iter().min(), "median_of_frame_medians_px": percentile(&meds, 50.0), "mean_of_frame_medians_px": meds.iter().filter(|x| x.is_finite()).sum::<f64>() / meds.iter().filter(|x| x.is_finite()).count().max(1) as f64, "worst_p95_px": p95s.iter().cloned().filter(|x| x.is_finite()).fold(0.0, f64::max), "max_px": mx, "worst_frames_by_p95": worst_by(&worst, 8), "rows": rows, "rendered": files, "seconds": t0.elapsed().as_secs_f64(), "caveat": CAVEAT_LK, "method": format!("feature LK, {}px window, level 0, direct from first frame, initialised by the fit's homography; forward-back < {fb}px; no residual-size rejection", 2 * win + 1)})) } },
    Command { name: "ecc_refine_track", description: "Refine a homography fit frame by frame with ECC (enhanced correlation coefficient) against the first frame. ECC (enhanced correlation coefficient) refinement of a fit: for every frame, Gauss-Newton maximisation of zero-mean normalised correlation between the FIRST frame's ROI and the warped frame, starting from the fit's homography and anchored directly to the first frame (no chaining). Writes a NEW fit (parent = the input fit; native quads carried over) so it can be compared and fed to lk_residual_audit / render_error_field. A frame whose correlation does not improve keeps its initial homography. Reports correlation before/after and how far the corners moved. Correlation is a photometric match score, not accuracy: it can be high on a wrong but self-similar surface and drops under exposure change or occlusion. Heavy: one blur and a few Gauss-Newton passes per frame.", mutates: false,
        schema: obj(vec![("fit_id", s("fit to refine, default newest")), ("max_iter", n("Gauss-Newton iterations per frame, 1..500, default 70")), ("eps", n("stop when the parameter step is below this, 1e-9..1e-2, default 1e-6")), ("mask_step", n("sample every Nth ROI pixel, 1..8, default 2")), ("roi", json!({"type":"array","items":{"type":"number"},"description":"8 numbers normalised convex quad in the first frame to correlate over; default the track set's ROI (use a small, flat face)"})), ("include_rows", json!({"type":"boolean"}))], &[]),
        run: |ss, a| { keys(a, &["fit_id", "max_iter", "eps", "mask_step", "roi", "include_rows"])?; let t0 = Instant::now();
            let (fit_id, hs, tid, corners0, nh, nq, lam, fsc, carried) = { let m = lock_f(); let id = latest(&m, a, "fit_id", "fit")?; let f = &m[&id]; (id.clone(), f.h.clone(), f.track_id.clone(), f.corners0, f.native_h.clone(), f.native_quads.clone(), f.lambda, f.f_scale, f.carried.clone()) };
            let (clip_id, first, roi_d, w, h) = { let t = lock_t(); let ts = t.get(&tid).ok_or("the fit's track set no longer exists")?; (ts.clip_id.clone(), ts.first, ts.roi, ts.w, ts.h) };
            let c = media_clip(ss, &clip_id)?; if (ss.project.width as usize, ss.project.height as usize) != (w, h) { return Err("project size changed since tracking; re-run track_features".into()); }
            let iters = int(a, "max_iter", 70, 1, 500)?; let eps = num(a, "eps", 1e-6, 1e-9, 1e-2)?; let step = int(a, "mask_step", 2, 1, 8)?; let roi = quad_arg(a, "roi", w, h, 0.0, 1.0)?.unwrap_or(roi_d);
            let pts: Vec<(u32, u32)> = { let (x0, x1) = (roi.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize, roi.iter().map(|p| p.0).fold(0.0, f64::max).ceil().min(w as f64 - 1.0) as usize); let (y0, y1) = (roi.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize, roi.iter().map(|p| p.1).fold(0.0, f64::max).ceil().min(h as f64 - 1.0) as usize);
                let mut v = vec![]; let mut y = y0; while y <= y1 { let mut x = x0; while x <= x1 { if in_quad(&roi, (x as f64, y as f64)) { v.push((x as u32, y as u32)); } x += step; } y += step; } v };
            if pts.len() < 200 { return Err(format!("the ROI holds only {} sample pixels (need 200); enlarge the ROI or lower mask_step", pts.len())); }
            let Session { project, cache, .. } = ss; let tpl = blur5(&crate::tracking::clip_gray_pub(project, cache, &c, first)?, w, h);
            let mut out_h = vec![]; let mut eccs = vec![]; let mut rows = vec![]; let mut moved = vec![]; let mut failed = vec![]; let mut kept = vec![]; let mut gain = vec![];
            for i in 0..hs.len() {
                crate::guard::check(); let img = blur5(&crate::tracking::clip_gray_pub(project, cache, &c, first + i)?, w, h);
                match ecc(&tpl, &img, w, h, &pts, &hs[i], iters, eps) { None => { out_h.push(hs[i]); eccs.push((f64::NAN, f64::NAN, false)); failed.push(first + i); rows.push(json!({"frame": first + i, "rel_frame": i, "status": "failed_kept_initial"})); }
                    Some(r) => { let keep = !(r.rho > r.rho_init) || !r.rho.is_finite(); let hn = if keep { hs[i] } else { r.h }; let mv = match (corners_at(&hs[i], &corners0), corners_at(&hn, &corners0)) { (Some(a_), Some(b_)) => (0..4).map(|k| dist(a_[k], b_[k])).fold(0.0, f64::max), _ => f64::NAN };
                        if keep { kept.push(first + i); } else { moved.push(mv); } gain.push(if keep { 0.0 } else { r.rho - r.rho_init }); out_h.push(hn); eccs.push((r.rho_init, if keep { r.rho_init } else { r.rho }, r.converged));
                        rows.push(json!({"frame": first + i, "rel_frame": i, "status": if keep { "kept_initial" } else if r.converged { "refined" } else { "refined_not_converged" }, "rho_init": r.rho_init, "rho": r.rho, "iterations": r.iters, "max_corner_move_px": mv})); } }
            }
            let mut fits = lock_f(); let id = format!("fit{}", fits.len() + 1);
            fits.insert(id.clone(), Fit { track_id: tid.clone(), kind: "ecc_refined".into(), parent: Some(fit_id.clone()), h: out_h, corners0, native_h: nh, native_quads: nq, lambda: lam, f_scale: fsc, carried, ecc: Some(eccs) });
            let mut o = json!({"fit_id": id, "parent_fit": fit_id, "frames": hs.len(), "refined": moved.len(), "kept_initial_frames": kept, "failed_frames": failed, "corner_move_px": stats(&moved), "mean_rho_gain": gain.iter().sum::<f64>() / gain.len().max(1) as f64, "mask_pixels": pts.len(), "seconds": t0.elapsed().as_secs_f64(), "caveat": "ECC correlation is a photometric match score, not an accuracy measure. Refinement is anchored to the first frame, so a surface that changes appearance (exposure, occlusion, parallax) can pull it off. Run lk_residual_audit and held-out checks on the new fit before trusting it."});
            if flag(a, "include_rows", true)? { o["rows"] = json!(rows); } Ok(o) } },
    Command { name: "render_error_field", description: "Render a fit's error field as PNGs: predicted feature positions with magnified vectors to where the tracker measured them. Render the error field of a fit as PNGs: each selected feature is drawn at its predicted location (green dot) with a red vector to where the tracker measured it, magnified (default 3x) so sub-pixel disagreement is visible; blue quad = fitted corners. Default features: held-out only. Writes one errorfield-<frame>.png per frame plus a sidecar .json carrying the caveat. This visualises disagreement between a fit and tracked features; it is NOT independently verified truth, and a feature that drifted shows up as an error in the fit.", mutates: false,
        schema: obj(vec![("fit_id", s("default newest")), ("out_dir", s("directory to write into (created)")), ("frames", json!({"type":"array","items":{"type":"number"},"description":"relative frames, default first / one third / two thirds / last"})), ("magnify", n("vector magnification 1..50, default 3")), ("set", json!({"type":"string","enum":["heldout","train","all"],"description":"default heldout"}))], &["out_dir"]),
        run: |ss, a| { keys(a, &["fit_id", "out_dir", "frames", "magnify", "set"])?; let t0 = Instant::now(); let dir = str_arg(a, "out_dir")?.trim_end_matches('/').to_string(); if dir.is_empty() { return Err("'out_dir' is empty".into()); }
            let m = lock_f(); let fit_id = latest(&m, a, "fit_id", "fit")?; let f = &m[&fit_id]; let t = lock_t(); let ts = t.get(&f.track_id).ok_or("the fit's track set no longer exists")?; let n = ts.tr.len();
            let frames = frame_list(a, "frames", n)?.unwrap_or_else(|| { let mut v = vec![0, n / 3, 2 * n / 3, n - 1]; v.dedup(); v }); if frames.is_empty() { return Err("'frames' is empty".into()); } let mg = num(a, "magnify", 3.0, 1.0, 50.0)?; let set = choice(a, "set", "heldout", &["heldout", "train", "all"])?;
            let c = media_clip(ss, &ts.clip_id)?; let (w, h) = (ts.w, ts.h); if (ss.project.width as usize, ss.project.height as usize) != (w, h) { return Err("project size changed since tracking; re-run track_features".into()); }
            std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create out_dir: {e}"))?; let Session { project, cache, .. } = ss; let mut files = vec![];
            for &i in &frames { crate::guard::check(); let g = crate::tracking::clip_gray_pub(project, cache, &c, ts.first + i)?; let mut fr = base_frame(&g, w, h); let (mut shown, mut lens) = (0, vec![]);
                for j in 0..ts.p0.len() { let hold = j % ts.hold_every == 0; if !ts.alive[i][j] || (set == "heldout" && !hold) || (set == "train" && hold) { continue; } let Some(pr) = apply(&f.h[i], ts.p0[j]) else { continue }; let q = ts.tr[i][j]; lens.push(dist(pr, q));
                    arrow(&mut fr, pr, (pr.0 + mg * (q.0 - pr.0), pr.1 + mg * (q.1 - pr.1)), [1.0, 0.0, 0.0]); disc(&mut fr, pr, 2, [0.0, 1.0, 0.0]); shown += 1; }
                if let Some(cq) = corners_at(&f.h[i], &f.corners0) { for k in 0..4 { line(&mut fr, cq[k], cq[(k + 1) % 4], [0.2, 0.6, 1.0]); } }
                let msg = format!("{} feature residual vectors x{} (not ground truth)", set, mg); banner(&mut fr, &msg);
                let path = format!("{dir}/errorfield-{}.png", ts.first + i); crate::render::write_image(&fr, &path)?;
                let side = format!("{dir}/errorfield-{}.json", ts.first + i); let meta = json!({"fit_id": fit_id, "frame": ts.first + i, "features_drawn": shown, "magnify": mg, "set": set, "residual_px": stats(&lens), "legend": "green dot = fit-predicted feature location; red vector = (tracked - predicted) x magnify; blue quad = fitted corners", "caveat": CAVEAT_HELDOUT}); std::fs::write(&side, serde_json::to_string_pretty(&meta).unwrap()).map_err(|e| format!("cannot write {side}: {e}"))?;
                files.push(json!({"frame": ts.first + i, "png": path, "meta": side, "features_drawn": shown, "residual_px": stats(&lens)})); }
            Ok(json!({"fit_id": fit_id, "files": files, "seconds": t0.elapsed().as_secs_f64(), "legend": "green dot = predicted, red vector = tracked minus predicted, magnified; blue quad = fitted corners", "caveat": CAVEAT_HELDOUT})) } },
] }
