//! Push and Gradient Wipe transitions (add_push_wipe), Premiere's Push and After Effects' Gradient Wipe.
//! push: the incoming clip slides in from one edge while the outgoing picture slides out of the opposite edge, in step.
//! gradient_wipe: the incoming clip is revealed in order of a ramp's value: a linear, radial or angular gradient, or the luma
//! of another clip (hidden from the composite, like a hidden track-matte source). Dark pixels reveal first; invert flips it.
//! The outgoing picture is the abutting clip that ends where this one starts on its track (its last frame is held, as
//! morph_cut does), or, with no abutting clip, whatever is already drawn under the incoming clip (an overlap on a lower track).
use crate::effects::Frame;
use crate::model::{Clip, Project, Transition};
use crate::transitions::{Mask, Plan};

pub const KINDS: &[&str] = &["push", "gradient_wipe"];
pub const DIRECTIONS: &[&str] = &["left", "right", "up", "down"];
pub const GRADIENTS: &[&str] = &["linear", "radial", "angular"];

fn sstep(x: f32) -> f32 { let t = x.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Unit vector of the way the pictures travel (screen y down).
pub fn dir_vec(d: &str) -> (f64, f64) { match d { "right" => (1.0, 0.0), "up" => (0.0, -1.0), "down" => (0.0, 1.0), _ => (-1.0, 0.0) } }

/// Bilinear premultiplied sample of `f` at (x, y) in pixel-centre coordinates; transparent outside the frame.
fn sample(f: &Frame, x: f64, y: f64) -> [f32; 4] {
    let (x, y) = (x - 0.5, y - 0.5); let (x0, y0) = (x.floor(), y.floor()); let (fx, fy) = ((x - x0) as f32, (y - y0) as f32); let (x0, y0) = (x0 as i64, y0 as i64);
    let mut o = [0f32; 4];
    for (dx, dy, wgt) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
        let (sx, sy) = (x0 + dx, y0 + dy); if wgt <= 0.0 || sx < 0 || sy < 0 || sx >= f.w as i64 || sy >= f.h as i64 { continue; }
        let i = (sy as usize * f.w + sx as usize) * 4; for j in 0..4 { o[j] += f.d[i + j] * wgt; } }
    o
}
/// The canvas moved by (ox, oy) px; the uncovered strip is transparent.
pub fn shift(f: &mut Frame, ox: f64, oy: f64) {
    if ox.abs() < 1e-6 && oy.abs() < 1e-6 { return; } let src = Frame { w: f.w, h: f.h, d: f.d.clone() };
    for y in 0..f.h { for x in 0..f.w { let s = sample(&src, x as f64 + 0.5 - ox, y as f64 + 0.5 - oy); let i = (y * f.w + x) * 4; f.d[i..i + 4].copy_from_slice(&s); } }
}
/// Premultiplied `a`, moved by (ox, oy) px, drawn over `out`.
pub fn over_shifted(out: &mut Frame, a: &Frame, ox: f64, oy: f64) {
    for y in 0..out.h { for x in 0..out.w { let s = sample(a, x as f64 + 0.5 - ox, y as f64 + 0.5 - oy); if s[3] <= 0.0 { continue; } let i = (y * out.w + x) * 4;
        for j in 0..4 { out.d[i + j] = s[j] + out.d[i + j] * (1.0 - s[3]); } } }
}

/// push: the incoming clip enters from the edge opposite `direction` ((1-k) of the frame away) while the outgoing picture
/// leaves by k of the frame, so the two always meet edge to edge.
pub fn push_plan(p: &mut Plan, tr: &Transition, k: f32, w: usize, h: usize) {
    let dir = tr.spec.as_ref().map(|s| s.direction.as_str()).unwrap_or("left"); let (ux, uy) = dir_vec(dir); let (wf, hf, kk) = (w as f64, h as f64, k as f64);
    p.dx = -ux * (1.0 - kk) * wf; p.dy = -uy * (1.0 - kk) * hf;
    let (ox, oy) = (ux * kk * wf, uy * kk * hf);
    p.under = Some(Box::new(move |out, prev| match prev { Some(a) => over_shifted(out, a, ox, oy), None => shift(out, ox, oy) }));
}

/// The gradient's value 0..1 at a pixel centre: linear along `angle` degrees (0 = left to right, 90 = top to bottom),
/// radial from `center` (0 at the centre, 1 at the farthest corner) or angular (clockwise from `angle`, around `center`).
pub fn gradient_value(kind: &str, angle: f64, center: [f64; 2], w: usize, h: usize) -> impl Fn(usize, usize) -> f32 {
    let (wf, hf) = (w as f64, h as f64); let (c, s) = (angle.to_radians().cos(), angle.to_radians().sin()); let half = (wf * c.abs() + hf * s.abs()) / 2.0;
    let (cx, cy) = (center[0] * wf, center[1] * hf); let far = [(0.0, 0.0), (wf, 0.0), (0.0, hf), (wf, hf)].iter().map(|(x, y)| ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt()).fold(1e-9, f64::max);
    let kind = match kind { "radial" => 1, "angular" => 2, _ => 0 }; let a0 = angle.to_radians();
    move |x, y| { let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        let v = match kind { 1 => ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() / far,
            2 => ((py - cy).atan2(px - cx) - a0).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU,
            _ => ((px - wf / 2.0) * c + (py - hf / 2.0) * s) / (2.0 * half.max(1e-9)) + 0.5 }; v.clamp(0.0, 1.0) as f32 }
}
/// Reveal of a ramp value `l` at progress k with softness s: 0 everywhere at k = 0, 1 everywhere at k = 1, smoothstep edge of width s.
pub fn reveal(l: f32, k: f32, soft: f32, invert: bool) -> f32 { let l = if invert { 1.0 - l } else { l }; let s = soft.max(1e-4); sstep((k * (1.0 + s) - l) / s) }

pub fn gradient_plan(p: &mut Plan, tr: &Transition, k: f32, w: usize, h: usize) {
    let sp = tr.spec.clone().unwrap_or_default(); let g = gradient_value(&sp.gradient, tr.angle, sp.center, w, h); let (soft, inv) = (tr.softness.clamp(0.0, 1.0) as f32, sp.invert);
    p.mask = Some(Box::new(move |x, y| reveal(g(x, y), k, soft, inv)));
    p.under = Some(Box::new(|out, prev| if let Some(a) = prev { over_shifted(out, a, 0.0, 0.0) }));
}
/// The mask of a gradient wipe driven by a luma field (one value 0..1 per pixel, row-major, width w).
pub fn field_mask(tr: &Transition, k: f32, field: Vec<f32>, w: usize) -> Mask {
    let (soft, inv) = (tr.softness.clamp(0.0, 1.0) as f32, tr.spec.as_ref().map(|s| s.invert).unwrap_or(false));
    Box::new(move |x, y| reveal(field.get(y * w + x).copied().unwrap_or(0.0), k, soft, inv))
}
/// Rec.709 luma of a premultiplied frame over black, 0..1.
pub fn luma_of(f: &Frame) -> Vec<f32> { (0..f.w * f.h).map(|i| (0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2]).clamp(0.0, 1.0)).collect() }

/// Any clip on `c`'s track that ends where `c` starts (within 1.5 frames, no overlap): the outgoing shot of a push or wipe.
pub fn abut_prev<'a>(p: &'a Project, c: &Clip) -> Option<&'a Clip> {
    let tol = 1.5 / p.fps.max(1.0);
    p.tracks.iter().find(|t| t.clips.iter().any(|x| x.id == c.id))?.clips.iter().map(|x| &**x)
        .filter(|x| x.id != c.id && x.start + x.duration <= c.start + 1e-6 && c.start - (x.start + x.duration) < tol)
        .max_by(|a, b| (a.start + a.duration).total_cmp(&(b.start + b.duration)))
}
/// Time the outgoing clip is sampled at: its last frame.
pub fn prev_time(p: &Project, a: &Clip) -> f64 { a.start + a.duration - 0.5 / p.fps.max(1.0) }
/// Time a luma clip is sampled at: project time, held to the clip's own span.
pub fn luma_time(p: &Project, l: &Clip, t: f64) -> f64 { t.clamp(l.start, (l.start + l.duration - 0.5 / p.fps.max(1.0)).max(l.start)) }
/// Clips (and the time each is sampled at) a push / gradient wipe on `c` reads besides `c` itself, for decoding.
pub fn needs(p: &Project, c: &Clip, t: f64) -> Vec<(String, f64)> {
    let Some(tr) = c.transition_in.as_ref().filter(|tr| KINDS.contains(&tr.kind.as_str())) else { return vec![] };
    let mut v = vec![]; if let Some(a) = abut_prev(p, c) { v.push((a.id.clone(), prev_time(p, a))); }
    if let Some(l) = tr.spec.as_ref().and_then(|s| s.luma_clip.as_deref()).and_then(|id| p.find_clip(id)) { v.push((l.id.clone(), luma_time(p, l, t))); }
    v
}
/// Prepare the plan of a push / gradient wipe that needs other clips: the luma clip's field becomes the mask.
pub fn luma_mask(p: &Project, tr: &Transition, k_lt: f64, t: f64, w: usize, cache: &crate::render::Cache) -> Option<Mask> {
    if tr.kind != "gradient_wipe" { return None; }
    let l = tr.spec.as_ref().and_then(|s| s.luma_clip.as_deref()).and_then(|id| p.find_clip(id))?;
    let f = crate::render::render_clip_alone(p, &l.id, luma_time(p, l, t), cache)?;
    let k = crate::transitions::ease(&tr.ease, (k_lt / tr.duration).clamp(0.0, 1.0)) as f32;
    Some(field_mask(tr, k, luma_of(&f), w))
}
/// The outgoing abutting clip's held last frame, rendered alone (premultiplied, transparent around it).
pub fn outgoing(p: &Project, c: &Clip, cache: &crate::render::Cache) -> Option<Frame> { let a = abut_prev(p, c)?; crate::render::render_clip_alone(p, &a.id, prev_time(p, a), cache) }
