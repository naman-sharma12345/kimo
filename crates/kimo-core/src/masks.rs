//! Mask / selection / matte stack. A clip owns an ordered stack of MaskLayers; each layer is evaluated to a coverage buffer
//! (0..1 per pixel), post-processed (expand/choke, feather, invert, opacity) and combined with the stack below it
//! (add, subtract, intersect, xor, lighten, darken, difference). Layer kinds cover shapes (rect, ellipse, bezier path, regular
//! polygon / star, gradients) and selections (luma, color, HSL qualifier, difference matte, magic wand, scribble select, edges,
//! own alpha, procedural noise). All numeric params are keyframable `Prop`s; paths animate with per-point keyframes.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use crate::effects::Frame;
use crate::keyframe::Prop;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct PathPt { pub x: f64, pub y: f64, #[serde(default)] pub ix: f64, #[serde(default)] pub iy: f64, #[serde(default)] pub ox: f64, #[serde(default)] pub oy: f64, /// per-point feather width in px@720p (0 = use the layer feather)
    #[serde(default)] pub f: f64 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathKey { pub t: f64, pub points: Vec<PathPt>, #[serde(default)] pub hold: bool }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MaskLayer {
    pub id: String,
    #[serde(default)] pub name: String,
    pub kind: String,
    #[serde(default = "d_mode")] pub mode: String,
    #[serde(default)] pub invert: bool,
    #[serde(default = "d_true")] pub enabled: bool,
    #[serde(default)] pub props: BTreeMap<String, Prop>,
    #[serde(default)] pub points: Vec<PathPt>,
    #[serde(default = "d_true")] pub closed: bool,
    #[serde(default)] pub path_keys: Vec<PathKey>,
    #[serde(default)] pub fg: Vec<[f64; 2]>,
    #[serde(default)] pub bg: Vec<[f64; 2]>,
    #[serde(default)] pub strings: BTreeMap<String, String>,
}
fn d_mode() -> String { "add".into() } fn d_true() -> bool { true }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Matte { pub source: String, #[serde(default = "d_mm")] pub mode: String, #[serde(default)] pub hide_source: bool }
fn d_mm() -> String { "alpha".into() }

pub const KINDS: &[&str] = &["rect", "ellipse", "path", "linear", "radial", "luma", "color", "hsl", "difference", "wand", "scribble", "edge", "alpha", "noise", "matte"];
pub const MODES: &[&str] = &["add", "subtract", "intersect", "xor", "lighten", "darken", "difference", "none"];

pub fn defaults(kind: &str) -> Vec<(&'static str, f64)> {
    match kind {
        "rect" => vec![("cx", 0.5), ("cy", 0.5), ("w", 0.5), ("h", 0.5), ("rot", 0.0), ("round", 0.0)],
        "ellipse" => vec![("cx", 0.5), ("cy", 0.5), ("w", 0.5), ("h", 0.5), ("rot", 0.0)],
        "linear" => vec![("angle", 0.0), ("pos", 0.5), ("soft", 0.1)],
        "radial" => vec![("cx", 0.5), ("cy", 0.5), ("r", 0.3), ("soft", 0.2)],
        "luma" => vec![("lo", 0.5), ("hi", 1.0), ("soft", 0.1)],
        "color" => vec![("r", 0.0), ("g", 1.0), ("b", 0.0), ("tol", 0.2), ("soft", 0.1), ("unmix", 1.0)],
        "hsl" => vec![("hue", 0.0), ("hue_width", 30.0), ("s_lo", 0.2), ("s_hi", 1.0), ("l_lo", 0.0), ("l_hi", 1.0), ("soft", 0.1)],
        "difference" => vec![("tol", 0.08), ("soft", 0.06)],
        "wand" => vec![("x", 0.5), ("y", 0.5), ("tol", 0.12), ("soft", 0.05), ("contiguous", 1.0)],
        "scribble" => vec![("soft", 0.15), ("smooth", 2.0)],
        "edge" => vec![("thr", 0.15), ("soft", 0.1), ("thickness", 2.0)],
        "alpha" => vec![("thr", 0.5), ("soft", 0.1)],
        "matte" => vec![("start", 0.0), ("hold", 1.0)],
        "noise" => vec![("scale", 120.0), ("thr", 0.5), ("soft", 0.08), ("seed", 1.0), ("speed", 0.0)],
        _ => vec![],
    }
}

pub fn new_layer(id: String, kind: &str) -> MaskLayer {
    let mut props = BTreeMap::new();
    for (k, v) in defaults(kind) { props.insert(k.to_string(), Prop::new(v)); }
    for (k, v) in [("feather", 0.0), ("expand", 0.0), ("opacity", 1.0)] { props.insert(k.to_string(), Prop::new(v)); }
    MaskLayer { id, name: String::new(), kind: kind.to_string(), mode: "add".into(), invert: false, enabled: true, props, points: vec![], closed: true, path_keys: vec![], fg: vec![], bg: vec![], strings: BTreeMap::new() }
}

/// Regular polygon (sides>=3) or star (inner>0: inner radius ratio) as path points, normalized canvas coords. aspect = w/h of the canvas.
pub fn ngon(cx: f64, cy: f64, r: f64, sides: usize, inner: f64, rot_deg: f64, aspect: f64) -> Vec<PathPt> {
    let n = sides.max(3); let mut v = vec![]; let steps = if inner > 0.0 { n * 2 } else { n };
    for i in 0..steps { let rr = if inner > 0.0 && i % 2 == 1 { r * inner } else { r };
        let a = (rot_deg - 90.0).to_radians() + i as f64 / steps as f64 * std::f64::consts::TAU; v.push(PathPt { x: cx + rr * a.cos() / aspect, y: cy + rr * a.sin(), ..Default::default() }); }
    v
}

fn g(l: &MaskLayer, k: &str, d: f64, t: f64) -> f32 { l.props.get(k).map(|p| p.at(t)).unwrap_or(d) as f32 }
fn sstep(a: f32, b: f32, x: f32) -> f32 { if b <= a { return if x >= a { 1.0 } else { 0.0 }; } let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Path at time t. Default interpolation is a cardinal spline through the keys (no stop at every key); strings.interp = "smooth" (smoothstep per segment, legacy) or "linear".
pub fn path_at(l: &MaskLayer, t: f64) -> Vec<PathPt> {
    let ks = &l.path_keys; if ks.is_empty() { return l.points.clone(); }
    if t <= ks[0].t { return ks[0].points.clone(); } if t >= ks[ks.len() - 1].t { return ks[ks.len() - 1].points.clone(); }
    let i = ks.iter().position(|k| k.t > t).unwrap_or(ks.len() - 1) - 1; let (a, b) = (&ks[i], &ks[i + 1]);
    if a.hold || a.points.len() != b.points.len() { return a.points.clone(); }
    let mode = l.strings.get("interp").map(|s| s.as_str()).unwrap_or("spline"); let u = (t - a.t) / (b.t - a.t).max(1e-9);
    let comp = |p: &PathPt| [p.x, p.y, p.ix, p.iy, p.ox, p.oy, p.f];
    let prev = if i > 0 && ks[i - 1].points.len() == a.points.len() { Some(&ks[i - 1]) } else { None }; let next = if i + 2 < ks.len() && ks[i + 2].points.len() == a.points.len() { Some(&ks[i + 2]) } else { None };
    let dt = b.t - a.t; let (h00, h10, h01, h11) = { let u2 = u * u; let u3 = u2 * u; (2.0 * u3 - 3.0 * u2 + 1.0, u3 - 2.0 * u2 + u, -2.0 * u3 + 3.0 * u2, u3 - u2) };
    (0..a.points.len()).map(|n| { let (pa, pb) = (comp(&a.points[n]), comp(&b.points[n])); let mut o = [0f64; 7];
        for c in 0..7 { o[c] = match mode { "linear" => pa[c] + (pb[c] - pa[c]) * u, "smooth" => { let w = u * u * (3.0 - 2.0 * u); pa[c] + (pb[c] - pa[c]) * w }
            _ => { let ma = match prev { Some(p) => (pb[c] - comp(&p.points[n])[c]) / (b.t - p.t).max(1e-9), None => (pb[c] - pa[c]) / dt }; let mb = match next { Some(q) => (comp(&q.points[n])[c] - pa[c]) / (q.t - a.t).max(1e-9), None => (pb[c] - pa[c]) / dt };
                h00 * pa[c] + h10 * dt * ma + h01 * pb[c] + h11 * dt * mb } }; }
        PathPt { x: o[0], y: o[1], ix: o[2], iy: o[3], ox: o[4], oy: o[5], f: o[6].max(0.0) } }).collect()
}

pub fn flatten_pub(pts: &[PathPt], closed: bool, w: usize, h: usize) -> Vec<(f64, f64, f64)> { flatten(pts, closed, w, h) }
fn flatten(pts: &[PathPt], closed: bool, w: usize, h: usize) -> Vec<(f64, f64, f64)> {
    let n = pts.len(); let (wf, hf) = (w as f64, h as f64); let mut poly = vec![]; let segs = if closed { n } else { n.saturating_sub(1) };
    for i in 0..segs { let a = &pts[i]; let b = &pts[(i + 1) % n];
        let (p0, p1, p2, p3) = ((a.x * wf, a.y * hf), ((a.x + a.ox) * wf, (a.y + a.oy) * hf), ((b.x + b.ix) * wf, (b.y + b.iy) * hf), (b.x * wf, b.y * hf));
        let straight = a.ox == 0.0 && a.oy == 0.0 && b.ix == 0.0 && b.iy == 0.0; let ext = (p0.0 - p3.0).abs() + (p0.1 - p3.1).abs() + (p1.0 - p0.0).abs() + (p1.1 - p0.1).abs() + (p3.0 - p2.0).abs() + (p3.1 - p2.1).abs();
        let steps = if straight { 1 } else { ((ext / 6.0) as usize).clamp(16, 96) };
        for s in 0..steps { let u = s as f64 / steps as f64; let v = 1.0 - u;
            poly.push((v*v*v*p0.0 + 3.0*v*v*u*p1.0 + 3.0*v*u*u*p2.0 + u*u*u*p3.0, v*v*v*p0.1 + 3.0*v*v*u*p1.1 + 3.0*v*u*u*p2.1 + u*u*u*p3.1, a.f + (b.f - a.f) * u)); } }
    poly
}

/// Exact-area coverage rasterizer (signed-area accumulation): sub-pixel accurate, so moving paths do not shimmer.
fn raster_path(pts: &[PathPt], closed: bool, w: usize, h: usize) -> Vec<f32> {
    let poly = flatten(pts, closed, w, h); let m = poly.len(); let mut cov = vec![0f32; w * h]; if m < 3 { return cov; }
    let stride = w + 3; let mut acc = vec![0f32; stride * h]; let wf = w as f32;
    for i in 0..m { let (a, b) = (poly[i], poly[(i + 1) % m]); let (p0, p1) = ((a.0 as f32, a.1 as f32), (b.0 as f32, b.1 as f32));
        if p0.1 == p1.1 { continue; } let (dir, p0, p1) = if p0.1 < p1.1 { (1.0f32, p0, p1) } else { (-1.0f32, p1, p0) };
        let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1); let mut x = p0.0; let y0 = p0.1.max(0.0) as usize; if p0.1 < 0.0 { x -= p0.1 * dxdy; }
        for y in y0..h.min(p1.1.ceil().max(0.0) as usize) { let ls = y * stride; let dy = ((y + 1) as f32).min(p1.1) - (y as f32).max(p0.1); let xnext = x + dxdy * dy; let d = dy * dir;
            let (xa, xb) = (x.clamp(0.0, wf), xnext.clamp(0.0, wf)); let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
            let x0f = x0.floor(); let x0i = x0f as usize; let x1c = x1.ceil(); let x1i = x1c as usize;
            if x1i <= x0i + 1 { let xmf = 0.5 * (xa + xb) - x0f; acc[ls + x0i] += d - d * xmf; acc[ls + x0i + 1] += d * xmf; }
            else { let sc = 1.0 / (x1 - x0); let x0fr = x0 - x0f; let a0 = 0.5 * sc * (1.0 - x0fr) * (1.0 - x0fr); let x1fr = x1 - x1c + 1.0; let am = 0.5 * sc * x1fr * x1fr;
                acc[ls + x0i] += d * a0;
                if x1i == x0i + 2 { acc[ls + x0i + 1] += d * (1.0 - a0 - am); } else { let a1 = sc * (1.5 - x0fr); acc[ls + x0i + 1] += d * (a1 - a0); for xi in x0i + 2..x1i - 1 { acc[ls + xi] += d * sc; } let a2 = a1 + (x1i - x0i - 3) as f32 * sc; acc[ls + x1i - 1] += d * (1.0 - a2 - am); }
                acc[ls + x1i] += d * am; }
            x = xnext; } }
    for y in 0..h { let mut sum = 0f32; for x in 0..w { sum += acc[y * stride + x]; cov[y * w + x] = sum.abs().min(1.0); } }
    cov
}

/// Exact distance to the flattened path, interpolating the per-point feather width at the nearest point.
fn variable_feather(pts: &[PathPt], closed: bool, base: &[f32], w: usize, h: usize, layer_f: f32, s: f32) -> Vec<f32> {
    let poly = flatten(pts, closed, w, h); let m = poly.len(); if m < 3 { return base.to_vec(); }
    let fw = |f: f64| if f > 0.0 { f as f32 * s } else { layer_f * s }; let maxf = poly.iter().map(|p| fw(p.2)).fold(0.0f32, f32::max).max(1.0);
    let band = maxf / 2.0 + 2.0; let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &poly { x0 = x0.min(p.0 as f32); y0 = y0.min(p.1 as f32); x1 = x1.max(p.0 as f32); y1 = y1.max(p.1 as f32); }
    let (bx0, by0, bx1, by1) = (((x0 - band).floor().max(0.0)) as usize, ((y0 - band).floor().max(0.0)) as usize, ((x1 + band).ceil().min(w as f32)) as usize, ((y1 + band).ceil().min(h as f32)) as usize);
    let mut out = base.to_vec();
    // distance field on a coarse test: only evaluate pixels close to an edge (skip deep interior/exterior cheaply via segment bbox grid)
    let segs: Vec<((f32, f32), (f32, f32), f32, f32)> = (0..m).map(|i| { let (a, b) = (poly[i], poly[(i + 1) % m]); ((a.0 as f32, a.1 as f32), (b.0 as f32, b.1 as f32), fw(a.2), fw(b.2)) }).collect();
    let cell = 32usize; let (gw, gh) = ((w + cell - 1) / cell, (h + cell - 1) / cell); let mut grid: Vec<Vec<u32>> = vec![vec![]; gw * gh];
    for (si, sg) in segs.iter().enumerate() { let (sx0, sx1, sy0, sy1) = (sg.0.0.min(sg.1.0) - band, sg.0.0.max(sg.1.0) + band, sg.0.1.min(sg.1.1) - band, sg.0.1.max(sg.1.1) + band);
        for gy in ((sy0.max(0.0) as usize) / cell)..=(((sy1.max(0.0) as usize) / cell).min(gh - 1)) { for gx in ((sx0.max(0.0) as usize) / cell)..=(((sx1.max(0.0) as usize) / cell).min(gw - 1)) { grid[gy * gw + gx].push(si as u32); } } }
    for y in by0..by1 { for x in bx0..bx1 { let cl = &grid[(y / cell) * gw + x / cell]; if cl.is_empty() { continue; }
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5); let mut best = f32::MAX; let mut bf = layer_f * s;
        for &si in cl { let sg = &segs[si as usize]; let (ex, ey) = (sg.1.0 - sg.0.0, sg.1.1 - sg.0.1); let l2 = (ex * ex + ey * ey).max(1e-9); let u = (((px - sg.0.0) * ex + (py - sg.0.1) * ey) / l2).clamp(0.0, 1.0);
            let (dx, dy) = (sg.0.0 + ex * u - px, sg.0.1 + ey * u - py); let d2 = dx * dx + dy * dy; if d2 < best { best = d2; bf = sg.2 + (sg.3 - sg.2) * u; } }
        let d = best.sqrt(); if d > bf.max(1.0) / 2.0 + 1.5 { continue; }
        let inside = base[y * w + x] > 0.5; let sd = if inside { -d } else { d }; let half = (bf / 2.0).max(0.5); out[y * w + x] = 1.0 - sstep(-half, half, sd); } }
    out
}

fn edt1(f: &mut [f64], n: usize, v: &mut [usize], z: &mut [f64], d: &mut [f64]) {
    let mut k = 0usize; v[0] = 0; z[0] = f64::MIN; z[1] = f64::MAX;
    for q in 1..n { loop { let p = v[k]; let s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64)); if s <= z[k] && k > 0 { k -= 1; continue; } if s <= z[k] { k = 0; v[0] = q; z[0] = f64::MIN; z[1] = f64::MAX; break; } k += 1; v[k] = q; z[k] = s; z[k + 1] = f64::MAX; break; } }
    k = 0; for q in 0..n { while z[k + 1] < q as f64 { k += 1; } let p = v[k]; d[q] = (q as f64 - p as f64).powi(2) + f[p]; }
}
/// Euclidean distance (px) from every pixel to the nearest pixel where `set` is true.
pub(crate) fn edt(set: &[bool], w: usize, h: usize) -> Vec<f32> {
    const INF: f64 = 1e12; let mut g: Vec<f64> = set.iter().map(|&b| if b { 0.0 } else { INF }).collect(); let n = w.max(h); let (mut v, mut z, mut d, mut f) = (vec![0usize; n + 1], vec![0f64; n + 2], vec![0f64; n + 1], vec![0f64; n + 1]);
    for x in 0..w { for y in 0..h { f[y] = g[y * w + x]; } edt1(&mut f, h, &mut v, &mut z, &mut d); for y in 0..h { g[y * w + x] = d[y]; } }
    for y in 0..h { for x in 0..w { f[x] = g[y * w + x]; } edt1(&mut f, w, &mut v, &mut z, &mut d); for x in 0..w { g[y * w + x] = d[x]; } }
    g.iter().map(|v| v.sqrt() as f32).collect()
}
/// Smooth, circular grow (r>0) or choke (r<0) from a signed distance field of the coverage.
fn morph(c: &mut Vec<f32>, w: usize, h: usize, r: f32) {
    if r == 0.0 { return; } let inside: Vec<bool> = c.iter().map(|&v| v > 0.5).collect(); let outside: Vec<bool> = inside.iter().map(|b| !b).collect();
    let (d_out, d_in) = (edt(&inside, w, h), edt(&outside, w, h)); // distance to nearest inside / outside pixel
    let mut sd: Vec<f32> = (0..w * h).map(|i| if inside[i] { -(d_in[i] - 0.5) + (c[i] - 1.0).abs() * 0.0 } else { d_out[i] - 0.5 }).collect();
    // refine the band with the coverage itself (sub-pixel position of the edge)
    // only pixels that really sit on the edge (within 1.5 px of the 0.5 contour); stray low coverage far from the shape must not be treated as an edge
    for i in 0..w * h { if c[i] > 0.0 && c[i] < 1.0 && sd[i].abs() <= 1.5 { sd[i] = 0.5 - c[i]; } }
    let mut t = vec![0f32; w * h]; box_pass(&sd, &mut t, w, h, 1, true); box_pass(&t, &mut sd, w, h, 1, false);
    for i in 0..w * h { c[i] = (0.5 - (sd[i] - r)).clamp(0.0, 1.0); }
}

fn srgb_lin(v: f32) -> f32 { let v = v.clamp(0.0, 1.0); if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
pub fn lab(r: f32, g: f32, b: f32) -> [f32; 3] { let (r, g, b) = (srgb_lin(r), srgb_lin(g), srgb_lin(b));
    let (x, y, z) = ((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047, 0.2126 * r + 0.7152 * g + 0.0722 * b, (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883);
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 }; let (fx, fy, fz) = (f(x), f(y), f(z)); [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)] }
fn de(a: &[f32; 3], b: &[f32; 3]) -> f32 { ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() / 100.0 }

/// Guided filter (He et al.): snaps a coverage map to the edges of the image. r in px, eps regularisation.
pub fn guided(cov: &mut Vec<f32>, f: &Frame, r: usize, eps: f32) {
    let (w, h) = (f.w, f.h); let n = w * h; let r = r.max(1).min(w.min(h) / 2 - 1); let i: Vec<f32> = (0..n).map(|k| lumf(f, k)).collect();
    let mean = |src: &[f32]| -> Vec<f32> { let mut t = vec![0f32; n]; let mut o = vec![0f32; n]; box_pass(src, &mut t, w, h, r, true); box_pass(&t, &mut o, w, h, r, false); o };
    let (mi, mp) = (mean(&i), mean(cov)); let ip: Vec<f32> = (0..n).map(|k| i[k] * cov[k]).collect(); let ii: Vec<f32> = (0..n).map(|k| i[k] * i[k]).collect(); let (mip, mii) = (mean(&ip), mean(&ii));
    let mut a = vec![0f32; n]; let mut b = vec![0f32; n]; for k in 0..n { let var = mii[k] - mi[k] * mi[k]; let cv = mip[k] - mi[k] * mp[k]; a[k] = cv / (var + eps); b[k] = mp[k] - a[k] * mi[k]; }
    let (ma, mb) = (mean(&a), mean(&b)); for k in 0..n { cov[k] = (ma[k] * i[k] + mb[k]).clamp(0.0, 1.0); }
}

fn raster_path_legacy(pts: &[PathPt], closed: bool, w: usize, h: usize) -> Vec<f32> {
    let mut cov = vec![0f32; w * h]; let n = pts.len(); if n < 3 { return cov; }
    let (wf, hf) = (w as f64, h as f64); let mut poly: Vec<(f64, f64)> = vec![];
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs { let a = &pts[i]; let b = &pts[(i + 1) % n];
        let (p0, p1, p2, p3) = ((a.x * wf, a.y * hf), ((a.x + a.ox) * wf, (a.y + a.oy) * hf), ((b.x + b.ix) * wf, (b.y + b.iy) * hf), (b.x * wf, b.y * hf));
        let straight = a.ox == 0.0 && a.oy == 0.0 && b.ix == 0.0 && b.iy == 0.0; let steps = if straight { 1 } else { 24 };
        for s in 0..steps { let u = s as f64 / steps as f64; let v = 1.0 - u;
            poly.push((v*v*v*p0.0 + 3.0*v*v*u*p1.0 + 3.0*v*u*u*p2.0 + u*u*u*p3.0, v*v*v*p0.1 + 3.0*v*v*u*p1.1 + 3.0*v*u*u*p2.1 + u*u*u*p3.1)); } }
    let m = poly.len(); if m < 3 { return cov; }
    let ymin = poly.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor().max(0.0) as usize; let ymax = (poly.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil() as i64).clamp(0, h as i64) as usize;
    const SS: usize = 4; let mut xs: Vec<(f64, i32)> = vec![];
    for y in ymin..ymax { for sy in 0..SS { let yy = y as f64 + (sy as f64 + 0.5) / SS as f64; xs.clear();
        for i in 0..m { let (a, b) = (poly[i], poly[(i + 1) % m]); if (a.1 <= yy && b.1 > yy) || (b.1 <= yy && a.1 > yy) { let t = (yy - a.1) / (b.1 - a.1); xs.push((a.0 + t * (b.0 - a.0), if b.1 > a.1 { 1 } else { -1 })); } }
        xs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap()); let mut wind = 0; 
        for k in 0..xs.len().saturating_sub(1) { wind += xs[k].1; if wind != 0 { let (x0, x1) = (xs[k].0.max(0.0), xs[k + 1].0.min(wf)); if x1 <= x0 { continue; }
            let (i0, i1) = (x0.floor() as usize, (x1.ceil() as usize).min(w)); for x in i0..i1 { let cx0 = (x as f64).max(x0); let cx1 = ((x + 1) as f64).min(x1); if cx1 > cx0 { cov[y * w + x] += ((cx1 - cx0) / SS as f64) as f32; } } } }
    } }
    for v in cov.iter_mut() { *v = v.min(1.0); } cov
}

pub(crate) fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horiz: bool) {
    // Bit-identical to the old per-line running sum; threaded by rows / column strips.
    if w == 0 || h == 0 { return; } let n = (2 * r + 1) as f32;
    if horiz {
        crate::par::rows_mut(&mut dst[..w * h], w, |y, out| { let row = &src[y * w..(y + 1) * w];
            let mut acc = 0f32; for i in 0..=r.min(w - 1) { acc += row[i]; } acc += row[0] * r as f32;
            for i in 0..w { out[i] = acc / n; let add = row[(i + r + 1).min(w - 1)]; let sub = row[i.saturating_sub(r)]; acc += add - sub; } });
    } else {
        const C: usize = 64; let strips = (w + C - 1) / C; let p = crate::par::SyncPtr(dst.as_mut_ptr());
        crate::par::ranges(strips, 1, |a, b| { let p = p; for st in a..b { let c0 = st * C; let c1 = (c0 + C).min(w); let m = c1 - c0; let mut acc = [0f32; C];
            for i in 0..=r.min(h - 1) { for c in 0..m { acc[c] += src[i * w + c0 + c]; } } for c in 0..m { acc[c] += src[c0 + c] * r as f32; }
            for i in 0..h { let (ra, rs) = ((i + r + 1).min(h - 1), i.saturating_sub(r));
                for c in 0..m { unsafe { *p.0.add(i * w + c0 + c) = acc[c] / n; } acc[c] += src[ra * w + c0 + c] - src[rs * w + c0 + c]; } } } });
    }
}
pub fn blur_cov(c: &mut Vec<f32>, w: usize, h: usize, sigma: f32) {
    if sigma < 0.4 { return; } let r = ((sigma * 1.7).round() as usize).max(1).min(w.min(h) / 2 - 1); let mut t = vec![0f32; w * h];
    for _ in 0..3 { box_pass(c, &mut t, w, h, r, true); box_pass(&t, c, w, h, r, false); }
}
fn morph_legacy(c: &mut Vec<f32>, w: usize, h: usize, r: i32) {
    if r == 0 { return; } let grow = r > 0; let r = r.unsigned_abs() as usize; let mut t = vec![0f32; w * h];
    for (horiz, src_is_c) in [(true, true), (false, false)] {
        let (len, lines) = if horiz { (w, h) } else { (h, w) };
        for l in 0..lines { let idx = |i: usize| if horiz { l * w + i } else { i * w + l };
            for i in 0..len { let (lo, hi) = (i.saturating_sub(r), (i + r).min(len - 1)); let mut m = if grow { 0f32 } else { 1f32 };
                for k in lo..=hi { let v = if src_is_c { c[idx(k)] } else { t[idx(k)] }; m = if grow { m.max(v) } else { m.min(v) }; }
                if src_is_c { t[idx(i)] = m } else { c[idx(i)] = m } } } }
}

pub struct Ctx<'a> { pub refs: &'a dyn Fn(&str) -> Option<Frame>, pub fps: f64 }

fn lumf(f: &Frame, i: usize) -> f32 { 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2] }
fn rgb2hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) { let mx = r.max(g).max(b); let mn = r.min(g).min(b); let l = (mx + mn) / 2.0; let d = mx - mn;
    if d < 1e-6 { return (0.0, 0.0, l); } let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let hh = if mx == r { ((g - b) / d).rem_euclid(6.0) } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 }; (hh * 60.0, s, l) }
fn vhash(x: i64, y: i64, s: i64) -> f32 { let mut n = (x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ s.wrapping_mul(2147483647)) as u64; n = (n ^ (n >> 13)).wrapping_mul(1274126177); ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0 }
fn vnoise(x: f32, y: f32, s: i64) -> f32 { let (xi, yi) = (x.floor(), y.floor()); let (fx, fy) = (x - xi, y - yi); let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy)); let (xi, yi) = (xi as i64, yi as i64);
    let a = vhash(xi, yi, s); let b = vhash(xi + 1, yi, s); let c = vhash(xi, yi + 1, s); let d = vhash(xi + 1, yi + 1, s); a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v }

/// Raw coverage of one layer (before expand/feather/invert/opacity).
fn raw(l: &MaskLayer, f: &Frame, t: f64, ctx: &Ctx) -> Vec<f32> {
    let (w, h) = (f.w, f.h); let (wf, hf) = (w as f32, h as f32); let mut c = vec![0f32; w * h];
    match l.kind.as_str() {
        "rect" | "ellipse" => { let (cx, cy, mw, mh, rot) = (g(l, "cx", 0.5, t) * wf, g(l, "cy", 0.5, t) * hf, g(l, "w", 0.5, t) * wf / 2.0, g(l, "h", 0.5, t) * hf / 2.0, g(l, "rot", 0.0, t).to_radians());
            let rr = g(l, "round", 0.0, t).clamp(0.0, 1.0) * mw.min(mh); let (s, co) = (rot.sin(), rot.cos()); let ell = l.kind == "ellipse";
            for y in 0..h { for x in 0..w { let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy); let (ux, uy) = (px * co + py * s, -px * s + py * co);
                let d = if ell { let k = ((ux / mw.max(0.5)).powi(2) + (uy / mh.max(0.5)).powi(2)).sqrt(); (k - 1.0) * mw.min(mh) } else { let (qx, qy) = (ux.abs() - (mw - rr), uy.abs() - (mh - rr)); (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rr };
                c[y * w + x] = (0.5 - d).clamp(0.0, 1.0); } } }
        "path" => { let leg = g(l, "legacy", 0.0, t) > 0.5; let pts = path_at(l, t); c = if leg { raster_path_legacy(&pts, l.closed, w, h) } else { raster_path(&pts, l.closed, w, h) };
            if !leg && pts.iter().any(|p| p.f > 0.0) { c = variable_feather(&pts, l.closed, &c, w, h, 0.0, h as f32 / 720.0); } }
        "linear" => { let (a, pos, soft) = (g(l, "angle", 0.0, t).to_radians(), g(l, "pos", 0.5, t), g(l, "soft", 0.1, t).max(1e-4)); let (dx, dy) = (a.cos(), a.sin()); let ext = (wf * dx.abs() + hf * dy.abs()).max(1.0);
            for y in 0..h { for x in 0..w { let u = (((x as f32 + 0.5 - wf / 2.0) * dx + (y as f32 + 0.5 - hf / 2.0) * dy) / ext) + 0.5; c[y * w + x] = 1.0 - sstep(pos - soft / 2.0, pos + soft / 2.0, u); } } }
        "radial" => { let (cx, cy, r, soft) = (g(l, "cx", 0.5, t) * wf, g(l, "cy", 0.5, t) * hf, g(l, "r", 0.3, t) * hf, g(l, "soft", 0.2, t).max(1e-4) * hf);
            for y in 0..h { for x in 0..w { let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt(); c[y * w + x] = 1.0 - sstep(r - soft / 2.0, r + soft / 2.0, d); } } }
        "luma" => { let (lo, hi, so) = (g(l, "lo", 0.5, t), g(l, "hi", 1.0, t), g(l, "soft", 0.1, t) / 2.0 + 1e-4);
            for i in 0..w * h { let v = lumf(f, i); c[i] = sstep(lo - so, lo + so, v) * (1.0 - sstep(hi - so, hi + so, v)); } }
        "color" => { let (kr, kg, kb_, tol, so) = (g(l, "r", 0.0, t), g(l, "g", 1.0, t), g(l, "b", 0.0, t), g(l, "tol", 0.2, t), g(l, "soft", 0.1, t));
            let leg = g(l, "legacy", 0.0, t) > 0.5; let kl = lab(kr, kg, kb_);
            for i in 0..w * h { let d = if leg { ((f.d[i*4] - kr).powi(2) + (f.d[i*4+1] - kg).powi(2) + (f.d[i*4+2] - kb_).powi(2)).sqrt() / 1.732 } else { de(&lab(f.d[i*4], f.d[i*4+1], f.d[i*4+2]), &kl) * 0.9 }; c[i] = 1.0 - sstep(tol, tol + so + 1e-4, d); }
            if !leg && g(l, "unmix", 1.0, t) > 0.0 { // closed-form alpha unmix against the local foreground colour: P = a F + (1-a) K
                let n = w * h; let mut kb = c.clone(); blur_cov(&mut kb, w, h, 4.0 * h as f32 / 720.0); let wgt: Vec<f32> = kb.iter().map(|&v| if v < 0.02 { 1.0 } else { 0.0 }).collect(); let sg = 8.0 * h as f32 / 720.0;
                let mut nb = wgt.clone(); blur_cov(&mut nb, w, h, sg); let mut ch: Vec<Vec<f32>> = (0..3).map(|j| (0..n).map(|i| f.d[i * 4 + j] * wgt[i]).collect()).collect(); for x in ch.iter_mut() { blur_cov(x, w, h, sg); }
                let key = [kr, kg, kb_]; for i in 0..n { if kb[i] < 0.02 || nb[i] < 0.01 { continue; } let fc = [ch[0][i] / nb[i], ch[1][i] / nb[i], ch[2][i] / nb[i]];
                    let (fk, pk) = ([fc[0] - key[0], fc[1] - key[1], fc[2] - key[2]], [f.d[i*4] - key[0], f.d[i*4+1] - key[1], f.d[i*4+2] - key[2]]); let den = fk[0] * fk[0] + fk[1] * fk[1] + fk[2] * fk[2]; if den < 1e-4 { continue; }
                    let a = ((pk[0] * fk[0] + pk[1] * fk[1] + pk[2] * fk[2]) / den).clamp(0.0, 1.0); c[i] = 1.0 - a; } } }
        "hsl" => { let (hu, hw, s0, s1, l0, l1, so) = (g(l, "hue", 0.0, t), g(l, "hue_width", 30.0, t), g(l, "s_lo", 0.2, t), g(l, "s_hi", 1.0, t), g(l, "l_lo", 0.0, t), g(l, "l_hi", 1.0, t), g(l, "soft", 0.1, t) + 1e-4);
            for i in 0..w * h { let (hh, ss, ll) = rgb2hsl(f.d[i*4], f.d[i*4+1], f.d[i*4+2]); let mut dh = (hh - hu).abs() % 360.0; if dh > 180.0 { dh = 360.0 - dh; }
                c[i] = (1.0 - sstep(hw / 2.0, hw / 2.0 + so * 90.0, dh)) * sstep(s0 - so, s0, ss) * (1.0 - sstep(s1, s1 + so, ss)) * sstep(l0 - so, l0, ll) * (1.0 - sstep(l1, l1 + so, ll)); } }
        "difference" => { if let Some(rf) = l.strings.get("ref_clip").and_then(|id| (ctx.refs)(id)) { let (tol, so) = (g(l, "tol", 0.08, t), g(l, "soft", 0.06, t) + 1e-4);
            for i in 0..w * h { let d = ((f.d[i*4] - rf.d[i*4]).abs() + (f.d[i*4+1] - rf.d[i*4+1]).abs() + (f.d[i*4+2] - rf.d[i*4+2]).abs()) / 3.0; c[i] = sstep(tol, tol + so, d); } } }
        "wand" => { let (sx, sy) = (((g(l, "x", 0.5, t) * wf) as usize).min(w - 1), ((g(l, "y", 0.5, t) * hf) as usize).min(h - 1)); let (tol, so, contig) = (g(l, "tol", 0.12, t), g(l, "soft", 0.05, t), g(l, "contiguous", 1.0, t) > 0.5);
            let mut sc = [0f32; 3]; let mut n = 0.0; for dy in -1i32..=1 { for dx in -1i32..=1 { let (x, y) = ((sx as i32 + dx).clamp(0, w as i32 - 1) as usize, (sy as i32 + dy).clamp(0, h as i32 - 1) as usize); for j in 0..3 { sc[j] += f.d[(y * w + x) * 4 + j]; } n += 1.0; } } for j in 0..3 { sc[j] /= n; }
            let sl = lab(sc[0], sc[1], sc[2]); let dist = |i: usize| de(&lab(f.d[i*4], f.d[i*4+1], f.d[i*4+2]), &sl) * 0.9; let val = |i: usize| 1.0 - sstep(tol, tol + so + 1e-4, dist(i));
            if !contig { for i in 0..w * h { c[i] = val(i); } } else { let mut st = vec![sy * w + sx]; let mut seen = vec![false; w * h]; seen[sy * w + sx] = true;
                while let Some(i) = st.pop() { let v = val(i); if v <= 0.0 { continue; } c[i] = v; let (x, y) = (i % w, i / w);
                    for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] { if nx < w && ny < h { let j = ny * w + nx; if !seen[j] { seen[j] = true; st.push(j); } } } } } }
        "scribble" => { let samp = |pts: &Vec<[f64; 2]>| -> Vec<[f32; 3]> { let mut v = vec![]; for p in pts { let (cx, cy) = ((p[0] * wf as f64) as i32, (p[1] * hf as f64) as i32); for dy in (-6..=6).step_by(3) { for dx in (-6..=6).step_by(3) { let (x, y) = ((cx + dx).clamp(0, w as i32 - 1) as usize, (cy + dy).clamp(0, h as i32 - 1) as usize); let i = (y * w + x) * 4; v.push([f.d[i], f.d[i+1], f.d[i+2]]); } } } v };
            let (fg, bg) = (samp(&l.fg), samp(&l.bg)); if !fg.is_empty() && !bg.is_empty() { let (fgp, bgp): (Vec<(f32, f32)>, Vec<(f32, f32)>) = (l.fg.iter().map(|p| (p[0] as f32 * wf, p[1] as f32 * hf)).collect(), l.bg.iter().map(|p| (p[0] as f32 * wf, p[1] as f32 * hf)).collect());
                let so = g(l, "soft", 0.15, t).max(0.02);
                for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; let px = [f.d[i], f.d[i+1], f.d[i+2]];
                    let dm = |s: &Vec<[f32; 3]>| s.iter().map(|q| (px[0]-q[0]).powi(2) + (px[1]-q[1]).powi(2) + (px[2]-q[2]).powi(2)).fold(f32::MAX, f32::min).sqrt();
                    let sp = |s: &Vec<(f32, f32)>| s.iter().map(|q| ((x as f32 - q.0).powi(2) + (y as f32 - q.1).powi(2)).sqrt()).fold(f32::MAX, f32::min);
                    let (df, db) = (dm(&fg) + sp(&fgp) / hf * 0.15, dm(&bg) + sp(&bgp) / hf * 0.15); c[y * w + x] = sstep(0.5 - so, 0.5 + so, db / (df + db + 1e-6)); } }
                let sm = g(l, "smooth", 2.0, t); blur_cov(&mut c, w, h, sm); for v in c.iter_mut() { *v = sstep(0.35, 0.65, *v); } } }
        "edge" => { let (thr, so, th) = (g(l, "thr", 0.15, t), g(l, "soft", 0.1, t) + 1e-4, g(l, "thickness", 2.0, t));
            for y in 1..h.saturating_sub(1) { for x in 1..w.saturating_sub(1) { let p = |xx: usize, yy: usize| lumf(f, yy * w + xx); let gx = -p(x-1,y-1) - 2.0*p(x-1,y) - p(x-1,y+1) + p(x+1,y-1) + 2.0*p(x+1,y) + p(x+1,y+1); let gy = -p(x-1,y-1) - 2.0*p(x,y-1) - p(x+1,y-1) + p(x-1,y+1) + 2.0*p(x,y+1) + p(x+1,y+1);
                c[y * w + x] = sstep(thr, thr + so, (gx * gx + gy * gy).sqrt() / 4.0); } } morph(&mut c, w, h, th * hf / 720.0 / 2.0); }
        "matte" => { if let Some(st) = l.strings.get("dir").and_then(|d| crate::matte::load(d)) { let fi = (t - g(l, "start", 0.0, t) as f64) * ctx.fps; if fi >= -0.5 || g(l, "hold", 1.0, t) > 0.5 { c = crate::matte::frame_alpha(&st, fi, w, h); if fi > (st.n - 1) as f64 + 0.5 && g(l, "hold", 1.0, t) < 0.5 { c.iter_mut().for_each(|v| *v = 0.0); } } } }
        "alpha" => { let (thr, so) = (g(l, "thr", 0.5, t), g(l, "soft", 0.1, t) / 2.0 + 1e-4); for i in 0..w * h { c[i] = sstep(thr - so, thr + so, f.d[i * 4 + 3]); } }
        "noise" => { let (sc, thr, so, seed, sp) = (g(l, "scale", 120.0, t).max(2.0) * hf / 720.0, g(l, "thr", 0.5, t), g(l, "soft", 0.08, t) + 1e-4, g(l, "seed", 1.0, t) as i64, g(l, "speed", 0.0, t));
            let tt = t as f32 * sp; for y in 0..h { for x in 0..w { let (u, v) = ((x as f32 + tt * sc) / sc, (y as f32 - tt * sc * 0.6) / sc); let n = 0.55 * vnoise(u, v, seed) + 0.3 * vnoise(u * 2.1, v * 2.1, seed + 7) + 0.15 * vnoise(u * 4.3, v * 4.3, seed + 13); c[y * w + x] = sstep(thr - so, thr + so, n); } } }
        _ => {}
    }
    c
}

pub fn layer_coverage(l: &MaskLayer, f: &Frame, t: f64, ctx: &Ctx) -> Vec<f32> {
    let (w, h) = (f.w, f.h); let s = h as f32 / 720.0; let leg = g(l, "legacy", 0.0, t) > 0.5;
    // temporal supersampling of animated geometry (mask motion blur): shutter in degrees of a frame, samples default 6
    let shut = g(l, "shutter", 0.0, t) as f64; let geometric = matches!(l.kind.as_str(), "path" | "rect" | "ellipse" | "linear" | "radial");
    let mut c = if shut > 0.0 && geometric { let n = (g(l, "samples", 6.0, t) as usize).clamp(2, 32); let mut acc = vec![0f32; w * h];
        for k in 0..n { let off = (k as f64 / (n - 1) as f64 - 0.5) * shut / 360.0 / ctx.fps.max(1.0); let r = raw(l, f, t + off, ctx); for i in 0..w * h { acc[i] += r[i]; } } for v in acc.iter_mut() { *v /= n as f32; } acc } else { raw(l, f, t, ctx) };
    let rf = g(l, "refine", 0.0, t) * s; if rf > 0.0 { guided(&mut c, f, rf.round() as usize, g(l, "refine_eps", 1e-3, t).max(1e-6)); }
    let ex = g(l, "expand", 0.0, t) * s; if ex != 0.0 { if leg { morph_legacy(&mut c, w, h, ex.round() as i32); } else { morph(&mut c, w, h, ex); } }
    let fe = g(l, "feather", 0.0, t) * s; if fe > 0.0 && !(l.kind == "path" && l.points.iter().chain(l.path_keys.iter().flat_map(|k| k.points.iter())).any(|p| p.f > 0.0) && !leg) { blur_cov(&mut c, w, h, fe / 2.0); }
    let op = g(l, "opacity", 1.0, t).clamp(0.0, 1.0);
    for v in c.iter_mut() { if l.invert { *v = 1.0 - *v; } *v *= op; } c
}

pub fn stack_coverage(layers: &[MaskLayer], f: &Frame, t: f64, ctx: &Ctx) -> Option<Vec<f32>> {
    let act: Vec<&MaskLayer> = layers.iter().filter(|l| l.enabled && l.mode != "none").collect(); if act.is_empty() { return None; }
    let n = f.w * f.h; let first = act[0].mode.as_str(); let mut acc = vec![if first == "subtract" || first == "intersect" { 1.0f32 } else { 0.0 }; n];
    for l in act { let c = layer_coverage(l, f, t, ctx);
        for i in 0..n { let (a, b) = (acc[i], c[i]); acc[i] = match l.mode.as_str() { "subtract" => a * (1.0 - b), "intersect" => a * b, "xor" | "difference" => (a - b).abs(), "lighten" => a.max(b), "darken" => a.min(b), _ => a + b - a * b }; } }
    Some(acc)
}

pub fn apply_stack(f: &mut Frame, layers: &[MaskLayer], t: f64, ctx: &Ctx) {
    let Some(c) = stack_coverage(layers, f, t, ctx) else { return };
    let (w, h) = (f.w, f.h); let s = h as f32 / 720.0;
    let dec = layers.iter().filter(|l| l.enabled).map(|l| g(l, "decontam", 0.0, t)).fold(0.0f32, f32::max); let despill = layers.iter().filter(|l| l.enabled && l.kind == "color").map(|l| (g(l, "despill", 0.0, t), l)).fold((0.0f32, None), |a, b| if b.0 > a.0 { (b.0, Some(b.1)) } else { a });
    if dec > 0.0 || despill.0 > 0.0 { let r = ((dec.max(3.0)) * s).round() as usize; let n = w * h;
        let wgt: Vec<f32> = c.iter().map(|&v| sstep(0.85, 1.0, v)).collect(); let mut nb: Vec<f32> = wgt.clone(); blur_cov(&mut nb, w, h, r as f32 / 1.7);
        let mut chan: Vec<Vec<f32>> = (0..3).map(|j| (0..n).map(|i| f.d[i * 4 + j] * wgt[i]).collect()).collect(); for ch in chan.iter_mut() { blur_cov(ch, w, h, r as f32 / 1.7); }
        let mut band: Vec<f32> = wgt.iter().map(|v| 1.0 - v).collect(); blur_cov(&mut band, w, h, r as f32 / 1.7);
        let key = despill.1.map(|l| [g(l, "r", 0.0, t), g(l, "g", 1.0, t), g(l, "b", 0.0, t)]); let amt = despill.0;
        for i in 0..n { let cv = c[i]; if cv <= 0.002 { continue; }
            if dec > 0.0 && nb[i] > 0.01 && wgt[i] < 1.0 { let k = (1.0 - wgt[i]).clamp(0.0, 1.0); for j in 0..3 { let fgc = chan[j][i] / nb[i]; f.d[i * 4 + j] = f.d[i * 4 + j] * (1.0 - k) + fgc * k; } }
            if let Some(kc) = key { let loc = (band[i] * 2.0).clamp(0.0, 1.0); if loc > 0.0 { let (r, g_, b) = (f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]); let ky = 0.2126 * kc[0] + 0.7152 * kc[1] + 0.0722 * kc[2]; let u = [kc[0] - ky, kc[1] - ky, kc[2] - ky]; let un = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
                if un > 1e-3 { let py = 0.2126 * r + 0.7152 * g_ + 0.0722 * b; let pc = [r - py, g_ - py, b - py]; let sp = ((pc[0] * u[0] + pc[1] * u[1] + pc[2] * u[2]) / un).max(0.0);
                    for j in 0..3 { f.d[i * 4 + j] -= amt * loc * sp * u[j] / un; } } } } } }
    if let Some(kl) = layers.iter().find(|l| l.enabled && l.kind == "color" && g(l, "unmix", 1.0, t) > 0.0 && g(l, "legacy", 0.0, t) < 0.5) { let kc = [g(kl, "r", 0.0, t), g(kl, "g", 1.0, t), g(kl, "b", 0.0, t)];
        for i in 0..w * h { let a = c[i]; if a > 0.03 && a < 0.985 { for j in 0..3 { f.d[i * 4 + j] = ((f.d[i * 4 + j] - (1.0 - a) * kc[j]) / a).clamp(0.0, 1.0); } } } }
    for i in 0..w * h { f.d[i * 4 + 3] *= c[i]; }
}

/// Track matte coverage from a rendered matte source frame.
pub fn matte_coverage(m: &Matte, src: &Frame) -> Vec<f32> {
    (0..src.w * src.h).map(|i| { let a = src.d[i * 4 + 3]; let l = 0.2126 * src.d[i * 4] + 0.7152 * src.d[i * 4 + 1] + 0.0722 * src.d[i * 4 + 2];
        match m.mode.as_str() { "luma" => l * a, "inv_luma" => 1.0 - l * a, "inv_alpha" => 1.0 - a, _ => a } }).collect()
}

#[cfg(test)]
mod edt_tests {
    use super::*;
    /// The distance transform must match brute force on a concave, row-sparse shape (the 80-point contour case).
    #[test]
    fn edt_matches_brute_force() {
        let (w, h) = (64usize, 40usize); let mut set = vec![false; w * h];
        for y in 5..35 { for x in 8..30 { let a = ((x as f32 - 19.0).powi(2) + (y as f32 - 20.0).powi(2)).sqrt(); if a < 8.0 + 4.0 * ((y as f32) * 0.9).sin() { set[y * w + x] = true; } } }
        let d = edt(&set, w, h);
        for y in 0..h { for x in 0..w { let mut best = f32::MAX; for yy in 0..h { for xx in 0..w { if set[yy * w + xx] { best = best.min((((x as f32 - xx as f32).powi(2) + (y as f32 - yy as f32).powi(2))) as f32); } } }
            assert!((d[y * w + x] - best.sqrt()).abs() < 1e-3, "edt mismatch at ({x},{y}): {} vs {}", d[y * w + x], best.sqrt()); } }
    }
}
