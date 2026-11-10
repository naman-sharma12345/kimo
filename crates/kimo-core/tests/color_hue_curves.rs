//! Curves: identity is bit-exact, plus Resolve-style hue-vs-sat, hue-vs-hue (periodic over the Rec.709 vectorscope angle) and
//! lum-vs-sat, all monotone cubic Fritsch–Carlson, on `point_curves` / `set_color_curve`. The reference below re-implements
//! Fritsch–Carlson (1980) and the luma-preserving chroma model in f64; the effect must match within 1e-6.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};
const TOL: f64 = 1e-6;

/// Fritsch–Carlson tangents. periodic: x in degrees, the point after the last is first+360.
fn fc(x: &[f64], y: &[f64], periodic: bool) -> Vec<f64> {
    let n = x.len(); let ni = if periodic { n } else { n - 1 };
    let d: Vec<f64> = (0..ni).map(|k| if k + 1 < n { (y[k + 1] - y[k]) / (x[k + 1] - x[k]) } else { (y[0] - y[k]) / (x[0] + 360.0 - x[k]) }).collect();
    let mut m: Vec<f64> = (0..n).map(|k| {
        if !periodic && k == 0 { return d[0]; } if !periodic && k == n - 1 { return d[n - 2]; }
        let a = if k == 0 { d[ni - 1] } else { d[k - 1] }; let b = d[k]; if a * b <= 0.0 { 0.0 } else { (a + b) / 2.0 } }).collect();
    for k in 0..ni { let k1 = (k + 1) % n; if d[k] == 0.0 { m[k] = 0.0; m[k1] = 0.0; } else { let (a, b) = (m[k] / d[k], m[k1] / d[k]); let s = a * a + b * b; if s > 9.0 { let tau = 3.0 / s.sqrt(); m[k] = tau * a * d[k]; m[k1] = tau * b * d[k]; } } }
    m }
fn herm(x0: f64, x1: f64, y0: f64, y1: f64, m0: f64, m1: f64, u: f64) -> f64 { let h = x1 - x0; let t = (u - x0) / h; let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * h * m0 + (-2.0 * t3 + 3.0 * t2) * y1 + (t3 - t2) * h * m1 }
fn hue_eval(p: &[[f64; 2]], h: f64) -> f64 { let x: Vec<f64> = p.iter().map(|q| q[0]).collect(); let y: Vec<f64> = p.iter().map(|q| q[1]).collect(); let m = fc(&x, &y, true); let n = x.len();
    let mut u = (h - x[0]).rem_euclid(360.0) + x[0]; if u >= x[0] + 360.0 { u = x[0]; }
    let i = x.iter().rposition(|v| *v <= u).unwrap(); let (x1, y1) = if i + 1 < n { (x[i + 1], y[i + 1]) } else { (x[0] + 360.0, y[0]) }; herm(x[i], x1, y[i], y1, m[i], m[(i + 1) % n], u) }
fn lin_eval(p: &[[f64; 2]], v: f64) -> f64 { let x: Vec<f64> = p.iter().map(|q| q[0]).collect(); let y: Vec<f64> = p.iter().map(|q| q[1]).collect(); let m = fc(&x, &y, false); let n = x.len();
    let u = v.clamp(x[0], x[n - 1]); if u >= x[n - 1] { return y[n - 1]; } let i = x.iter().rposition(|q| *q <= u).unwrap(); herm(x[i], x[i + 1], y[i], y[i + 1], m[i], m[i + 1], u) }
/// Rec.709 luma + vectorscope chroma: hue = atan2(cr, cb), k scales chroma, dh rotates it; luma is kept.
fn expect(c: [f64; 3], hs: Option<&[[f64; 2]]>, hh: Option<&[[f64; 2]]>, ls: Option<&[[f64; 2]]>) -> [f64; 3] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; let cb = (c[2] - y) / 1.8556; let cr = (c[0] - y) / 1.5748; let rho = (cb * cb + cr * cr).sqrt(); if rho == 0.0 { return c; }
    let mut h = cr.atan2(cb).to_degrees(); if h < 0.0 { h += 360.0; }
    let k = (hs.map_or(1.0, |p| hue_eval(p, h)) * ls.map_or(1.0, |p| lin_eval(p, y))).max(0.0); let dh = hh.map_or(0.0, |p| hue_eval(p, h));
    let (cb2, cr2) = if dh == 0.0 { (cb * k, cr * k) } else { let a = (h + dh).to_radians(); (rho * k * a.cos(), rho * k * a.sin()) };
    let r = y + 1.5748 * cr2; let b = y + 1.8556 * cb2; [r, (y - 0.2126 * r - 0.0722 * b) / 0.7152, b] }
fn pr(curves: &[(&str, &[[f64; 2]])]) -> Params { let mut p = Params::new();
    for (pre, pts) in curves { p.insert(format!("{pre}_count"), Prop::new(pts.len() as f64)); for (i, q) in pts.iter().enumerate() { p.insert(format!("{pre}_x{i}"), Prop::new(q[0])); p.insert(format!("{pre}_y{i}"), Prop::new(q[1])); } } p }
fn chart() -> Vec<[f32; 4]> { let mut v = vec![]; for i in 0..24 { let h = i as f64 * 15.0; let (r, g, b) = hsv(h, 0.7, 0.8); v.push([r as f32, g as f32, b as f32, 1.0]); } v.push([0.5, 0.5, 0.5, 1.0]); v.push([0.9, 0.2, 0.3, 0.4]); v.push([0.05, 0.1, 0.6, 1.0]); v }
fn hsv(h: f64, s: f64, v: f64) -> (f64, f64, f64) { let c = v * s; let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs()); let m = v - c; let (r, g, b) = match (h / 60.0) as u32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) }; (r + m, g + m, b + m) }
fn run(p: &Params, px: &[[f32; 4]]) -> Frame { let mut f = Frame { w: px.len(), h: 1, d: px.iter().flatten().copied().collect() }; kimo_core::color::apply3("point_curves", p, &mut f, 0.0, 0).unwrap(); f }
fn check(p: &Params, hs: Option<&[[f64; 2]]>, hh: Option<&[[f64; 2]]>, ls: Option<&[[f64; 2]]>, what: &str) {
    let px = chart(); let f = run(p, &px);
    for (i, q) in px.iter().enumerate() { let e = expect([q[0] as f64, q[1] as f64, q[2] as f64], hs, hh, ls);
        for j in 0..3 { let g = f.d[i * 4 + j] as f64; assert!((g - e[j]).abs() <= TOL, "{what}: px{i} ch{j} got {g:.9} expected {:.9}", e[j]); }
        assert_eq!(f.d[i * 4 + 3].to_bits(), q[3].to_bits());
        let (y0, y1) = (0.2126 * q[0] as f64 + 0.7152 * q[1] as f64 + 0.0722 * q[2] as f64, 0.2126 * f.d[i * 4] as f64 + 0.7152 * f.d[i * 4 + 1] as f64 + 0.0722 * f.d[i * 4 + 2] as f64);
        assert!((y1 - y0).abs() <= TOL, "{what}: luma moved at px{i}: {y0} -> {y1}"); } }

const HS: [[f64; 2]; 4] = [[0.0, 1.0], [90.0, 1.6], [200.0, 0.4], [300.0, 1.0]];
const HH: [[f64; 2]; 3] = [[20.0, 0.0], [110.0, 25.0], [250.0, -30.0]];
const LS: [[f64; 2]; 4] = [[0.0, 0.2], [0.3, 1.0], [0.7, 1.0], [1.0, 0.5]];

#[test] fn hue_vs_sat_exact() { check(&pr(&[("hs", &HS)]), Some(&HS), None, None, "hue_sat"); }
#[test] fn hue_vs_hue_exact() { check(&pr(&[("hh", &HH)]), None, Some(&HH), None, "hue_hue"); }
#[test] fn lum_vs_sat_exact() { check(&pr(&[("ls", &LS)]), None, None, Some(&LS), "lum_sat"); }
#[test] fn all_three_together_exact() { check(&pr(&[("hs", &HS), ("hh", &HH), ("ls", &LS)]), Some(&HS), Some(&HH), Some(&LS), "hs+hh+ls"); }

#[test]
fn fritsch_carlson_hand_values_and_no_overshoot() {
    // two-knot segment of a steep step: knots (0,0),(0.5,0),(0.6,1),(1,1): FC sets tangents 0 at the flat knots, so the
    // segment 0.5..0.6 is the cubic 3t^2-2t^3 exactly. At x=0.55 (t=0.5) -> 0.5; at x=0.52 (t=0.2) -> 0.104.
    let p: [[f64; 2]; 4] = [[0.0, 0.0], [0.5, 0.0], [0.6, 1.0], [1.0, 1.0]];
    assert!((lin_eval(&p, 0.55) - 0.5).abs() < 1e-12 && (lin_eval(&p, 0.52) - 0.104).abs() < 1e-12);
    let ls: [[f64; 2]; 4] = [[0.0, 0.5], [0.5, 0.5], [0.6, 1.5], [1.0, 1.5]];
    let mut px = vec![]; for i in 0..=200 { let v = i as f32 / 200.0; px.push([v * 0.9 + 0.05, v * 0.6, v * 0.3, 1.0]); }
    let f = run(&pr(&[("ls", &ls)]), &px); let mut prev_k = 0.0;
    for (i, q) in px.iter().enumerate() { let c = [q[0] as f64, q[1] as f64, q[2] as f64]; let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let o = [f.d[i * 4] as f64, f.d[i * 4 + 1] as f64, f.d[i * 4 + 2] as f64]; if (c[0] - y).abs() < 1e-3 { continue; } let k = (o[0] - y) / (c[0] - y);
        assert!(k >= 0.5 - 1e-6 && k <= 1.5 + 1e-6, "overshoot k={k} at y={y}"); assert!(k >= prev_k - 1e-6, "not monotone at y={y}"); prev_k = k;
        assert!((k - lin_eval(&ls, y)).abs() <= 1e-5, "k {k} vs {} at y={y}", lin_eval(&ls, y)); }
}

#[test]
fn identity_curves_are_bit_exact() {
    let mut px = vec![]; for i in -40..1040 { let v = i as f32 / 997.0; px.push([v, 1.0 - v, v * v, 1.0]); }
    let ident: [[f64; 2]; 4] = [[0.0, 0.0], [0.13, 0.13], [0.61, 0.61], [1.0, 1.0]];
    let hs1: [[f64; 2]; 3] = [[0.0, 1.0], [120.0, 1.0], [240.0, 1.0]]; let hh0: [[f64; 2]; 2] = [[10.0, 0.0], [200.0, 0.0]]; let ls1: [[f64; 2]; 2] = [[0.0, 1.0], [1.0, 1.0]];
    for (what, p) in [("defaults", Params::new()), ("explicit identity m/r", pr(&[("m", &ident), ("r", &ident)])), ("identity hue curves", pr(&[("hs", &hs1), ("hh", &hh0), ("ls", &ls1)]))] {
        let f = run(&p, &px); let bad = px.iter().flatten().zip(&f.d).position(|(a, b)| a.to_bits() != b.to_bits()); assert!(bad.is_none(), "{what}: bit changed at {bad:?}"); }
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":16,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.9,0.2,0.1],"color2":[0.1,0.3,0.95],"angle":90,"start":0,"duration":2}))["clip_id"].clone(); (s, id) }
fn render(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }

#[test]
fn tool_hue_channels_render_exact() {
    let (mut s, id) = scene(); let before = render(&s);
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"hue_sat","points":HS}));
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"hue_hue","points":HH}));
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"lum_sat","points":LS}));
    let after = render(&s);
    for i in 0..before.len() / 4 { let e = expect([before[i * 4] as f64, before[i * 4 + 1] as f64, before[i * 4 + 2] as f64], Some(&HS), Some(&HH), Some(&LS));
        for j in 0..3 { assert!((after[i * 4 + j] as f64 - e[j]).abs() <= TOL, "render px{i} ch{j}: {} vs {}", after[i * 4 + j], e[j]); } }
}

#[test]
fn tool_refusals_leave_project_untouched() {
    let (mut s, id) = scene(); let snap = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id":id,"channel":"hue_sat","points":[[0,1],[360,1]]}), "hue"), (json!({"clip_id":id,"channel":"hue_sat","points":[[0,1],[90,5]]}), "hue_sat"),
        (json!({"clip_id":id,"channel":"hue_hue","points":[[0,0],[90,200]]}), "hue_hue"), (json!({"clip_id":id,"channel":"lum_sat","points":[[0,1],[3,1]]}), "lum_sat"),
        (json!({"clip_id":id,"channel":"hue_sat","points":[[10,1],[10.0001,1]]}), "hue"), (json!({"clip_id":id,"channel":"sat_vs_sat","points":[[0,1],[1,1]]}), "channel")] {
        let e = dispatch(&mut s, "set_color_curve", &a).expect_err(&format!("{a} must be refused")); assert!(e.to_string().contains(want), "{a}: '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), snap, "{a} mutated the project"); }
}
