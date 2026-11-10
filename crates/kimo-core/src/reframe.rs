//! Auto reframe (Premiere Pro's Auto Reframe). A media clip whose aspect differs from the canvas (a 16:9 shot in a 9:16 short)
//! normally decodes fitted (letterboxed) or centre-cropped. With a reframe window the clip decodes cover-scaled at full
//! resolution and a canvas-shaped window is cut from it per frame; `reframe_x` / `reframe_y` (0 = left/top edge, 1 = right/bottom,
//! 0.5 = centre) say where, keyed in SOURCE seconds (the media file's own clock: in_point + decoded frame j / fps x speed), so the
//! window stays on the content through split, trim, slip, speed changes, reverse, freeze and time remap.
//!
//! `analyze` finds the subject: per sampled frame a saliency map (frequency-tuned colour uniqueness, centre-surround contrast,
//! a skin-tone prior for people, and frame-difference motion with the global level removed), collapsed to a column (or row)
//! profile; the window that holds the most of it is chosen, its position refined to the saliency centroid inside it, and the
//! path smoothed like a camera operator (median then Gaussian; `static` holds one weighted position).
use crate::model::*;
use serde_json::{json, Value};

pub const MOTIONS: &[&str] = &["static", "slow", "default", "fast"];
pub const PROPS: [&str; 2] = ["reframe_x", "reframe_y"];
const AW: usize = 256; // analysis long side, px

/// True when the clip carries a reframe window.
/// Props keyed in source seconds (left alone when an edit moves the clip's own clock).
pub fn source_keyed(k: &str) -> bool { PROPS.contains(&k) }
pub fn active(c: &Clip) -> bool { PROPS.iter().any(|k| c.props.contains_key(*k)) }

/// Source seconds of the media clock position `m` (seconds of decoded frames from the clip's in point): decode runs at speed 1
/// for retimed clips (frames are source frames) and at the clip speed otherwise.
pub fn source_time(c: &Clip, m: f64) -> f64 { c.in_point + m * if c.retimed() { 1.0 } else { c.speed } }

/// Window position (each 0..1 of the overflow) for decoded media frame `j`.
pub fn pos(c: &Clip, j: usize, fps: f64) -> (f64, f64) {
    let t = source_time(c, j as f64 / fps.max(1e-9));
    let g = |k: &str| c.props.get(k).map(|p| p.at(t)).filter(|v| v.is_finite()).unwrap_or(0.5).clamp(0.0, 1.0);
    (g("reframe_x"), g("reframe_y"))
}

/// Cache-signature fragment: changes whenever the window path does.
pub fn sig(c: &Clip) -> String { PROPS.iter().map(|k| c.props.get(*k).map(|p| serde_json::to_string(p).unwrap_or_default()).unwrap_or_default()).collect::<Vec<_>>().join("|") }

/// Piecewise-linear ffmpeg expression in the frame number `n` (0-based in this decode window) for values v[i].
fn pw_expr(v: &[f64]) -> String {
    if v.is_empty() { return "0.5".into(); }
    // breakpoints: keep i where the run stops being linear (tolerance 1e-5)
    let mut bp = vec![0usize];
    let mut i = 0usize;
    while i + 1 < v.len() {
        let mut j = i + 1;
        while j + 1 < v.len() { let k = j + 1; let pred = v[i] + (v[k] - v[i]) * ((j - i) as f64 / (k - i) as f64); if (pred - v[j]).abs() > 1e-5 { break; }
            // every interior point of i..k must stay on the chord
            let ok = (i + 1..k).all(|m| (v[i] + (v[k] - v[i]) * ((m - i) as f64 / (k - i) as f64) - v[m]).abs() <= 1e-5); if !ok { break; } j = k; }
        bp.push(j); i = j;
    }
    let f = |x: f64| format!("{:.6}", x);
    let mut e = f(v[*bp.last().unwrap()]);
    for w in bp.windows(2).rev() { let (a, b) = (w[0], w[1]); let seg = if b == a { f(v[a]) } else { format!("{}+({})*(n-{a})/{}", f(v[a]), f(v[b] - v[a]), b - a) }; e = format!("if(lt(n,{b}),{seg},{e})"); }
    e
}

/// The ffmpeg filter that replaces the fit step for a reframed clip decoding frames a..a+n: cover-scale, RGBA, then the window.
pub fn ffmpeg_fit(c: &Clip, w: u32, h: u32, fps: f64, a: usize, n: usize) -> String {
    let (xs, ys): (Vec<f64>, Vec<f64>) = (0..n.max(1)).map(|i| pos(c, a + i, fps)).unzip();
    format!("scale={w}:{h}:force_original_aspect_ratio=increase:flags=bicubic,format=rgba,crop={w}:{h}:'(iw-ow)*({})':'(ih-oh)*({})'", pw_expr(&xs), pw_expr(&ys))
}

/// Cut the canvas-aspect window at (rx, ry) out of a full-resolution frame (native/linear decode path); the caller fits it.
pub fn crop_frame(src: &crate::effects::Frame, cw: usize, ch: usize, rx: f64, ry: f64) -> crate::effects::Frame {
    let (sw, sh) = (src.w as f64, src.h as f64); let a = cw as f64 / ch as f64;
    let (ww, wh) = if sw / sh > a { ((sh * a).round().max(1.0).min(sw), sh) } else { (sw, (sw / a).round().max(1.0).min(sh)) };
    let (ww, wh) = (ww as usize, wh as usize);
    let x0 = ((src.w - ww) as f64 * rx).round() as usize; let y0 = ((src.h - wh) as f64 * ry).round() as usize;
    let mut d = vec![0f32; ww * wh * 4];
    for y in 0..wh { let s = ((y0 + y) * src.w + x0) * 4; d[y * ww * 4..(y + 1) * ww * 4].copy_from_slice(&src.d[s..s + ww * 4]); }
    crate::effects::Frame { w: ww, h: wh, d }
}

fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 { return src.to_vec(); }
    let mut tmp = vec![0f32; w * h]; let mut out = vec![0f32; w * h];
    for y in 0..h { let row = &src[y * w..(y + 1) * w]; let mut pre = vec![0f64; w + 1]; for x in 0..w { pre[x + 1] = pre[x] + row[x] as f64; }
        for x in 0..w { let (lo, hi) = (x.saturating_sub(r), (x + r + 1).min(w)); tmp[y * w + x] = ((pre[hi] - pre[lo]) / (hi - lo) as f64) as f32; } }
    for x in 0..w { let mut pre = vec![0f64; h + 1]; for y in 0..h { pre[y + 1] = pre[y] + tmp[y * w + x] as f64; }
        for y in 0..h { let (lo, hi) = (y.saturating_sub(r), (y + r + 1).min(h)); out[y * w + x] = ((pre[hi] - pre[lo]) / (hi - lo) as f64) as f32; } }
    out
}
fn blur2(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> { let a = box_blur(src, w, h, r); box_blur(&a, w, h, r) }
fn norm(v: &mut [f32]) { let mut s: Vec<f32> = v.iter().copied().filter(|x| x.is_finite()).collect(); if s.is_empty() { return; } let k = ((s.len() - 1) as f64 * 0.98) as usize; let (_, p, _) = s.select_nth_unstable_by(k, |a, b| a.total_cmp(b)); let mut q = *p; if q <= 1e-6 { q = v.iter().copied().fold(0f32, f32::max); } if q <= 1e-6 { v.iter_mut().for_each(|x| *x = 0.0); return; } for x in v.iter_mut() { *x = (*x / q).clamp(0.0, 1.0); } }
fn median_of(v: &mut Vec<f32>) -> f32 { if v.is_empty() { return 0.0; } let k = v.len() / 2; let (_, m, _) = v.select_nth_unstable_by(k, |a, b| a.total_cmp(b)); *m }

/// Per-frame saliency of rgb24 frames (w x h). Returns one map per frame.
pub fn saliency(frames: &[Vec<u8>], w: usize, h: usize) -> Vec<Vec<f32>> {
    let n = w * h; let rs = (h.min(w) / 8).max(2);
    let mut prev_l: Option<Vec<f32>> = None; let mut out: Vec<Vec<f32>> = Vec::with_capacity(frames.len()); let mut motions: Vec<Option<Vec<f32>>> = vec![];
    for f in frames {
        let (mut l, mut ca, mut cb, mut skin) = (vec![0f32; n], vec![0f32; n], vec![0f32; n], vec![0f32; n]);
        for i in 0..n { let (r, g, b) = (f[i * 3] as f32, f[i * 3 + 1] as f32, f[i * 3 + 2] as f32);
            l[i] = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0; ca[i] = (r - g) / 255.0; cb[i] = (0.5 * (r + g) - b) / 255.0;
            let (y, ucb, ucr) = (0.299 * r + 0.587 * g + 0.114 * b, 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b, 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b);
            skin[i] = if (77.0..=127.0).contains(&ucb) && (133.0..=173.0).contains(&ucr) && y > 40.0 { 1.0 } else { 0.0 }; }
        let (ls, as_, bs) = (blur2(&l, w, h, 1), blur2(&ca, w, h, 1), blur2(&cb, w, h, 1));
        let (ll, al, bl) = (blur2(&l, w, h, rs), blur2(&ca, w, h, rs), blur2(&cb, w, h, rs));
        let mean = |v: &[f32]| v.iter().map(|x| *x as f64).sum::<f64>() / n.max(1) as f64;
        let (lm, am, bm) = (mean(&ls) as f32, mean(&as_) as f32, mean(&bs) as f32);
        let mut ft: Vec<f32> = (0..n).map(|i| ((ls[i] - lm).powi(2) + (as_[i] - am).powi(2) + (bs[i] - bm).powi(2)).sqrt()).collect();
        let mut cs: Vec<f32> = (0..n).map(|i| ((ls[i] - ll[i]).powi(2) + (as_[i] - al[i]).powi(2) + (bs[i] - bl[i]).powi(2)).sqrt()).collect();
        norm(&mut ft); norm(&mut cs); let skin = blur2(&skin, w, h, 2);
        let motion = prev_l.as_ref().map(|p| { let mut m: Vec<f32> = (0..n).map(|i| (ls[i] - p[i]).abs()).collect(); let mut s = m.clone(); let med = median_of(&mut s); m.iter_mut().for_each(|x| *x = (*x - med).max(0.0)); let mut m = blur2(&m, w, h, 2); norm(&mut m); m });
        prev_l = Some(ls);
        let base: Vec<f32> = (0..n).map(|i| (0.5 * ft[i] + 0.5 * cs[i]) * (1.0 + 1.5 * skin[i])).collect();
        out.push(base); motions.push(motion);
    }
    if motions.len() > 1 && motions[0].is_none() { motions[0] = motions[1].clone(); }
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    for (s, m) in out.iter_mut().zip(motions) {
        for y in 0..h { for x in 0..w { let i = y * w + x; let mv = m.as_ref().map_or(0.0, |m| m[i]);
            let pr = 0.85 + 0.15 * (-(((x as f32 - cx) / (0.5 * w as f32)).powi(2) + ((y as f32 - cy) / (0.5 * h as f32)).powi(2))).exp();
            s[i] = (s[i] + 0.8 * mv) * pr; } }
    }
    out
}

fn median_filter(v: &[f64], r: usize) -> Vec<f64> { (0..v.len()).map(|i| { let mut w: Vec<f64> = v[i.saturating_sub(r)..(i + r + 1).min(v.len())].to_vec(); w.sort_by(|a, b| a.total_cmp(b)); w[w.len() / 2] }).collect() }
/// Ramer-Douglas-Peucker on (t, v), keeping the ends.
pub fn rdp(p: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    if p.len() < 3 { return p.to_vec(); }
    let (a, b) = (p[0], p[p.len() - 1]); let mut best = (0usize, 0f64);
    for (i, q) in p.iter().enumerate().skip(1).take(p.len() - 2) { let u = if b.0 > a.0 { (q.0 - a.0) / (b.0 - a.0) } else { 0.0 }; let d = (q.1 - (a.1 + (b.1 - a.1) * u)).abs(); if d > best.1 { best = (i, d); } }
    if best.1 <= tol { return vec![a, b]; }
    let mut l = rdp(&p[..=best.0], tol); let r = rdp(&p[best.0..], tol); l.pop(); l.extend(r); l
}

pub struct Plan { pub axis_x: bool, pub keys: Vec<(f64, f64)>, pub report: Value }

/// The media clock span of a clip in seconds (frames the renderer decodes / fps).
fn media_span(c: &Clip, fps: f64) -> f64 { let span = if c.time_remap.is_some() { c.remap_span(0., c.duration).1 + 1. / fps } else if c.speed_curve.is_some() { c.source_time(c.duration) } else { c.duration }; span.max(1.0 / fps) }

/// Analyse a media clip and plan its reframe window path (does not change the project).
pub fn analyze(p: &Project, clip_id: &str, motion: &str) -> Result<Plan, String> {
    let t0 = std::time::Instant::now();
    if !MOTIONS.contains(&motion) { return Err(format!("unknown motion '{motion}' (valid: {})", MOTIONS.join(", "))); }
    let c = p.find_clip(clip_id).ok_or_else(|| format!("no such clip {clip_id}"))?;
    let Source::Media { asset } = &c.source else { return Err(format!("auto_reframe needs a media (video or image) clip: '{clip_id}' is not one")); };
    let a = p.assets.get(asset).ok_or(format!("asset {asset} missing"))?; let path = p.resolve(&a.path);
    let (sw, sh, _, _) = crate::render::probe(&path)?; if sw == 0 || sh == 0 { return Err(format!("clip '{clip_id}' has no video frames to reframe")); }
    let (pw, ph) = (p.width as f64, p.height as f64); let (ca, sa) = (pw / ph, sw as f64 / sh as f64);
    if (sa / ca - 1.0).abs() < 0.01 { return Err(format!("clip '{clip_id}' is {sw}x{sh}, already the canvas aspect ({}x{}): there is nothing to reframe", p.width, p.height)); }
    let axis_x = sa > ca;
    let (aw, ah) = if sa >= 1.0 { (AW, ((AW as f64 / sa).round() as usize).max(8)) } else { (((AW as f64 * sa).round() as usize).max(8), AW) };
    let still = crate::render::is_still_path(&path);
    let af = p.fps.min(10.0).max(1.0); let span = media_span(c, p.fps); let nf = if still { 1 } else { ((span * af).ceil() as usize).max(1) };
    let speed = if c.retimed() { 1.0 } else { c.speed.max(0.01) };
    let vf = format!("setpts=PTS/{speed},fps={af},scale={aw}:{ah}:flags=area,format=rgb24");
    let mut cmd = std::process::Command::new("ffmpeg"); cmd.args(["-v", "error"]); if still { cmd.args(["-loop", "1"]); }
    cmd.args(["-ss", &format!("{}", c.in_point), "-t", &format!("{}", if still { 1.0 } else { span * speed + 1.0 / af }), "-i", &path, "-vf", &vf, "-frames:v", &nf.to_string(), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]);
    let fsz = aw * ah * 3; let buf = crate::color::process::capture(&mut cmd, fsz * nf + 16, std::time::Duration::from_secs(120 + nf as u64 / 10))?;
    let frames: Vec<Vec<u8>> = buf.chunks_exact(fsz).map(|c| c.to_vec()).collect();
    if frames.is_empty() { return Err(format!("could not decode any frame of clip '{clip_id}' for analysis")); }
    let sal = saliency(&frames, aw, ah);
    let len = if axis_x { aw } else { ah };
    let win = if axis_x { (ah as f64 * ca).min(aw as f64) } else { (aw as f64 / ca).min(ah as f64) };
    let ov = (len as f64 - win).max(1e-6); let wl = win.round() as usize;
    let mut raw = vec![]; let mut conf = vec![]; let mut profiles = vec![];
    for s in &sal {
        let mut prof = vec![0f64; len];
        for y in 0..ah { for x in 0..aw { let v = s[y * aw + x] as f64; prof[if axis_x { x } else { y }] += v * v; } }
        let mut pre = vec![0f64; len + 1]; for i in 0..len { pre[i + 1] = pre[i] + prof[i]; }
        let total = pre[len];
        let centre = (len - wl) as f64 * 0.5;
        let mut best = (0usize, f64::MIN);
        for o in 0..=(len - wl) { let sc = (pre[o + wl] - pre[o]) * (1.0 - 1e-3 * ((o as f64 - centre).abs() / ov)); if sc > best.1 { best = (o, sc); } }
        let o = best.0; let inside = pre[o + wl] - pre[o];
        let r = if total <= 1e-9 { raw.last().copied().unwrap_or(0.5) } else {
            let com = (o..o + wl).map(|i| prof[i] * (i as f64 + 0.5)).sum::<f64>() / inside.max(1e-12);
            ((com - win * 0.5) / ov).clamp(0.0, 1.0) };
        raw.push(r); conf.push(if total > 1e-9 { inside / total } else { 0.0 }); profiles.push(pre);
    }
    let smooth: Vec<f64> = match motion {
        "static" => { let ws: f64 = conf.iter().sum::<f64>().max(1e-9); let m = raw.iter().zip(&conf).map(|(r, w)| r * w).sum::<f64>() / ws; vec![m; raw.len()] }
        m => { let (mr, sig_s) = match m { "slow" => (3, 1.0), "fast" => (1, 0.2), _ => (2, 0.5) }; let md = median_filter(&raw, mr); crate::stabilize::gauss_odd(&md, sig_s * af).iter().map(|v| v.clamp(0.0, 1.0)).collect() }
    };
    let kept = |r: f64, pre: &Vec<f64>| { let o = ((r * ov).round() as usize).min(len - wl); let t = pre[len]; if t <= 1e-9 { 1.0 } else { (pre[o + wl] - pre[o]) / t } };
    let n = smooth.len() as f64;
    let mean_kept = smooth.iter().zip(&profiles).map(|(r, p)| kept(*r, p)).sum::<f64>() / n;
    let centre_kept = profiles.iter().map(|p| kept(0.5, p)).sum::<f64>() / n;
    let worst = smooth.iter().zip(&profiles).map(|(r, p)| kept(*r, p)).fold(1.0, f64::min);
    let pts: Vec<(f64, f64)> = smooth.iter().enumerate().map(|(i, v)| (source_time(c, i as f64 / af), *v)).collect();
    let keys = if motion == "static" || still { vec![(source_time(c, 0.0), smooth[0])] } else { rdp(&pts, 0.002) };
    let (lo, hi) = smooth.iter().fold((1f64, 0f64), |(a, b), v| (a.min(*v), b.max(*v)));
    let src_win = if axis_x { [(sh as f64 * ca).round(), sh as f64] } else { [sw as f64, (sw as f64 / ca).round()] };
    let report = json!({"clip_id": clip_id, "axis": if axis_x { "x" } else { "y" }, "motion": motion, "source": [sw, sh], "window_px": src_win,
        "keys": keys.len(), "pan_range": [(lo * 1000.0).round() / 1000.0, (hi * 1000.0).round() / 1000.0],
        "subject_kept": (mean_kept * 1000.0).round() / 1000.0, "subject_kept_worst": (worst * 1000.0).round() / 1000.0, "centre_crop_kept": (centre_kept * 1000.0).round() / 1000.0,
        "analysed_frames": frames.len(), "ms": t0.elapsed().as_millis() as u64});
    Ok(Plan { axis_x, keys, report })
}

/// Write a plan (or a fixed position) onto the clip: the analysed axis gets the keys, the other axis is centred.
pub fn write(c: &mut Clip, axis_x: bool, keys: &[(f64, f64)]) {
    use crate::keyframe::{Ease, Prop};
    let mut pr = Prop::new(keys.first().map_or(0.5, |k| k.1));
    if keys.len() > 1 { for (t, v) in keys { pr.set_key(*t, *v, Ease::Linear); } }
    let (on, off) = if axis_x { ("reframe_x", "reframe_y") } else { ("reframe_y", "reframe_x") };
    c.props.insert(on.into(), pr); c.props.insert(off.into(), Prop::new(0.5));
}
