//! HSL qualifier matte (Resolve qualifier): mask kind "hsl" with optional per-range softness hue_soft (deg), sat_soft, lum_soft.
//! coverage = (1 - ss(w/2, w/2+HS, dh)) * ss(s_lo-SS, s_lo, s) * (1 - ss(s_hi, s_hi+SS, s)) * ss(l_lo-LS, l_lo, l) * (1 - ss(l_hi, l_hi+LS, l)),
//! ss = smoothstep t^2(3-2t); HS/SS/LS = the new props when set, else soft*90 / soft as before (soft+1e-4). Values below are worked by hand.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::keyframe::Prop;
use kimo_core::masks::{layer_coverage, new_layer, Ctx};
use serde_json::{json, Value};
const TOL: f64 = 1e-5;
fn cov(props: &[(&str, f64)], invert: bool, rgb: [f32; 3]) -> f64 {
    let mut l = new_layer("q".into(), "hsl"); for (k, v) in props { l.props.insert(k.to_string(), Prop::new(*v)); } l.invert = invert;
    let f = Frame { w: 1, h: 1, d: vec![rgb[0], rgb[1], rgb[2], 1.0] }; let refs = |_: &str| -> Option<Frame> { None }; let ctx = Ctx { refs: &refs, fps: 25.0 };
    layer_coverage(&l, &f, 0.0, &ctx)[0] as f64 }
fn ss(a: f64, b: f64, x: f64) -> f64 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
// HSL(40 deg, 1, 0.5) = (1, 2/3, 0)
const H40: [f32; 3] = [1.0, 0.6666667, 0.0];

#[test]
fn hue_softness_exact() {
    // hue 0 width 60 hue_soft 30: dh = 40 -> t = (40-30)/30 = 1/3 -> 1 - (1/9)(3 - 2/3) = 1 - 7/27 = 20/27
    let v = cov(&[("hue", 0.0), ("hue_width", 60.0), ("hue_soft", 30.0), ("s_lo", 0.2)], false, H40); assert!((v - 20.0 / 27.0).abs() < TOL, "hue ramp {v} vs {}", 20.0 / 27.0);
    let v = cov(&[("hue", 0.0), ("hue_width", 60.0), ("hue_soft", 30.0)], true, H40); assert!((v - 7.0 / 27.0).abs() < TOL, "inverted {v}");
    // inside the width: exactly 1; past width/2 + soft: exactly 0
    assert_eq!(cov(&[("hue", 30.0), ("hue_width", 60.0), ("hue_soft", 30.0)], false, H40), 1.0);
    assert_eq!(cov(&[("hue", 200.0), ("hue_width", 60.0), ("hue_soft", 30.0)], false, H40), 0.0);
}

#[test]
fn sat_and_lum_softness_exact() {
    // HSL(0, 0.4, 0.5) = (0.7, 0.3, 0.3): s_lo 0.5 with sat_soft 0.2 -> ss(0.3, 0.5, 0.4) = 0.5
    let v = cov(&[("hue", 0.0), ("hue_width", 40.0), ("s_lo", 0.5), ("sat_soft", 0.2)], false, [0.7, 0.3, 0.3]); assert!((v - 0.5).abs() < TOL, "sat ramp {v}");
    // HSL(0, 1, 0.7) = (1, 0.4, 0.4): l_hi 0.6 with lum_soft 0.2 -> 1 - ss(0.6, 0.8, 0.7) = 0.5
    let v = cov(&[("hue", 0.0), ("hue_width", 40.0), ("l_hi", 0.6), ("lum_soft", 0.2)], false, [1.0, 0.4, 0.4]); assert!((v - 0.5).abs() < TOL, "lum ramp {v}");
    // l = 0.65 -> t = 0.25 -> 1 - 0.0625*2.5 = 0.84375 ; HSL(0,1,0.65) = (1, 0.3, 0.3)
    let v = cov(&[("hue", 0.0), ("hue_width", 40.0), ("l_hi", 0.6), ("lum_soft", 0.2)], false, [1.0, 0.3, 0.3]); assert!((v - 0.84375).abs() < TOL, "lum ramp 0.65 {v}");
}

#[test]
fn unset_softness_keeps_the_old_formula() {
    // no new props: the ramps are soft*90 deg and soft (soft = 0.1 default, +1e-4)
    let so = 0.1 + 1e-4; let e = 1.0 - ss(15.0, 15.0 + so * 90.0, 20.0);
    // HSL(20,1,0.5) = (1, 1/3, 0)
    let v = cov(&[("hue", 0.0), ("hue_width", 30.0)], false, [1.0, 0.33333334, 0.0]); assert!((v - e).abs() < 1e-4, "old formula {v} vs {e}");
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene(col: [f32; 3]) -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":16,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":col,"start":0,"duration":2}))["clip_id"].clone(); (s, id) }

#[test]
fn qualifier_matte_through_the_mask_stack() {
    let (mut s, id) = scene(H40);
    c(&mut s, "add_mask", json!({"clip_id":id,"kind":"hsl","params":{"hue":0.0,"hue_width":60.0,"hue_soft":30.0,"s_lo":0.2}}));
    let f = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap();
    let m = &s.project.tracks[0].clips[0].mask.as_ref().unwrap().layers[0]; assert_eq!(m.props["hue_soft"].base, 30.0);
    // solid over black: red channel = 1.0 * matte
    let r = f.d[(8 * 32 + 16) * 4] as f64; assert!((r - 20.0 / 27.0).abs() < 2e-3, "rendered matte {r} vs {}", 20.0 / 27.0);
}

#[test]
fn refusals_leave_project_untouched() {
    let (mut s, id) = scene(H40); let snap = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id":id,"kind":"hsl","params":{"hue_soft":200.0}}), "hue_soft"), (json!({"clip_id":id,"kind":"hsl","params":{"sat_soft":1.5}}), "sat_soft"),
        (json!({"clip_id":id,"kind":"rect","params":{"lum_soft":0.2}}), "lum_soft"), (json!({"clip_id":id,"kind":"hsl","params":{"lum_soft":-0.5}}), "lum_soft")] {
        let e = dispatch(&mut s, "add_mask", &a).expect_err(&format!("{a} must be refused")); assert!(e.to_string().contains(want), "{a}: '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), snap, "{a} mutated the project"); }
}
