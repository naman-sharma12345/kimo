//! SVG import: turns an SVG document into animatable layers of vector shapes.
//!
//! Every top-level group (`<g>`, or a lone drawable) of the document becomes one layer; every drawable inside it becomes one
//! shape with its own path (absolute M/L/C/Z in the root user space, every transform already applied), fill, stroke and
//! opacities. `ext::svg_tools` maps the layers onto kimo path-shape clips (one precomp per layer), so a layer can be moved,
//! recoloured and drawn on with the ordinary tools.
//!
//! Supported: path (every command, arcs become cubics), rect (rx/ry), circle, ellipse, line, polyline, polygon, nested g,
//! `use` (href to an element by id, with x/y), transform (matrix/translate/scale/rotate/skewX/skewY), presentation
//! attributes, `style=""`, `<style>` rules with class / id / tag selectors (and comma lists), colours (#rgb, #rgba,
//! #rrggbb, #rrggbbaa, rgb()/rgba(), common names, currentColor), linear and radial gradients (approximated by their first
//! and last stop as a two-colour linear blend), fill-rule, opacity / fill-opacity / stroke-opacity, display:none and
//! visibility:hidden. Not supported, reported in `skipped`: text, image, foreignObject, filters, masks and clip paths
//! (the shapes are imported unclipped), patterns, markers, dashes.
use std::collections::HashMap;

pub type Pt = [f64; 2];
/// Affine [a b c d e f]: x' = a x + c y + e, y' = b x + d y + f (SVG matrix order).
pub type Aff = [f64; 6];
pub const ID: Aff = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
pub fn mul(m: &Aff, n: &Aff) -> Aff { [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3], m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]] }
pub fn ap(m: &Aff, p: Pt) -> Pt { [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]] }
/// Mean linear scale of an affine (stroke widths scale by it).
pub fn scale_of(m: &Aff) -> f64 { (m[0] * m[3] - m[1] * m[2]).abs().sqrt() }

#[derive(Clone, Debug, PartialEq)]
pub enum Seg { M(Pt), L(Pt), C(Pt, Pt, Pt), Z }

/// Normalized path: absolute moveto / lineto / cubic / close only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathData(pub Vec<Seg>);
impl PathData {
    pub fn map(&self, f: impl Fn(Pt) -> Pt) -> PathData { PathData(self.0.iter().map(|s| match s { Seg::M(p) => Seg::M(f(*p)), Seg::L(p) => Seg::L(f(*p)), Seg::C(a, b, c) => Seg::C(f(*a), f(*b), f(*c)), Seg::Z => Seg::Z }).collect()) }
    pub fn to_d(&self) -> String {
        let r = |v: f64| { let s = format!("{:.2}", v); let s = s.trim_end_matches('0').trim_end_matches('.').to_string(); if s == "-0" { "0".into() } else { s } };
        let mut o = String::new();
        for s in &self.0 { if !o.is_empty() { o.push(' '); } match s {
            Seg::M(p) => o += &format!("M{} {}", r(p[0]), r(p[1])), Seg::L(p) => o += &format!("L{} {}", r(p[0]), r(p[1])),
            Seg::C(a, b, c) => o += &format!("C{} {} {} {} {} {}", r(a[0]), r(a[1]), r(b[0]), r(b[1]), r(c[0]), r(c[1])), Seg::Z => o.push('Z') } }
        o
    }
    /// Control-point bounding box [x0,y0,x1,y1] (contains the curve).
    pub fn bbox(&self) -> Option<[f64; 4]> {
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN]; let mut any = false;
        let mut add = |p: &Pt| { b[0] = b[0].min(p[0]); b[1] = b[1].min(p[1]); b[2] = b[2].max(p[0]); b[3] = b[3].max(p[1]); any = true; };
        for s in &self.0 { match s { Seg::M(p) | Seg::L(p) => add(p), Seg::C(a, c, d) => { add(a); add(c); add(d); } Seg::Z => {} } }
        if any { Some(b) } else { None }
    }
    pub fn drawable(&self) -> bool { self.0.iter().any(|s| matches!(s, Seg::L(_) | Seg::C(..))) }
}

/// Parse SVG path data into absolute M/L/C/Z with transform `m` applied. Quadratics and arcs become cubics.
pub fn parse_path(d: &str, m: &Aff) -> Result<PathData, String> {
    let toks = crate::paths::tokens(d)?; let mut out: Vec<Seg> = vec![];
    let (mut cx, mut cy, mut sx, mut sy, mut lcx, mut lcy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64); let mut last = ' ';
    let t = |p: Pt| ap(m, p);
    for (cmd, a) in &toks {
        let rel = cmd.is_ascii_lowercase(); let up = cmd.to_ascii_uppercase();
        let need = match up { 'M' | 'L' | 'T' => 2, 'H' | 'V' => 1, 'C' => 6, 'S' | 'Q' => 4, 'A' => 7, _ => 0 };
        if up == 'Z' { out.push(Seg::Z); cx = sx; cy = sy; last = 'Z'; continue; }
        if out.is_empty() && up != 'M' { return Err("path must start with M (moveto)".into()); }
        if a.is_empty() || a.len() % need != 0 { return Err(format!("command '{cmd}' needs groups of {need} numbers, got {}", a.len())); }
        for (gi, g) in a.chunks(need).enumerate() {
            let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
            match up {
                'M' => { let p = [g[0] + ox, g[1] + oy]; if gi == 0 { out.push(Seg::M(t(p))); sx = p[0]; sy = p[1]; last = 'M'; } else { out.push(Seg::L(t(p))); last = 'L'; } cx = p[0]; cy = p[1]; }
                'L' => { let p = [g[0] + ox, g[1] + oy]; out.push(Seg::L(t(p))); cx = p[0]; cy = p[1]; last = 'L'; }
                'H' => { let x = g[0] + ox; out.push(Seg::L(t([x, cy]))); cx = x; last = 'L'; }
                'V' => { let y = g[0] + oy; out.push(Seg::L(t([cx, y]))); cy = y; last = 'L'; }
                'C' | 'S' => {
                    let c1 = if up == 'C' { [g[0] + ox, g[1] + oy] } else if last == 'C' { [2.0 * cx - lcx, 2.0 * cy - lcy] } else { [cx, cy] };
                    let (c2, p) = if up == 'C' { ([g[2] + ox, g[3] + oy], [g[4] + ox, g[5] + oy]) } else { ([g[0] + ox, g[1] + oy], [g[2] + ox, g[3] + oy]) };
                    out.push(Seg::C(t(c1), t(c2), t(p))); lcx = c2[0]; lcy = c2[1]; cx = p[0]; cy = p[1]; last = 'C'; }
                'Q' | 'T' => {
                    let q = if up == 'Q' { [g[0] + ox, g[1] + oy] } else if last == 'Q' { [2.0 * cx - lcx, 2.0 * cy - lcy] } else { [cx, cy] };
                    let p = if up == 'Q' { [g[2] + ox, g[3] + oy] } else { [g[0] + ox, g[1] + oy] };
                    let c1 = [cx + 2.0 / 3.0 * (q[0] - cx), cy + 2.0 / 3.0 * (q[1] - cy)]; let c2 = [p[0] + 2.0 / 3.0 * (q[0] - p[0]), p[1] + 2.0 / 3.0 * (q[1] - p[1])];
                    out.push(Seg::C(t(c1), t(c2), t(p))); lcx = q[0]; lcy = q[1]; cx = p[0]; cy = p[1]; last = 'Q'; }
                'A' => { let p = [g[5] + ox, g[6] + oy]; for s in arc_to_cubics([cx, cy], g[0], g[1], g[2], g[3] != 0.0, g[4] != 0.0, p) { out.push(match s { Seg::L(q) => Seg::L(t(q)), Seg::C(a, b, c) => Seg::C(t(a), t(b), t(c)), o => o }); } cx = p[0]; cy = p[1]; last = 'A'; }
                _ => {}
            }
        }
    }
    Ok(PathData(out))
}

/// SVG elliptical arc (endpoint parameterization, F.6) as cubic Beziers, at most 90 degrees each.
pub fn arc_to_cubics(p0: Pt, rx: f64, ry: f64, rot_deg: f64, large: bool, sweep: bool, p1: Pt) -> Vec<Seg> {
    if (p0[0] - p1[0]).abs() < 1e-12 && (p0[1] - p1[1]).abs() < 1e-12 { return vec![]; }
    let (mut rx, mut ry) = (rx.abs(), ry.abs()); if rx < 1e-9 || ry < 1e-9 { return vec![Seg::L(p1)]; }
    let phi = rot_deg.to_radians(); let (cp, sp) = (phi.cos(), phi.sin());
    let (dx, dy) = ((p0[0] - p1[0]) / 2.0, (p0[1] - p1[1]) / 2.0); let (x1, y1) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    let lam = (x1 / rx).powi(2) + (y1 / ry).powi(2); if lam > 1.0 { let s = lam.sqrt(); rx *= s; ry *= s; }
    let num = (rx * ry).powi(2) - (rx * y1).powi(2) - (ry * x1).powi(2); let den = (rx * y1).powi(2) + (ry * x1).powi(2);
    let co = (if large == sweep { -1.0 } else { 1.0 }) * (num / den).max(0.0).sqrt();
    let (cxp, cyp) = (co * rx * y1 / ry, -co * ry * x1 / rx); let (ccx, ccy) = (cp * cxp - sp * cyp + (p0[0] + p1[0]) / 2.0, sp * cxp + cp * cyp + (p0[1] + p1[1]) / 2.0);
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let th1 = ang(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry); let mut dth = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
    if !sweep && dth > 0.0 { dth -= std::f64::consts::TAU; } else if sweep && dth < 0.0 { dth += std::f64::consts::TAU; }
    let n = (dth.abs() / std::f64::consts::FRAC_PI_2 - 1e-9).ceil().max(1.0) as usize; let step = dth / n as f64; let k = 4.0 / 3.0 * (step / 4.0).tan();
    let e = |th: f64| [ccx + cp * rx * th.cos() - sp * ry * th.sin(), ccy + sp * rx * th.cos() + cp * ry * th.sin()];
    let de = |th: f64| [-cp * rx * th.sin() - sp * ry * th.cos(), -sp * rx * th.sin() + cp * ry * th.cos()];
    (0..n).map(|i| { let (a, b) = (th1 + step * i as f64, th1 + step * (i + 1) as f64); let (pa, pb, da, db) = (e(a), if i + 1 == n { p1 } else { e(b) }, de(a), de(b));
        Seg::C([pa[0] + k * da[0], pa[1] + k * da[1]], [pb[0] - k * db[0], pb[1] - k * db[1]], pb) }).collect()
}

/// Parse a `transform` attribute.
pub fn parse_transform(s: &str) -> Result<Aff, String> {
    let mut m = ID; let mut rest = s.trim();
    while !rest.is_empty() {
        let open = rest.find('(').ok_or_else(|| format!("bad transform '{s}'"))?; let close = rest.find(')').ok_or_else(|| format!("bad transform '{s}'"))?;
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let v: Vec<f64> = rest[open + 1..close].split(|c: char| c == ',' || c.is_whitespace()).filter(|x| !x.is_empty()).map(|x| x.parse::<f64>().map_err(|_| format!("bad number '{x}' in transform"))).collect::<Result<_, _>>()?;
        let g = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
        let n: Aff = match name {
            "matrix" if v.len() == 6 => [v[0], v[1], v[2], v[3], v[4], v[5]],
            "translate" if !v.is_empty() => [1.0, 0.0, 0.0, 1.0, g(0, 0.0), g(1, 0.0)],
            "scale" if !v.is_empty() => [g(0, 1.0), 0.0, 0.0, g(1, g(0, 1.0)), 0.0, 0.0],
            "rotate" if !v.is_empty() => { let a = g(0, 0.0).to_radians(); let (c, sn) = (a.cos(), a.sin()); let r = [c, sn, -sn, c, 0.0, 0.0];
                if v.len() >= 3 { mul(&mul(&[1.0, 0.0, 0.0, 1.0, v[1], v[2]], &r), &[1.0, 0.0, 0.0, 1.0, -v[1], -v[2]]) } else { r } }
            "skewX" if v.len() == 1 => [1.0, 0.0, v[0].to_radians().tan(), 1.0, 0.0, 0.0],
            "skewY" if v.len() == 1 => [1.0, v[0].to_radians().tan(), 0.0, 1.0, 0.0, 0.0],
            _ => return Err(format!("unsupported transform '{name}({})'", &rest[open + 1..close])),
        };
        m = mul(&m, &n); rest = rest[close + 1..].trim();
    }
    Ok(m)
}

const NAMED: &[(&str, [u8; 3])] = &[("black", [0, 0, 0]), ("white", [255, 255, 255]), ("red", [255, 0, 0]), ("green", [0, 128, 0]), ("lime", [0, 255, 0]), ("blue", [0, 0, 255]),
    ("yellow", [255, 255, 0]), ("orange", [255, 165, 0]), ("purple", [128, 0, 128]), ("gray", [128, 128, 128]), ("grey", [128, 128, 128]), ("silver", [192, 192, 192]),
    ("navy", [0, 0, 128]), ("teal", [0, 128, 128]), ("maroon", [128, 0, 0]), ("olive", [128, 128, 0]), ("aqua", [0, 255, 255]), ("cyan", [0, 255, 255]), ("fuchsia", [255, 0, 255]),
    ("magenta", [255, 0, 255]), ("pink", [255, 192, 203]), ("brown", [165, 42, 42]), ("gold", [255, 215, 0]), ("darkgray", [169, 169, 169]), ("darkgrey", [169, 169, 169]),
    ("lightgray", [211, 211, 211]), ("lightgrey", [211, 211, 211]), ("darkblue", [0, 0, 139]), ("darkgreen", [0, 100, 0]), ("darkred", [139, 0, 0]), ("skyblue", [135, 206, 235]),
    ("steelblue", [70, 130, 180]), ("tomato", [255, 99, 71]), ("coral", [255, 127, 80]), ("salmon", [250, 128, 114]), ("crimson", [220, 20, 60]), ("indigo", [75, 0, 130]),
    ("violet", [238, 130, 238]), ("beige", [245, 245, 220]), ("ivory", [255, 255, 240]), ("khaki", [240, 230, 140]), ("tan", [210, 180, 140]), ("chocolate", [210, 105, 30]),
    ("whitesmoke", [245, 245, 245]), ("gainsboro", [220, 220, 220]), ("dimgray", [105, 105, 105]), ("dimgrey", [105, 105, 105]), ("slategray", [112, 128, 144]), ("orangered", [255, 69, 0]),
    ("forestgreen", [34, 139, 34]), ("seagreen", [46, 139, 87]), ("royalblue", [65, 105, 225]), ("dodgerblue", [30, 144, 255]), ("deepskyblue", [0, 191, 255]), ("turquoise", [64, 224, 208])];

/// A CSS colour as linear-free sRGB 0..1 plus alpha. `None` for none/transparent.
pub fn parse_color(s: &str, current: [f64; 3]) -> Result<Option<([f64; 3], f64)>, String> {
    let s = s.trim(); let l = s.to_ascii_lowercase();
    if l == "none" || l == "transparent" { return Ok(None); }
    if l == "currentcolor" { return Ok(Some((current, 1.0))); }
    if let Some(h) = l.strip_prefix('#') {
        let hv = |x: &str| u8::from_str_radix(x, 16).map_err(|_| format!("bad colour '{s}'"));
        let v: Vec<u8> = match h.len() { 3 | 4 => h.chars().map(|c| hv(&format!("{c}{c}"))).collect::<Result<_, _>>()?, 6 | 8 => (0..h.len() / 2).map(|i| hv(&h[i * 2..i * 2 + 2])).collect::<Result<_, _>>()?, _ => return Err(format!("bad colour '{s}'")) };
        let a = v.get(3).map_or(1.0, |x| *x as f64 / 255.0); return Ok(Some(([v[0] as f64 / 255.0, v[1] as f64 / 255.0, v[2] as f64 / 255.0], a)));
    }
    if let Some(inner) = l.strip_prefix("rgba(").or_else(|| l.strip_prefix("rgb(")).and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(|c: char| c == ',' || c == '/' || c.is_whitespace()).filter(|x| !x.is_empty()).collect(); if parts.len() < 3 { return Err(format!("bad colour '{s}'")); }
        let ch = |x: &str| -> Result<f64, String> { if let Some(p) = x.strip_suffix('%') { p.parse::<f64>().map(|v| v / 100.0).map_err(|_| format!("bad colour '{s}'")) } else { x.parse::<f64>().map(|v| v / 255.0).map_err(|_| format!("bad colour '{s}'")) } };
        let a = match parts.get(3) { Some(x) => if let Some(p) = x.strip_suffix('%') { p.parse::<f64>().map_err(|_| format!("bad colour '{s}'"))? / 100.0 } else { x.parse::<f64>().map_err(|_| format!("bad colour '{s}'"))? }, None => 1.0 };
        return Ok(Some(([ch(parts[0])?.clamp(0.0, 1.0), ch(parts[1])?.clamp(0.0, 1.0), ch(parts[2])?.clamp(0.0, 1.0)], a.clamp(0.0, 1.0))));
    }
    NAMED.iter().find(|(n, _)| *n == l).map(|(_, c)| Some(([c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0], 1.0))).ok_or_else(|| format!("unknown colour '{s}'"))
}

/// Two-colour approximation of a gradient paint: colours at the first and last stop and the blend direction (degrees, canvas y down).
#[derive(Clone, Debug, PartialEq)]
pub struct Grad { pub c1: [f64; 3], pub c2: [f64; 3], pub angle: f64 }

#[derive(Clone, Debug, PartialEq)]
pub enum Paint { None, Color([f64; 3]), Grad(Grad) }

#[derive(Clone, Debug)]
pub struct Shape { pub id: String, pub tag: String, pub path: PathData, pub fill: Paint, pub fill_opacity: f64, pub stroke: Paint, pub stroke_width: f64, pub stroke_opacity: f64, pub opacity: f64, pub evenodd: bool }
#[derive(Clone, Debug)]
pub struct Layer { pub name: String, pub shapes: Vec<Shape> }
#[derive(Clone, Debug)]
pub struct Doc { pub view: [f64; 4], pub layers: Vec<Layer>, pub skipped: Vec<String> }
impl Doc {
    pub fn bbox(&self) -> Option<[f64; 4]> { let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN]; let mut any = false; for l in &self.layers { for s in &l.shapes { if let Some(x) = s.path.bbox() { b = [b[0].min(x[0]), b[1].min(x[1]), b[2].max(x[2]), b[3].max(x[3])]; any = true; } } } if any { Some(b) } else { None } }
}

#[derive(Clone, Debug)]
struct Style { fill: String, stroke: String, stroke_width: f64, fill_opacity: f64, stroke_opacity: f64, fill_rule: String, color: [f64; 3], visible: bool }
impl Default for Style { fn default() -> Self { Style { fill: "black".into(), stroke: "none".into(), stroke_width: 1.0, fill_opacity: 1.0, stroke_opacity: 1.0, fill_rule: "nonzero".into(), color: [0.0; 3], visible: true } } }

struct Css { rules: Vec<(String, Vec<(String, String)>)> }
impl Css {
    fn parse(text: &str) -> Css {
        let text = strip_comments(text); let mut rules = vec![];
        for block in text.split('}') { let Some((sel, body)) = block.split_once('{') else { continue }; let decls = decls(body);
            for s in sel.split(',') { let s = s.trim(); if !s.is_empty() && !s.starts_with('@') { rules.push((s.to_string(), decls.clone())); } } }
        Css { rules }
    }
    /// Declarations matching a node, in specificity order (tag < class < id).
    fn for_node(&self, n: roxmltree::Node) -> Vec<(String, String)> {
        let tag = n.tag_name().name(); let id = n.attribute("id").unwrap_or(""); let classes: Vec<&str> = n.attribute("class").unwrap_or("").split_whitespace().collect();
        let mut hit: Vec<(u8, usize, &Vec<(String, String)>)> = vec![];
        for (i, (sel, d)) in self.rules.iter().enumerate() {
            let spec = if let Some(c) = sel.strip_prefix('.') { if classes.contains(&c) { Some(1) } else { None } }
                else if let Some(x) = sel.strip_prefix('#') { if x == id { Some(2) } else { None } }
                else if let Some((t, c)) = sel.split_once('.') { if t == tag && classes.contains(&c) { Some(1) } else { None } }
                else if sel == tag || sel == "*" { Some(0) } else { None };
            if let Some(s) = spec { hit.push((s, i, d)); }
        }
        hit.sort_by_key(|h| (h.0, h.1)); hit.into_iter().flat_map(|h| h.2.clone()).collect()
    }
}
fn strip_comments(s: &str) -> String { let mut o = String::new(); let mut r = s; while let Some(i) = r.find("/*") { o.push_str(&r[..i]); match r[i..].find("*/") { Some(j) => r = &r[i + j + 2..], None => { r = ""; } } } o.push_str(r); o }
fn decls(body: &str) -> Vec<(String, String)> { body.split(';').filter_map(|d| { let (k, v) = d.split_once(':')?; let v = v.trim().trim_end_matches("!important").trim(); Some((k.trim().to_ascii_lowercase(), v.to_string())) }).collect() }

fn num(s: Option<&str>, d: f64) -> f64 { s.and_then(|v| { let v = v.trim().trim_end_matches("px"); v.parse::<f64>().ok() }).filter(|v| v.is_finite()).unwrap_or(d) }

struct Ctx<'a> { css: Css, ids: HashMap<&'a str, roxmltree::Node<'a, 'a>>, grads: HashMap<&'a str, roxmltree::Node<'a, 'a>>, skipped: Vec<String>, depth: usize, count: usize }

pub const MAX_SHAPES: usize = 2000;

/// Parse an SVG document.
pub fn parse(svg: &str) -> Result<Doc, String> {
    let opt = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = roxmltree::Document::parse_with_options(svg, opt).map_err(|e| format!("not a valid SVG/XML document: {e}"))?;
    let root = doc.root_element(); if root.tag_name().name() != "svg" { return Err(format!("root element is <{}>, expected <svg>", root.tag_name().name())); }
    let mut css_text = String::new(); let mut ids = HashMap::new(); let mut grads = HashMap::new();
    for n in doc.descendants().filter(|n| n.is_element()) { if n.tag_name().name() == "style" { for t in n.children() { if let Some(x) = t.text() { css_text.push_str(x); css_text.push('\n'); } } } if let Some(id) = n.attribute("id") { ids.insert(id, n); if n.tag_name().name().ends_with("Gradient") { grads.entry(id).or_insert(n); } } }
    let mut ctx = Ctx { css: Css::parse(&css_text), ids, grads, skipped: vec![], depth: 0, count: 0 };
    let w = num(root.attribute("width"), 0.0); let h = num(root.attribute("height"), 0.0);
    let vb: Option<[f64; 4]> = root.attribute("viewBox").and_then(|v| { let p: Vec<f64> = v.split(|c: char| c == ',' || c.is_whitespace()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect(); if p.len() == 4 && p[2] > 0.0 && p[3] > 0.0 { Some([p[0], p[1], p[2], p[3]]) } else { None } });
    let root_style = style_of(&ctx, root, &Style::default())?;
    // layers = children of the root, unwrapping single wrapper groups (Illustrator "Layer_1", Inkscape layer)
    let mut parent = root; let mut m = ID; let mut st = root_style;
    loop {
        let kids: Vec<_> = parent.children().filter(|c| c.is_element() && is_drawable_or_group(c)).collect();
        if kids.len() == 1 && kids[0].tag_name().name() == "g" && kids[0].children().filter(|c| c.is_element() && is_drawable_or_group(c)).count() >= 2 {
            let g = kids[0]; m = mul(&m, &match g.attribute("transform") { Some(t) => parse_transform(t)?, None => ID }); st = style_of(&ctx, g, &st)?; if !st.visible { break; } parent = g; continue;
        }
        break;
    }
    let mut layers = vec![]; let mut unnamed = 0;
    for (i, c) in parent.children().filter(|c| c.is_element() && is_drawable_or_group(c)).enumerate() {
        let mut shapes = vec![]; let op = 1.0; walk(&mut ctx, c, &m, &st, op, &mut shapes)?;
        if shapes.is_empty() { continue; }
        let name = c.attribute(("http://www.inkscape.org/namespaces/inkscape", "label")).or(c.attribute("data-name")).or(c.attribute("id")).map(String::from).unwrap_or_else(|| { unnamed += 1; format!("{}_{}", c.tag_name().name(), i + 1) });
        layers.push(Layer { name, shapes });
    }
    // de-duplicate names
    let mut seen: HashMap<String, usize> = HashMap::new(); for l in layers.iter_mut() { let k = seen.entry(l.name.clone()).or_insert(0); *k += 1; if *k > 1 { l.name = format!("{}_{}", l.name, k); } }
    let view = vb.or_else(|| if w > 0.0 && h > 0.0 { Some([0.0, 0.0, w, h]) } else { None });
    let mut d = Doc { view: [0.0; 4], layers, skipped: ctx.skipped, };
    d.view = match view { Some(v) => v, None => { let b = d.bbox().ok_or("SVG has nothing drawable")?; [b[0], b[1], (b[2] - b[0]).max(1e-6), (b[3] - b[1]).max(1e-6)] } };
    if d.layers.is_empty() { return Err(format!("SVG has nothing drawable{}", if d.skipped.is_empty() { String::new() } else { format!(" (skipped: {})", d.skipped.join(", ")) })); }
    Ok(d)
}

fn is_drawable_or_group(n: &roxmltree::Node) -> bool { matches!(n.tag_name().name(), "g" | "a" | "switch" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "use" | "svg" | "text" | "image") }

fn style_of(ctx: &Ctx, n: roxmltree::Node, parent: &Style) -> Result<Style, String> {
    let mut s = parent.clone(); s.visible = parent.visible;
    let mut props: Vec<(String, String)> = vec![];
    for k in ["fill", "stroke", "stroke-width", "fill-opacity", "stroke-opacity", "fill-rule", "color", "display", "visibility"] { if let Some(v) = n.attribute(k) { props.push((k.into(), v.into())); } }
    props.extend(ctx.css.for_node(n)); if let Some(st) = n.attribute("style") { props.extend(decls(st)); }
    for (k, v) in props { match k.as_str() {
        "fill" => s.fill = v, "stroke" => s.stroke = v, "stroke-width" => s.stroke_width = num(Some(&v), s.stroke_width).max(0.0),
        "fill-opacity" => s.fill_opacity = num(Some(&v), 1.0).clamp(0.0, 1.0), "stroke-opacity" => s.stroke_opacity = num(Some(&v), 1.0).clamp(0.0, 1.0),
        "fill-rule" => s.fill_rule = v, "color" => { if let Ok(Some((c, _))) = parse_color(&v, s.color) { s.color = c; } }
        "display" => if v.trim() == "none" { s.visible = false; }, "visibility" => s.visible = !matches!(v.trim(), "hidden" | "collapse"),
        _ => {} } }
    Ok(s)
}
fn own_opacity(ctx: &Ctx, n: roxmltree::Node) -> f64 {
    let mut o = num(n.attribute("opacity"), 1.0); for (k, v) in ctx.css.for_node(n).into_iter().chain(n.attribute("style").map(decls).unwrap_or_default()) { if k == "opacity" { o = num(Some(&v), 1.0); } } o.clamp(0.0, 1.0)
}

fn paint(ctx: &Ctx, v: &str, current: [f64; 3], bbox: Option<[f64; 4]>, m: &Aff, alpha: &mut f64) -> Result<Paint, String> {
    let v = v.trim();
    if let Some(rest) = v.strip_prefix("url(") {
        let (inner, fallback) = match rest.split_once(')') { Some((a, b)) => (a, b.trim()), None => (rest, "") };
        let id = inner.trim().trim_matches(|c| c == '\'' || c == '"').trim_start_matches('#');
        // paint servers resolve to gradients first: real files reuse an id for a gradient and a group
        return match ctx.grads.get(id).or(ctx.ids.get(id)) { Some(g) => gradient(ctx, *g, bbox, m).map(|o| o.map_or(Paint::None, Paint::Grad)), None => if fallback.is_empty() { Ok(Paint::None) } else { paint(ctx, fallback, current, bbox, m, alpha) } };
    }
    Ok(match parse_color(v, current)? { None => Paint::None, Some((c, a)) => { *alpha *= a; Paint::Color(c) } })
}

fn gradient(ctx: &Ctx, g: roxmltree::Node, bbox: Option<[f64; 4]>, m: &Aff) -> Result<Option<Grad>, String> {
    let kind = g.tag_name().name(); if kind != "linearGradient" && kind != "radialGradient" { return Ok(None); }
    // stops may live on an href'd gradient
    let mut src = g; for _ in 0..8 { if src.children().any(|c| c.tag_name().name() == "stop") { break; } match src.attribute(("http://www.w3.org/1999/xlink", "href")).or(src.attribute("href")).and_then(|h| { let h = h.trim_start_matches('#'); ctx.grads.get(h).or(ctx.ids.get(h)) }) { Some(n) => src = *n, None => break } }
    let mut stops: Vec<[f64; 3]> = vec![];
    for s in src.children().filter(|c| c.tag_name().name() == "stop") {
        let mut col = s.attribute("stop-color").unwrap_or("black").to_string(); if let Some(st) = s.attribute("style") { for (k, v) in decls(st) { if k == "stop-color" { col = v; } } }
        if let Ok(Some((c, _))) = parse_color(&col, [0.0; 3]) { stops.push(c); }
    }
    if stops.is_empty() { return Ok(None); }
    let (c1, c2) = (stops[0], *stops.last().unwrap());
    let angle = if kind == "radialGradient" { 90.0 } else {
        let user = g.attribute("gradientUnits") == Some("userSpaceOnUse"); let pct = |s: Option<&str>, d: f64| s.map(|v| if let Some(p) = v.trim().strip_suffix('%') { p.parse::<f64>().unwrap_or(d * 100.0) / 100.0 } else { v.trim().parse::<f64>().unwrap_or(d) }).unwrap_or(d);
        let (mut x1, mut y1, mut x2, mut y2) = (pct(g.attribute("x1"), 0.0), pct(g.attribute("y1"), 0.0), pct(g.attribute("x2"), 1.0), pct(g.attribute("y2"), 0.0));
        if !user { if let Some(b) = bbox { x1 = b[0] + x1 * (b[2] - b[0]); x2 = b[0] + x2 * (b[2] - b[0]); y1 = b[1] + y1 * (b[3] - b[1]); y2 = b[1] + y2 * (b[3] - b[1]); } }
        else { let (p, q) = (ap(m, [x1, y1]), ap(m, [x2, y2])); x1 = p[0]; y1 = p[1]; x2 = q[0]; y2 = q[1]; }
        (y2 - y1).atan2(x2 - x1).to_degrees()
    };
    Ok(Some(Grad { c1, c2, angle }))
}

fn walk(ctx: &mut Ctx, n: roxmltree::Node, m: &Aff, parent: &Style, op: f64, out: &mut Vec<Shape>) -> Result<(), String> {
    let tag = n.tag_name().name();
    if matches!(tag, "defs" | "style" | "title" | "desc" | "metadata" | "clipPath" | "mask" | "symbol" | "linearGradient" | "radialGradient" | "pattern" | "marker" | "filter") { return Ok(()); }
    ctx.depth += 1; if ctx.depth > 64 { ctx.depth -= 1; return Err("SVG nests too deeply (> 64 levels or a <use> cycle)".into()); }
    let r = walk_inner(ctx, n, m, parent, op, out); ctx.depth -= 1; r
}
fn walk_inner(ctx: &mut Ctx, n: roxmltree::Node, m: &Aff, parent: &Style, op: f64, out: &mut Vec<Shape>) -> Result<(), String> {
    let tag = n.tag_name().name();
    let st = style_of(ctx, n, parent)?; if !st.visible { return Ok(()); }
    let mut m = match n.attribute("transform") { Some(t) => mul(m, &parse_transform(t)?), None => *m }; let op = op * own_opacity(ctx, n);
    if n.attribute("clip-path").is_some() || n.attribute("mask").is_some() { let w = format!("{tag} clip-path/mask ignored"); if !ctx.skipped.contains(&w) { ctx.skipped.push(w); } }
    let a = |k: &str| num(n.attribute(k), 0.0);
    let pd: Option<PathData> = match tag {
        "g" | "a" | "switch" => { for c in n.children().filter(|c| c.is_element()) { walk(ctx, c, &m, &st, op, out)?; } None }
        "svg" => { m = mul(&m, &[1.0, 0.0, 0.0, 1.0, a("x"), a("y")]); for c in n.children().filter(|c| c.is_element()) { walk(ctx, c, &m, &st, op, out)?; } None }
        "use" => { let href = n.attribute(("http://www.w3.org/1999/xlink", "href")).or(n.attribute("href")).unwrap_or("").trim_start_matches('#');
            match ctx.ids.get(href).copied() { Some(t) => { let mm = mul(&m, &[1.0, 0.0, 0.0, 1.0, a("x"), a("y")]);
                    if t.tag_name().name() == "symbol" { for c in t.children().filter(|c| c.is_element()) { walk(ctx, c, &mm, &st, op, out)?; } } else { walk(ctx, t, &mm, &st, op, out)?; } }
                None => ctx.skipped.push(format!("use #{href} (not found)")) } None }
        "path" => match n.attribute("d") { Some(d) if !d.trim().is_empty() => Some(parse_path(d, &m).map_err(|e| format!("path{}: {e}", n.attribute("id").map(|i| format!(" #{i}")).unwrap_or_default()))?), _ => None },
        "rect" => { let (x, y, w, h) = (a("x"), a("y"), a("width"), a("height")); if w <= 0.0 || h <= 0.0 { None } else {
            let (mut rx, mut ry) = (n.attribute("rx").map(|v| num(Some(v), 0.0)), n.attribute("ry").map(|v| num(Some(v), 0.0))); if rx.is_none() { rx = ry; } if ry.is_none() { ry = rx; }
            let (rx, ry) = (rx.unwrap_or(0.0).clamp(0.0, w / 2.0), ry.unwrap_or(0.0).clamp(0.0, h / 2.0));
            let d = if rx > 0.0 && ry > 0.0 { format!("M{} {} H{} A{rx} {ry} 0 0 1 {} {} V{} A{rx} {ry} 0 0 1 {} {} H{} A{rx} {ry} 0 0 1 {} {} V{} A{rx} {ry} 0 0 1 {} {} Z", x + rx, y, x + w - rx, x + w, y + ry, y + h - ry, x + w - rx, y + h, x + rx, x, y + h - ry, y + ry, x + rx, y) }
                else { format!("M{x} {y} H{} V{} H{x} Z", x + w, y + h) };
            Some(parse_path(&d, &m)?) } }
        "circle" | "ellipse" => { let (cx, cy) = (a("cx"), a("cy")); let (rx, ry) = if tag == "circle" { (a("r"), a("r")) } else { (a("rx"), a("ry")) };
            if rx <= 0.0 || ry <= 0.0 { None } else { Some(parse_path(&format!("M{} {cy} A{rx} {ry} 0 0 1 {cx} {} A{rx} {ry} 0 0 1 {} {cy} A{rx} {ry} 0 0 1 {cx} {} A{rx} {ry} 0 0 1 {} {cy} Z", cx + rx, cy + ry, cx - rx, cy - ry, cx + rx), &m)?) } }
        "line" => Some(parse_path(&format!("M{} {} L{} {}", a("x1"), a("y1"), a("x2"), a("y2")), &m)?),
        "polyline" | "polygon" => { let v: Vec<f64> = n.attribute("points").unwrap_or("").split(|c: char| c == ',' || c.is_whitespace()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
            if v.len() < 4 { None } else { let mut d = format!("M{} {}", v[0], v[1]); for p in v[2..].chunks(2).filter(|p| p.len() == 2) { d += &format!(" L{} {}", p[0], p[1]); } if tag == "polygon" { d += " Z"; } Some(parse_path(&d, &m)?) } }
        "text" | "image" | "foreignObject" => { let w = format!("<{tag}> (not imported; add text with add_text)"); if !ctx.skipped.contains(&w) { ctx.skipped.push(w); } None }
        _ => None,
    };
    let Some(path) = pd else { return Ok(()) }; if !path.drawable() { return Ok(()); }
    ctx.count += 1; if ctx.count > MAX_SHAPES { return Err(format!("SVG has more than {MAX_SHAPES} shapes; simplify it or import a part with `only`")); }
    let bb = path.bbox(); let (mut fa, mut sa) = (st.fill_opacity, st.stroke_opacity);
    let fill = paint(ctx, &st.fill, st.color, bb, &m, &mut fa)?; let stroke = paint(ctx, &st.stroke, st.color, bb, &m, &mut sa)?;
    // open subpaths (lines, polylines) have no fill area worth drawing in kimo's renderer; keep their stroke only
    let fill = if matches!(tag, "line" | "polyline") { Paint::None } else { fill };
    if fill == Paint::None && (stroke == Paint::None || st.stroke_width <= 0.0) { return Ok(()); }
    out.push(Shape { id: n.attribute("id").unwrap_or("").to_string(), tag: tag.into(), path, fill, fill_opacity: fa, stroke, stroke_width: st.stroke_width * scale_of(&m), stroke_opacity: sa, opacity: op, evenodd: st.fill_rule.trim() == "evenodd" });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: Pt, b: Pt) -> bool { (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 }

    #[test]
    fn colours() {
        assert_eq!(parse_color("#fff", [0.0; 3]).unwrap(), Some(([1.0; 3], 1.0)));
        assert_eq!(parse_color("#ff000080", [0.0; 3]).unwrap().unwrap().0, [1.0, 0.0, 0.0]);
        assert!((parse_color("#ff000080", [0.0; 3]).unwrap().unwrap().1 - 128.0 / 255.0).abs() < 1e-9);
        assert_eq!(parse_color("rgb(0, 255, 0)", [0.0; 3]).unwrap(), Some(([0.0, 1.0, 0.0], 1.0)));
        assert_eq!(parse_color("none", [0.0; 3]).unwrap(), None);
        assert_eq!(parse_color("currentColor", [0.5; 3]).unwrap(), Some(([0.5; 3], 1.0)));
        assert!(parse_color("notacolour", [0.0; 3]).is_err());
    }
    #[test]
    fn transforms_compose_in_svg_order() {
        let m = parse_transform("translate(10 20) scale(2)").unwrap(); assert!(near(ap(&m, [1.0, 1.0]), [12.0, 22.0]));
        let r = parse_transform("rotate(90 10 10)").unwrap(); assert!(near(ap(&r, [20.0, 10.0]), [10.0, 20.0]));
        let k = parse_transform("matrix(1 0 0 1 5 6)").unwrap(); assert!(near(ap(&k, [0.0, 0.0]), [5.0, 6.0]));
        assert!(parse_transform("wobble(3)").is_err());
    }
    #[test]
    fn relative_and_smooth_commands_become_absolute_cubics() {
        let p = parse_path("m10 10 h10 v10 q5 5 10 0 t10 0 s5 5 10 0 z", &ID).unwrap();
        assert_eq!(p.0[0], Seg::M([10.0, 10.0])); assert_eq!(p.0[1], Seg::L([20.0, 10.0])); assert_eq!(p.0[2], Seg::L([20.0, 20.0]));
        match &p.0[3] { Seg::C(_, _, e) => assert!(near(*e, [30.0, 20.0])), o => panic!("{o:?}") }
        match &p.0[4] { Seg::C(_, _, e) => assert!(near(*e, [40.0, 20.0])), o => panic!("{o:?}") }
        assert_eq!(*p.0.last().unwrap(), Seg::Z);
    }
    #[test]
    fn arcs_become_cubics_on_the_circle() {
        let segs = arc_to_cubics([10.0, 0.0], 10.0, 10.0, 0.0, false, true, [-10.0, 0.0]); assert_eq!(segs.len(), 2);
        for s in &segs { if let Seg::C(_, _, e) = s { assert!((e[0].hypot(e[1]) - 10.0).abs() < 1e-6); } }
        // midpoint of the first quarter cubic stays within 0.03% of the radius
        if let Seg::C(a, b, e) = &segs[0] { let p0 = [10.0, 0.0]; let mid = [0.125 * p0[0] + 0.375 * a[0] + 0.375 * b[0] + 0.125 * e[0], 0.125 * p0[1] + 0.375 * a[1] + 0.375 * b[1] + 0.125 * e[1]]; assert!((mid[0].hypot(mid[1]) - 10.0).abs() < 0.003); }
    }
    #[test]
    fn top_level_groups_become_layers_with_inherited_styles() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
          <style>.hot { fill: #ff0000 } #b rect { fill: blue }</style>
          <g id="Layer_1"><g id="a" fill="#00ff00" transform="translate(10 0)"><rect width="10" height="10"/><circle class="hot" cx="5" cy="30" r="5"/></g>
          <g id="b" stroke="black" stroke-width="2" fill="none"><line x1="0" y1="0" x2="50" y2="0"/><path d="M0 0 L10 10" style="stroke: white"/></g>
          <text x="0" y="0">hi</text></g></svg>"##;
        let d = parse(svg).unwrap();
        assert_eq!(d.view, [0.0, 0.0, 100.0, 50.0]);
        assert_eq!(d.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["a", "b"], "the wrapper Layer_1 is unwrapped; text is skipped");
        let a = &d.layers[0]; assert_eq!(a.shapes.len(), 2); assert_eq!(a.shapes[0].fill, Paint::Color([0.0, 1.0, 0.0])); assert_eq!(a.shapes[1].fill, Paint::Color([1.0, 0.0, 0.0]));
        assert_eq!(a.shapes[0].path.bbox().unwrap(), [10.0, 0.0, 20.0, 10.0], "group transform applied");
        let b = &d.layers[1]; assert_eq!(b.shapes[0].fill, Paint::None); assert_eq!(b.shapes[0].stroke, Paint::Color([0.0; 3])); assert_eq!(b.shapes[0].stroke_width, 2.0); assert_eq!(b.shapes[1].stroke, Paint::Color([1.0; 3]));
        assert!(d.skipped.iter().any(|s| s.contains("text")));
    }
    #[test]
    fn gradients_use_and_hidden_elements() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="40" height="40">
          <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000"/><stop offset="1" style="stop-color:#fff"/></linearGradient>
          <rect id="r" width="5" height="5" fill="url(#g)"/></defs>
          <use xlink:href="#r" x="10" y="10"/><circle cx="1" cy="1" r="1" display="none"/><g opacity="0.5"><rect width="4" height="4" fill="red" fill-opacity="0.5"/></g></svg>"##;
        let d = parse(svg).unwrap(); assert_eq!(d.view, [0.0, 0.0, 40.0, 40.0]);
        let dup = parse(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 9 9"><defs><linearGradient id="sky"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient></defs><g id="sky"><rect width="9" height="9" fill="url(#sky)"/></g></svg>"##).unwrap();
        assert!(matches!(dup.layers[0].shapes[0].fill, Paint::Grad(_)), "a group sharing the gradient's id must not hide it"); assert_eq!(d.layers.len(), 2, "hidden circle dropped");
        match &d.layers[0].shapes[0].fill { Paint::Grad(g) => { assert_eq!(g.c1, [0.0; 3]); assert_eq!(g.c2, [1.0; 3]); assert!((g.angle - 90.0).abs() < 1e-9); } o => panic!("{o:?}") }
        assert_eq!(d.layers[0].shapes[0].path.bbox().unwrap(), [10.0, 10.0, 15.0, 15.0]);
        let s = &d.layers[1].shapes[0]; assert_eq!(s.opacity, 0.5); assert_eq!(s.fill_opacity, 0.5);
    }
    #[test]
    fn errors_are_loud() {
        assert!(parse("<html/>").unwrap_err().contains("expected <svg>"));
        assert!(parse("<svg xmlns='http://www.w3.org/2000/svg'><text>x</text></svg>").unwrap_err().contains("nothing drawable"));
        assert!(parse("not xml").is_err());
        assert!(parse(r#"<svg xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L1 1 X"/></svg>"#).is_err());
    }
}
