//! Resolve-style lift/gamma/gain/offset colour wheels: `primaries` model 1 and the `set_color_wheels` tool.
//! Per channel y = ((gain-lift)*x + lift + offset)^(1/gamma) (ASC CDL slope = gain-lift, offset = lift+offset, power = 1/gamma),
//! then a luma-preserving master on Rec.709 Y (adds Y2-Y to R,G,B), contrast about pivot, luma-preserving saturation.
//! Expected values below are computed here in f64 from those formulas; the effect works in f32 (tolerance 1e-6).
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

const TOL: f64 = 1e-6;
fn params(kv: &[(&str, f64)]) -> Params { kv.iter().map(|(k, v)| (k.to_string(), Prop::new(*v))).collect() }
fn frame(px: &[[f32; 4]]) -> Frame { Frame { w: px.len(), h: 1, d: px.iter().flatten().copied().collect() } }
fn y709(c: [f64; 3]) -> f64 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn lgg(x: f64, lift: f64, gamma: f64, gain: f64, offset: f64) -> f64 { let y = (gain - lift) * x + lift + offset; if gamma != 1.0 { y.max(0.0).powf(1.0 / gamma) } else { y } }
struct W { lift: [f64; 4], gamma: [f64; 4], gain: [f64; 4], offset: [f64; 4], contrast: f64, pivot: f64, sat: f64 }
fn neutral() -> W { W { lift: [0.0; 4], gamma: [1.0; 4], gain: [1.0; 4], offset: [0.0; 4], contrast: 1.0, pivot: 0.435, sat: 1.0 } }
fn expect(w: &W, c: [f64; 3]) -> [f64; 3] {
    let mut o = [0.0; 3]; for j in 0..3 { o[j] = lgg(c[j], w.lift[j], w.gamma[j], w.gain[j], w.offset[j]); }
    let y = y709(o); let y2 = lgg(y, w.lift[3], w.gamma[3], w.gain[3], w.offset[3]); for j in 0..3 { o[j] += y2 - y; }
    for j in 0..3 { o[j] = (o[j] - w.pivot) * w.contrast + w.pivot; }
    let y = y709(o); for j in 0..3 { o[j] = y + w.sat * (o[j] - y); } o }
fn wparams(w: &W, space: f64) -> Params {
    let mut v: Vec<(String, f64)> = vec![("model".into(), 1.0), ("space".into(), space), ("contrast".into(), w.contrast), ("pivot".into(), w.pivot), ("saturation".into(), w.sat)];
    for (n, a) in [("lift", w.lift), ("gamma", w.gamma), ("gain", w.gain), ("offset", w.offset)] { for (j, ch) in ["r", "g", "b", "m"].iter().enumerate() { v.push((format!("{n}_{ch}"), a[j])); } }
    v.into_iter().map(|(k, x)| (k, Prop::new(x))).collect() }
const PIX: [[f32; 4]; 5] = [[0.2, 0.5, 0.8, 1.0], [0.05, 0.9, 0.35, 0.5], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0], [0.6, 0.18, 0.02, 0.25]];
fn run(pr: &Params, px: &[[f32; 4]]) -> Frame { let mut f = frame(px); kimo_core::color::apply3("primaries", pr, &mut f, 0.0, 0).unwrap(); f }
fn check(got: &Frame, px: &[[f32; 4]], w: &W, what: &str) {
    for (i, p) in px.iter().enumerate() { let e = expect(w, [p[0] as f64, p[1] as f64, p[2] as f64]);
        for j in 0..3 { let g = got.d[i * 4 + j] as f64; assert!((g - e[j]).abs() <= TOL, "{what}: px{i} ch{j} got {g:.9} expected {:.9}", e[j]); }
        assert_eq!(got.d[i * 4 + 3].to_bits(), p[3].to_bits(), "{what}: alpha px{i} must be untouched"); } }

#[test]
fn per_channel_lift_gamma_gain_offset_exact() {
    let mut w = neutral(); w.lift = [0.1, -0.05, 0.02, 0.0]; w.gamma = [1.0, 1.25, 0.8, 1.0]; w.gain = [1.2, 0.9, 1.1, 1.0]; w.offset = [0.01, 0.0, -0.03, 0.0];
    check(&run(&wparams(&w, 0.0), &PIX), &PIX, &w, "per-channel");
    // hand value, written out: red of (0.2,0.5,0.8): (1.2-0.1)*0.2 + 0.1 + 0.01 = 0.33; green: ((0.9+0.05)*0.5 - 0.05)^(1/1.25) = 0.425^0.8
    let f = run(&wparams(&w, 0.0), &PIX);
    assert!((f.d[0] as f64 - 0.33).abs() <= TOL, "red {}", f.d[0]); assert!((f.d[1] as f64 - 0.425f64.powf(0.8)).abs() <= TOL, "green {}", f.d[1]);
}

#[test]
fn master_is_luma_preserving_chroma_and_full_chain_exact() {
    let mut w = neutral(); w.lift = [0.0, 0.0, 0.0, 0.05]; w.gamma = [1.0, 1.0, 1.0, 1.1]; w.gain = [1.0, 1.0, 1.0, 0.95];
    let f = run(&wparams(&w, 0.0), &PIX); check(&f, &PIX, &w, "master");
    for (i, p) in PIX.iter().enumerate() { let c = [p[0] as f64, p[1] as f64, p[2] as f64]; let o = [f.d[i * 4] as f64, f.d[i * 4 + 1] as f64, f.d[i * 4 + 2] as f64];
        let (y0, y1) = (y709(c), y709(o)); assert!(((o[0] - y1) - (c[0] - y0)).abs() <= TOL && ((o[2] - y1) - (c[2] - y0)).abs() <= TOL, "master moved chroma at px{i}");
        assert!((y1 - lgg(y0, 0.05, 1.1, 0.95, 0.0)).abs() <= TOL, "master luma px{i}"); }
    let mut w = neutral(); w.lift = [0.03, 0.0, -0.02, 0.05]; w.gamma = [0.9, 1.0, 1.2, 1.1]; w.gain = [1.1, 1.0, 0.95, 0.95]; w.offset = [0.0, 0.02, 0.0, -0.01]; w.contrast = 1.2; w.pivot = 0.4; w.sat = 0.7;
    check(&run(&wparams(&w, 0.0), &PIX), &PIX, &w, "full chain");
}

#[test]
fn saturation_preserves_luma() {
    let mut w = neutral(); w.sat = 1.6; let f = run(&wparams(&w, 0.0), &PIX); check(&f, &PIX, &w, "sat");
    for (i, p) in PIX.iter().enumerate() { let y0 = y709([p[0] as f64, p[1] as f64, p[2] as f64]); let y1 = y709([f.d[i * 4] as f64, f.d[i * 4 + 1] as f64, f.d[i * 4 + 2] as f64]); assert!((y1 - y0).abs() <= TOL, "sat changed luma at px{i}: {y0} -> {y1}"); }
}

#[test]
fn per_channel_matches_asc_cdl_effect() {
    let mut w = neutral(); w.lift = [0.1, -0.05, 0.02, 0.0]; w.gamma = [1.0, 1.25, 0.8, 1.0]; w.gain = [1.2, 0.9, 1.1, 1.0]; w.offset = [0.01, 0.0, -0.03, 0.0]; w.sat = 0.8;
    let px = [[0.2f32, 0.5, 0.8, 1.0], [0.3, 0.4, 0.5, 1.0], [0.6, 0.18, 0.4, 1.0]]; // all results stay inside 0..1, where CDL's clamp is a no-op
    let a = run(&wparams(&w, 0.0), &px);
    let mut kv = vec![("saturation", w.sat)]; let names = [("slope_r", "offset_r", "power_r"), ("slope_g", "offset_g", "power_g"), ("slope_b", "offset_b", "power_b")];
    for j in 0..3 { kv.push((names[j].0, w.gain[j] - w.lift[j])); kv.push((names[j].1, w.lift[j] + w.offset[j])); kv.push((names[j].2, 1.0 / w.gamma[j])); }
    let mut b = frame(&px); kimo_core::color::apply3("cdl", &params(&kv), &mut b, 0.0, 0).unwrap();
    for i in 0..a.d.len() { assert!((a.d[i] - b.d[i]).abs() as f64 <= TOL, "wheels vs cdl at {i}: {} vs {}", a.d[i], b.d[i]); }
}

#[test]
fn acescct_space_grades_in_log() {
    // ACEScct (S-2016-001) in f64, written out here
    let cct = |x: f64| if x <= 0.0078125 { 10.5402377416545 * x + 0.0729055341958355 } else { (x.log2() + 9.72) / 17.52 };
    let lin = |v: f64| if v <= 0.155251141552511 { (v - 0.0729055341958355) / 10.5402377416545 } else { 2f64.powf(v * 17.52 - 9.72) };
    let mut w = neutral(); w.offset = [0.05, 0.0, -0.02, 0.0]; w.gain = [1.0, 1.1, 1.0, 1.0];
    let px = [[0.18f32, 0.18, 0.18, 1.0], [0.5, 0.05, 0.9, 1.0]]; let f = run(&wparams(&w, 3.0), &px);
    for (i, p) in px.iter().enumerate() { for j in 0..3 { let e = lin(lgg(cct(p[j] as f64), w.lift[j], w.gamma[j], w.gain[j], w.offset[j])); let g = f.d[i * 4 + j] as f64;
        assert!((g - e).abs() <= TOL * e.max(1.0), "acescct px{i} ch{j}: got {g} expected {e}"); } }
}

#[test]
fn neutral_is_bit_identical_to_off() {
    let px = [[0.2f32, 0.5, 0.8, 1.0], [-0.25, 1.75, 0.0, 0.5], [1e-8, 3.0, -1.0, 0.0], [0.333333, 0.1, 0.9, 1.0]];
    for space in [0.0, 3.0, 1.0] { let f = run(&wparams(&neutral(), space), &px); let o = frame(&px);
        assert!(f.d.iter().zip(&o.d).all(|(a, b)| a.to_bits() == b.to_bits()), "neutral model 1 (space {space}) changed bits: {:?}", f.d); }
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, Value) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":16,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.9,0.2,0.1],"color2":[0.1,0.3,0.95],"angle":90,"start":0,"duration":2}))["clip_id"].clone();
    (s, id) }
fn render(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }

#[test]
fn tool_set_color_wheels_renders_exact_and_reports_cdl() {
    let (mut s, id) = scene(); let before = render(&s);
    let r = c(&mut s, "set_color_wheels", json!({"clip_id":id,"lift":[0.1,-0.05,0.02,0.0],"gamma":[1.0,1.25,0.8,1.0],"gain":[1.2,0.9,1.1,1.0],"offset":[0.01,0.0,-0.03,0.0],"saturation":0.9}));
    assert!(r["effect_id"].is_string(), "{r}");
    let cdl = &r["cdl"]; let sl: Vec<f64> = serde_json::from_value(cdl["slope"].clone()).unwrap(); let of: Vec<f64> = serde_json::from_value(cdl["offset"].clone()).unwrap(); let pw: Vec<f64> = serde_json::from_value(cdl["power"].clone()).unwrap();
    for (a, b) in sl.iter().zip([1.1, 0.95, 1.08]) { assert!((a - b).abs() < 1e-12, "slope {sl:?}"); } for (a, b) in of.iter().zip([0.11, -0.05, -0.01]) { assert!((a - b).abs() < 1e-12, "offset {of:?}"); } for (a, b) in pw.iter().zip([1.0, 0.8, 1.25]) { assert!((a - b).abs() < 1e-12, "power {pw:?}"); }
    let after = render(&s); let mut w = neutral(); w.lift = [0.1, -0.05, 0.02, 0.0]; w.gamma = [1.0, 1.25, 0.8, 1.0]; w.gain = [1.2, 0.9, 1.1, 1.0]; w.offset = [0.01, 0.0, -0.03, 0.0]; w.sat = 0.9;
    for i in 0..before.len() / 4 { let e = expect(&w, [before[i * 4] as f64, before[i * 4 + 1] as f64, before[i * 4 + 2] as f64]); for j in 0..3 { assert!((after[i * 4 + j] as f64 - e[j]).abs() <= TOL, "render px{i} ch{j}: {} vs {}", after[i * 4 + j], e[j]); } }
    // a master move has no per-channel CDL equivalent
    let r2 = c(&mut s, "set_color_wheels", json!({"clip_id":id,"effect_id":r["effect_id"],"lift":[0,0,0,0.05]})); assert!(r2["cdl"].is_null(), "{r2}"); assert_eq!(r2["effect_id"], r["effect_id"]);
}

#[test]
fn tool_neutral_render_is_byte_identical() {
    let (mut s, id) = scene(); let before = render(&s);
    c(&mut s, "set_color_wheels", json!({"clip_id":id,"lift":[0,0,0,0],"gamma":[1,1,1,1],"gain":[1,1,1,1],"offset":[0,0,0,0],"space":"acescct"}));
    let after = render(&s); assert!(before.iter().zip(&after).all(|(a, b)| a.to_bits() == b.to_bits()), "neutral wheels changed the render");
}

#[test]
fn tool_refusals_leave_project_untouched() {
    let (mut s, id) = scene(); let snap = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id":id,"lift":[0.1,0.0,0.0]}), "lift"), (json!({"clip_id":id,"gamma":[0.0,1,1,1]}), "gamma"), (json!({"clip_id":id,"gain":[-1,1,1,1]}), "gain"),
        (json!({"clip_id":id,"offset":[0,"x",0,0]}), "offset"), (json!({"clip_id":id,"saturation":9}), "saturation"), (json!({"clip_id":id,"space":"lab"}), "space"),
        (json!({"clip_id":id,"contrast":-1}), "contrast"), (json!({"clip_id":id,"effect_id":"nope"}), "effect"), (json!({"clip_id":"nope"}), "clip")] {
        let e = dispatch(&mut s, "set_color_wheels", &a).expect_err(&format!("{a} must be refused")); assert!(e.to_string().to_lowercase().contains(want), "{a}: error '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), snap, "{a} mutated the project"); }
}
