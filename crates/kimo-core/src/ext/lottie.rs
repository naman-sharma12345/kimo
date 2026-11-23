//! Lottie (Bodymovin JSON) both ways, so motion graphics made in kimo play in web and app players and Lottie files
//! from LottieFiles / After Effects come in as editable kimo layers.
//!
//! export_lottie writes shape layers (rect, ellipse, polygon, line, Bezier paths with morph keys, trim, offset,
//! repeater, gradient fill/stroke), text as glyph outlines (exact look, no font needed), solids, gradients, still images
//! (embedded), nested groups and markers. Keyframes keep their easing where Lottie can express it exactly (linear,
//! hold, in, out, custom Bezier); anything else (springs, bounces, expressions, parenting, fades from transitions) is
//! sampled once per frame. Whatever cannot be expressed is listed in `warnings`.
//!
//! import_lottie reads shape, solid, text, null and precomp layers, nested groups with their transforms, parenting,
//! fills, strokes, gradients, trim paths, repeaters and path morphs, and places the animation as one group clip on a
//! new track, fitted to the canvas. Transforms keep their keyframes and Bezier easing when they map one to one, and are
//! sampled per frame otherwise (parenting, rotated or skewed parents, animated anchors).
//!
//! The feature set follows EffectCraft's Lottie support (Apache-2.0, github.com/storytold/effectcraft); this is an
//! independent implementation for kimo's model.
use super::kit::*;
use super::svg_tools::{effect, new_clip, new_track, path_clip};
use crate::keyframe::{Ease, Key, Prop};
use crate::model::{Clip, Project, Source, Track};

// ---------------------------------------------------------------- shared geometry

/// One Bezier subpath in Lottie form: vertices with in/out tangents relative to each vertex.
#[derive(Clone, Debug, Default, PartialEq)]
struct Sub { v: Vec<[f64; 2]>, i: Vec<[f64; 2]>, o: Vec<[f64; 2]>, c: bool }

fn ensure(subs: &mut Vec<Sub>, at: [f64; 2]) { if subs.last().map_or(true, |s| s.c) { subs.push(Sub { v: vec![at], i: vec![[0.0; 2]], o: vec![[0.0; 2]], c: false }); } }
fn curve(subs: &mut Vec<Sub>, from: [f64; 2], c1: [f64; 2], c2: [f64; 2], p: [f64; 2]) {
    ensure(subs, from); let s = subs.last_mut().unwrap(); let lv = *s.v.last().unwrap(); *s.o.last_mut().unwrap() = [c1[0] - lv[0], c1[1] - lv[1]];
    s.v.push(p); s.i.push([c2[0] - p[0], c2[1] - p[1]]); s.o.push([0.0; 2]);
}
fn close(subs: &mut [Sub]) {
    if let Some(s) = subs.last_mut() { if s.c { return; }
        let n = s.v.len(); if n > 1 { let (f, l) = (s.v[0], s.v[n - 1]); if (f[0] - l[0]).abs() < 1e-6 && (f[1] - l[1]).abs() < 1e-6 { s.i[0] = s.i[n - 1]; s.v.pop(); s.i.pop(); s.o.pop(); } }
        s.c = true; }
}

/// SVG path data (kimo's path shapes) to Lottie subpaths. Arcs become straight segments (kimo paths have none).
fn d_to_subs(d: &str) -> Result<Vec<Sub>, String> {
    let mut subs: Vec<Sub> = vec![]; let (mut cur, mut start) = ([0.0; 2], [0.0; 2]); let (mut lc2, mut lq): (Option<[f64; 2]>, Option<[f64; 2]>) = (None, None);
    for (cmd, args) in crate::paths::tokens(d)? {
        let up = cmd.to_ascii_uppercase(); let rel = cmd.is_ascii_lowercase();
        if up == 'Z' { close(&mut subs); cur = start; lc2 = None; lq = None; continue; }
        let ar = match up { 'M' | 'L' | 'T' => 2, 'H' | 'V' => 1, 'C' => 6, 'S' | 'Q' => 4, 'A' => 7, _ => 0 }; if ar == 0 { continue; }
        for (n, a) in args.chunks(ar).enumerate() { if a.len() < ar { break; }
            let c0 = cur; let pt = |x: f64, y: f64| if rel { [c0[0] + x, c0[1] + y] } else { [x, y] };
            match up {
                'M' if n == 0 => { let p = pt(a[0], a[1]); subs.push(Sub { v: vec![p], i: vec![[0.0; 2]], o: vec![[0.0; 2]], c: false }); cur = p; start = p; lc2 = None; lq = None; }
                'M' | 'L' | 'H' | 'V' | 'A' => { let p = match up { 'H' => [if rel { c0[0] + a[0] } else { a[0] }, c0[1]], 'V' => [c0[0], if rel { c0[1] + a[0] } else { a[0] }], 'A' => pt(a[5], a[6]), _ => pt(a[0], a[1]) };
                    curve(&mut subs, c0, c0, p, p); cur = p; lc2 = None; lq = None; }
                'C' => { let (c1, c2, p) = (pt(a[0], a[1]), pt(a[2], a[3]), pt(a[4], a[5])); curve(&mut subs, c0, c1, c2, p); cur = p; lc2 = Some(c2); lq = None; }
                'S' => { let c1 = lc2.map_or(c0, |c| [2.0 * c0[0] - c[0], 2.0 * c0[1] - c[1]]); let (c2, p) = (pt(a[0], a[1]), pt(a[2], a[3])); curve(&mut subs, c0, c1, c2, p); cur = p; lc2 = Some(c2); lq = None; }
                'Q' | 'T' => { let q = if up == 'Q' { pt(a[0], a[1]) } else { lq.map_or(c0, |q| [2.0 * c0[0] - q[0], 2.0 * c0[1] - q[1]]) }; let p = if up == 'Q' { pt(a[2], a[3]) } else { pt(a[0], a[1]) };
                    let c1 = [c0[0] + 2.0 / 3.0 * (q[0] - c0[0]), c0[1] + 2.0 / 3.0 * (q[1] - c0[1])]; let c2 = [p[0] + 2.0 / 3.0 * (q[0] - p[0]), p[1] + 2.0 / 3.0 * (q[1] - p[1])];
                    curve(&mut subs, c0, c1, c2, p); cur = p; lq = Some(q); lc2 = None; }
                _ => {}
            } } }
    // straight segments were written as curves with handles on their ends: zero them so the file stays clean
    for s in &mut subs { for t in s.i.iter_mut().chain(s.o.iter_mut()) { if t[0].abs() < 1e-9 && t[1].abs() < 1e-9 { *t = [0.0; 2]; } } }
    Ok(subs)
}
fn subs_to_d(subs: &[Sub]) -> String {
    let f = |p: [f64; 2]| format!("{:.3} {:.3}", p[0], p[1]); let mut d = String::new();
    for s in subs { if s.v.is_empty() { continue; } d += &format!("M{}", f(s.v[0])); let n = s.v.len();
        let seg = |d: &mut String, j: usize, k: usize| { let (a, b) = (s.v[j], s.v[k]); let (o, i) = (s.o[j], s.i[k]);
            if o == [0.0; 2] && i == [0.0; 2] { *d += &format!(" L{}", f(b)); } else { *d += &format!(" C{} {} {}", f([a[0] + o[0], a[1] + o[1]]), f([b[0] + i[0], b[1] + i[1]]), f(b)); } };
        for j in 1..n { seg(&mut d, j - 1, j); } if s.c { if n > 1 { seg(&mut d, n - 1, 0); } d += " Z"; } d += " "; }
    d.trim().to_string()
}
fn sub_json(s: &Sub) -> Value { json!({"c": s.c, "v": s.v.iter().map(|p| json!([r3(p[0]), r3(p[1])])).collect::<Vec<_>>(), "i": s.i.iter().map(|p| json!([r3(p[0]), r3(p[1])])).collect::<Vec<_>>(), "o": s.o.iter().map(|p| json!([r3(p[0]), r3(p[1])])).collect::<Vec<_>>()}) }
fn r3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

/// 2D affine [a, b, c, d, e, f]: x' = a x + c y + e, y' = b x + d y + f.
type Aff = [f64; 6];
const ID: Aff = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
fn mul(m: &Aff, n: &Aff) -> Aff { [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3], m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]] }
fn apply(m: &Aff, p: [f64; 2]) -> [f64; 2] { [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]] }
fn lin(m: &Aff, p: [f64; 2]) -> [f64; 2] { [m[0] * p[0] + m[2] * p[1], m[1] * p[0] + m[3] * p[1]] }
fn tr(x: f64, y: f64) -> Aff { [1.0, 0.0, 0.0, 1.0, x, y] }
fn sc(x: f64, y: f64) -> Aff { [x, 0.0, 0.0, y, 0.0, 0.0] }
fn rot(deg: f64) -> Aff { let (s, c) = deg.to_radians().sin_cos(); [c, s, -s, c, 0.0, 0.0] }
fn det(m: &Aff) -> f64 { m[0] * m[3] - m[1] * m[2] }
/// Uniform scale + translation only (no rotation, skew or mirror): keyframes can be carried over one to one.
fn simple(m: &Aff) -> bool { m[1].abs() < 1e-9 && m[2].abs() < 1e-9 && (m[0] - m[3]).abs() < 1e-9 && m[0] > 0.0 }
fn map_subs(subs: &[Sub], m: &Aff) -> Vec<Sub> { subs.iter().map(|s| Sub { v: s.v.iter().map(|p| apply(m, *p)).collect(), i: s.i.iter().map(|p| lin(m, *p)).collect(), o: s.o.iter().map(|p| lin(m, *p)).collect(), c: s.c }).collect() }

// ---------------------------------------------------------------- export

const BLEND: &[(&str, u32)] = &[("normal", 0), ("multiply", 1), ("screen", 2), ("overlay", 3), ("darken", 4), ("lighten", 5), ("color_dodge", 6), ("color_burn", 7), ("hard_light", 8), ("soft_light", 9), ("difference", 10), ("exclusion", 11), ("hue", 12), ("saturation", 13), ("color", 14), ("luminosity", 15), ("add", 16)];

struct Ex<'a> { p: &'a Project, fps: f64, warn: Vec<String>, assets: Vec<Value>, ind: i64, embed: bool, comps: usize }

fn st(v: Value) -> Value { json!({"a": 0, "k": v}) }
fn kv(v: f64, dims: usize) -> Value { if dims == 1 { json!([r3(v)]) } else { json!(vec![r3(v); dims]) } }
fn kv0(v: f64, dims: usize) -> Value { if dims == 1 { json!(r3(v)) } else { json!(vec![r3(v); dims]) } }
fn lin_key(t: f64, v: f64, dims: usize) -> Value { json!({"t": r3(t), "s": kv(v, dims), "o": {"x": vec![0.0; dims], "y": vec![0.0; dims]}, "i": {"x": vec![1.0; dims], "y": vec![1.0; dims]}}) }

impl Ex<'_> {
    fn warn(&mut self, m: impl Into<String>) { let m = m.into(); if !self.warn.contains(&m) { self.warn.push(m); } }
    /// A kimo property as a Lottie animatable value. `f` maps kimo units to Lottie units (linear, so easing carries over);
    /// `dims` repeats the value (2 for scale). Exact easing where Lottie has it, per-frame samples elsewhere.
    fn anim(&mut self, p: Option<&Prop>, def: f64, f: &dyn Fn(f64) -> f64, dims: usize, dur: f64) -> Value {
        let Some(p) = p else { return st(kv0(f(def), dims)) };
        if !p.expr.is_empty() { return self.sampled(&|t| f(p.at(t)), dims, dur); }
        let k = &p.keys; if k.is_empty() { return st(kv0(f(p.base), dims)); } if k.len() == 1 { return st(kv0(f(k[0].v), dims)); }
        let mut out = vec![];
        for j in 0..k.len() {
            let a = &k[j]; let (t, v) = (a.t * self.fps, f(a.v));
            let Some(b) = k.get(j + 1) else { out.push(json!({"t": r3(t), "s": kv(v, dims)})); break };
            if a.ease == Ease::Hold { out.push(json!({"t": r3(t), "s": kv(v, dims), "h": 1})); continue; }
            let bez = if a.out.is_some() || b.inn.is_some() { None } else { a.bez.or(match a.ease { Ease::Linear => Some([0.0, 0.0, 1.0, 1.0]), Ease::In => Some([1.0 / 3.0, 0.0, 2.0 / 3.0, 0.0]), Ease::Out => Some([1.0 / 3.0, 1.0, 2.0 / 3.0, 1.0]), _ => None }) };
            match bez {
                Some(z) => out.push(json!({"t": r3(t), "s": kv(v, dims), "o": {"x": vec![z[0]; dims], "y": vec![z[1]; dims]}, "i": {"x": vec![z[2]; dims], "y": vec![z[3]; dims]}})),
                None => { // a curve Lottie cannot state exactly (back, elastic, bounce, tangent handles): one linear key per frame across the segment
                    let (f0, f1) = (a.t * self.fps, b.t * self.fps); let mut fr = f0; while fr < f1 - 1e-6 { out.push(lin_key(fr, f(p.raw(fr / self.fps)), dims)); fr = (fr.floor() + 1.0).max(fr + 1e-3); } }
            }
        }
        json!({"a": 1, "k": out})
    }
    fn sampled(&mut self, g: &dyn Fn(f64) -> f64, dims: usize, dur: f64) -> Value {
        let n = (dur * self.fps).ceil().max(1.0) as usize; let vals: Vec<f64> = (0..=n).map(|i| g(i as f64 / self.fps)).collect();
        if vals.iter().all(|v| (v - vals[0]).abs() < 1e-9) { return st(kv0(vals[0], dims)); }
        json!({"a": 1, "k": vals.iter().enumerate().map(|(i, v)| if i == n { json!({"t": i as f64, "s": kv(*v, dims)}) } else { lin_key(i as f64, *v, dims) }).collect::<Vec<_>>()})
    }

    /// The layer transform. `anchor` is where the layer's own content has its pivot (0,0 for centred vector content).
    fn transform(&mut self, c: &Clip, w: f64, h: f64, anchor: [f64; 2], extra_scale: [f64; 2]) -> Value {
        let (cx, cy, dur) = (w / 2.0, h / 2.0, c.duration);
        let fade = c.transition_in.as_ref().filter(|t| ["crossfade", "dissolve", "dip_black", "dip_white", "dip_color"].contains(&t.kind.as_str()) && t.duration > 0.0).cloned();
        if let Some(t) = c.transition_in.as_ref() { if fade.is_none() { self.warn(format!("transition '{}' is not exported (Lottie has no transitions); the clip cuts in", t.kind)); } }
        let a = st(json!([anchor[0], anchor[1], 0]));
        if c.parent.is_some() { // kimo parenting composes x/y/scale/rotation/opacity; Lottie's does not carry opacity, so the world transform is sampled
            let p = self.p; let s = |k: &'static str| move |t: f64| crate::render::xprop(p, c, k, t);
            let (fx, fy, fs, fr) = (s("x"), s("y"), s("scale"), s("rotation"));
            let op = opacity_fn(p, c, fade.clone());
            return json!({"a": a, "p": {"s": true, "x": self.sampled(&|t| fx(t) + cx, 1, dur), "y": self.sampled(&|t| fy(t) + cy, 1, dur)},
                "s": self.sampled(&|t| fs(t) * 100.0 * extra_scale[0], 2, dur), "r": self.sampled(&fr, 1, dur), "o": self.sampled(&|t| op(t) * 100.0, 1, dur)}); }
        let x = self.anim(c.props.get("x"), 0.0, &|v| v + cx, 1, dur); let y = self.anim(c.props.get("y"), 0.0, &|v| v + cy, 1, dur);
        let s = if (extra_scale[0] - extra_scale[1]).abs() < 1e-9 { self.anim(c.props.get("scale"), 1.0, &|v| v * 100.0 * extra_scale[0], 2, dur) }
            else { let sp = c.props.get("scale").cloned(); let es = extra_scale; let g = move |t: f64| sp.as_ref().map_or(1.0, |p| p.at(t)); let n = (dur * self.fps).ceil().max(1.0) as usize;
                json!({"a": 1, "k": (0..=n).map(|i| { let v = g(i as f64 / self.fps) * 100.0; json!({"t": i, "s": [v * es[0], v * es[1]], "o": {"x": [0, 0], "y": [0, 0]}, "i": {"x": [1, 1], "y": [1, 1]}}) }).collect::<Vec<_>>()}) };
        let r = self.anim(c.props.get("rotation"), 0.0, &|v| v, 1, dur);
        let o = if fade.is_some() { let op = opacity_fn(self.p, c, fade.clone()); self.sampled(&|t| op(t) * 100.0, 1, dur) } else { self.anim(c.props.get("opacity"), 1.0, &|v| v.clamp(0.0, 1.0) * 100.0, 1, dur) };
        json!({"a": a, "p": {"s": true, "x": x, "y": y}, "s": s, "r": r, "o": o})
    }
    fn layers(&mut self, tracks: &[Track], w: f64, h: f64) -> Vec<Value> {
        let mut out = vec![];
        for t in tracks.iter().rev() { let mut cl: Vec<&Clip> = t.clips.iter().map(|c| &**c).collect(); cl.sort_by(|a, b| b.start.partial_cmp(&a.start).unwrap_or(std::cmp::Ordering::Equal));
            for c in cl { if let Some(l) = self.layer(c, w, h) { out.push(l); } } }
        out
    }

    fn layer(&mut self, c: &Clip, w: f64, h: f64) -> Option<Value> {
        let fps = self.fps; self.ind += 1; let ind = self.ind;
        let nm = c.name.clone().unwrap_or_else(|| c.id.clone());
        let bm = BLEND.iter().find(|(n, _)| *n == c.blend).map(|b| b.1).unwrap_or_else(|| { self.warn(format!("blend mode '{}' has no Lottie equivalent; exported as normal", c.blend)); 0 });
        for e in &c.effects { if !e.kind.starts_with("shape_") { self.warn(format!("effect '{}' is not exported (Lottie players draw only a few effects)", e.kind)); } }
        if c.mask.is_some() { self.warn(format!("{nm}: masks are not exported")); }
        if c.motion_blur > 0.0 { self.warn("motion blur is not exported (Lottie players do not draw it)"); }
        if c.echo.is_some() || c.posterize_fps > 0.0 || c.time_remap.is_some() || c.speed_curve.is_some() { self.warn(format!("{nm}: time effects (echo, posterize time, time remap, speed ramps) are not exported")); }
        let mut l = json!({"ddd": 0, "ind": ind, "nm": nm, "sr": 1, "ao": 0, "ip": r3(c.start * fps), "op": r3((c.start + c.duration) * fps), "st": r3(c.start * fps), "bm": bm});
        match &c.source {
            Source::Shape { shape, width, height, fill, stroke_width, stroke_color, sides, corner_radius } => {
                let geom = self.geometry(c, shape, *width, *height, *sides, *corner_radius)?;
                let g = self.group(c, geom, Some(*fill).filter(|f| f[0] >= 0.0), *stroke_width, *stroke_color, [*width, *height]);
                l["ty"] = json!(4); l["ks"] = self.transform(c, w, h, [0.0, 0.0], [1.0, 1.0]); l["shapes"] = json!([g]);
            }
            Source::Text { text, font, size, color, letter_spacing, stroke_width, stroke_color, .. } => {
                let fx = c.text_fx.as_ref();
                if c.props.get("reveal").map_or(false, |p| !p.keys.is_empty() || p.base < 1.0) || fx.map_or(false, |f| !f.animators.is_empty()) { self.warn(format!("{nm}: text animators and typewriter reveal are not exported; the text is static")); }
                if fx.map_or(false, |f| f.bg.is_some() || f.shadow_opacity > 0.0 || f.glow > 0.0 || f.gradient.is_some()) { self.warn(format!("{nm}: text background, shadow, glow and gradient are not exported")); }
                let glyphs = match crate::text::glyph_paths(text, font, *size, *letter_spacing, fx.map_or(0.0, |f| f.weight), &align_of(&c.source), w, h, fx) { Ok(g) => g, Err(e) => { self.warn(format!("{nm}: text skipped ({e})")); return None; } };
                let mut items = vec![];
                for g in glyphs { let subs = d_to_subs(&g.d).unwrap_or_default(); if subs.is_empty() { continue; }
                    let mut it: Vec<Value> = subs.iter().map(|s| json!({"ty": "sh", "ks": st(sub_json(s))})).collect();
                    it.push(json!({"ty": "fl", "c": st(json!([color[0], color[1], color[2], 1])), "o": st(json!(100)), "r": 1}));
                    if *stroke_width > 0.0 { it.push(json!({"ty": "st", "c": st(json!([stroke_color[0], stroke_color[1], stroke_color[2], 1])), "o": st(json!(100)), "w": st(json!(stroke_width)), "lc": 2, "lj": 2, "ml": 4})); }
                    it.push(tr_identity()); items.push(json!({"ty": "gr", "nm": g.ch.to_string(), "it": it})); }
                l["ty"] = json!(4); l["ks"] = self.transform(c, w, h, [0.0, 0.0], [1.0, 1.0]); l["shapes"] = json!(items);
            }
            Source::Solid { color } => { l["ty"] = json!(1); l["sc"] = json!(hex(color)); l["sw"] = json!(w); l["sh"] = json!(h); l["ks"] = self.transform(c, w, h, [w / 2.0, h / 2.0], [1.0, 1.0]); }
            Source::Gradient { c1, c2 } => {
                let it = vec![json!({"ty": "rc", "d": 1, "s": st(json!([w, h])), "p": st(json!([0, 0])), "r": st(json!(0))}),
                    json!({"ty": "gf", "t": 1, "s": st(json!([0, -h / 2.0])), "e": st(json!([0, h / 2.0])), "g": {"p": 2, "k": st(json!([0, c1[0], c1[1], c1[2], 1, c2[0], c2[1], c2[2]]))}, "o": st(json!(100)), "r": 1}), tr_identity()];
                l["ty"] = json!(4); l["ks"] = self.transform(c, w, h, [0.0, 0.0], [1.0, 1.0]); l["shapes"] = json!([{"ty": "gr", "it": it}]);
            }
            Source::Precomp { tracks } => {
                self.comps += 1; let id = format!("comp_{}", self.comps); let layers = self.layers(tracks, w, h);
                self.assets.push(json!({"id": id, "nm": nm, "layers": layers}));
                l["ty"] = json!(0); l["refId"] = json!(id); l["w"] = json!(w); l["h"] = json!(h); l["ks"] = self.transform(c, w, h, [w / 2.0, h / 2.0], [1.0, 1.0]);
            }
            Source::Media { asset } => {
                let Some(a) = self.p.assets.get(asset) else { return None };
                let path = self.p.resolve(&a.path);
                if !crate::render::is_still_path(&path) { self.warn(format!("{nm}: video and audio clips are not exported (Lottie holds only still images)")); return None; }
                let (aw, ah) = (a.width.max(1) as f64, a.height.max(1) as f64);
                let fit = c.props.get("fit").map_or(0.0, |q| q.at(0.0)); let (kx, ky) = if fit >= 1.5 { (w / aw, h / ah) } else { let k = if fit >= 0.5 { (w / aw).max(h / ah) } else { (w / aw).min(h / ah) }; (k, k) };
                let id = format!("image_{}", self.assets.len());
                let asset_json = if self.embed { let bytes = std::fs::read(&path).ok()?; if bytes.len() > 12 << 20 { self.warn(format!("{nm}: image over 12 MB not embedded")); return None; }
                        let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default(); let mime = match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "webp" => "image/webp", "gif" => "image/gif", _ => "image/png" };
                        json!({"id": id, "w": aw, "h": ah, "u": "", "p": format!("data:{mime};base64,{}", crate::perception::audio_raw::b64(&bytes)), "e": 1}) }
                    else { let pp = std::path::Path::new(&path); json!({"id": id, "w": aw, "h": ah, "u": pp.parent().map(|d| format!("{}/", d.to_string_lossy())).unwrap_or_default(), "p": pp.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(), "e": 0}) };
                self.assets.push(asset_json);
                l["ty"] = json!(2); l["refId"] = json!(id); l["ks"] = self.transform(c, w, h, [aw / 2.0, ah / 2.0], [kx, ky]);
            }
            other => { let k = serde_json::to_value(other).ok().and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from)).unwrap_or_else(|| "this".into()); self.warn(format!("{k} layers are not exported (no Lottie equivalent)")); return None; }
        }
        Some(l)
    }

    fn geometry(&mut self, c: &Clip, shape: &str, w: f64, h: f64, sides: u32, cr: f64) -> Option<Vec<Value>> {
        Some(match shape {
            "ellipse" => vec![json!({"ty": "el", "d": 1, "s": st(json!([w, h])), "p": st(json!([0, 0]))})],
            "polygon" => vec![json!({"ty": "sr", "sy": 2, "d": 1, "pt": st(json!(sides.max(3))), "p": st(json!([0, 0])), "r": st(json!(0)), "or": st(json!(w.min(h) / 2.0)), "os": st(json!(0))})],
            "path" => {
                let e = c.effects.iter().find(|e| e.kind == "shape_path")?; let base = e.strings.get("d")?;
                if e.strings.get("fill_rule").map_or(false, |r| r == "evenodd") { self.warn("even-odd fill becomes Lottie's even-odd fill rule; players that ignore it fill holes"); }
                let keys: Vec<(f64, String, String)> = e.strings.get("keys").and_then(|k| serde_json::from_str(k).ok()).unwrap_or_default();
                let base = d_to_subs(base).ok()?;
                if keys.is_empty() { return Some(base.iter().map(|s| json!({"ty": "sh", "ks": st(sub_json(s))})).collect()); }
                let ks: Vec<(f64, Vec<Sub>, String)> = keys.iter().filter_map(|(t, d, e)| Some((*t, d_to_subs(d).ok()?, e.clone()))).collect();
                if ks.iter().any(|k| k.1.len() != ks[0].1.len() || k.1.iter().zip(&ks[0].1).any(|(a, b)| a.v.len() != b.v.len())) { self.warn("a path morph whose keys differ in point count is exported static"); return Some(base.iter().map(|s| json!({"ty": "sh", "ks": st(sub_json(s))})).collect()); }
                (0..ks[0].1.len()).map(|si| { let mut kk = vec![];
                    for (j, (t, subs, ease)) in ks.iter().enumerate() { let s = json!([sub_json(&subs[si])]); let t = r3(t * self.fps);
                        if j + 1 == ks.len() { kk.push(json!({"t": t, "s": s})); } else if ease == "hold" { kk.push(json!({"t": t, "s": s, "h": 1})); }
                        else { let z = match ease.as_str() { "in" => [1.0 / 3.0, 0.0, 2.0 / 3.0, 0.0], "out" => [1.0 / 3.0, 1.0, 2.0 / 3.0, 1.0], "in_out" => [0.65, 0.0, 0.35, 1.0], _ => [0.0, 0.0, 1.0, 1.0] };
                            kk.push(json!({"t": t, "s": s, "o": {"x": [z[0]], "y": [z[1]]}, "i": {"x": [z[2]], "y": [z[3]]}})); } }
                    json!({"ty": "sh", "ks": {"a": 1, "k": kk}}) }).collect()
            }
            "line" => vec![json!({"ty": "rc", "d": 1, "s": st(json!([w, h.max(2.0)])), "p": st(json!([0, 0])), "r": st(json!(0))})],
            _ => vec![json!({"ty": "rc", "d": 1, "s": st(json!([w, h])), "p": st(json!([0, 0])), "r": st(json!(cr))})],
        })
    }

    /// One shape group: geometry, then modifiers (offset, trim), paint (fill or gradient fill, stroke or gradient stroke), repeater, transform.
    fn group(&mut self, c: &Clip, geom: Vec<Value>, fill: Option<[f64; 3]>, sw: f64, sc_: [f64; 3], size: [f64; 2]) -> Value {
        let dur = c.duration; let mut it = geom; let mut rep = None; let (mut gfill, mut gstroke) = (None, None);
        let ends = |ang: f64| { let (s, co) = ang.to_radians().sin_cos(); let ext = (size[0] / 2.0 * co).abs() + (size[1] / 2.0 * s).abs(); (json!([-co * ext, -s * ext]), json!([co * ext, s * ext])) };
        for e in &c.effects { let p = |k: &str| e.params.get(k).map_or(0.0, |q| q.base);
            match e.kind.as_str() {
                "shape_offset" => { let a = self.anim(e.params.get("amount"), 0.0, &|v| v, 1, dur); it.push(json!({"ty": "op", "a": a, "lj": 2, "ml": st(json!(4))})); }
                "shape_trim" => { let s = self.anim(e.params.get("start"), 0.0, &|v| v * 100.0, 1, dur); let en = self.anim(e.params.get("end"), 1.0, &|v| v * 100.0, 1, dur); let o = self.anim(e.params.get("offset"), 0.0, &|v| v * 360.0, 1, dur);
                    it.push(json!({"ty": "tm", "s": s, "e": en, "o": o, "m": 1})); }
                "shape_repeater" => { rep = Some(json!({"ty": "rp", "c": st(json!(p("copies").round().clamp(1.0, 64.0))), "o": st(json!(0)), "m": 1,
                    "tr": {"ty": "tr", "p": st(json!([p("dx"), p("dy")])), "a": st(json!([0, 0])), "s": st(json!([p("scale") * 100.0, p("scale") * 100.0])), "r": st(json!(p("rotation"))), "so": st(json!(100)), "eo": st(json!(100))}})); }
                "shape_gradient_fill" => { let (s, en) = ends(p("angle")); gfill = Some(json!({"ty": "gf", "t": 1, "s": st(s), "e": st(en), "g": {"p": 2, "k": st(json!([0, p("r1"), p("g1"), p("b1"), 1, p("r2"), p("g2"), p("b2")]))}, "o": st(json!(100)), "r": 1})); }
                "shape_gradient_stroke" => { let (s, en) = ends(p("angle")); gstroke = Some((s, en, [p("r1"), p("g1"), p("b1"), p("r2"), p("g2"), p("b2")])); }
                "shape_merge" => self.warn("merge paths is not exported; the shape is exported without it"),
                _ => {}
            } }
        let fa = self.anim(c.props.get("fill_alpha"), 1.0, &|v| v.clamp(0.0, 1.0) * 100.0, 1, dur);
        if let Some(g) = gfill { let mut g = g; g["o"] = fa.clone(); it.push(g); } else if let Some(f) = fill { it.push(json!({"ty": "fl", "c": st(json!([f[0], f[1], f[2], 1])), "o": fa, "r": 1})); }
        if sw > 0.0 { let so = self.anim(c.props.get("stroke_alpha"), 1.0, &|v| v.clamp(0.0, 1.0) * 100.0, 1, dur);
            match gstroke { Some((s, en, g)) => it.push(json!({"ty": "gs", "t": 1, "s": st(s), "e": st(en), "g": {"p": 2, "k": st(json!([0, g[0], g[1], g[2], 1, g[3], g[4], g[5]]))}, "o": so, "w": st(json!(sw)), "lc": 2, "lj": 2, "ml": 4})),
                None => it.push(json!({"ty": "st", "c": st(json!([sc_[0], sc_[1], sc_[2], 1])), "o": so, "w": st(json!(sw)), "lc": 2, "lj": 2, "ml": 4})) } }
        if let Some(r) = rep { it.push(r); }
        it.push(tr_identity());
        json!({"ty": "gr", "nm": c.name.clone().unwrap_or_else(|| "shape".into()), "it": it})
    }
}
fn opacity_fn<'b>(p: &'b Project, c: &'b Clip, fade: Option<crate::model::Transition>) -> impl Fn(f64) -> f64 + 'b {
    move |t: f64| { let base = if c.parent.is_some() { crate::render::xprop(p, c, "opacity", t) } else { c.props.get("opacity").map_or(1.0, |q| q.at(t)) }.clamp(0.0, 1.0);
        match &fade { Some(tr) => base * crate::transitions::ease(&tr.ease, (t / tr.duration).clamp(0.0, 1.0)), None => base } }
}
fn tr_identity() -> Value { json!({"ty": "tr", "p": st(json!([0, 0])), "a": st(json!([0, 0])), "s": st(json!([100, 100])), "r": st(json!(0)), "o": st(json!(100))}) }
fn hex(c: &[f64; 3]) -> String { format!("#{:02x}{:02x}{:02x}", (c[0].clamp(0.0, 1.0) * 255.0).round() as u8, (c[1].clamp(0.0, 1.0) * 255.0).round() as u8, (c[2].clamp(0.0, 1.0) * 255.0).round() as u8) }
fn align_of(s: &Source) -> String { match s { Source::Text { align, .. } if !align.is_empty() => align.clone(), _ => "center".into() } }

pub fn export_doc(p: &Project, embed: bool) -> (Value, Vec<String>) {
    let fps = if p.fps > 0.0 { p.fps } else { 30.0 }; let (w, h) = (p.width as f64, p.height as f64);
    let mut ex = Ex { p, fps, warn: vec![], assets: vec![], ind: 0, embed, comps: 0 };
    let layers = ex.layers(&p.tracks, w, h);
    let markers: Vec<Value> = p.markers.iter().map(|m| json!({"tm": r3(m.time * fps), "cm": m.name, "dr": r3(m.duration * fps)})).collect();
    let doc = json!({"v": "5.7.4", "fr": fps, "ip": 0, "op": (p.duration() * fps).ceil().max(1.0), "w": p.width, "h": p.height, "nm": if p.name.is_empty() { "kimo" } else { p.name.as_str() }, "ddd": 0, "assets": ex.assets, "layers": layers, "markers": markers, "meta": {"g": "kimo"}});
    (doc, ex.warn)
}

fn export_lottie(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let embed = a.get("embed_images").and_then(|v| v.as_bool()).unwrap_or(true);
    if ss.project.tracks.iter().all(|t| t.clips.is_empty()) { return Err("the project is empty; nothing to export".into()); }
    let (doc, warnings) = export_doc(&ss.project, embed);
    let text = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
    let layers = doc["layers"].as_array().map_or(0, |l| l.len());
    let mut r = json!({"layers": layers, "frames": doc["op"], "fps": doc["fr"], "warnings": warnings, "bytes": text.len()});
    match a.get("path").and_then(|v| v.as_str()) {
        Some(path) => { if !path.to_lowercase().ends_with(".json") { return Err("path must end in .json (a Lottie file)".into()); }
            if std::path::Path::new(path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
            std::fs::write(path, &text).map_err(|e| format!("could not write {path}: {e}"))?; r["path"] = json!(path); }
        None => { if text.len() > 2_000_000 { return Err("the Lottie JSON is over 2 MB; pass path to write it to a file".into()); } r["lottie"] = doc; }
    }
    Ok(r)
}

// ---------------------------------------------------------------- import

fn num(v: &Value) -> f64 { v.as_f64().or_else(|| v.as_array().and_then(|a| a.first()).and_then(|x| x.as_f64())).unwrap_or(0.0) }
fn nums(v: &Value) -> Vec<f64> { match v { Value::Array(a) => a.iter().map(num).collect(), Value::Number(_) => vec![num(v)], _ => vec![] } }
fn is_anim(p: &Value) -> bool { p.get("k").and_then(|k| k.as_array()).and_then(|a| a.first()).map_or(false, |f| f.is_object() && f.get("t").is_some()) }
fn kfs(p: &Value) -> &[Value] { p.get("k").and_then(|k| k.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) }
/// A key's start value; old files keep the end value in `e` and leave the last key without `s`.
fn key_s(ks: &[Value], j: usize) -> Vec<f64> { ks[j].get("s").map(nums).filter(|v| !v.is_empty()).or_else(|| if j > 0 { ks[j - 1].get("e").map(nums) } else { None }).unwrap_or_default() }
fn handle(k: &Value, which: &str, axis: &str, d: usize) -> f64 { let v = &k[which][axis]; match v { Value::Array(a) => a.get(d).or(a.first()).and_then(|x| x.as_f64()).unwrap_or(0.0), _ => v.as_f64().unwrap_or(if which == "o" { 0.0 } else { 1.0 }) } }
fn bez_y(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let b = |p1: f64, p2: f64, t: f64| 3.0 * (1.0 - t).powi(2) * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t;
    let (mut lo, mut hi) = (0.0, 1.0); for _ in 0..50 { let m = 0.5 * (lo + hi); if b(x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0), m) < x { lo = m; } else { hi = m; } } b(y1, y2, 0.5 * (lo + hi))
}
/// The value of a Lottie property at a frame (all dimensions).
fn eval(p: &Value, f: f64) -> Vec<f64> {
    if !is_anim(p) { return p.get("k").map(nums).unwrap_or_default(); }
    let ks = kfs(p); let n = ks.len(); let t = |j: usize| ks[j].get("t").map(num).unwrap_or(0.0);
    if f <= t(0) { return key_s(ks, 0); }
    for j in 0..n.saturating_sub(1) { if f < t(j + 1) {
        let a = key_s(ks, j); if ks[j].get("h").map(num) == Some(1.0) { return a; }
        let b = ks[j].get("e").map(nums).filter(|v| !v.is_empty()).unwrap_or_else(|| key_s(ks, j + 1)); let u = (f - t(j)) / (t(j + 1) - t(j)).max(1e-9);
        return a.iter().enumerate().map(|(d, av)| { let bv = b.get(d).copied().unwrap_or(*av); let y = bez_y(handle(&ks[j], "o", "x", d), handle(&ks[j], "o", "y", d), handle(&ks[j], "i", "x", d), handle(&ks[j], "i", "y", d), u); av + (bv - av) * y }).collect(); } }
    let last = key_s(ks, n - 1); if last.is_empty() && n > 1 { ks[n - 2].get("e").map(nums).unwrap_or_default() } else { last }
}
fn eval1(p: &Value, f: f64, d: usize, def: f64) -> f64 { if p.is_null() { return def; } eval(p, f).get(d).copied().unwrap_or(def) }
/// A Lottie property as a kimo Prop (one dimension), keeping keyframe times and Bezier easing; `m` maps units linearly.
fn to_prop(p: &Value, d: usize, def: f64, m: &dyn Fn(f64) -> f64, tmap: &dyn Fn(f64) -> f64) -> Prop {
    if p.is_null() { return Prop::new(m(def)); }
    if !is_anim(p) { return Prop::new(m(eval1(p, 0.0, d, def))); }
    let ks = kfs(p); let mut out = Prop::new(m(def));
    for j in 0..ks.len() { let s = key_s(ks, j); let Some(v) = s.get(d).or(s.first()) else { continue }; let t = tmap(ks[j].get("t").map(num).unwrap_or(0.0));
        let hold = ks[j].get("h").map(num) == Some(1.0); let has = ks[j].get("o").is_some() && ks[j].get("i").is_some();
        let bez = if !hold && has && j + 1 < ks.len() { Some([handle(&ks[j], "o", "x", d), handle(&ks[j], "o", "y", d), handle(&ks[j], "i", "x", d), handle(&ks[j], "i", "y", d)]) } else { None };
        out.keys.push(Key { t, v: m(*v), ease: if hold { Ease::Hold } else { Ease::Linear }, bez, out: None, inn: None }); }
    out.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal)); out.keys.dedup_by(|b, a| (a.t - b.t).abs() < 1e-9);
    if let Some(k) = out.keys.first() { out.base = k.v; } out
}
fn prop_static(p: &Prop) -> bool { p.keys.len() < 2 }

/// A transform (layer `ks` or group `tr`).
#[derive(Clone)]
struct Xf { a: Value, px: Value, py: Value, pxy: Value, s: Value, r: Value, o: Value, skew: bool }
fn xf(v: &Value) -> Xf {
    let p = &v["p"]; let split = p.get("s").and_then(|s| s.as_bool()).unwrap_or(false);
    Xf { a: v["a"].clone(), px: if split { p["x"].clone() } else { Value::Null }, py: if split { p["y"].clone() } else { Value::Null }, pxy: if split { Value::Null } else { p.clone() },
        s: v["s"].clone(), r: if v["r"].is_null() { v["rz"].clone() } else { v["r"].clone() }, o: v["o"].clone(), skew: v.get("sk").map_or(false, |k| eval1(k, 0.0, 0, 0.0).abs() > 1e-6) }
}
impl Xf {
    fn animated(&self) -> bool { [&self.a, &self.px, &self.py, &self.pxy, &self.s, &self.r, &self.o].iter().any(|p| is_anim(p)) }
    fn pos(&self, f: f64) -> [f64; 2] { if self.pxy.is_null() { [eval1(&self.px, f, 0, 0.0), eval1(&self.py, f, 0, 0.0)] } else { [eval1(&self.pxy, f, 0, 0.0), eval1(&self.pxy, f, 1, 0.0)] } }
    fn mat(&self, f: f64) -> Aff { let (p, a) = (self.pos(f), [eval1(&self.a, f, 0, 0.0), eval1(&self.a, f, 1, 0.0)]); let s = [eval1(&self.s, f, 0, 100.0) / 100.0, eval1(&self.s, f, 1, 100.0) / 100.0];
        mul(&mul(&mul(&tr(p[0], p[1]), &rot(eval1(&self.r, f, 0, 0.0))), &sc(s[0], s[1])), &tr(-a[0], -a[1])) }
    fn op(&self, f: f64) -> f64 { eval1(&self.o, f, 0, 100.0) / 100.0 }
    /// Keys can be carried over one to one: static anchor, uniform scale, no skew.
    fn direct(&self) -> bool { !is_anim(&self.a) && !self.skew && (!is_anim(&self.s) || kfs(&self.s).iter().all(|k| { let s = nums(&k["s"]); s.len() < 2 || (s[0] - s[1]).abs() < 1e-9 })) && { let s = eval(&self.s, 0.0); s.len() < 2 || (s[0] - s[1]).abs() < 1e-9 } }
}

/// Paint and modifiers in scope for a group's paths.
#[derive(Clone, Default)]
struct Style { fill: Option<([f64; 3], Value, bool)>, stroke: Option<([f64; 3], Value, Value)>, gfill: Option<[f64; 7]>, trims: Vec<(Value, Value, Value)>, rep: Option<[f64; 5]> }
enum Node { Leaf { subs: Vec<Sub>, morph: Option<Value>, style: Style }, Group { chain: Vec<Xf>, children: Vec<Node> }, Text { doc: Value, font: String }, Comp { chain: Vec<Xf>, layers: Vec<Value>, ip: f64, st: f64 } }

fn rect_subs(c: [f64; 2], s: [f64; 2], r: f64) -> Sub {
    let (hw, hh) = (s[0] / 2.0, s[1] / 2.0); let r = r.min(hw).min(hh).max(0.0); let k = 0.5523 * r;
    if r < 1e-6 { return Sub { v: vec![[c[0] + hw, c[1] - hh], [c[0] + hw, c[1] + hh], [c[0] - hw, c[1] + hh], [c[0] - hw, c[1] - hh]], i: vec![[0.0; 2]; 4], o: vec![[0.0; 2]; 4], c: true }; }
    let (l, rt, t, b) = (c[0] - hw, c[0] + hw, c[1] - hh, c[1] + hh);
    Sub { v: vec![[rt, t + r], [rt, b - r], [rt - r, b], [l + r, b], [l, b - r], [l, t + r], [l + r, t], [rt - r, t]],
        i: vec![[0.0, -k], [0.0; 2], [k, 0.0], [0.0; 2], [0.0, k], [0.0; 2], [-k, 0.0], [0.0; 2]], o: vec![[0.0; 2], [0.0, k], [0.0; 2], [-k, 0.0], [0.0; 2], [0.0, -k], [0.0; 2], [k, 0.0]], c: true }
}
fn ellipse_subs(c: [f64; 2], s: [f64; 2]) -> Sub { let (rx, ry) = (s[0] / 2.0, s[1] / 2.0); let (kx, ky) = (0.5523 * rx, 0.5523 * ry);
    Sub { v: vec![[c[0], c[1] - ry], [c[0] + rx, c[1]], [c[0], c[1] + ry], [c[0] - rx, c[1]]], i: vec![[-kx, 0.0], [0.0, -ky], [kx, 0.0], [0.0, ky]], o: vec![[kx, 0.0], [0.0, ky], [-kx, 0.0], [0.0, -ky]], c: true } }
fn star_subs(it: &Value) -> Sub { let c = eval(&it["p"], 0.0); let c = [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0)];
    let n = eval1(&it["pt"], 0.0, 0, 5.0).round().max(3.0) as usize; let (or, ir) = (eval1(&it["or"], 0.0, 0, 50.0), eval1(&it["ir"], 0.0, 0, 25.0)); let r0 = eval1(&it["r"], 0.0, 0, 0.0).to_radians();
    let star = it["sy"].as_u64() == Some(1); let m = if star { n * 2 } else { n };
    let v: Vec<[f64; 2]> = (0..m).map(|i| { let a = r0 - std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / m as f64; let r = if star && i % 2 == 1 { ir } else { or }; [c[0] + r * a.cos(), c[1] + r * a.sin()] }).collect();
    Sub { i: vec![[0.0; 2]; v.len()], o: vec![[0.0; 2]; v.len()], v, c: true } }
fn shape_val(v: &Value) -> Option<Sub> { let v = if v.is_array() { v.get(0)? } else { v };
    let pts = |k: &str| -> Vec<[f64; 2]> { v[k].as_array().map(|a| a.iter().map(|p| [num(&p[0]), num(&p[1])]).collect()).unwrap_or_default() };
    let s = Sub { v: pts("v"), i: pts("i"), o: pts("o"), c: v["c"].as_bool().unwrap_or(false) }; if s.v.is_empty() || s.i.len() != s.v.len() || s.o.len() != s.v.len() { return None; } Some(s) }
fn color3(v: &Value) -> [f64; 3] { let c = eval(v, 0.0); let g = |i: usize| c.get(i).copied().unwrap_or(0.0); let k = if c.iter().take(3).any(|x| *x > 1.0) { 1.0 / 255.0 } else { 1.0 }; [g(0) * k, g(1) * k, g(2) * k] }

struct Im<'a> { warn: Vec<String>, fr: f64, assets: &'a [Value], fonts: &'a Value, depth: usize }
impl Im<'_> {
    fn warn(&mut self, m: impl Into<String>) { let m = m.into(); if !self.warn.contains(&m) { self.warn.push(m); } }

    /// Shape items (top first) to nodes, with `inherited` paint from enclosing groups.
    fn items(&mut self, items: &[Value], inherited: &Style) -> Vec<Node> {
        let mut style = inherited.clone(); let mut own_fill = false; let mut own_stroke = false;
        for it in items { if it.get("hd").and_then(|h| h.as_bool()) == Some(true) { continue; }
            match it["ty"].as_str().unwrap_or("") {
                "fl" => if !own_fill { own_fill = true; style.fill = Some((color3(&it["c"]), it["o"].clone(), it["r"].as_u64() == Some(2))); style.gfill = None; if is_anim(&it["c"]) { self.warn("animated fill colours import as their first colour"); } },
                "gf" => if !own_fill { own_fill = true; let g = eval(&it["g"]["k"], 0.0); let np = it["g"]["p"].as_u64().unwrap_or(2) as usize; let (s, e) = (eval(&it["s"], 0.0), eval(&it["e"], 0.0));
                    if g.len() >= 8 && s.len() >= 2 && e.len() >= 2 { let last = (np.max(2) - 1) * 4; let ang = (e[1] - s[1]).atan2(e[0] - s[0]).to_degrees();
                        style.gfill = Some([g[1], g[2], g[3], g.get(last + 1).copied().unwrap_or(g[1]), g.get(last + 2).copied().unwrap_or(g[2]), g.get(last + 3).copied().unwrap_or(g[3]), ang]);
                        style.fill = Some(([g[1], g[2], g[3]], it["o"].clone(), it["r"].as_u64() == Some(2))); if np > 2 { self.warn("gradients with more than two stops import with their first and last colours"); } if it["t"].as_u64() == Some(2) { self.warn("radial gradients import as linear"); } } },
                "st" | "gs" => if !own_stroke { own_stroke = true; let col = if it["ty"] == "st" { color3(&it["c"]) } else { let g = eval(&it["g"]["k"], 0.0); [g.get(1).copied().unwrap_or(1.0), g.get(2).copied().unwrap_or(1.0), g.get(3).copied().unwrap_or(1.0)] };
                    style.stroke = Some((col, it["o"].clone(), it["w"].clone())); if it.get("d").and_then(|d| d.as_array()).map_or(false, |d| !d.is_empty()) { self.warn("dashed strokes import solid"); } },
                "tm" => style.trims.push((it["s"].clone(), it["e"].clone(), it["o"].clone())),
                "rp" => style.rep = Some([eval1(&it["c"], 0.0, 0, 3.0), eval1(&it["tr"]["p"], 0.0, 0, 0.0), eval1(&it["tr"]["p"], 0.0, 1, 0.0), eval1(&it["tr"]["r"], 0.0, 0, 0.0), eval1(&it["tr"]["s"], 0.0, 0, 100.0) / 100.0]),
                "rd" | "mm" | "zz" | "pb" | "tw" | "op" | "ms" => self.warn(format!("shape modifier '{}' is not imported", it["ty"].as_str().unwrap_or(""))),
                _ => {}
            } }
        let mut out = vec![]; let mut geo: Vec<Sub> = vec![]; let mut morph: Option<Value> = None; let mut geo_at: Option<usize> = None; let mut n_anim = 0;
        for it in items { if it.get("hd").and_then(|h| h.as_bool()) == Some(true) { continue; }
            let ty = it["ty"].as_str().unwrap_or("");
            if matches!(ty, "sh" | "rc" | "el" | "sr") && geo_at.is_none() { geo_at = Some(out.len()); }
            match ty {
                "gr" => { let its = it["it"].as_array().cloned().unwrap_or_default(); let trv = its.iter().find(|x| x["ty"] == "tr").cloned();
                    let children = self.items(&its, &style); if children.is_empty() { continue; }
                    out.push(Node::Group { chain: trv.map(|t| vec![xf(&t)]).unwrap_or_default(), children }); }
                "sh" => { if is_anim(&it["ks"]) { n_anim += 1; morph = Some(it["ks"].clone()); } let v = if is_anim(&it["ks"]) { kfs(&it["ks"]).first().map(|k| k["s"].clone()).unwrap_or(Value::Null) } else { it["ks"]["k"].clone() };
                    if let Some(s) = shape_val(&v) { geo.push(if it["d"].as_u64() == Some(3) { reverse(s) } else { s }); } }
                "rc" => { let (p, s) = (eval(&it["p"], 0.0), eval(&it["s"], 0.0)); if s.len() >= 2 { geo.push(rect_subs([p.first().copied().unwrap_or(0.0), p.get(1).copied().unwrap_or(0.0)], [s[0], s[1]], eval1(&it["r"], 0.0, 0, 0.0))); } if is_anim(&it["s"]) || is_anim(&it["p"]) { self.warn("animated rectangle/ellipse/star sizes import as their first value"); } }
                "el" => { let (p, s) = (eval(&it["p"], 0.0), eval(&it["s"], 0.0)); if s.len() >= 2 { geo.push(ellipse_subs([p.first().copied().unwrap_or(0.0), p.get(1).copied().unwrap_or(0.0)], [s[0], s[1]])); } if is_anim(&it["s"]) || is_anim(&it["p"]) { self.warn("animated rectangle/ellipse/star sizes import as their first value"); } }
                "sr" => geo.push(star_subs(it)),
                _ => {}
            } }
        if !geo.is_empty() && (style.fill.is_some() || style.stroke.is_some()) {
            if n_anim > 1 || (n_anim == 1 && geo.len() > 1) { self.warn("a group with several paths where one morphs imports static"); morph = None; }
            out.insert(geo_at.unwrap_or(0).min(out.len()), Node::Leaf { subs: geo, morph, style });
        }
        out
    }

    /// A layer list (top first) to nodes with their time spans: (node, ip, op, st).
    fn layer_nodes(&mut self, layers: &[Value]) -> Vec<(Node, f64, f64, f64, String)> {
        let by_ind = |i: i64| layers.iter().find(|l| l["ind"].as_i64() == Some(i));
        let mut out = vec![];
        for l in layers {
            let nm = l["nm"].as_str().unwrap_or("layer").to_string();
            if l.get("hd").and_then(|h| h.as_bool()) == Some(true) || l["td"].as_u64() == Some(1) { if l["td"].as_u64() == Some(1) { self.warn("track mattes are not imported; matte layers are skipped"); } continue; }
            if l["tt"].as_u64().unwrap_or(0) > 0 { self.warn("track mattes are not imported; matted layers show unmatted"); }
            if l.get("masksProperties").and_then(|m| m.as_array()).map_or(false, |m| !m.is_empty()) { self.warn("layer masks are not imported"); }
            if l.get("ef").and_then(|m| m.as_array()).map_or(false, |m| !m.is_empty()) { self.warn("layer effects are not imported"); }
            if l["ddd"].as_u64() == Some(1) { self.warn("3D layers import flat"); }
            if l["sr"].as_f64().map_or(false, |s| (s - 1.0).abs() > 1e-9) { self.warn("time-stretched layers import at normal speed"); }
            // the parent chain, outermost first
            let mut chain = vec![xf(&l["ks"])]; let mut cur = l.clone(); let mut guard = 0;
            while let Some(pi) = cur["parent"].as_i64() { guard += 1; if guard > 32 { break; } let Some(pl) = by_ind(pi) else { break }; chain.insert(0, xf(&pl["ks"])); cur = pl.clone(); }
            let (ip, op, st) = (num(&l["ip"]), num(&l["op"]), num(&l["st"]));
            let node = match l["ty"].as_u64().unwrap_or(99) {
                4 => { let sh = l["shapes"].as_array().cloned().unwrap_or_default(); let ch = self.items(&sh, &Style::default()); if ch.is_empty() { continue; } Node::Group { chain, children: ch } }
                1 => { let (w, h) = (num(&l["sw"]), num(&l["sh"])); let col = crate::svgimport::parse_color(l["sc"].as_str().unwrap_or("#000000"), [0.0; 3]).ok().flatten().map(|c| c.0).unwrap_or([0.0; 3]);
                    Node::Group { chain, children: vec![Node::Leaf { subs: vec![rect_subs([w / 2.0, h / 2.0], [w, h], 0.0)], morph: None, style: Style { fill: Some((col, Value::Null, false)), ..Default::default() } }] } }
                5 => { let doc = l["t"]["d"]["k"].as_array().and_then(|a| a.first()).map(|k| k["s"].clone()).unwrap_or(Value::Null); if doc.is_null() { continue; }
                    if l["t"]["d"]["k"].as_array().map_or(0, |a| a.len()) > 1 { self.warn("text that changes over time imports with its first text"); }
                    if l["t"]["a"].as_array().map_or(false, |a| !a.is_empty()) { self.warn("text animators are not imported"); }
                    let fam = doc["f"].as_str().unwrap_or("").to_string(); let font = self.fonts["list"].as_array().and_then(|fl| fl.iter().find(|f| f["fName"].as_str() == Some(fam.as_str()))).and_then(|f| f["fFamily"].as_str()).unwrap_or(&fam).to_string();
                    Node::Group { chain, children: vec![Node::Text { doc, font }] } }
                0 => { let Some(asset) = self.assets.iter().find(|a| a["id"] == l["refId"]) else { self.warn(format!("{nm}: precomp asset missing")); continue };
                    if l.get("tm").is_some() { self.warn("precomp time remapping is not imported"); }
                    if self.depth > 16 { self.warn("precomps nested deeper than 16 are skipped"); continue; }
                    Node::Comp { chain, layers: asset["layers"].as_array().cloned().unwrap_or_default(), ip, st } }
                3 => continue, // nulls only move their children (followed through the parent chain)
                2 => { self.warn("image layers are not imported"); continue; }
                6 => { self.warn("audio layers are not imported"); continue; }
                _ => { self.warn("camera and other non-visual layers are skipped"); continue; }
            };
            out.push((node, ip, op, st, nm));
        }
        out
    }
}
fn reverse(s: Sub) -> Sub { let n = s.v.len(); let idx: Vec<usize> = (0..n).rev().collect(); Sub { v: idx.iter().map(|&k| s.v[k]).collect(), i: idx.iter().map(|&k| s.o[k]).collect(), o: idx.iter().map(|&k| s.i[k]).collect(), c: s.c } }

/// Turns nodes into kimo clips. `m` maps node space to the current clip space (pixels from the canvas centre).
struct Emit<'a, 'b> { ss: &'a mut Session, im: Im<'b> }
impl Emit<'_, '_> {
    /// Clips for `node` (top first). Times: clip spans [start, start+dur] on the current clock; `tmap` maps a Lottie frame of this layer to clip-local seconds.
    fn node(&mut self, node: &Node, m: &Aff, op_mul: f64, start: f64, dur: f64, tmap: &dyn Fn(f64) -> f64, nm: &str) -> Vec<Clip> {
        let fr = self.im.fr;
        match node {
            Node::Leaf { subs, morph, style } => {
                let world = map_subs(subs, m); let k = det(m).abs().sqrt(); let d = subs_to_d(&world); if d.is_empty() { return vec![]; }
                let (x0, y0, x1, y1) = world.iter().flat_map(|s| s.v.iter()).fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| (b.0.min(p[0]), b.1.min(p[1]), b.2.max(p[0]), b.3.max(p[1])));
                let sw = style.stroke.as_ref().map_or(0.0, |s| eval1(&s.2, 0.0, 0, 1.0) * k); let scol = style.stroke.as_ref().map_or([0.0; 3], |s| s.0);
                let evenodd = style.fill.as_ref().map_or(false, |f| f.2);
                let mut c = path_clip(self.ss, &d, evenodd, [(x1 - x0).max(1.0), (y1 - y0).max(1.0)], style.fill.as_ref().map(|f| f.0), sw, scol, start, dur);
                c.name = Some(nm.to_string());
                if let Some((_, o, _)) = &style.fill { if !o.is_null() { let p = to_prop(o, 0, 100.0, &|v| (v / 100.0).clamp(0.0, 1.0), tmap); if !(prop_static(&p) && (p.base - 1.0).abs() < 1e-9) { c.props.insert("fill_alpha".into(), p); } } }
                if let Some((_, o, _)) = &style.stroke { if !o.is_null() { let p = to_prop(o, 0, 100.0, &|v| (v / 100.0).clamp(0.0, 1.0), tmap); if !(prop_static(&p) && (p.base - 1.0).abs() < 1e-9) { c.props.insert("stroke_alpha".into(), p); } } }
                if (op_mul - 1.0).abs() > 1e-9 { c.props.insert("opacity".into(), Prop::new(op_mul.clamp(0.0, 1.0))); }
                if let Some(g) = style.gfill { c.effects.push(effect("shape_gradient_fill", &[("r1", g[0]), ("g1", g[1]), ("b1", g[2]), ("r2", g[3]), ("g2", g[4]), ("b2", g[5]), ("angle", g[6])])); }
                if let Some((s, e, o)) = style.trims.last() { if style.stroke.is_none() { self.im.warn("trim paths on filled shapes only trims strokes in kimo"); }
                    let mut ef = effect("shape_trim", &[]); ef.params.insert("start".into(), to_prop(s, 0, 0.0, &|v| v / 100.0, tmap)); ef.params.insert("end".into(), to_prop(e, 0, 100.0, &|v| v / 100.0, tmap)); ef.params.insert("offset".into(), to_prop(o, 0, 0.0, &|v| v / 360.0, tmap)); c.effects.push(ef);
                    if style.trims.len() > 1 { self.im.warn("nested trim paths import as the innermost one"); } }
                if let Some(r) = style.rep { if !simple(m) && (r[1] != 0.0 || r[2] != 0.0) { self.im.warn("repeaters under rotated groups import approximately"); }
                    c.effects.push(effect("shape_repeater", &[("copies", r[0].round().clamp(1.0, 64.0)), ("dx", r[1] * k), ("dy", r[2] * k), ("rotation", r[3]), ("scale", r[4])])); }
                if let Some(mk) = morph { // path keys in the same structure as the base path
                    let mut keys: Vec<(f64, String, String)> = vec![]; let ks = kfs(mk);
                    for (j, kk) in ks.iter().enumerate() { let v = if kk.get("s").is_some() { kk["s"].clone() } else if j > 0 { ks[j - 1]["e"].clone() } else { continue };
                        let Some(s) = shape_val(&v) else { continue }; if s.v.len() != subs[0].v.len() { continue; }
                        let ease = if kk.get("h").map(num) == Some(1.0) { "hold" } else { let (ox, oy, ix, iy) = (handle(kk, "o", "x", 0), handle(kk, "o", "y", 0), handle(kk, "i", "x", 0), handle(kk, "i", "y", 0));
                            match (ox > oy + 0.1, iy > ix + 0.1) { (true, true) => "in_out", (true, false) => "in", (false, true) => "out", _ => "linear" } };
                        keys.push((tmap(num(&kk["t"])), subs_to_d(&map_subs(&[s], m)), ease.to_string())); }
                    if keys.len() >= 2 { if let Some(e) = c.effects.iter_mut().find(|e| e.kind == "shape_path") { e.strings.insert("keys".into(), serde_json::to_string(&keys).unwrap_or_default()); } } }
                let _ = fr; vec![c]
            }
            Node::Text { doc, font } => {
                let k = det(m).abs().sqrt(); let size = num(&doc["s"]).max(1.0) * k; let text = doc["t"].as_str().unwrap_or("").replace('\r', "\n").replace('\u{3}', "\n");
                let col = { let c = nums(&doc["fc"]); if c.len() >= 3 { let s = if c.iter().take(3).any(|x| *x > 1.0) { 1.0 / 255.0 } else { 1.0 }; [c[0] * s, c[1] * s, c[2] * s] } else { [1.0; 3] } };
                let align = match doc["j"].as_u64().unwrap_or(0) { 1 => "right", 2 => "center", _ => "left" };
                // Lottie anchors text at the first baseline; kimo centres the block on x/y: shift by an estimate of the block
                let lines: Vec<&str> = text.split('\n').collect(); let wmax = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * size * 0.52; let lh = num(&doc["lh"]).max(size / k.max(1e-9)) * k;
                let dx = match align { "left" => wmax / 2.0, "right" => -wmax / 2.0, _ => 0.0 }; let dy = -0.35 * size + (lines.len() as f64 - 1.0) * lh / 2.0;
                let o = apply(m, [0.0, 0.0]); let ang = m[1].atan2(m[0]).to_degrees(); let (sa, ca) = ang.to_radians().sin_cos();
                self.im.warn("text layers import as kimo text placed from an estimate of the block size; check their position");
                let src = Source::Text { text, font: font.clone(), size, color: col, letter_spacing: num(&doc["tr"]) / 1000.0 * size, stroke_width: if doc.get("sw").is_some() && doc.get("sc").is_some() { num(&doc["sw"]) * k } else { 0.0 }, stroke_color: { let c = nums(&doc["sc"]); if c.len() >= 3 { [c[0], c[1], c[2]] } else { [0.0; 3] } }, shadow: 0.0, align: align.into() };
                let mut c = new_clip(self.ss, src, start, dur); c.name = Some(nm.to_string());
                c.props.insert("x".into(), Prop::new(o[0] + dx * ca - dy * sa)); c.props.insert("y".into(), Prop::new(o[1] + dx * sa + dy * ca)); if ang.abs() > 1e-6 { c.props.insert("rotation".into(), Prop::new(ang)); }
                if (op_mul - 1.0).abs() > 1e-9 { c.props.insert("opacity".into(), Prop::new(op_mul)); }
                vec![c]
            }
            Node::Group { chain, children } => {
                let animated = chain.iter().any(|x| x.animated());
                if !animated { // fold the static transform into the children
                    let g = chain.iter().fold(ID, |acc, x| mul(&acc, &x.mat(0.0))); let o = chain.last().map_or(1.0, |x| x.op(0.0));
                    let mm = mul(m, &g); return children.iter().flat_map(|ch| self.node(ch, &mm, op_mul * o, start, dur, tmap, nm)).collect::<Vec<_>>();
                }
                let x = chain.last().unwrap(); let direct = chain.len() == 1 && simple(m) && x.direct();
                // inner space: the group's anchor at the origin, already scaled by the outer scale so inner content stays canvas-sized
                let a0 = [eval1(&x.a, 0.0, 0, 0.0), eval1(&x.a, 0.0, 1, 0.0)]; let k = det(m).abs().sqrt().max(1e-9);
                let inner = mul(&sc(k, k), &tr(-a0[0], -a0[1]));
                let kids: Vec<Clip> = children.iter().flat_map(|ch| self.node(ch, &inner, 1.0, 0.0, dur, tmap, nm)).collect(); if kids.is_empty() { return vec![]; }
                let mut c = if kids.len() == 1 && kids[0].props.get("x").map_or(true, |p| p.keys.is_empty() && p.base == 0.0) && kids[0].props.get("y").map_or(true, |p| p.keys.is_empty() && p.base == 0.0)
                    && kids[0].props.get("scale").map_or(true, |p| p.keys.is_empty() && p.base == 1.0) && kids[0].props.get("rotation").map_or(true, |p| p.keys.is_empty() && p.base == 0.0) && kids[0].parent.is_none() {
                    let mut c = kids.into_iter().next().unwrap(); c.start = start; c } // one child: carry the transform on it, no extra group
                    else { let tracks: Vec<Track> = kids.into_iter().rev().map(|cl| new_track(self.ss, nm, vec![cl])).collect(); let mut c = new_clip(self.ss, Source::Precomp { tracks }, start, dur); c.name = Some(nm.to_string()); c };
                let base_op = c.props.get("opacity").map_or(1.0, |p| p.base);
                if direct {
                    let (mk, b) = (m[0], [m[4], m[5]]); let set = |c: &mut Clip, k: &str, p: Prop| { c.props.insert(k.into(), p); };
                    if x.pxy.is_null() { set(&mut c, "x", to_prop(&x.px, 0, 0.0, &|v| mk * v + b[0], tmap)); set(&mut c, "y", to_prop(&x.py, 0, 0.0, &|v| mk * v + b[1], tmap)); }
                    else if is_anim(&x.pxy) && kfs(&x.pxy).iter().any(|kk| kk.get("to").map_or(false, |t| nums(t).iter().any(|v| v.abs() > 1e-6))) { self.im.warn("curved motion paths import as straight moves between keys"); set(&mut c, "x", to_prop(&x.pxy, 0, 0.0, &|v| mk * v + b[0], tmap)); set(&mut c, "y", to_prop(&x.pxy, 1, 0.0, &|v| mk * v + b[1], tmap)); }
                    else { set(&mut c, "x", to_prop(&x.pxy, 0, 0.0, &|v| mk * v + b[0], tmap)); set(&mut c, "y", to_prop(&x.pxy, 1, 0.0, &|v| mk * v + b[1], tmap)); }
                    set(&mut c, "scale", to_prop(&x.s, 0, 100.0, &|v| v / 100.0, tmap)); set(&mut c, "rotation", to_prop(&x.r, 0, 0.0, &|v| v, tmap));
                    set(&mut c, "opacity", to_prop(&x.o, 0, 100.0, &|v| (v / 100.0 * op_mul * base_op).clamp(0.0, 1.0), tmap));
                } else { // sample the composed transform once per frame
                    let n = (dur * fr).ceil().max(1.0) as usize; let (mut px, mut py, mut ps, mut pr, mut po) = (Prop::new(0.0), Prop::new(0.0), Prop::new(1.0), Prop::new(0.0), Prop::new(1.0)); let mut last_ang: Option<f64> = None;
                    // clip-local time -> layer frame: invert tmap (it is a shift)
                    let off = tmap(0.0);
                    for i in 0..=n { let lt = i as f64 / fr; let f = (lt - off) * fr;
                        let a = mul(m, &chain.iter().fold(ID, |acc, x| mul(&acc, &x.mat(f)))); let o = apply(&a, a0); let l = a; let mut ang = l[1].atan2(l[0]).to_degrees();
                        if let Some(p) = last_ang { while ang - p > 180.0 { ang -= 360.0; } while ang - p < -180.0 { ang += 360.0; } } last_ang = Some(ang);
                        let key = |p: &mut Prop, v: f64| p.keys.push(Key { t: lt, v, ease: Ease::Linear, bez: None, out: None, inn: None });
                        key(&mut px, o[0]); key(&mut py, o[1]); key(&mut ps, det(&l).abs().sqrt() / k); key(&mut pr, ang); key(&mut po, (x.op(f) * op_mul * base_op).clamp(0.0, 1.0)); }
                    if chain.iter().any(|x| x.skew) { self.im.warn("skew is not imported"); }
                    for (k, mut p) in [("x", px), ("y", py), ("scale", ps), ("rotation", pr), ("opacity", po)] { if p.keys.iter().all(|kk| (kk.v - p.keys[0].v).abs() < 1e-9) { p = Prop::new(p.keys[0].v); } else { p.base = p.keys[0].v; } c.props.insert(k.into(), p); }
                }
                vec![c]
            }
            Node::Comp { chain, layers, ip, st } => {
                let _ = (ip, st);
                let x = chain.last().unwrap(); let a0 = [eval1(&x.a, 0.0, 0, 0.0), eval1(&x.a, 0.0, 1, 0.0)]; let k = det(m).abs().sqrt().max(1e-9);
                let inner = mul(&sc(k, k), &tr(-a0[0], -a0[1]));
                // inner clock: precomp-local 0 = this layer's start; inner comp frame = layer frame (frame - st)
                let off = tmap(0.0); // clip-local seconds of layer frame 0
                self.im.depth += 1; let kids = self.layers(layers, &inner, off, dur); self.im.depth -= 1;
                if kids.is_empty() { return vec![]; }
                let tracks: Vec<Track> = kids.into_iter().rev().map(|cl| new_track(self.ss, nm, vec![cl])).collect();
                let mut g = new_clip(self.ss, Source::Precomp { tracks }, start, dur); g.name = Some(nm.to_string());
                // reuse the group path for the transform: an empty-children group with the same chain sets the props
                let static_ = !chain.iter().any(|x| x.animated());
                if static_ { let a = mul(m, &chain.iter().fold(ID, |acc, x| mul(&acc, &x.mat(0.0)))); let o = apply(&a, a0); g.props.insert("x".into(), Prop::new(o[0])); g.props.insert("y".into(), Prop::new(o[1]));
                    g.props.insert("scale".into(), Prop::new(det(&a).abs().sqrt() / k)); g.props.insert("rotation".into(), Prop::new(a[1].atan2(a[0]).to_degrees())); g.props.insert("opacity".into(), Prop::new((x.op(0.0) * op_mul).clamp(0.0, 1.0))); }
                else { let fr = self.im.fr; let n = (dur * fr).ceil().max(1.0) as usize; let mut ps: Vec<Prop> = vec![Prop::new(0.0), Prop::new(0.0), Prop::new(1.0), Prop::new(0.0), Prop::new(1.0)]; let mut last: Option<f64> = None;
                    for i in 0..=n { let lt = i as f64 / fr; let f = (lt - off) * fr; let a = mul(m, &chain.iter().fold(ID, |acc, x| mul(&acc, &x.mat(f)))); let o = apply(&a, a0); let mut ang = a[1].atan2(a[0]).to_degrees();
                        if let Some(p) = last { while ang - p > 180.0 { ang -= 360.0; } while ang - p < -180.0 { ang += 360.0; } } last = Some(ang);
                        for (j, v) in [o[0], o[1], det(&a).abs().sqrt() / k, ang, (x.op(f) * op_mul).clamp(0.0, 1.0)].into_iter().enumerate() { ps[j].keys.push(Key { t: lt, v, ease: Ease::Linear, bez: None, out: None, inn: None }); } }
                    for (name, mut p) in ["x", "y", "scale", "rotation", "opacity"].into_iter().zip(ps) { if p.keys.iter().all(|kk| (kk.v - p.keys[0].v).abs() < 1e-9) { p = Prop::new(p.keys[0].v); } else { p.base = p.keys[0].v; } g.props.insert(name.into(), p); } }
                vec![g]
            }
        }
    }
    /// Clips for a layer list (top first) placed on a clock where comp frame 0 is at `shift` seconds, inside a span of `span` seconds.
    fn layers(&mut self, layers: &[Value], m: &Aff, shift: f64, span: f64) -> Vec<Clip> {
        let nodes = self.im.layer_nodes(layers); let fr = self.im.fr; let mut out = vec![];
        for (node, ip, op, st, nm) in &nodes {
            let s_raw = ip / fr + shift; let start = s_raw.max(0.0); let end = (op / fr + shift).min(span); let dur = end - start; if dur <= 1e-6 { continue; }
            let (st, shift) = (*st, shift); let tmap = move |kf: f64| (kf + st) / fr + shift - start;
            out.extend(self.node(node, m, 1.0, start, dur, &tmap, nm));
        }
        out
    }
}

fn import_lottie(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let doc: Value = match (a.get("path").and_then(|v| v.as_str()), a.get("lottie")) {
        (Some(p), None) => { let t = std::fs::read_to_string(p).map_err(|e| format!("could not read {p}: {e}"))?; serde_json::from_str(&t).map_err(|e| format!("{p} is not valid JSON: {e}"))? }
        (None, Some(Value::String(s))) => serde_json::from_str(s).map_err(|e| format!("lottie is not valid JSON: {e}"))?,
        (None, Some(v @ Value::Object(_))) => v.clone(),
        _ => return Err("give path (a Lottie .json file) or lottie (the JSON itself)".into()),
    };
    let (lw, lh, fr) = (num(&doc["w"]), num(&doc["h"]), num(&doc["fr"]));
    if lw <= 0.0 || lh <= 0.0 || fr <= 0.0 || !doc["layers"].is_array() { return Err("not a Lottie animation (needs w, h, fr and layers)".into()); }
    let (ip, op) = (num(&doc["ip"]), num(&doc["op"])); let span = ((op - ip) / fr).max(1.0 / fr);
    let (pw, ph) = (ss.project.width as f64, ss.project.height as f64);
    let fit = match a.get("fit").and_then(|v| v.as_str()).unwrap_or("contain") { "contain" => (pw / lw).min(ph / lh), "cover" => (pw / lw).max(ph / lh), "none" => 1.0, o => return Err(format!("fit must be contain, cover or none, got '{o}'")) };
    let scale = opt_num(a, "scale", 1.0); if !(scale > 0.0) { return Err("scale must be > 0".into()); } let k = fit * scale;
    let (ox, oy) = (opt_num(a, "x", 0.0), opt_num(a, "y", 0.0));
    let m = mul(&tr(ox, oy), &mul(&sc(k, k), &tr(-lw / 2.0, -lh / 2.0)));
    let start = opt_num(a, "start", 0.0).max(0.0); let name = a.get("name").and_then(|v| v.as_str()).map(String::from).or_else(|| doc["nm"].as_str().map(String::from)).unwrap_or_else(|| "lottie".into());
    let assets = doc["assets"].as_array().cloned().unwrap_or_default(); let fonts = doc["fonts"].clone();
    let layers = doc["layers"].as_array().cloned().unwrap_or_default();
    let im = Im { warn: vec![], fr, assets: &assets, fonts: &fonts, depth: 0 };
    let mut em = Emit { ss, im };
    let clips = em.layers(&layers, &m, -ip / fr, span);
    let warnings = em.im.warn.clone();
    if clips.is_empty() { return Err(format!("nothing to import (no shape, solid, text or precomp layers){}", if warnings.is_empty() { String::new() } else { format!("; {}", warnings.join("; ")) })); }
    let n = clips.len(); let ss = em.ss;
    let tracks: Vec<Track> = clips.into_iter().rev().map(|c| { let nm = c.name.clone().unwrap_or_default(); new_track(ss, &nm, vec![c]) }).collect();
    let mut g = new_clip(ss, Source::Precomp { tracks }, start, span); g.name = Some(name.clone());
    let id = g.id.clone(); let t = new_track(ss, &name, vec![g]); let tid = t.id.clone(); ss.project.tracks.push(t);
    Ok(json!({"clip_id": id, "track_id": tid, "layers": n, "duration": span, "fit_scale": k, "warnings": warnings}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_lottie", description: "Export the timeline as a Lottie (Bodymovin JSON) animation for web and app players. Shapes, Bezier paths (with morphs), trim/offset/repeater, gradient fills and strokes, text (as glyph outlines: exact, no font needed), solids, gradients, still images (embedded), groups and markers. Keyframes keep linear/hold/in/out/custom-Bezier easing; other curves, expressions, parenting and crossfades are sampled per frame. Video, audio, effects, masks and 3D layers are skipped and listed in warnings.", mutates: false,
        schema: obj(vec![("path", s(".json file to write (else the JSON is returned as `lottie`, up to 2 MB)")), ("overwrite", json!({"type":"boolean"})), ("embed_images", json!({"type":"boolean","description":"embed still images as data URIs (default true); false references the files"}))], &[]),
        run: export_lottie },
    Command { name: "import_lottie", description: "Import a Lottie (Bodymovin JSON, e.g. from LottieFiles or After Effects) as editable kimo layers: one group clip on a new top track holding a path shape per shape group (fills, strokes, gradients, trim paths, repeaters, path morphs), solids, text and nested precomps. Transforms keep their keyframes and Bezier easing; parented or rotated chains are sampled per frame. Images, mattes, masks, effects and text animators are skipped and listed in warnings.", mutates: true,
        schema: obj(vec![("path", s("Lottie .json file")), ("lottie", json!({"description":"the Lottie JSON itself (object or string) instead of path"})), ("start", n("timeline seconds, default 0")),
            ("fit", json!({"type":"string","enum":["contain","cover","none"],"description":"scale the animation's canvas to the project canvas (default contain)"})), ("scale", n("extra scale, default 1")), ("x", n("px offset from the canvas centre")), ("y", n("px offset (down is +)")), ("name", s("name of the group clip"))], &[]),
        run: import_lottie },
] }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_paths_round_trip_through_lottie_subpaths() {
        let subs = d_to_subs("M0 0 L100 0 L100 50 Z M10 10 C20 0 40 0 50 10 Q60 20 70 10 Z").unwrap();
        assert_eq!(subs.len(), 2); assert!(subs[0].c && subs[1].c); assert_eq!(subs[0].v.len(), 3, "closing point merged into the first");
        assert_eq!(subs[1].o[0], [10.0, -10.0]); assert_eq!(subs[1].i[1], [-10.0, -10.0]);
        let again = d_to_subs(&subs_to_d(&subs)).unwrap(); for (a, b) in subs.iter().zip(&again) { assert_eq!(a.v.len(), b.v.len()); for (p, q) in a.v.iter().zip(&b.v) { assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3); } }
        let rel = d_to_subs("m10 10 h20 v20 h-20 z").unwrap(); assert_eq!(rel[0].v, vec![[10.0, 10.0], [30.0, 10.0], [30.0, 30.0], [10.0, 30.0]]);
    }
    #[test]
    fn lottie_bezier_keys_evaluate_like_the_player() {
        let p = json!({"a": 1, "k": [{"t": 0, "s": [0], "o": {"x": [0.333], "y": [0]}, "i": {"x": [0.667], "y": [1]}}, {"t": 10, "s": [100]}]});
        assert_eq!(eval1(&p, 0.0, 0, 0.0), 0.0); assert!((eval1(&p, 5.0, 0, 0.0) - 50.0).abs() < 1.0, "symmetric ease is half way at the middle"); assert!(eval1(&p, 2.0, 0, 0.0) < 20.0, "eases in"); assert_eq!(eval1(&p, 12.0, 0, 0.0), 100.0);
        let pr = to_prop(&p, 0, 0.0, &|v| v, &|f| f / 10.0); assert_eq!(pr.keys.len(), 2); assert_eq!(pr.keys[0].bez, Some([0.333, 0.0, 0.667, 1.0]));
        assert!((pr.at(0.5) - eval1(&p, 5.0, 0, 0.0)).abs() < 0.5, "kimo evaluates the imported curve the same way");
        let h = json!({"a": 1, "k": [{"t": 0, "s": [1], "h": 1}, {"t": 4, "s": [9]}]}); assert_eq!(eval1(&h, 3.9, 0, 0.0), 1.0); assert_eq!(eval1(&h, 4.0, 0, 0.0), 9.0);
    }
    #[test]
    fn transforms_compose_like_after_effects() {
        let x = xf(&json!({"a": {"a": 0, "k": [50, 50]}, "p": {"a": 0, "k": [200, 100]}, "s": {"a": 0, "k": [200, 200]}, "r": {"a": 0, "k": 90}, "o": {"a": 0, "k": 50}}));
        let m = x.mat(0.0); let p = apply(&m, [50.0, 50.0]); assert!((p[0] - 200.0).abs() < 1e-9 && (p[1] - 100.0).abs() < 1e-9, "anchor lands on position");
        let q = apply(&m, [60.0, 50.0]); assert!((q[0] - 200.0).abs() < 1e-9 && (q[1] - 120.0).abs() < 1e-9, "x axis turns to +y under 90 deg, doubled"); assert_eq!(x.op(0.0), 0.5); assert!(x.direct());
    }
}
