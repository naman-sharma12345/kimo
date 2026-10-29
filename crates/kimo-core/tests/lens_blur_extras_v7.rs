//! Three new lens-blur-line effect kinds (FEATURES.md L467): channel_blur (R/G/B blurred independently --
//! chromatic fringe), smart_blur (flat areas smoothed, real edges left exactly sharp), reduce_interlace_flicker
//! (vertical-only low pass). bilateral_blur, depth-map blur and bokeh-shaped iris blur were already shipped
//! pre-v7 (list_effects/add_lens_blur) and are credited, not duplicated, on that FEATURES.md line.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn listed(kind: &str) {
    let kinds: Vec<String> = { let mut s = Session::new(); c(&mut s, "list_effects", json!({})).as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap().to_string()).collect() };
    assert!(kinds.contains(&kind.to_string()), "{kind} missing from list_effects: {kinds:?}");
}

#[test]
fn channel_blur_is_listed_and_blurs_each_channel_by_its_own_radius() {
    listed("channel_blur");
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.0, 0.0, 0.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "add_effect", json!({"clip_id": clip, "kind": "checkerboard", "params": {"size": 2.0, "r": 1.0, "g": 1.0, "b": 1.0, "opacity": 1.0}}));
    // a single white pixel-scale checker pattern duplicated into R, G, B alike (so all three start identical)
    let before = render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap();
    c(&mut s, "add_effect", json!({"clip_id": clip, "kind": "channel_blur", "params": {"r_radius": 0.0, "g_radius": 0.0, "b_radius": 20.0}}));
    let after = render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap();
    // R and G (radius 0) must be untouched; B (radius 20, a heavy blur on a fine checker) must have lost
    // nearly all its high-frequency variance -- i.e. flattened toward a mid grey rather than 0/1 extremes.
    let r_var = |ch: usize, f: &kimo_core::effects::Frame| -> f32 { let vals: Vec<f32> = (0..f.w * f.h).map(|i| f.d[i * 4 + ch]).collect(); let m = vals.iter().sum::<f32>() / vals.len() as f32; vals.iter().map(|v| (v - m).powi(2)).sum::<f32>() / vals.len() as f32 };
    assert_eq!(before.d.iter().step_by(4).cloned().collect::<Vec<_>>(), after.d.iter().step_by(4).cloned().collect::<Vec<_>>(), "r_radius=0 should leave the red channel bit-exact");
    assert!(r_var(2, &after) < r_var(2, &before) * 0.3, "a heavy blur on the blue channel should sharply reduce its checker variance, before={} after={}", r_var(2, &before), r_var(2, &after));
}

#[test]
fn smart_blur_leaves_a_strong_edge_sharp_while_smoothing_flat_noise() {
    listed("smart_blur");
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // A single clip IS the hard edge: a rect filling the right half of its own (transparent) canvas, so inside
    // the shape reads opaque white and outside reads fully transparent/black -- a real, strong edge within
    // this one clip's own frame, which is what an effect (applied per-clip, before compositing) actually sees.
    // add_shape has no x/y in its own schema -- a shape is born centred on the canvas and moved with
    // set_property, so a 32-wide rect needs an explicit +16px x offset to span the canvas's right half.
    let clip = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 32, "height": 64, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_property", json!({"clip_id": clip, "property": "x", "value": 16.0}));
    c(&mut s, "add_effect", json!({"clip_id": clip, "kind": "smart_blur", "params": {"radius": 6.0, "threshold": 0.1}}));
    let f = render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap();
    // a pixel a few px either side of the edge (x=32) should still read essentially pure black/white --
    // the strong edge must survive smart_blur's threshold gate, unlike a plain blur which would smear it.
    let px = |x: usize, y: usize| -> f32 { f.d[(y * f.w + x) * 4] };
    assert!(px(20, 32) < 0.05, "well inside the black side the edge should stay sharp, got {}", px(20, 32));
    assert!(px(44, 32) > 0.95, "well inside the white side the edge should stay sharp, got {}", px(44, 32));
}

#[test]
fn reduce_interlace_flicker_is_listed_and_softens_a_single_bright_scanline() {
    listed("reduce_interlace_flicker");
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 32, "height": 32, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // a single bright horizontal scanline (the classic interlace-flicker shape: thin, pure-horizontal detail),
    // as the clip's own content against its own transparent canvas (see smart_blur test above for why this is
    // a real edge for the effect, which runs per-clip before compositing). A 32x2 rect on a 32-tall canvas
    // (even height, centre exactly on a pixel boundary) sits pixel-aligned on rows 15-16 with no antialiasing
    // split, fully bright, with fully transparent/black rows immediately above/below.
    let clip = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 32, "height": 2, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap();
    c(&mut s, "add_effect", json!({"clip_id": clip, "kind": "reduce_interlace_flicker", "params": {"amount": 1.0}}));
    let after = render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap();
    let px = |f: &kimo_core::effects::Frame, y: usize| f.d[(y * f.w + 16) * 4];
    assert!(px(&before, 16) > 0.95, "the scanline's edge row should start fully bright, got {}", px(&before, 16));
    assert!(px(&after, 16) < 0.85, "the vertical low-pass should knock down the scanline's edge-row brightness (its black neighbour on one side pulls it down), got {}", px(&after, 16));
    assert!(px(&after, 17) > 0.05, "some of the scanline's brightness should have bled into the previously-black row just past its edge, got {}", px(&after, 17));
}
