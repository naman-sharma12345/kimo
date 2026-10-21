//! Track mattes at After Effects parity: any clip can gate the clip below it by its alpha, alpha inverted, luma or luma
//! inverted (legacy inv_alpha / inv_luma still accepted and still stored). Luma is read in linear light, Rec.709
//! weights on the sRGB-decoded colour, by default; luma_space "display" reads the encoded values instead. The matte
//! clip renders premultiplied, so luma is taken on the unpremultiplied colour and scaled by alpha once: a soft white
//! edge gives the same coverage in luma as in alpha, with no dark fringe. Matte opacity, transforms and keyframes
//! all count; the matte clip is hidden from the output unless hide_source is false.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const W: usize = 32; // project_set needs >= 16 on each side
const H: usize = 16;
const TOL: f32 = 1e-6;

/// sRGB EOTF (encoded 0..1 -> linear), f64.
fn srgb_to_linear(c: f64) -> f64 { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
/// Linear-light Rec.709 luma of an encoded colour.
fn lin_luma(rgb: [f64; 3]) -> f64 { 0.2126 * srgb_to_linear(rgb[0]) + 0.7152 * srgb_to_linear(rgb[1]) + 0.0722 * srgb_to_linear(rgb[2]) }

/// Track 1: opaque `bg` solid; track 2: the `fill` solid (the matted clip); track 3 (top): the matte clip built from
/// `matte` (add_clip args without track/start/duration). All clips 0..1 s. Returns (session, fill id, matte id).
fn scene(bg: Value, fill: Value, matte: Value) -> (Session, Value, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t: Vec<Value> = (0..3).map(|_| c(&mut s, "add_track", json!({}))["track_id"].clone()).collect();
    c(&mut s, "add_clip", json!({"track_id":t[0],"source_type":"solid","color":bg,"start":0,"duration":1}));
    let f = c(&mut s, "add_clip", json!({"track_id":t[1],"source_type":"solid","color":fill,"start":0,"duration":1}))["clip_id"].clone();
    let mut m = matte; let o = m.as_object_mut().unwrap();
    o.insert("track_id".into(), t[2].clone()); o.insert("start".into(), json!(0)); o.insert("duration".into(), json!(1));
    let m = c(&mut s, "add_clip", m)["clip_id"].clone();
    (s, f, m)
}

fn solid(color: Value) -> Value { json!({"source_type":"solid","color":color}) }
/// White, opaque at the top row fading to fully transparent at the bottom.
fn white_fade() -> Value { json!({"source_type":"gradient","color":[1,1,1,1],"color2":[1,1,1,0]}) }

/// set_track_matte on `fill` from `matte`, with `extra` args (mode, luma_space, hide_source) merged in.
fn set_matte(s: &mut Session, fill: &Value, matte: &Value, extra: Value) {
    let mut a = json!({"clip_id":fill,"matte_clip_id":matte});
    for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    c(s, "set_track_matte", a);
}

/// Black bg, white fill, `matte` with `extra` matte args: the output rgb of every pixel is the coverage.
fn matted(matte: Value, extra: Value) -> Session {
    let (mut s, f, m) = scene(json!([0, 0, 0]), json!([1, 1, 1]), matte);
    set_matte(&mut s, &f, &m, extra);
    s
}

fn pixels_at(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap().d }
fn pixels(s: &Session) -> Vec<f32> { pixels_at(s, 0.5) }

/// Every pixel's r, g and b within TOL of `want`.
fn all_px(s: &Session, want: f64, what: &str) {
    let want = want as f32; let px = pixels(s);
    let mut off = 0; let mut first = None;
    for i in 0..W * H { let p = &px[i * 4..i * 4 + 4];
        if (0..3).any(|j| (p[j] - want).abs() > TOL) { off += 1; if first.is_none() { first = Some((i % W, i / W, [p[0], p[1], p[2]])); } } }
    assert_eq!(off, 0, "{what}: want {want} on every pixel; first off (x, y, rgb) {first:?}; {off}/{} px off", W * H);
}

const GREY: [f64; 3] = [0.5, 0.5, 0.5];

// ---- hard (opaque) grey matte ----

#[test]
fn hard_matte_alpha_is_full() {
    // opaque matte: alpha = 1.0
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"alpha"})), 1.0, "opaque grey, alpha");
}

#[test]
fn hard_matte_alpha_inverted_is_empty() {
    // 1 - 1 = 0.0
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"alpha_inverted"})), 0.0, "opaque grey, alpha_inverted");
}

#[test]
fn hard_matte_legacy_inv_alpha_is_empty() {
    // legacy name, same as alpha_inverted: 0.0
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"inv_alpha"})), 0.0, "opaque grey, inv_alpha");
}

#[test]
fn hard_matte_luma_is_linear_light() {
    // lin(0.5) = 0.21404114
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"luma"})), lin_luma(GREY), "opaque grey, luma (linear)");
}

#[test]
fn hard_matte_luma_inverted_is_linear_light() {
    // 1 - lin(0.5) = 0.78595886
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"luma_inverted"})), 1.0 - lin_luma(GREY), "opaque grey, luma_inverted (linear)");
}

#[test]
fn hard_matte_legacy_inv_luma_matches_luma_inverted() {
    // legacy name, same as luma_inverted: 1 - 0.21404114 = 0.78595886
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"inv_luma"})), 1.0 - lin_luma(GREY), "opaque grey, inv_luma (linear)");
}

#[test]
fn hard_matte_luma_display_space() {
    // encoded luma of 0.5 grey = 0.5
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"luma","luma_space":"display"})), 0.2126 * 0.5 + 0.7152 * 0.5 + 0.0722 * 0.5, "opaque grey, luma (display)");
}

#[test]
fn hard_matte_luma_inverted_display_space() {
    // 1 - 0.5 = 0.5
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"luma_inverted","luma_space":"display"})), 1.0 - (0.2126 * 0.5 + 0.7152 * 0.5 + 0.0722 * 0.5), "opaque grey, luma_inverted (display)");
}

#[test]
fn coloured_matte_luma_uses_rec709_weights_in_linear_light() {
    // 0.2126 lin(.5) + 0.7152 lin(.2) + 0.0722 lin(.8) ~= 0.112778
    all_px(&matted(solid(json!([0.5, 0.2, 0.8])), json!({"mode":"luma"})), lin_luma([0.5, 0.2, 0.8]), "opaque [0.5,0.2,0.8], luma (linear)");
}

// ---- soft (partial-alpha) grey matte, alpha 0.25 ----

fn soft_grey() -> Value { solid(json!([0.5, 0.5, 0.5, 0.25])) }

#[test]
fn soft_matte_alpha() {
    // a = 0.25
    all_px(&matted(soft_grey(), json!({"mode":"alpha"})), 0.25, "grey a=0.25, alpha");
}

#[test]
fn soft_matte_alpha_inverted() {
    // 1 - 0.25 = 0.75
    all_px(&matted(soft_grey(), json!({"mode":"alpha_inverted"})), 1.0 - 0.25, "grey a=0.25, alpha_inverted");
}

#[test]
fn soft_matte_luma_scales_by_alpha_once() {
    // 0.25 * lin(0.5) = 0.05351029
    all_px(&matted(soft_grey(), json!({"mode":"luma"})), 0.25 * lin_luma(GREY), "grey a=0.25, luma (linear)");
}

#[test]
fn soft_matte_luma_inverted() {
    // 1 - 0.25 * lin(0.5) = 0.94648971
    all_px(&matted(soft_grey(), json!({"mode":"luma_inverted"})), 1.0 - 0.25 * lin_luma(GREY), "grey a=0.25, luma_inverted (linear)");
}

#[test]
fn soft_matte_luma_display_space() {
    // 0.5 * 0.25 = 0.125
    all_px(&matted(soft_grey(), json!({"mode":"luma","luma_space":"display"})), 0.5 * 0.25, "grey a=0.25, luma (display)");
}

#[test]
fn soft_matte_luma_inverted_display_space() {
    // 1 - 0.125 = 0.875
    all_px(&matted(soft_grey(), json!({"mode":"luma_inverted","luma_space":"display"})), 1.0 - 0.5 * 0.25, "grey a=0.25, luma_inverted (display)");
}

// ---- matte layer opacity 0.5 on an opaque white matte ----

fn half_opacity_white(mode: &str) -> Session {
    let (mut s, f, m) = scene(json!([0, 0, 0]), json!([1, 1, 1]), solid(json!([1, 1, 1])));
    c(&mut s, "set_property", json!({"clip_id":m,"property":"opacity","value":0.5}));
    set_matte(&mut s, &f, &m, json!({"mode":mode}));
    s
}

#[test]
fn matte_opacity_alpha() {
    // a = opacity = 0.5
    all_px(&half_opacity_white("alpha"), 0.5, "white at opacity 0.5, alpha");
}

#[test]
fn matte_opacity_luma() {
    // lin(1) * 0.5 = 0.5
    all_px(&half_opacity_white("luma"), srgb_to_linear(1.0) * 0.5, "white at opacity 0.5, luma");
}

#[test]
fn matte_opacity_luma_inverted() {
    // 1 - 0.5 = 0.5
    all_px(&half_opacity_white("luma_inverted"), 1.0 - srgb_to_linear(1.0) * 0.5, "white at opacity 0.5, luma_inverted");
}

#[test]
fn matte_opacity_alpha_inverted() {
    // 1 - 0.5 = 0.5
    all_px(&half_opacity_white("alpha_inverted"), 1.0 - 0.5, "white at opacity 0.5, alpha_inverted");
}

// ---- matte transforms and keyframes ----

#[test]
fn keyframed_matte_position_moves_the_matte() {
    // x keyed 0 px at 0 s -> 32 px at 1 s (linear): at 0.5 s the matte is shifted right 16 px, columns 0..16 uncovered (0.0), 16..32 covered (1.0)
    let (mut s, f, m) = scene(json!([0, 0, 0]), json!([1, 1, 1]), solid(json!([1, 1, 1])));
    c(&mut s, "set_keyframe", json!({"clip_id":m,"property":"x","time":0,"value":0}));
    c(&mut s, "set_keyframe", json!({"clip_id":m,"property":"x","time":1,"value":W as f64}));
    set_matte(&mut s, &f, &m, json!({"mode":"alpha"}));
    let px = pixels(&s);
    for y in 0..H { for x in (0..=14).chain(17..=31) { let want: f32 = if x <= 14 { 0.0 } else { 1.0 }; let p = &px[(y * W + x) * 4..(y * W + x) * 4 + 3];
        assert!(p.iter().all(|v| (*v - want).abs() <= TOL), "t=0.5 matte shifted 16 px right: ({x},{y}) rgb {p:?}, want {want}"); } }
    let px = pixels_at(&s, 0.0);
    for y in 0..H { let p = &px[y * W * 4..y * W * 4 + 3]; assert!(p.iter().all(|v| (*v - 1.0).abs() <= TOL), "t=0 matte unshifted: (0,{y}) rgb {p:?}, want 1.0"); }
}

// ---- hidden by default / visible option ----

#[test]
fn matte_source_hidden_by_default() {
    // opaque grey matte not drawn, fill fully gated in: 1.0
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"alpha"})), 1.0, "hide_source omitted");
}

#[test]
fn matte_source_drawn_when_hide_source_false() {
    // the opaque grey matte drawn on top: 0.5
    all_px(&matted(solid(json!([0.5, 0.5, 0.5])), json!({"mode":"alpha","hide_source":false})), 0.5, "hide_source false");
}

// ---- premultiplied correctness on soft edges ----

#[test]
fn white_soft_edge_luma_equals_alpha() {
    // white luma is 1, so luma coverage = alpha coverage (not a * a)
    let (a, l) = (pixels(&matted(white_fade(), json!({"mode":"alpha"}))), pixels(&matted(white_fade(), json!({"mode":"luma"}))));
    for i in 0..W * H { for j in 0..3 { assert!((a[i * 4 + j] - l[i * 4 + j]).abs() <= TOL, "({},{}) ch {j}: alpha {} vs luma {}", i % W, i / W, a[i * 4 + j], l[i * 4 + j]); } }
}

#[test]
fn white_over_white_through_soft_luma_matte_has_no_fringe() {
    // white fill over white bg is white whatever the coverage: 1.0
    let (mut s, f, m) = scene(json!([1, 1, 1]), json!([1, 1, 1]), white_fade());
    set_matte(&mut s, &f, &m, json!({"mode":"luma"}));
    all_px(&s, 1.0, "white fill over white bg, white fade luma matte");
}

#[test]
fn red_over_blue_through_soft_alpha_matte_blends_cleanly() {
    // row y coverage = 1 - y/16: r = coverage, b = 1 - coverage, g = 0
    let (mut s, f, m) = scene(json!([0, 0, 1]), json!([1, 0, 0]), white_fade());
    set_matte(&mut s, &f, &m, json!({"mode":"alpha"}));
    let px = pixels(&s); let r = |x: usize, y: usize| px[(y * W + x) * 4];
    for y in 0..H { for x in 0..W { let p = &px[(y * W + x) * 4..(y * W + x) * 4 + 3];
        assert!(p[1].abs() <= TOL, "({x},{y}) g {} must be 0", p[1]);
        assert!((p[0] + p[2] - 1.0).abs() <= TOL, "({x},{y}) r + b = {} must be 1", p[0] + p[2]);
        if y > 0 { assert!(r(x, y) <= r(x, y - 1) + TOL, "({x},{y}) r {} rose above the row above {}", r(x, y), r(x, y - 1)); } } }
    // a 16-row gradient's bottom row has alpha 1/16 = 0.0625
    for x in 0..W { assert!(r(x, 0) > 0.9, "top row r {} > 0.9", r(x, 0)); assert!(r(x, H - 1) < 0.1, "bottom row r {} < 0.1", r(x, H - 1)); }
}

// ---- validation and stored format ----

#[test]
fn bad_mode_or_luma_space_is_refused() {
    let (mut s, f, m) = scene(json!([0, 0, 0]), json!([1, 1, 1]), solid(json!([1, 1, 1])));
    assert!(dispatch(&mut s, "set_track_matte", &json!({"clip_id":f,"matte_clip_id":m,"mode":"bogus"})).is_err(), "mode bogus must be refused");
    assert!(dispatch(&mut s, "set_track_matte", &json!({"clip_id":f,"matte_clip_id":m,"mode":"luma","luma_space":"bogus"})).is_err(), "luma_space bogus must be refused");
}

#[test]
fn stored_format_keeps_legacy_mode_names_and_omits_default_luma_space() {
    let (mut s, f, m) = scene(json!([0, 0, 0]), json!([1, 1, 1]), solid(json!([1, 1, 1])));
    set_matte(&mut s, &f, &m, json!({"mode":"alpha_inverted"}));
    assert_eq!(c(&mut s, "list_masks", json!({"clip_id":f}))["matte"]["mode"], "inv_alpha", "list_masks: alpha_inverted lists as inv_alpha");
    let stored = |s: &Session| serde_json::to_value(&s.project).unwrap()["tracks"][1]["clips"][0]["mask"]["matte"].clone();
    let mt = stored(&s);
    assert_eq!(mt["mode"], "inv_alpha", "alpha_inverted is stored as inv_alpha: {mt}");
    assert!(mt.get("luma_space").is_none(), "default luma_space must not be stored: {mt}");
    set_matte(&mut s, &f, &m, json!({"mode":"luma_inverted","luma_space":"display"}));
    let mt = stored(&s);
    assert_eq!(mt["mode"], "inv_luma", "luma_inverted is stored as inv_luma: {mt}");
    assert_eq!(mt["luma_space"], "display", "luma_space display is stored: {mt}");
}
