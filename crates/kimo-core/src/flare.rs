//! lens_flare mode 1 (v2): elements on the source -> frame-centre axis, P(u) = S + (C - S) * u (u = 0 source, 1 centre, 2 mirrored).
//! Source S in canvas px (pixel (i, j) sits at (i, j)), resolved exactly at render time t (no smoothing), first match wins:
//! 1. strings.source_clip = id of a clip moved by track_point (or keyed by hand): S = (x*w + X(lt) - X.base, y*h + Y(lt) - Y.base), X/Y that clip's
//!    x/y props at its local time lt = t - start. track_point keys X = base + tracked_px - x0*w, so with x/y = the tracked start point S is the tracked px.
//! 2. strings.source_light = id of a project light (set_light, point/spot): S = (pos.x*w, pos.y*h) of lighting::eval_light(light, t) (the lighting
//!    model puts lights above the frame plane, x/y in frame fractions, viewed straight down, so the screen position is the orthographic drop).
//! 3. params x, y (0..1): S = (x*w, y*h).
//! Occlusion: strings.occluder = clip id rendered alone at t; visible = 1 - occluded fraction of the source disc area (see `flare_visibility`); every element * visible.
//! The renderer injects the resolved S and visible as params _src_x/_src_y (px) and _visible on a copy of the effect (render::resolve_flares), so no statics.
//! Elements (B = brightness * visible, md = min(w, h), d = distance to the element centre, each 0 intensity skips it):
//! halo  : at S, r = halo_size*md: [1,.9,.7]*halo*B*exp(-2d^2/r^2) + white core [1,1,1]*1.2*halo*B*exp(-2d^2/(r/3)^2)
//! ring  : at S, r = ring_size*md: [1,.8,.5]*ring*B*exp(-((d/r - 1)*6)^2)
//! ghosts: k = 0..ghost_count-1 at P(ghost_spacing*(k+1)), r = ghost_size*md*(1 + 0.5*(k%3)), palette[k%5]*ghosts*B*exp(-2d^2/r^2)
//! streak: [.5,.7,1]*0.5*streak*B*exp(-3(dx/(streak_length*w))^2)*exp(-dy^2/(2*streak_width^2)), rows |dy| <= 3*streak_width+1
//! starburst: [1,.95,.85]*starburst*B*exp(-d/(ray_length*md))*cos^32(rays*(theta - ray_angle)/2), within d <= 6*ray_length*md
//! Discs/rings are evaluated inside a square of half side 1.8r+2 around the centre (as v1's disc()).
use crate::effects::{p, Frame, Params};

const PAL: [[f32; 3]; 5] = [[1.0, 0.4, 0.2], [0.3, 1.0, 0.5], [0.3, 0.5, 1.0], [1.0, 0.8, 0.3], [0.8, 0.3, 1.0]];

/// visible = 1 - (occluded area of the source disc) / (disc area). Pixel (i, j) is the square [i-.5, i+.5] x [j-.5, j+.5] carrying its alpha
/// (clamped 0..1; outside the frame alpha 0); its share of the disc of radius r px at (sx, sy) is measured on a 16x16 grid of sub-sample centres,
/// so a straight occluder edge across the disc is resolved to well under 1% of the disc area. r <= 0: 1 - alpha of the pixel under (sx, sy).
pub fn flare_visibility(alpha: &[f32], w: usize, h: usize, sx: f64, sy: f64, r: f64) -> f32 {
    if w == 0 || h == 0 || alpha.len() < w * h { return 1.0; }
    let a_at = |i: i64, j: i64| -> f64 { if i < 0 || j < 0 || i >= w as i64 || j >= h as i64 { 0.0 } else { alpha[j as usize * w + i as usize].clamp(0.0, 1.0) as f64 } };
    if !(r > 0.0) { return (1.0 - a_at((sx + 0.5).floor() as i64, (sy + 0.5).floor() as i64)) as f32; }
    const N: usize = 16; let (r2, mut cov) = (r * r, 0f64);
    for j in (sy - r - 0.5).floor() as i64..=(sy + r + 0.5).ceil() as i64 { for i in (sx - r - 0.5).floor() as i64..=(sx + r + 0.5).ceil() as i64 {
        let a = a_at(i, j); if a == 0.0 { continue; } let mut k = 0usize;
        for v in 0..N { let y = j as f64 - 0.5 + (v as f64 + 0.5) / N as f64 - sy; for u in 0..N { let x = i as f64 - 0.5 + (u as f64 + 0.5) / N as f64 - sx; if x * x + y * y <= r2 { k += 1; } } }
        cov += a * k as f64 / (N * N) as f64; } }
    (1.0 - cov / (std::f64::consts::PI * r2)).clamp(0.0, 1.0) as f32
}

/// Exact source position in canvas px at timeline time t (rules 1-3 above). Err for an unknown clip/light id or a directional light.
pub fn source_px(pj: &crate::model::Project, pr: &Params, strings: &std::collections::BTreeMap<String, String>, t: f64, lt: f64) -> Result<(f64, f64), String> {
    let (w, h) = (pj.width as f64, pj.height as f64); let (x, y) = (pr.get("x").map(|q| q.at(lt)).unwrap_or(0.75), pr.get("y").map(|q| q.at(lt)).unwrap_or(0.25));
    if let Some(id) = strings.get("source_clip") { let c = pj.tracks.iter().flat_map(|tr| tr.clips.iter()).find(|c| &c.id == id).ok_or_else(|| format!("lens_flare: no clip '{id}' for source_clip"))?; let ct = t - c.start;
        let d = |k: &str| c.props.get(k).map(|q| q.at(ct) - q.base).unwrap_or(0.0); return Ok((x * w + d("x"), y * h + d("y"))); }
    if let Some(id) = strings.get("source_light") { let l = pj.lights.get(id).ok_or_else(|| format!("lens_flare: no light '{id}' for source_light"))?; if l.kind == "directional" { return Err(format!("lens_flare: light '{id}' is directional (no position)")); }
        let l = crate::lighting::eval_light(l, t); return Ok((l.pos[0] * w, l.pos[1] * h)); }
    Ok((x * w, y * h))
}

fn disc(f: &mut Frame, cx: f32, cy: f32, r: f32, col: [f32; 3], k: f32, ring: bool) { if k == 0.0 || r <= 0.0 { return; } let (w, h) = (f.w as i32, f.h as i32); let rr = (r * 1.8) as i32 + 2; for y in (cy as i32 - rr).max(0)..(cy as i32 + rr).min(h) { for x in (cx as i32 - rr).max(0)..(cx as i32 + rr).min(w) {
    let d = (((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt()) / r; let v = if ring { (-((d - 1.0) * 6.0).powi(2)).exp() } else { (-d * d * 2.0).exp() } * k; let i = (y as usize * f.w + x as usize) * 4; for j in 0..3 { f.d[i + j] += col[j] * v; } } } }

/// lens_flare mode 1. Reads _src_x/_src_y/_visible injected by the renderer, else x/y and visible 1.
pub fn apply_v2(pr: &Params, f: &mut Frame, t: f64) {
    let g = |k: &str, d: f64| p(pr, k, d, t); let (w, h) = (f.w, f.h); let (wf, hf) = (w as f32, h as f32); let md = wf.min(hf); if w == 0 || h == 0 { return; }
    let (sx, sy) = (if pr.contains_key("_src_x") { g("_src_x", 0.0) } else { g("x", 0.75) * wf }, if pr.contains_key("_src_y") { g("_src_y", 0.0) } else { g("y", 0.25) * hf });
    let b = g("brightness", 1.0) * g("_visible", 1.0).clamp(0.0, 1.0); if b == 0.0 { return; } let (cx, cy) = (wf * 0.5, hf * 0.5);
    let (halo, hs) = (g("halo", 1.0), g("halo_size", 0.12) * md); disc(f, sx, sy, hs, [1.0, 0.9, 0.7], halo * b, false); disc(f, sx, sy, hs / 3.0, [1.0, 1.0, 1.0], 1.2 * halo * b, false);
    disc(f, sx, sy, g("ring_size", 0.25) * md, [1.0, 0.8, 0.5], g("ring", 0.25) * b, true);
    let (gi, gn, gsp, gsz) = (g("ghosts", 0.35), g("ghost_count", 5.0).round().clamp(0.0, 8.0) as usize, g("ghost_spacing", 0.4), g("ghost_size", 0.03));
    if gi != 0.0 { for k in 0..gn { let u = gsp * (k + 1) as f32; disc(f, sx + (cx - sx) * u, sy + (cy - sy) * u, gsz * md * (1.0 + 0.5 * (k % 3) as f32), PAL[k % 5], gi * b, false); } }
    let (st, sl, sw) = (g("streak", 1.0), g("streak_length", 0.45).max(1e-3) * wf, g("streak_width", 1.5).max(0.1));
    if st != 0.0 { let ry = (3.0 * sw) as i32 + 1; for yy in (sy as i32 - ry).max(0)..(sy as i32 + ry + 1).min(h as i32) { let dy = yy as f32 - sy; let vy = (-dy * dy / (2.0 * sw * sw)).exp() * 0.5 * st * b;
        for xx in 0..w { let d = (xx as f32 - sx) / sl; let v = (-3.0 * d * d).exp() * vy; let i = (yy as usize * w + xx) * 4; f.d[i] += 0.5 * v; f.d[i + 1] += 0.7 * v; f.d[i + 2] += v; } } }
    let (sb, rays, rl, ra) = (g("starburst", 0.5), g("rays", 8.0).round().max(0.0), g("ray_length", 0.2).max(1e-3) * md, g("ray_angle", 0.0).to_radians());
    if sb != 0.0 && rays > 0.0 { let rr = (6.0 * rl) as i32 + 1; for yy in (sy as i32 - rr).max(0)..(sy as i32 + rr + 1).min(h as i32) { for xx in (sx as i32 - rr).max(0)..(sx as i32 + rr + 1).min(w as i32) {
        let (dx, dy) = (xx as f32 - sx, yy as f32 - sy); let d = (dx * dx + dy * dy).sqrt(); if d > 6.0 * rl { continue; } let c = (rays * (dy.atan2(dx) - ra) / 2.0).cos(); let v = sb * b * (-d / rl).exp() * (c * c).powi(16);
        let i = (yy as usize * w + xx as usize) * 4; f.d[i] += v; f.d[i + 1] += 0.95 * v; f.d[i + 2] += 0.85 * v; } } }
}
