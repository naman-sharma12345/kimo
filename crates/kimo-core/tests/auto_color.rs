//! auto_color: one-call Auto Balance (Resolve's "A"). Per-channel robust black/white points (percentile) from sampled frames of the
//! isolated clip are mapped to their common (neutral) level, mids are neutralised by per-channel gamma from near-neutral pixels,
//! and the result is written as an editable primaries (model 1) effect, scaled by strength.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
/// A grey ramp (0.1 .. 0.7) with a colour cast effect already on the clip.
fn scene(cast: Value) -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":64,"height":16,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.1,0.1,0.1],"color2":[0.7,0.7,0.7],"angle":90,"start":0,"duration":2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"white_balance","params":cast})); (s, id) }
fn render(s: &Session) -> Vec<f32> { kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d }
/// Largest |r-g| or |b-g| over the frame: 0 for a neutral image.
fn spread(d: &[f32]) -> f32 { d.chunks_exact(4).map(|p| (p[0] - p[1]).abs().max((p[2] - p[1]).abs())).fold(0.0, f32::max) }

#[test]
fn auto_color_neutralises_a_gain_cast() {
    let (mut s, id) = scene(json!({"r_gain":1.25,"g_gain":1.0,"b_gain":0.75}));
    let before = render(&s); let sp0 = spread(&before); assert!(sp0 > 0.1, "fixture must carry a visible cast, spread {sp0}");
    let r = c(&mut s, "auto_color", json!({"clip_id":id}));
    let after = render(&s); let sp1 = spread(&after);
    assert!(sp1 <= 0.01, "auto_color must neutralise the cast: max channel spread {sp0} -> {sp1}");
    let fx = r["effect_id"].as_str().expect("effect_id").to_string();
    let clip = s.project.find_clip(id.as_str().unwrap()).unwrap(); let e = clip.effects.iter().find(|e| e.id == fx).unwrap();
    assert_eq!(e.kind, "primaries", "the balance is an editable primaries effect"); assert_eq!(e.params["model"].base, 1.0);
    // levels are kept (balance, not stretch): mean luma within 2%
    let luma = |d: &[f32]| d.chunks_exact(4).map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).sum::<f32>() / (d.len() / 4) as f32;
    assert!((luma(&after) - luma(&before)).abs() / luma(&before) < 0.02, "balance keeps overall level: {} -> {}", luma(&before), luma(&after));
    // set_color_wheels can edit what auto_color wrote
    c(&mut s, "set_color_wheels", json!({"clip_id":id,"effect_id":fx,"saturation":1.0}));
}

#[test]
fn auto_color_strength_and_stretch() {
    let (mut s, id) = scene(json!({"r_gain":1.25,"g_gain":1.0,"b_gain":0.75})); let before = render(&s);
    c(&mut s, "auto_color", json!({"clip_id":id,"strength":0.0})); assert_eq!(render(&s), before, "strength 0 is an exact identity");
    let (mut s, id) = scene(json!({"r_gain":1.25,"g_gain":1.0,"b_gain":0.75})); let sp0 = spread(&render(&s));
    c(&mut s, "auto_color", json!({"clip_id":id,"strength":0.5})); let sp = spread(&render(&s));
    assert!(sp > 0.3 * sp0 && sp < 0.7 * sp0, "strength 0.5 is halfway: spread {sp0} -> {sp}");
    let (mut s, id) = scene(json!({"r_gain":1.25,"g_gain":1.0,"b_gain":0.75}));
    let r = c(&mut s, "auto_color", json!({"clip_id":id,"stretch":true,"percentile":0.0})); let d = render(&s);
    let (lo, hi) = d.chunks_exact(4).fold((1f32, 0f32), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
    assert!(lo < 0.03 && hi > 0.97, "stretch maps the black/white points to 0..1: green {lo}..{hi}, result {r}");
}

#[test]
fn auto_color_neutralises_a_mid_cast() {
    // a gamma-like cast that a black/white map alone cannot fix: per-channel gamma from near-neutral pixels must
    let (mut s, id) = scene(json!({})); c(&mut s, "set_color_wheels", json!({"clip_id":id,"gamma":[1.12,1.0,0.9,1.0]}));
    let sp0 = spread(&render(&s));
    let r = c(&mut s, "auto_color", json!({"clip_id":id})); let sp1 = spread(&render(&s));
    assert!(sp1 < 0.25 * sp0 && sp1 < 0.015, "mids must be neutralised too: spread {sp0} -> {sp1}, result {r}");
    let off = { let (mut s, id) = scene(json!({})); c(&mut s, "set_color_wheels", json!({"clip_id":id,"gamma":[1.12,1.0,0.9,1.0]})); c(&mut s, "auto_color", json!({"clip_id":id,"mids":false})); spread(&render(&s)) };
    assert!(off > sp1, "mids:false skips the mid pass: {off} vs {sp1}");
}

#[test]
fn auto_color_refusals_leave_project_untouched() {
    let (mut s, id) = scene(json!({})); let snap = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"clip_id":"nope"}), "clip"), (json!({"clip_id":id,"strength":2.0}), "strength"), (json!({"clip_id":id,"percentile":30.0}), "percentile"), (json!({"clip_id":id,"time":9.0}), "time")] {
        let e = dispatch(&mut s, "auto_color", &a).expect_err(&format!("{a} must be refused")); assert!(e.to_string().contains(want), "{a}: '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), snap, "{a} mutated the project"); }
    // a flat frame has no range to balance
    let mut s2 = Session::new(); c(&mut s2, "project_set", json!({"width":16,"height":16,"fps":25})); let t = c(&mut s2, "add_track", json!({}))["track_id"].clone();
    let flat = c(&mut s2, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.4,0.3,0.2],"start":0,"duration":1}))["clip_id"].clone();
    let e = dispatch(&mut s2, "auto_color", &json!({"clip_id":flat})).expect_err("flat frame must be refused"); assert!(e.to_string().contains("flat"), "'{e}' lacks 'flat'");
}


/// A test image written as PNG through ffmpeg: rows 0..ns are strongly coloured (half a shaded red sheet, half a shaded lilac),
/// the remaining rows a neutral grey ramp 0.1..0.75. The shapes of clip2 (a red sheet and a lilac tumbler filling the frame).
fn coloured_scene(sat_rows: usize, cast: Value) -> (Session, Value, usize) {
    let (w, h) = (64usize, 64usize); let dir = std::env::temp_dir().join(format!("kimo-auto-color-{}-{sat_rows}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
    let mut px = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h { for x in 0..w { let t = x as f64 / (w - 1) as f64;
        let c = if y < sat_rows { let s = 0.6 + 0.4 * t; let base = if y < sat_rows / 2 { [0.85, 0.18, 0.28] } else { [0.78, 0.62, 0.9] }; base.map(|v| v * s) } else { [0.1 + 0.65 * t; 3] };
        px.extend(c.map(|v| (v * 255.0).round() as u8)); } }
    let (ppm, png) = (dir.join("scene.ppm"), dir.join("scene.png")); std::fs::write(&ppm, px).unwrap();
    let st = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&ppm).args(["-pix_fmt", "rgb24"]).arg(&png).status().unwrap(); assert!(st.success());
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":w,"height":h,"fps":25}));
    let a = c(&mut s, "import_media", json!({"path": png}))["asset_id"].clone(); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"media","asset_id":a,"start":0,"duration":2,"fit":"stretch"}))["clip_id"].clone();
    if cast != json!({}) { c(&mut s, "add_effect", json!({"clip_id":id,"kind":"white_balance","params":cast})); }
    (s, id, sat_rows) }
/// Render a media scene (its frames decoded first).
fn render_media(s: &mut Session) -> Vec<f32> { kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(0.5)).unwrap(); render(s) }
/// Largest |r-g| or |b-g| over the grey-ramp rows only.
fn neutral_spread(d: &[f32], from_row: usize) -> f32 { spread(&d[from_row * 64 * 4..]) }

#[test]
fn auto_color_ignores_a_frame_dominated_by_strong_colour() {
    // 70% of the frame is a red sheet and a lilac object, 30% true greys, no cast. Treating the brightest/darkest pixels per channel
    // as neutral turns the greys green (red and blue white points come from the coloured objects); the greys must stay grey.
    let (mut s, id, rows) = coloured_scene(45, json!({})); let before = render_media(&mut s); assert!(neutral_spread(&before, rows) < 0.01 && spread(&before) > 0.3, "fixture: grey rows grey, coloured rows coloured");
    let r = c(&mut s, "auto_color", json!({"clip_id":id})); let after = render_media(&mut s);
    let sp = neutral_spread(&after, rows); let moved = before.iter().zip(&after).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
    assert!(sp <= 0.01 && moved <= 0.02, "a frame that is mostly one strong colour must stay put: grey rows spread {sp}, largest change {moved}, result {r}");
    assert!(r["confidence"].as_f64().expect("confidence") < 0.5, "the result says how little neutral there was: {r}");
}

#[test]
fn auto_color_balances_greys_past_coloured_objects() {
    // 30% strong colour, 70% greys under a warm gain cast: the cast must still come out of the greys.
    let (mut s, id, rows) = coloured_scene(19, json!({"r_gain":1.25,"g_gain":1.0,"b_gain":0.75})); let sp0 = neutral_spread(&render_media(&mut s), rows); assert!(sp0 > 0.1, "fixture carries a cast {sp0}");
    let r = c(&mut s, "auto_color", json!({"clip_id":id})); let sp = neutral_spread(&render_media(&mut s), rows);
    assert!(sp <= 0.015, "greys must be neutralised despite the coloured third: spread {sp0} -> {sp}, result {r}");
    assert!(r["neutral_share"].as_f64().expect("neutral_share") > 0.6, "{r}");
}
