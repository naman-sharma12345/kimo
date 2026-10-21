//! Motion tracking and stabilization by block matching on decoded frames.
use crate::render::{prepare, Cache};
use crate::model::*;

fn gray(frame: &[u8], w: usize, h: usize) -> Vec<f32> { (0..w*h).map(|i| 0.299*frame[i*4] as f32 + 0.587*frame[i*4+1] as f32 + 0.114*frame[i*4+2] as f32).collect() }

/// Find where `tpl` (size*size, centred at (cx,cy) in prev) moved in `cur`, within +-range. Returns (x,y,score) of centre.
fn match_block(prev: &[f32], cur: &[f32], w: usize, h: usize, cx: f32, cy: f32, size: i32, range: i32) -> (f32, f32, f32) {
    let r = size / 2; let (ix, iy) = (cx.round() as i32, cy.round() as i32);
    let mut best = (f32::MAX, 0, 0); let mut grid = std::collections::HashMap::new();
    for dy in -range..=range { for dx in -range..=range {
        let mut sad = 0f32; let mut ok = true;
        'o: for y in (-r..r).step_by(2) { for x in (-r..r).step_by(2) {
            let (px, py, qx, qy) = (ix + x, iy + y, ix + x + dx, iy + y + dy);
            if px < 0 || py < 0 || qx < 0 || qy < 0 || px >= w as i32 || qx >= w as i32 || py >= h as i32 || qy >= h as i32 { ok = false; break 'o; }
            sad += (prev[py as usize*w+px as usize] - cur[qy as usize*w+qx as usize]).abs(); } }
        if !ok { continue; } grid.insert((dx, dy), sad); if sad < best.0 { best = (sad, dx, dy); } } }
    let (s, dx, dy) = best; let g = |a: i32, b: i32| grid.get(&(a, b)).copied().unwrap_or(s);
    let sub = |m: f32, c: f32, p: f32| { let d = m - 2.0*c + p; if d.abs() < 1e-6 { 0.0 } else { 0.5*(m - p)/d } };
    let (fx, fy) = (sub(g(dx-1, dy), s, g(dx+1, dy)), sub(g(dx, dy-1), s, g(dx, dy+1)));
    (cx + dx as f32 + fx.clamp(-0.5, 0.5), cy + dy as f32 + fy.clamp(-0.5, 0.5), s / ((size/2) * (size/2)) as f32 / 4.0)
}

fn clip_gray(p: &Project, cache: &mut Cache, c: &Clip, i: usize) -> Result<Vec<f32>, String> {
    let t = c.start + i as f64 / p.fps + 1e-4; crate::render::prepare_range(p, cache, t, t)?;
    let fr = cache.frames_of(&c.id).ok_or_else(|| if matches!(c.source, Source::Media { .. }) { format!("clip {} has no decoded frame at {t:.4} s (frame {i} of a {:.4} s clip from {:.4} s); is it hidden, or past its end?", c.id, c.duration, c.start) } else { format!("clip {} is not a media clip", c.id) })?; let gi = if c.retimed() { c.source_index(i as f64 / p.fps, p.fps) } else { i }; let b = fr.get(gi).ok_or("no frame")?; Ok(gray(b, p.width as usize, p.height as usize))
}
pub fn clip_gray_pub(p: &Project, cache: &mut Cache, c: &Clip, i: usize) -> Result<Vec<f32>, String> { clip_gray(p, cache, c, i) }
/// Frames of a clip that tracking reads: frame i is sampled at clip time i/fps + 1e-4 (`clip_gray`), so it exists while that
/// sample is inside the clip. A duration a hair over a whole frame count (1.266667 s = 38.00001 frames, 1.3334 s = 40.002
/// at 30 fps) is that count, not one more: the extra frame's sample would land past the clip's end with nothing to decode.
pub fn clip_frame_count(duration: f64, fps: f64) -> usize { (((duration - 1e-4) * fps).ceil() as usize).max(1) }
fn nframes(p: &Project, c: &Clip) -> usize { clip_frame_count(c.duration, p.fps) }

/// Similarity transform (scale, rotation, translation) mapping points a -> b, least squares (Umeyama) with iterative outlier rejection.
/// Returns (s, theta_rad, tx, ty) in pixel coords with q = s*R(theta)*p + t.
pub fn fit_similarity(a: &[(f64, f64)], b: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut keep: Vec<bool> = vec![true; a.len()]; let mut res = (1.0, 0.0, 0.0, 0.0);
    for _ in 0..4 {
        let idx: Vec<usize> = (0..a.len()).filter(|&i| keep[i]).collect(); if idx.len() < 2 { break; }
        let n = idx.len() as f64; let (mut ax, mut ay, mut bx, mut by) = (0.0, 0.0, 0.0, 0.0); for &i in &idx { ax += a[i].0; ay += a[i].1; bx += b[i].0; by += b[i].1; } ax /= n; ay /= n; bx /= n; by /= n;
        let (mut sxx, mut sxy, mut var) = (0.0, 0.0, 0.0); for &i in &idx { let (px, py, qx, qy) = (a[i].0 - ax, a[i].1 - ay, b[i].0 - bx, b[i].1 - by); sxx += px * qx + py * qy; sxy += px * qy - py * qx; var += px * px + py * py; }
        let th = sxy.atan2(sxx); let sc = if var > 1e-9 { (sxx * sxx + sxy * sxy).sqrt() / var } else { 1.0 }; let (c, s_) = (th.cos(), th.sin());
        let (tx, ty) = (bx - sc * (c * ax - s_ * ay), by - sc * (s_ * ax + c * ay)); res = (sc, th, tx, ty);
        let errs: Vec<f64> = (0..a.len()).map(|i| { let (px, py) = (sc * (c * a[i].0 - s_ * a[i].1) + tx, sc * (s_ * a[i].0 + c * a[i].1) + ty); ((px - b[i].0).powi(2) + (py - b[i].1).powi(2)).sqrt() }).collect();
        let mut sorted: Vec<f64> = idx.iter().map(|&i| errs[i]).collect(); sorted.sort_by(|x, y| x.partial_cmp(y).unwrap()); let med = sorted[sorted.len() / 2]; let lim = (med * 2.5).max(0.75);
        for i in 0..a.len() { keep[i] = errs[i] <= lim; }
    }
    res
}

pub struct TrackSample { pub t: f64, pub x: f64, pub y: f64, pub rot_deg: f64, pub scale: f64 }

/// Track a surface point (normalized) through a media clip using a ring of patches around it (translation + rotation + scale).
/// `rigid` = false tracks translation only (single patch). Memory is windowed: only ~24 frames are decoded at a time.
pub fn track_point(p: &Project, clip_id: &str, nx: f64, ny: f64, size: i32, range: i32, rigid: bool, radius: f64) -> Result<Vec<TrackSample>, String> {
    let c = p.find_clip(clip_id).ok_or("no such clip")?.clone(); let mut cache = Cache::new(); let (w, h) = (p.width as usize, p.height as usize); let n = nframes(p, &c);
    let (cx0, cy0) = (nx * w as f64, ny * h as f64); let r = radius * w.min(h) as f64;
    let offs: Vec<(f64, f64)> = if rigid { vec![(0.0, 0.0), (r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r), (r * 0.7, r * 0.7), (-r * 0.7, -r * 0.7), (r * 0.7, -r * 0.7), (-r * 0.7, r * 0.7)] } else { vec![(0.0, 0.0)] };
    let mut cur_pts: Vec<(f64, f64)> = offs.iter().map(|o| (cx0 + o.0, cy0 + o.1)).collect(); let ref_pts = cur_pts.clone();
    let mut prev = clip_gray(p, &mut cache, &c, 0)?; let mut out = vec![TrackSample { t: 0.0, x: cx0, y: cy0, rot_deg: 0.0, scale: 1.0 }];
    for i in 1..n { let cur = clip_gray(p, &mut cache, &c, i)?;
        for q in cur_pts.iter_mut() { let (x, y, _) = match_block(&prev, &cur, w, h, q.0 as f32, q.1 as f32, size, range); *q = (x as f64, y as f64); }
        // similarity of the whole patch ring relative to the first frame; refit from the origin each frame to avoid drift accumulation
        let (sc, th, tx, ty) = fit_similarity(&ref_pts, &cur_pts); let o = (sc * (th.cos() * cx0 - th.sin() * cy0) + tx, sc * (th.sin() * cx0 + th.cos() * cy0) + ty);
        out.push(TrackSample { t: i as f64 / p.fps, x: o.0, y: o.1, rot_deg: th.to_degrees(), scale: sc }); prev = cur; }
    Ok(out)
}

pub struct StabSample { pub t: f64, pub dx: f64, pub dy: f64, pub rot_deg: f64, pub scale: f64 }

fn gauss_smooth(v: &[f64], sigma: f64) -> Vec<f64> { if sigma < 0.3 { return v.to_vec(); } let r = (sigma * 3.0).ceil() as i64; let n = v.len() as i64;
    (0..n).map(|i| { let (mut s, mut ws) = (0.0, 0.0); for d in -r..=r { let j = (i + d).clamp(0, n - 1); let wgt = (-(d * d) as f64 / (2.0 * sigma * sigma)).exp(); s += v[j as usize] * wgt; ws += wgt; } s / ws }).collect() }

/// Two-pass stabilizer. Pass 1: estimate frame-to-frame similarity transforms from a grid of patches (robust fit), accumulate the camera path.
/// Pass 2: smooth the path (Gaussian, `smooth` = sigma in frames; `lock` = 1 holds the camera still), derive per-frame correction
/// (translation, rotation, scale) and the adaptive zoom needed to hide borders.
pub fn stabilize(p: &Project, clip_id: &str, smooth: f64, lock: bool, rotation: bool) -> Result<Vec<StabSample>, String> {
    let c = p.find_clip(clip_id).ok_or("no such clip")?.clone(); let mut cache = Cache::new(); let (w, h) = (p.width as usize, p.height as usize); let n = nframes(p, &c);
    let (gx, gy) = (6usize, 4usize); let mut pts = vec![]; for j in 0..gy { for i in 0..gx { pts.push(((i as f64 + 0.5) / gx as f64 * w as f64, (j as f64 + 0.5) / gy as f64 * h as f64)); } }
    let (ccx, ccy) = (w as f64 / 2.0, h as f64 / 2.0);
    // camera path as per-frame (log scale, angle, tx, ty) of content motion about the image center
    let mut ls = vec![0f64; n]; let mut an = vec![0f64; n]; let mut tx = vec![0f64; n]; let mut ty = vec![0f64; n];
    let mut prev = clip_gray(p, &mut cache, &c, 0)?;
    for i in 1..n { let cur = clip_gray(p, &mut cache, &c, i)?; let mut a = vec![]; let mut b = vec![];
        for &(px, py) in &pts { let (x, y, sc) = match_block(&prev, &cur, w, h, px as f32, py as f32, 40, 14); if sc < 40.0 { a.push((px - ccx, py - ccy)); b.push((x as f64 - ccx, y as f64 - ccy)); } }
        let (sc, th, dx, dy) = if a.len() >= 4 { fit_similarity(&a, &b) } else { (1.0, 0.0, 0.0, 0.0) };
        let (sc, th) = if rotation { (sc, th) } else { (1.0, 0.0) };
        // compose: path_i = step ∘ path_{i-1} (about image center)
        let (ps, pa, ptx, pty) = (ls[i - 1].exp(), an[i - 1], tx[i - 1], ty[i - 1]);
        let (c_, s_) = (th.cos(), th.sin()); ls[i] = (ps * sc).ln(); an[i] = pa + th; tx[i] = sc * (c_ * ptx - s_ * pty) + dx; ty[i] = sc * (s_ * ptx + c_ * pty) + dy; prev = cur; }
    let (sl, sa, stx, sty) = if lock { (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]) } else { (gauss_smooth(&ls, smooth), gauss_smooth(&an, smooth), gauss_smooth(&tx, smooth), gauss_smooth(&ty, smooth)) };
    // correction per frame: take frame i from the actual path to the smoothed path (small-angle decomposition)
    let mut out: Vec<StabSample> = (0..n).map(|i| StabSample { t: i as f64 / p.fps, dx: stx[i] - tx[i], dy: sty[i] - ty[i], rot_deg: (sa[i] - an[i]).to_degrees(), scale: (sl[i] - ls[i]).exp() }).collect();
    // adaptive zoom: smallest scale k so the corrected frame still covers the canvas at every frame
    let mut k: Vec<f64> = out.iter().map(|s| { let (th, sc) = (s.rot_deg.to_radians(), s.scale); let (c_, s_) = (th.cos(), th.sin()); let mut kk = 1f64;
        for &(cx, cy) in &[(-ccx, -ccy), (ccx, -ccy), (ccx, ccy), (-ccx, ccy)] { let (ux, uy) = (cx - s.dx, cy - s.dy); let (fx, fy) = ((c_ * ux + s_ * uy) / sc, (-s_ * ux + c_ * uy) / sc); kk = kk.max(fx.abs() / ccx).max(fy.abs() / ccy); } kk }).collect();
    let r = (smooth * 1.5).ceil() as usize + 2; let mx: Vec<f64> = (0..n).map(|i| k[i.saturating_sub(r)..(i + r + 1).min(n)].iter().cloned().fold(1.0, f64::max)).collect(); k = gauss_smooth(&mx, smooth.max(2.0) * 0.5);
    for i in 0..n { out[i].scale *= k[i].max(1.0) * 1.003; } Ok(out)
}
