//! Debt item 9: an alpha on a solid clip's colour ("#rrggbbaa" or [r,g,b,a], the 4th value 0..1) was dropped and the
//! solid drew fully opaque, so "#00000080" (a 50% black dim layer) silently drew solid black. It now becomes the
//! clip's fill_alpha through the same helper add_shape uses (commands.rs color_alpha): separate from layer opacity and
//! composed with it. A 3-value / "#rrggbb" / alpha-1 colour adds no prop and draws exactly as before.
//! Blend (render.rs blend_region, normal mode, f32): out = bg * (1 - a) + src * a, a = fill_alpha * opacity.
//! No tool, argument, schema or description changes (golden_registry stays byte-identical).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// `bg` full-frame solid on the bottom track, `top` solid on the track above it; returns the session and the top clip id.
fn scene(bg: Value, top: Value) -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t1,"source_type":"solid","color":bg,"start":0,"duration":1}));
    let id = c(&mut s, "add_clip", json!({"track_id":t2,"source_type":"solid","color":top,"start":0,"duration":1}))["clip_id"].clone();
    (s, id)
}

fn pixels(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }

/// Every pixel exactly `want` (rgb; alpha is the opaque canvas, 1).
fn exact(s: &Session, want: [f32; 3], what: &str) {
    let px = pixels(s);
    let off = px.chunks_exact(4).filter(|p| p[0] != want[0] || p[1] != want[1] || p[2] != want[2] || p[3] != 1.0).count();
    assert_eq!(off, 0, "{what}: first px {:?}, want {want:?} exactly, {off}/{} px off (alpha ignored = drawn opaque)", &px[..4], px.len() / 4);
}

fn blend(bg: [f32; 3], src: [f32; 3], a: f32) -> [f32; 3] { [0, 1, 2].map(|j| bg[j] * (1.0 - a) + src[j] * a) }

#[test]
fn opaque_solid_still_covers_and_adds_no_prop() {
    for top in [json!([0, 0, 1]), json!([0, 0, 1, 1]), json!("#0000ff"), json!("#0000ffff")] {
        let (s, _) = scene(json!([1, 0, 0]), top.clone());
        exact(&s, [0.0, 0.0, 1.0], &format!("opaque blue {top} over red"));
        assert!(!s.project.tracks[1].clips[0].props.contains_key("fill_alpha"), "opaque {top} must not add fill_alpha (saved projects unchanged)");
    }
}

#[test]
fn half_alpha_black_over_a_known_background_exact() {
    let bg = [0.8f64 as f32, 0.6f64 as f32, 0.4f64 as f32];
    let a = (128.0f64 / 255.0) as f32;
    let (s, _) = scene(json!([0.8, 0.6, 0.4]), json!("#00000080"));
    exact(&s, blend(bg, [0.0; 3], a), "#00000080 over [0.8,0.6,0.4]: bg * (1 - 128/255)");
    let (s, _) = scene(json!([0.8, 0.6, 0.4]), json!([0, 0, 0, 0.5]));
    exact(&s, blend(bg, [0.0; 3], 0.5), "[0,0,0,0.5] over [0.8,0.6,0.4]: bg * 0.5 = [0.4,0.3,0.2]");
}

#[test]
fn solid_colour_alpha_is_drawn() {
    let (s, _) = scene(json!([1, 0, 0]), json!("#0000ff80"));
    exact(&s, blend([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], (128.0f64 / 255.0) as f32), "#0000ff80 over red");
    let (s, _) = scene(json!([1, 0, 0]), json!([0, 0, 1, 0.25]));
    exact(&s, [0.75, 0.0, 0.25], "[0,0,1,0.25] over red");
    let (s, _) = scene(json!([1, 0, 0]), json!([0, 0, 1, 0]));
    exact(&s, [1.0, 0.0, 0.0], "[0,0,1,0] over red (fully transparent)");
}

#[test]
fn solid_alpha_composes_with_layer_opacity() {
    let (mut s, id) = scene(json!([1, 0, 0]), json!([0, 0, 1, 0.5]));
    c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0.5}));
    exact(&s, [0.75, 0.0, 0.25], "alpha 0.5 x opacity 0.5 = a 0.25 over red");
}

#[test]
fn fill_alpha_property_works_on_a_solid() {
    let (mut s, id) = scene(json!([1, 0, 0]), json!([0, 0, 1]));
    c(&mut s, "set_property", json!({"clip_id":id,"property":"fill_alpha","value":0.5}));
    exact(&s, [0.5, 0.0, 0.5], "set_property fill_alpha 0.5 on a solid");
}

/// The solid path and add_shape's fill path read the 4th value the same way (same helper, 0..1), so they cannot disagree.
#[test]
fn solid_alpha_matches_shape_fill_alpha() {
    for col in [json!([0, 0, 1, 0.5]), json!("#0000ff80"), json!([0, 0, 1, 0.2]), json!("#0000ff33")] {
        let (s, _) = scene(json!([1, 0, 0]), col.clone());
        let solid_alpha = s.project.tracks[1].clips[0].props["fill_alpha"].clone();
        let solid_px = pixels(&s);
        let mut sh = Session::new();
        c(&mut sh, "project_set", json!({"width":32,"height":18,"fps":25}));
        let t1 = c(&mut sh, "add_track", json!({}))["track_id"].clone();
        let t2 = c(&mut sh, "add_track", json!({}))["track_id"].clone();
        c(&mut sh, "add_clip", json!({"track_id":t1,"source_type":"solid","color":[1,0,0],"start":0,"duration":1}));
        c(&mut sh, "add_shape", json!({"track_id":t2,"shape":"rect","width":300,"height":200,"fill":col,"start":0,"duration":1}));
        let shape_alpha = sh.project.tracks[1].clips[0].props["fill_alpha"].clone();
        assert_eq!(serde_json::to_value(&solid_alpha).unwrap(), serde_json::to_value(&shape_alpha).unwrap(), "{col}: solid fill_alpha == shape fill_alpha");
        let shape_px = pixels(&sh);
        let i = (9 * 32 + 16) * 4;
        assert_eq!(&solid_px[i..i + 4], &shape_px[i..i + 4], "{col}: solid centre px == shape centre px");
    }
}
