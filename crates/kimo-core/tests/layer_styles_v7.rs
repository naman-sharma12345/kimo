//! ls_gradient_overlay (layerstyles.rs): the one missing item on FEATURES.md's layer-styles line --
//! inner_shadow/bevel/stroke already existed; drop_shadow/outline/outer_glow already exist as effects.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn project(w: u32, h: u32) -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": w, "height": h, "fps": 24})); s }
fn rendered(s: &Session, t: f64) -> kimo_core::effects::Frame { let mut cache = Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &kimo_core::effects::Frame, x: u32, y: u32) -> [f32; 4] { let o = ((y as usize) * f.w + x as usize) * 4; [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]] }

/// A wide flat-white rect filling almost the whole 320x180 canvas, so the gradient has real left/right span to read.
fn wide_shape(s: &mut Session) -> String {
    let t = c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 300, "height": 160, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string()
}

#[test] fn list_layer_styles_includes_gradient_overlay() {
    let mut s = Session::new();
    let v = c(&mut s, "list_layer_styles", json!({}));
    let names: Vec<&str> = v["styles"].as_array().unwrap().iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert!(names.contains(&"ls_gradient_overlay"), "got {names:?}");
}

#[test] fn gradient_overlay_blends_color1_to_color2_left_to_right() {
    let mut s = project(320, 180);
    let clip = wide_shape(&mut s);
    // angle 90 = left -> right (same convention as Gradient Fill/Gradient Stroke)
    c(&mut s, "set_layer_style", json!({"clip_id": clip, "style": "gradient_overlay", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 0.0, "g2": 0.0, "b2": 1.0, "angle": 90.0, "opacity": 1.0}}));
    let f = rendered(&s, 0.5);
    let left = px(&f, 15, 90); let right = px(&f, 305, 90);
    assert!(left[0] > 0.7 && left[2] < 0.3, "left edge should read mostly red, got {left:?}");
    assert!(right[2] > 0.7 && right[0] < 0.3, "right edge should read mostly blue, got {right:?}");
}

#[test] fn gradient_overlay_opacity_blends_with_the_original_fill() {
    let mut s = project(320, 180);
    let clip = wide_shape(&mut s); // white fill
    c(&mut s, "set_layer_style", json!({"clip_id": clip, "style": "gradient_overlay", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 1.0, "g2": 0.0, "b2": 0.0, "angle": 90.0, "opacity": 0.5}}));
    let f = rendered(&s, 0.5);
    let p = px(&f, 160, 90);
    // halfway between white (1,1,1) and pure red (1,0,0) at opacity 0.5: ~(1, 0.5, 0.5)
    assert!((p[1] - 0.5).abs() < 0.05 && (p[2] - 0.5).abs() < 0.05, "expected a 50/50 blend toward red, got {p:?}");
}

#[test] fn gradient_overlay_multiply_darkens_instead_of_replacing() {
    let mut s = project(320, 180);
    let clip = wide_shape(&mut s); // white fill: multiply by pure red should read as pure red; normal would too, so prove multiply differs on a NON-white base
    c(&mut s, "set_property", json!({"clip_id": clip, "property": "opacity", "value": 1.0}));
    // swap the shape's own fill to a mid-grey so multiply (darkens toward black) is distinguishable from normal (replaces outright)
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let grey = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 300, "height": 160, "fill": [0.6, 0.6, 0.6, 1.0], "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_layer_style", json!({"clip_id": grey, "style": "gradient_overlay", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 1.0, "g2": 0.0, "b2": 0.0, "angle": 90.0, "opacity": 1.0}, "strings": {"blend": "multiply"}}));
    let f = rendered(&s, 0.5);
    let p = px(&f, 160, 90);
    // multiply red(1,0,0) onto grey(0.6,0.6,0.6) -> (0.6, 0, 0), distinctly darker-red than a normal overlay's pure (1,0,0)
    assert!(p[0] < 0.75 && p[0] > 0.4, "expected multiply-darkened red, got {p:?}");
    assert!(p[1] < 0.05 && p[2] < 0.05, "green/blue should be fully knocked out by the red multiply, got {p:?}");
}
