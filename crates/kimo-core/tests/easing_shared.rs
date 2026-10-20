//! One shared easing primitive (keyframe::Ease::apply) with AE/easings.net overshoot and spring curves: in_back, in_out_back,
//! in/out/in_out_elastic, in/out/in_out_bounce, next to the ten kimo always had (unchanged). Every keyframe tool takes the same
//! list (keyframe::EASE_NAMES); transitions and motion paths route the shared names through it too. Overshoot leaves 0..1 mid
//! segment, so bounded properties are clamped where read (keyframe::BOUNDED_PROPS: opacity, fill/stroke alpha, reveal, volume >= 0);
//! position/scale/rotation stay unclamped so the overshoot reads as motion.
use kimo_core::commands::{dispatch, Session};
use kimo_core::keyframe::{clamp_prop, Ease, EASE_NAMES};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

#[test]
fn new_curves_golden_at_0_half_1() {
    // hand-computed closed forms at u = 0.5 (c1 = 1.70158; elastic periods 2pi/3 and 2pi/4.5; bounce n = 7.5625, d = 2.75)
    let table: &[(&str, f64)] = &[("in_back", 2.70158 * 0.125 - 1.70158 * 0.25), ("in_out_back", 0.5), ("in_elastic", -1.0 / 64.0), ("out_elastic", 1.0 + 1.0 / 64.0),
        ("in_out_elastic", 0.5), ("in_bounce", 0.234375), ("out_bounce", 0.765625), ("in_out_bounce", 0.5)];
    for (name, half) in table {
        let e = Ease::parse(name).unwrap_or_else(|| panic!("{name} is not an easing"));
        assert_eq!(e.apply(0.0), 0.0, "{name}(0) must be exactly 0");
        assert_eq!(e.apply(1.0), 1.0, "{name}(1) must be exactly 1");
        let v = e.apply(0.5); assert!((v - half).abs() < 1e-12, "{name}(0.5) = {v}, hand value {half}");
        assert!(EASE_NAMES.contains(name), "{name} missing from EASE_NAMES");
    }
    // overshoot really leaves 0..1 (that is the point of back/elastic)
    assert!(Ease::OutElastic.apply(0.1) > 1.2 && Ease::InBack.apply(0.2) < 0.0);
}

#[test]
fn old_curves_unchanged() {
    for u in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0f64] {
        assert_eq!(Ease::In.apply(u), u * u * u); assert_eq!(Ease::Out.apply(u), 1.0 - (1.0 - u).powi(3));
        assert_eq!(Ease::OutBack.apply(u), 1.0 + 2.2 * (u - 1.0).powi(3) + 1.2 * (u - 1.0).powi(2));
        assert_eq!(Ease::InOut.apply(u), if u < 0.5 { 4.0 * u * u * u } else { 1.0 - (-2.0 * u + 2.0).powi(3) / 2.0 });
        // transitions keep their own curves for their own names ("in_out" is smoothstep there) and take the shared ones for new names
        assert_eq!(kimo_core::transitions::ease("in_out", u), u * u * (3.0 - 2.0 * u));
        assert_eq!(kimo_core::transitions::ease("out_back", u), { let (c1, c3) = (1.70158, 2.70158); 1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2) });
        assert_eq!(kimo_core::transitions::ease("out_elastic", u), Ease::OutElastic.apply(u));
    }
}

fn solid_scene() -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[1,1,1],"start":0,"duration":2}))["clip_id"].clone();
    (s, id)
}

#[test]
fn position_overshoots_unclamped() {
    let (mut s, id) = solid_scene();
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"x","time":0,"value":0,"ease":"out_elastic"}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"x","time":1,"value":100}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"scale","time":0,"value":1,"ease":"in_back"}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"scale","time":1,"value":2}));
    let p = &s.project.tracks[0].clips[0].props;
    let x = p["x"].at(0.1); assert_eq!(x, 100.0 * Ease::OutElastic.apply(0.1), "x at 0.1 s"); assert!(x > 100.0, "out_elastic x must overshoot past 100, got {x}");
    let sc = p["scale"].at(0.2); assert_eq!(sc, 1.0 + Ease::InBack.apply(0.2), "scale at 0.2 s"); assert!(sc < 1.0, "in_back scale must dip below 1, got {sc}");
    assert_eq!(p["x"].at(1.0), 100.0); assert_eq!(p["x"].at(0.0), 0.0);
}

#[test]
fn bounded_opacity_is_clamped_where_drawn() {
    // white solid over the black canvas; opacity out_elastic 0 -> 1 overshoots to 1.25 at 0.1 s: drawn as exactly 1 (not 1.25)
    let (mut s, id) = solid_scene();
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"opacity","time":0,"value":0,"ease":"out_elastic"}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"opacity","time":1,"value":1}));
    assert!(s.project.tracks[0].clips[0].props["opacity"].at(0.1) > 1.0, "the curve itself overshoots");
    let px = kimo_core::render::render_frame(&s.project, 0.1, &s.cache).unwrap().d;
    assert_eq!(&px[..4], &[1.0, 1.0, 1.0, 1.0], "opacity overshoot must clamp to 1 (got {:?})", &px[..4]);
    // in_back undershoots below 0 at 0.2 s: drawn as exactly 0 (the black canvas, not a negative blend)
    let (mut s, id) = solid_scene();
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"opacity","time":0,"value":0,"ease":"in_back"}));
    c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"opacity","time":1,"value":1}));
    let px = kimo_core::render::render_frame(&s.project, 0.2, &s.cache).unwrap().d;
    assert_eq!(&px[..4], &[0.0, 0.0, 0.0, 1.0], "opacity undershoot must clamp to 0 (got {:?})", &px[..4]);
    assert_eq!(clamp_prop("volume", -0.2), 0.0); assert_eq!(clamp_prop("volume", 1.7), 1.7); assert_eq!(clamp_prop("x", -5.0), -5.0); assert_eq!(clamp_prop("fill_alpha", 1.3), 1.0);
}

#[test]
fn every_keyframe_tool_takes_the_new_names() {
    let (mut s, id) = solid_scene();
    for e in ["in_out_elastic", "in_bounce", "in_out_back"] {
        c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"rotation","time":0.5,"value":10,"ease":e}));
        c(&mut s, "set_camera_keyframe", json!({"property":"cam_x","time":0.5,"value":10,"ease":e}));
        c(&mut s, "set_audio_volume_key", json!({"clip_id":id,"time":0.5,"value":0.5,"ease":e}));
        c(&mut s, "add_transition", json!({"clip_id":id,"kind":"crossfade","duration":0.5,"ease":e}));
    }
    for tool in ["set_keyframe", "set_camera_keyframe", "set_mask_prop_key", "set_keyframes", "set_audio_volume_key", "set_speed_ramp", "set_animator_keyframe", "add_transition"] {
        let t = kimo_core::commands::find_command(tool).unwrap_or_else(|| panic!("no tool {tool}")).schema.to_string();
        assert!(t.contains("out_elastic") && t.contains("in_out_bounce"), "{tool} schema must list the shared easings");
    }
}
