//! Gradient transparency (AE "Gradient Ramp" / Premiere "Ramp" with transparent stops): an alpha on a gradient clip's
//! colours ("#rrggbbaa" or [r,g,b,a], 4th value 0..1, the same reading as solids and add_shape fill) was dropped and the
//! gradient drew fully opaque, so a "#00000000" -> "#000000cc" caption scrim covered the shot in black. Each end's
//! alpha is now drawn: alpha = lerp(a_top, a_bottom, k), colour interpolated premultiplied (a transparent end adds no
//! dark fringe), times the clip's fill_alpha (set_property already accepted fill_alpha on any clip, but nothing read it
//! on a gradient); layer opacity composes on top in blend_region (a = alpha * opacity). An opaque gradient adds no
//! prop and draws exactly as before. No tool, argument, schema or description changes (golden_registry unchanged).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const W: usize = 32;
const H: usize = 18;

/// `bg` full-frame solid on the bottom track, a gradient `top` -> `bottom` on the track above; returns session + gradient id.
fn scene(bg: Value, top: Value, bottom: Value) -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t1,"source_type":"solid","color":bg,"start":0,"duration":1}));
    let id = c(&mut s, "add_clip", json!({"track_id":t2,"source_type":"gradient","color":top,"color2":bottom,"start":0,"duration":1}))["clip_id"].clone();
    (s, id)
}

fn pixels(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }

/// Straight colour + alpha of gradient row y, as the source draws it (f64 math, then f32), for ends (c1,a1) -> (c2,a2), times fill.
fn src_row(c1: [f64; 3], a1: f64, c2: [f64; 3], a2: f64, fill: f64, y: usize) -> ([f32; 3], f32) {
    let k = y as f64 / H as f64;
    if a1 == 1.0 && a2 == 1.0 { return ([0, 1, 2].map(|j| (c1[j] * (1.0 - k) + c2[j] * k) as f32), fill as f32); }
    let al = if a1 == a2 { a1 } else { a1 * (1.0 - k) + a2 * k };
    ([0, 1, 2].map(|j| if al > 0.0 { ((c1[j] * a1 * (1.0 - k) + c2[j] * a2 * k) / al) as f32 } else { (c1[j] * (1.0 - k) + c2[j] * k) as f32 }), (al * fill) as f32)
}

/// blend_region normal mode, f32: out = bg * (1 - a) + src * a, a = src_alpha * opacity.
fn over(bg: [f32; 3], src: [f32; 3], a: f32) -> [f32; 3] { [0, 1, 2].map(|j| bg[j] * (1.0 - a) + src[j] * a) }

/// Every pixel of every row exactly the expected blend (alpha is the opaque canvas, 1).
fn exact_rows(s: &Session, want: impl Fn(usize) -> [f32; 3], what: &str) {
    let px = pixels(s);
    let mut off = 0; let mut first = None;
    for y in 0..H { let w = want(y); for x in 0..W { let p = &px[(y * W + x) * 4..(y * W + x) * 4 + 4];
        if p[0] != w[0] || p[1] != w[1] || p[2] != w[2] || p[3] != 1.0 { off += 1; if first.is_none() { first = Some((y, [p[0], p[1], p[2], p[3]], w)); } } } }
    assert_eq!(off, 0, "{what}: first off (row, got, want) {first:?} exactly; {off}/{} px off (alpha ignored = drawn opaque)", W * H);
}

#[test]
fn opaque_gradient_draws_as_before_and_adds_no_prop() {
    for (top, bottom) in [(json!([1, 0, 0]), json!([0, 0, 1])), (json!("#ff0000"), json!("#0000ffff")), (json!([1, 0, 0, 1]), json!([0, 0, 1]))] {
        let (s, _) = scene(json!([0, 1, 0]), top.clone(), bottom.clone());
        exact_rows(&s, |y| src_row([1.0, 0.0, 0.0], 1.0, [0.0, 0.0, 1.0], 1.0, 1.0, y).0, &format!("opaque {top}->{bottom}"));
        let p = &s.project.tracks[1].clips[0].props;
        assert!(!p.contains_key("color_alpha") && !p.contains_key("color2_alpha") && !p.contains_key("fill_alpha"), "opaque {top}->{bottom} must add no alpha prop (saved projects unchanged)");
    }
}

#[test]
fn fade_to_clear_scrim_over_a_known_background_exact() {
    let bg = [0.8f64 as f32, 0.6f64 as f32, 0.4f64 as f32];
    let (s, _) = scene(json!([0.8, 0.6, 0.4]), json!("#00000000"), json!("#000000cc"));
    exact_rows(&s, |y| { let (c, a) = src_row([0.0; 3], 0.0, [0.0; 3], 204.0 / 255.0, 1.0, y); over(bg, c, a) }, "#00000000 -> #000000cc scrim over [0.8,0.6,0.4]: bg * (1 - 0.8 k)");
    let (s, _) = scene(json!([0.8, 0.6, 0.4]), json!([0, 0, 0, 0]), json!([0, 0, 0, 0.5]));
    exact_rows(&s, |y| { let (c, a) = src_row([0.0; 3], 0.0, [0.0; 3], 0.5, 1.0, y); over(bg, c, a) }, "[0,0,0,0] -> [0,0,0,0.5] over [0.8,0.6,0.4]");
    // the top row is fully clear (bg untouched), the bottom row is the scrim at its strongest
    let px = pixels(&s); assert_eq!(&px[..4], &[bg[0], bg[1], bg[2], 1.0], "row 0 of a clear->0.5 scrim must leave the bg exactly");
}

#[test]
fn transparent_end_adds_no_dark_fringe() {
    // red fading to a fully transparent blue end over green: premultiplied interpolation keeps the colour pure red
    let (s, _) = scene(json!([0, 1, 0]), json!([1, 0, 0, 1]), json!("#0000ff00"));
    exact_rows(&s, |y| { let (c, a) = src_row([1.0, 0.0, 0.0], 1.0, [0.0, 0.0, 1.0], 0.0, 1.0, y); assert_eq!(c, [1.0, 0.0, 0.0], "row {y} straight colour"); over([0.0, 1.0, 0.0], c, a) }, "red -> clear blue over green");
}

#[test]
fn fill_alpha_property_works_on_a_gradient() {
    let (mut s, id) = scene(json!([0, 1, 0]), json!([1, 0, 0]), json!([0, 0, 1]));
    c(&mut s, "set_property", json!({"clip_id":id,"property":"fill_alpha","value":0.5}));
    exact_rows(&s, |y| { let (c, a) = src_row([1.0, 0.0, 0.0], 1.0, [0.0, 0.0, 1.0], 1.0, 0.5, y); over([0.0, 1.0, 0.0], c, a) }, "set_property fill_alpha 0.5 on an opaque red->blue gradient over green");
}

#[test]
fn gradient_alpha_composes_with_layer_opacity() {
    let (mut s, id) = scene(json!([1, 1, 1]), json!("#00000000"), json!([0, 0, 0, 1]));
    c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0.5}));
    exact_rows(&s, |y| { let (c, a) = src_row([0.0; 3], 0.0, [0.0; 3], 1.0, 1.0, y); over([1.0; 3], c, a * 0.5f32) }, "clear->black at layer opacity 0.5 over white: a = k * 0.5");
}

#[test]
fn white_to_transparent_over_a_bright_frame_never_darkens() {
    // Naman 06:20Z: fade-to-transparent must interpolate premultiplied, so no pixel ends darker than the background.
    // "#00000000" is CSS "transparent" (transparent BLACK): a straight-alpha lerp toward it would grey the white (a dark fringe).
    let bg = [0.9f64 as f32, 0.8f64 as f32, 0.7f64 as f32];
    for (bottom, c2) in [(json!("#ffffff00"), [1.0; 3]), (json!("#00000000"), [0.0; 3]), (json!([0, 0, 0, 0]), [0.0; 3])] {
        let (s, _) = scene(json!([0.9, 0.8, 0.7]), json!("#ffffff"), bottom.clone());
        exact_rows(&s, |y| { let (c, a) = src_row([1.0; 3], 1.0, c2, 0.0, 1.0, y); assert_eq!(c, [1.0; 3], "row {y}: premultiplied colour stays white toward {bottom}"); over(bg, c, a) }, &format!("white -> {bottom} over [0.9,0.8,0.7]"));
        let px = pixels(&s);
        let darker = px.chunks_exact(4).filter(|p| p[0] < bg[0] || p[1] < bg[1] || p[2] < bg[2]).count();
        assert_eq!(darker, 0, "white -> {bottom}: {darker} px darker than the bright background (dark fringe)");
    }
}
