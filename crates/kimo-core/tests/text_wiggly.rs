//! After Effects Wiggly Selector on text animators: add_text_animator with wiggly true multiplies the range selector by a
//! smooth random value per unit between wiggle_min and wiggle_max (default -1..1), changing wiggle_freq times a second;
//! wiggle_correlation 0 lets every unit wander on its own, 1 moves them all together. Same seed, same motion.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const W: usize = 240;
const H: usize = 120;
fn scene() -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 25}));
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone(); let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t0, "source_type": "solid", "color": [0, 0, 0], "start": 0, "duration": 4}));
    let id = c(&mut s, "add_text", json!({"track_id": t1, "text": "HHHHH", "size": 28, "color": [1, 1, 1], "start": 0, "duration": 4}))["clip_id"].clone();
    (s, id)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap().d }
fn ink(d: &[f32], x: usize, y: usize) -> f32 { d[(y * W + x) * 4] }
/// column spans of the five glyphs, read off the still text
fn glyph_spans(d: &[f32]) -> Vec<(usize, usize)> {
    let inked: Vec<bool> = (0..W).map(|x| (0..H).any(|y| ink(d, x, y) > 0.3)).collect();
    let mut v = vec![]; let mut x = 0; while x < W { if inked[x] { let a = x; while x < W && inked[x] { x += 1; } v.push((a, x)); } else { x += 1; } } v
}
/// ink-weighted centre row of each glyph span
fn centres(d: &[f32], spans: &[(usize, usize)]) -> Vec<f32> { spans.iter().map(|&(a, b)| { let (mut m, mut sy) = (0f32, 0f32); for y in 0..H { for x in a..b { let v = ink(d, x, y); m += v; sy += v * y as f32; } } sy / m.max(1e-6) }).collect() }
fn base() -> (Vec<(usize, usize)>, Vec<f32>) { let (s, _) = scene(); let d = frame(&s, 0.0); let sp = glyph_spans(&d); assert_eq!(sp.len(), 5, "five glyphs: {sp:?}"); let c0 = centres(&d, &sp); (sp, c0) }
fn offsets(s: &Session, t: f64, sp: &[(usize, usize)], c0: &[f32]) -> Vec<f32> { let d = frame(s, t); centres(&d, sp).iter().zip(c0).map(|(a, b)| a - b).collect() }
fn spread(v: &[f32]) -> f32 { v.iter().cloned().fold(f32::MIN, f32::max) - v.iter().cloned().fold(f32::MAX, f32::min) }

#[test]
fn wiggly_moves_each_character_by_its_own_amount() {
    let (sp, c0) = base(); let (mut s, id) = scene();
    c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "smoothness": 0, "wiggly": true, "wiggle_correlation": 0, "wiggle_freq": 2, "wiggle_seed": 3}));
    let o = offsets(&s, 0.7, &sp, &c0);
    assert!(spread(&o) > 6.0, "characters must sit at different heights under a wiggly selector (y 24 px): offsets {o:?}");
    for v in &o { assert!(v.abs() <= 24.5, "each offset stays within wiggle_min..max x 24 px: {o:?}"); }
    let o2 = offsets(&s, 1.6, &sp, &c0);
    let moved = o.iter().zip(&o2).filter(|(a, b)| (*a - *b).abs() > 1.5).count();
    assert!(moved >= 3, "the wiggle changes over time (t 0.7 -> 1.6): {o:?} -> {o2:?}");
    // smooth in time: one frame later nobody jumps more than a few px
    let o3 = offsets(&s, 0.72, &sp, &c0);
    for (a, b) in o.iter().zip(&o3) { assert!((a - b).abs() < 4.0, "smooth wiggle, not jitter per frame: {o:?} -> {o3:?}"); }
}

#[test]
fn correlation_one_moves_all_characters_together() {
    let (sp, c0) = base(); let (mut s, id) = scene();
    c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "smoothness": 0, "wiggly": true, "wiggle_correlation": 1, "wiggle_freq": 2, "wiggle_seed": 3}));
    let (a, b) = (offsets(&s, 0.7, &sp, &c0), offsets(&s, 1.6, &sp, &c0));
    assert!(spread(&a) < 0.75 && spread(&b) < 0.75, "correlation 1: one shared wiggle: {a:?} / {b:?}");
    assert!((a[0] - b[0]).abs() > 1.5, "and it still wiggles over time: {} -> {}", a[0], b[0]);
}

#[test]
fn min_max_bound_the_wiggle_and_seed_and_freq_behave() {
    let (sp, c0) = base();
    // min = max = 0.5: every unit gets exactly half the property, whatever the noise
    let (mut s, id) = scene();
    c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "smoothness": 0, "wiggly": true, "wiggle_min": 0.5, "wiggle_max": 0.5}));
    for v in offsets(&s, 1.3, &sp, &c0) { assert!((v - 12.0).abs() < 0.75, "min=max=0.5 -> 12 px: {v}"); }
    // min 0, max 1: all offsets downward only
    let (mut s, id) = scene();
    c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "smoothness": 0, "wiggly": true, "wiggle_min": 0, "wiggle_max": 1, "wiggle_correlation": 0}));
    for t in [0.2, 0.9, 2.5] { for v in offsets(&s, t, &sp, &c0) { assert!(v > -0.75 && v < 24.75, "min 0 max 1 keeps offsets in 0..24 px: {v} at {t}"); } }
    // same seed -> same frame; another seed -> another pattern; freq 0 -> frozen
    let mk = |seed: u64, freq: f64| { let (mut s, id) = scene(); c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "smoothness": 0, "wiggly": true, "wiggle_correlation": 0, "wiggle_seed": seed, "wiggle_freq": freq})); s };
    assert_eq!(frame(&mk(5, 2.0), 0.9), frame(&mk(5, 2.0), 0.9), "same seed, same picture");
    assert_ne!(frame(&mk(5, 2.0), 0.9), frame(&mk(6, 2.0), 0.9), "another seed, another pattern");
    let z = mk(5, 0.0); assert_eq!(frame(&z, 0.3), frame(&z, 2.9), "wiggle_freq 0 holds still");
}

#[test]
fn wiggly_composes_with_the_range_selector_and_refuses_bad_values() {
    let (sp, c0) = base(); let (mut s, id) = scene();
    // the range selects only the first 2 of 5 characters: the other three never move
    c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 24, "start": 0, "end": 0.4, "smoothness": 0, "wiggly": true, "wiggle_correlation": 0}));
    for t in [0.4, 1.1, 2.2] { let o = offsets(&s, t, &sp, &c0); for v in &o[2..] { assert!(v.abs() < 0.05, "unselected characters stay put: {o:?}"); } }
    for (bad, want) in [(json!({"wiggly": true, "wiggle_min": 0.8, "wiggle_max": 0.2}), "wiggle_min"), (json!({"wiggly": true, "wiggle_correlation": 1.5}), "wiggle_correlation"),
        (json!({"wiggly": true, "wiggle_freq": -1}), "wiggle_freq"), (json!({"wiggle_freq": 3}), "wiggly")] {
        let mut a = bad.clone(); a["clip_id"] = id.clone(); a["y"] = json!(10);
        let e = dispatch(&mut s, "add_text_animator", &a).unwrap_err(); assert!(e.contains(want), "{bad} -> {e}");
    }
    // wiggle_min / wiggle_max are keyframable like the other animator properties
    let an = c(&mut s, "add_text_animator", json!({"clip_id": id, "unit": "char", "y": 10, "wiggly": true}))["animator_id"].clone();
    c(&mut s, "set_animator_keyframe", json!({"clip_id": id, "animator_id": an, "prop": "wiggle_max", "time": 1, "value": 0.2}));
}
