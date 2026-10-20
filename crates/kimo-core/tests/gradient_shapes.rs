//! Linear gradients at any angle and radial gradients (After Effects Gradient Ramp linear/radial, Premiere Ramp, Resolve gradient
//! generator): add_clip source_type gradient takes `angle` (degrees, color -> color2: 0 top->bottom, 90 left->right; keyframable as the
//! clip property gradient_angle) and `gradient` = linear | radial (color at the centre, color2 at the corners). The default stays the
//! vertical gradient kimo always drew, bit for bit. End alphas and fill_alpha work the same in every shape.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const W: usize = 32;
const H: usize = 18;

fn grad(extra: Value) -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let mut a = json!({"track_id":t,"source_type":"gradient","color":[1,0,0],"color2":[0,0,1],"start":0,"duration":2});
    for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    let id = c(&mut s, "add_clip", a)["clip_id"].clone();
    (s, id)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap().d }

/// red -> blue at position k, as the gradient draws it (opaque ends)
fn rgb(k: f64) -> [f32; 3] { [(1.0 * (1.0 - k) + 0.0 * k) as f32, 0.0, (0.0 * (1.0 - k) + 1.0 * k) as f32] }
fn k_linear(deg: f64, x: usize, y: usize) -> f64 { let r = deg.to_radians(); let (sx, cy) = (r.sin(), r.cos()); let (wf, hf) = (W as f64, H as f64);
    (((x as f64 + 0.5 - wf / 2.0) * sx + (y as f64 + 0.5 - hf / 2.0) * cy) / ((wf * sx).abs() + (hf * cy).abs()) + 0.5).clamp(0.0, 1.0) }
fn k_radial(x: usize, y: usize) -> f64 { let (wf, hf) = (W as f64, H as f64); let (dx, dy) = (x as f64 + 0.5 - wf / 2.0, y as f64 + 0.5 - hf / 2.0); ((dx * dx + dy * dy).sqrt() / ((wf * wf + hf * hf).sqrt() / 2.0)).clamp(0.0, 1.0) }

fn exact(d: &[f32], want: impl Fn(usize, usize) -> [f32; 3], what: &str) {
    let mut off = 0; let mut first = None;
    for y in 0..H { for x in 0..W { let p = &d[(y * W + x) * 4..(y * W + x) * 4 + 4]; let w = want(x, y);
        if p[0] != w[0] || p[1] != w[1] || p[2] != w[2] || p[3] != 1.0 { off += 1; if first.is_none() { first = Some((x, y, [p[0], p[1], p[2]], w)); } } } }
    assert_eq!(off, 0, "{what}: first off (x, y, got, want) {first:?}; {off}/{} px off", W * H);
}

#[test]
fn default_gradient_is_unchanged_vertical() {
    let (s, id) = grad(json!({}));
    exact(&frame(&s, 0.5), |_, y| rgb(y as f64 / H as f64), "default red->blue");
    let p = &s.project.find_clip(id.as_str().unwrap()).unwrap().props;
    assert!(!p.contains_key("gradient_angle") && !p.contains_key("gradient_radial"), "no angle/shape prop unless asked");
}

#[test]
fn linear_gradient_at_an_angle() {
    for deg in [90.0, 45.0, 180.0, -30.0] {
        let (s, _) = grad(json!({"angle":deg}));
        let d = frame(&s, 0.5);
        exact(&d, |x, y| rgb(k_linear(deg, x, y)), &format!("angle {deg}"));
    }
    // 90 = left -> right: the left column is (nearly) the first colour, the right column (nearly) the second
    let (s, _) = grad(json!({"angle":90}));
    let d = frame(&s, 0.5);
    assert!(d[0] > 0.95 && d[2] < 0.05, "left edge red at 90 deg, got {:?}", &d[..3]);
    let r = (W - 1) * 4; assert!(d[r] < 0.05 && d[r + 2] > 0.95, "right edge blue at 90 deg, got {:?}", &d[r..r + 3]);
}

#[test]
fn radial_gradient_centre_to_corners() {
    let (s, _) = grad(json!({"gradient":"radial"}));
    let d = frame(&s, 0.5);
    exact(&d, |x, y| rgb(k_radial(x, y)), "radial red centre -> blue corners");
    let ctr = (H / 2 * W + W / 2) * 4; assert!(d[ctr] > 0.9, "centre is red, got {:?}", &d[ctr..ctr + 3]);
    assert!(d[2] > 0.9, "corner is blue, got {:?}", &d[..3]);
}

#[test]
fn angle_is_keyframable_and_shapes_keep_alpha() {
    let (mut s, id) = grad(json!({"angle":0}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"gradient_angle","time":0,"value":0}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"gradient_angle","time":1,"value":180}));
    exact(&frame(&s, 1.0), |x, y| rgb(k_linear(180.0, x, y)), "keyframed angle at 1 s = 180 (bottom->top)");
    // radial scrim: clear centre -> black corners (a vignette) over white
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t1,"source_type":"solid","color":[1,1,1],"start":0,"duration":1}));
    c(&mut s, "add_clip", json!({"track_id":t2,"source_type":"gradient","gradient":"radial","color":"#00000000","color2":"#000000ff","start":0,"duration":1}));
    exact(&frame(&s, 0.5), |x, y| { let a = k_radial(x, y) as f32; [1.0 * (1.0 - a) + 0.0 * a; 3] }, "radial vignette over white: 1 - k");
}

#[test]
fn bad_shape_and_misplaced_args_are_refused() {
    let mut s = Session::new();
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let e = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"gradient","gradient":"conic","start":0,"duration":1})).unwrap_err(); assert!(e.contains("bad gradient 'conic'"), "{e}");
    let e = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"solid","angle":45,"start":0,"duration":1})).unwrap_err(); assert!(e.contains("gradient only"), "{e}");
}
