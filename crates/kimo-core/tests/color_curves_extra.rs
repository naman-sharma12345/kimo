//! Curves, the rest of Resolve's seven: hue-vs-lum, sat-vs-sat and sat-vs-lum on `point_curves` / `set_color_curve`.
//! Model (re-implemented here in f64, the effect must match within 1e-6): Rec.709 Y, Cb=(B-Y)/1.8556, Cr=(R-Y)/1.5748,
//! rho=|(Cb,Cr)|, saturation S=2*rho (about 1 at the Rec.709 primaries), hue=atan2(Cr,Cb).
//! Chroma is scaled by hue_sat(h)*lum_sat(Y)*sat_sat(S) (and rotated by hue_hue), luma is scaled by
//! (1+(hue_lum(h)-1)*min(1,2S)) * sat_lum(S), so hue_lum leaves greys alone; chroma (Cb,Cr) is kept by both luma curves.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};
const TOL: f64 = 1e-6;

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
type C<'a> = Option<&'a [[f64; 2]]>;
fn expect(c: [f64; 3], hl: C, ss: C, sl: C) -> [f64; 3] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; let cb = (c[2] - y) / 1.8556; let cr = (c[0] - y) / 1.5748; let rho = (cb * cb + cr * cr).sqrt(); let s = 2.0 * rho;
    if rho == 0.0 { let g = sl.map_or(1.0, |p| lin_eval(p, 0.0)).max(0.0); if g == 1.0 { return c; } let y2 = y * g; return [y2, y2, y2]; }
    let mut h = cr.atan2(cb).to_degrees(); if h < 0.0 { h += 360.0; }
    let k = ss.map_or(1.0, |p| lin_eval(p, s)).max(0.0);
    let gy = ((1.0 + (hl.map_or(1.0, |p| hue_eval(p, h)) - 1.0) * (2.0 * s).min(1.0)) * sl.map_or(1.0, |p| lin_eval(p, s))).max(0.0);
    let (cb2, cr2) = (cb * k, cr * k); let y2 = y * gy;
    let r = y2 + 1.5748 * cr2; let b = y2 + 1.8556 * cb2; [r, (y2 - 0.2126 * r - 0.0722 * b) / 0.7152, b] }
fn pr(curves: &[(&str, &[[f64; 2]])]) -> Params { let mut p = Params::new();
    for (pre, pts) in curves { p.insert(format!("{pre}_count"), Prop::new(pts.len() as f64)); for (i, q) in pts.iter().enumerate() { p.insert(format!("{pre}_x{i}"), Prop::new(q[0])); p.insert(format!("{pre}_y{i}"), Prop::new(q[1])); } } p }
fn hsv(h: f64, s: f64, v: f64) -> (f64, f64, f64) { let c = v * s; let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs()); let m = v - c; let (r, g, b) = match (h / 60.0) as u32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) }; (r + m, g + m, b + m) }
fn chart() -> Vec<[f32; 4]> { let mut v = vec![]; for i in 0..24 { for s in [0.15, 0.45, 0.9] { let (r, g, b) = hsv(i as f64 * 15.0, s, 0.8); v.push([r as f32, g as f32, b as f32, 1.0]); } } v.push([0.5, 0.5, 0.5, 1.0]); v.push([0.2, 0.2, 0.2, 1.0]); v.push([0.9, 0.2, 0.3, 0.4]); v }
fn run(p: &Params, px: &[[f32; 4]]) -> Frame { let mut f = Frame { w: px.len(), h: 1, d: px.iter().flatten().copied().collect() }; kimo_core::color::apply3("point_curves", p, &mut f, 0.0, 0).unwrap(); f }
const HL: [[f64; 2]; 4] = [[0.0, 1.0], [100.0, 1.3], [230.0, 1.0], [350.0, 0.6]]; // brighten reds/oranges, darken blues
const SS: [[f64; 2]; 3] = [[0.0, 1.0], [0.4, 1.4], [1.2, 0.7]]; // lift weak colour, tame strong colour
const SL: [[f64; 2]; 3] = [[0.0, 1.0], [0.5, 1.0], [1.2, 0.8]]; // darken only the most saturated
fn check(p: &Params, hl: C, ss: C, sl: C, what: &str) {
    let px = chart(); let f = run(p, &px);
    for (i, q) in px.iter().enumerate() { let e = expect([q[0] as f64, q[1] as f64, q[2] as f64], hl, ss, sl);
        for j in 0..3 { assert!((f.d[i * 4 + j] as f64 - e[j]).abs() <= TOL, "{what}: px{i} ch{j}: got {} want {}", f.d[i * 4 + j], e[j]); }
        assert_eq!(f.d[i * 4 + 3], q[3], "{what}: alpha px{i}"); }
}
fn ycc(c: &[f32]) -> (f64, f64, f64) { let (r, g, b) = (c[0] as f64, c[1] as f64, c[2] as f64); let y = 0.2126 * r + 0.7152 * g + 0.0722 * b; (y, (b - y) / 1.8556, (r - y) / 1.5748) }

#[test] fn hue_vs_lum_exact() { check(&pr(&[("hl", &HL)]), Some(&HL), None, None, "hue_lum"); }
#[test] fn sat_vs_sat_exact() { check(&pr(&[("ss", &SS)]), None, Some(&SS), None, "sat_sat"); }
#[test] fn sat_vs_lum_exact() { check(&pr(&[("sl", &SL)]), None, None, Some(&SL), "sat_lum"); }
#[test] fn all_three_combined_exact() { check(&pr(&[("hl", &HL), ("ss", &SS), ("sl", &SL)]), Some(&HL), Some(&SS), Some(&SL), "combined"); }

#[test]
fn hue_vs_lum_moves_luma_only_and_spares_greys() {
    let blue = [0.1f32, 0.2, 0.9, 1.0]; let grey = [0.5f32, 0.5, 0.5, 1.0];
    let f = run(&pr(&[("hl", &HL)]), &[blue, grey]);
    let (y0, cb0, cr0) = ycc(&blue); let (y1, cb1, cr1) = ycc(&f.d[0..4]);
    assert!(y1 < y0 * 0.8, "hue_lum dips at blue: luma {y0} -> {y1} must drop");
    assert!((cb1 - cb0).abs() < 1e-6 && (cr1 - cr0).abs() < 1e-6, "hue_lum keeps chroma: ({cb0},{cr0}) -> ({cb1},{cr1})");
    assert_eq!(&f.d[4..8], &grey, "a grey has no hue: hue_lum must leave it bit-exact");
}

#[test]
fn identity_extra_curves_bit_exact() {
    let px = chart(); let f = run(&pr(&[("hl", &[[0.0, 1.0], [180.0, 1.0]]), ("ss", &[[0.0, 1.0], [1.0, 1.0]]), ("sl", &[[0.0, 1.0], [1.0, 1.0]])]), &px);
    assert_eq!(f.d, px.iter().flatten().copied().collect::<Vec<f32>>(), "identity hue_lum/sat_sat/sat_lum must be bit-exact");
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn scene() -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":16,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.9,0.2,0.1],"color2":[0.1,0.3,0.95],"angle":90,"start":0,"duration":2}))["clip_id"].clone(); (s, id) }
fn render(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }

#[test]
fn tool_new_channels_render_exact() {
    let (mut s, id) = scene(); let before = render(&s);
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"hue_lum","points":HL}));
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"sat_sat","points":SS}));
    c(&mut s, "set_color_curve", json!({"clip_id":id,"channel":"sat_lum","points":SL}));
    let after = render(&s);
    // three separate point_curves effects run in order: hue_lum, then sat_sat, then sat_lum
    for i in 0..before.len() / 4 { let mut e = [before[i * 4] as f64, before[i * 4 + 1] as f64, before[i * 4 + 2] as f64];
        e = expect(e, Some(&HL), None, None); e = expect([e[0] as f32 as f64, e[1] as f32 as f64, e[2] as f32 as f64], None, Some(&SS), None); e = expect([e[0] as f32 as f64, e[1] as f32 as f64, e[2] as f32 as f64], None, None, Some(&SL));
        for j in 0..3 { assert!((after[i * 4 + j] as f64 - e[j]).abs() <= 1e-5, "render px{i} ch{j}: {} vs {}", after[i * 4 + j], e[j]); } }
    assert!(before.iter().zip(&after).any(|(a, b)| (a - b).abs() > 1e-3), "the three curves must change the render");
}

#[test]
fn tool_new_channel_refusals_leave_project_untouched() {
    let (mut s, id) = scene(); let snap = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id":id,"channel":"hue_lum","points":[[0,1],[90,5]]}), "hue_lum"), (json!({"clip_id":id,"channel":"sat_sat","points":[[0,1],[3,1]]}), "sat_sat"),
        (json!({"clip_id":id,"channel":"sat_lum","points":[[0,1],[1,-0.5]]}), "sat_lum"), (json!({"clip_id":id,"channel":"hue_lum","points":[[0,1],[360,1]]}), "hue")] {
        let e = dispatch(&mut s, "set_color_curve", &a).expect_err(&format!("{a} must be refused")); assert!(e.to_string().contains(want), "{a}: '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), snap, "{a} mutated the project"); }
}

/// The hue axis is the Rec.709 CbCr angle, not HSV: the tool must say so (small models assume HSV blue = 240), and the
/// documented anchors must be where the effect actually puts the colours.
#[test]
fn hue_axis_documented_and_anchors_true() {
    let cmd = kimo_core::commands::registry_ref().iter().find(|c| c.name == "set_color_curve").expect("set_color_curve");
    let doc = cmd.schema["properties"]["points"]["description"].as_str().unwrap_or("");
    assert!(doc.contains("CbCr") && doc.contains("not HSV") && doc.contains("blue 355") && doc.contains("red 103"), "set_color_curve points must document the hue axis (CbCr angle, red 103, blue 355); got '{doc}'");
    for (name, col, at) in [("blue", [0.05f32, 0.1, 0.9, 1.0], 355.0), ("red", [0.9, 0.1, 0.05, 1.0], 103.0), ("green", [0.1, 0.8, 0.1, 1.0], 230.0), ("yellow", [0.9, 0.85, 0.1, 1.0], 175.0)] {
        let far = (at + 180.0f64) % 360.0; let mut pts = vec![[at, 0.5], [far, 1.0]]; pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let f = run(&pr(&[("hl", pts.as_slice())]), &[col]); let (y0, _, _) = ycc(&col); let (y1, _, _) = ycc(&f.d[0..4]);
        assert!(y1 < y0 * 0.75, "{name}: a hue_lum dip at the documented {at} deg must darken it: luma {y0} -> {y1}");
    }
}
