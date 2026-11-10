//! Text animators, AE-style, the two parts kimo's animators lacked:
//! - Fill Color: an animator can tint the characters (or words / lines) its range selector picks, `fill` [r,g,b] or hex with
//!   `fill_amount` 0..1 (keyframable with set_animator_keyframe prop=fill_amount). Colour = base*(1-k*amount) + fill*k*amount, k the
//!   selector amount, so a keyed offset sweeps a colour wave across the text.
//! - Selector ease (AE Ease High/Low in spirit): `ease` (any of the shared easing names) reshapes the selector amount k -> ease(k).
//! A bad selector shape, ease or fill colour is refused instead of silently drawing a square selector.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const W: usize = 128;
const H: usize = 48;

/// white text on an opaque black solid; returns (session, text clip id)
fn scene(text: &str) -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25}));
    let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t0,"source_type":"solid","color":[0,0,0],"start":0,"duration":2}));
    let id = c(&mut s, "add_text", json!({"track_id":t1,"text":text,"size":32,"color":[1,1,1],"start":0,"duration":2}))["clip_id"].clone();
    (s, id)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap().d }
fn px(d: &[f32], x: usize, y: usize) -> [f32; 3] { let i = (y * W + x) * 4; [d[i], d[i + 1], d[i + 2]] }
/// pixels in columns [x0, x1) that carry ink (r > 0.5)
fn inked(d: &[f32], x0: usize, x1: usize) -> Vec<[f32; 3]> { let mut v = vec![]; for y in 0..H { for x in x0..x1 { let p = px(d, x, y); if p[0].max(p[1]).max(p[2]) > 0.5 { v.push(p); } } } v }
fn all_px(d: &[f32], x0: usize, x1: usize) -> Vec<[f32; 3]> { let mut v = vec![]; for y in 0..H { for x in x0..x1 { v.push(px(d, x, y)); } } v }

#[test]
fn fill_tints_only_the_selected_characters() {
    let (mut s, id) = scene("HH");
    c(&mut s, "add_text_animator", json!({"clip_id":id,"unit":"char","start":0,"end":0.5,"smoothness":0,"fill":[1,0,0]}));
    let d = frame(&s, 0.5);
    let (l, r) = (all_px(&d, 0, W / 2), all_px(&d, W / 2, W));
    assert!(inked(&d, 0, W / 2).len() > 20 && inked(&d, W / 2, W).len() > 20, "both glyphs must draw");
    let bad_l = l.iter().filter(|p| p[1] != 0.0 || p[2] != 0.0).count();
    assert_eq!(bad_l, 0, "first H (selected) must be pure red x coverage; first off {:?}", l.iter().find(|p| p[1] != 0.0 || p[2] != 0.0));
    let bad_r = r.iter().filter(|p| p[0] != p[1] || p[1] != p[2]).count();
    assert_eq!(bad_r, 0, "second H (not selected) must stay white x coverage; first off {:?}", r.iter().find(|p| p[0] != p[1] || p[1] != p[2]));
}

#[test]
fn fill_amount_half_mixes_half_way() {
    let (mut s, id) = scene("HH");
    c(&mut s, "add_text_animator", json!({"clip_id":id,"unit":"char","start":0,"end":0.5,"smoothness":0,"fill":"#ff0000","fill_amount":0.5}));
    let d = frame(&s, 0.5);
    let l = inked(&d, 0, W / 2); assert!(l.len() > 20);
    // white (1,1,1) half way to red (1,0,0) = (1, 0.5, 0.5), times coverage over black
    let off: Vec<_> = l.iter().filter(|p| p[1] != p[2] || p[1] * 2.0 != p[0]).collect();
    assert_eq!(off.len(), 0, "want (a, a/2, a/2); first off {:?}", off.first());
}

#[test]
fn fill_amount_is_keyframable() {
    let (mut s, id) = scene("HH");
    let an = c(&mut s, "add_text_animator", json!({"clip_id":id,"unit":"char","start":0,"end":0.5,"smoothness":0,"fill":[1,0,0]}))["animator_id"].clone();
    c(&mut s, "set_animator_keyframe", json!({"clip_id":id,"animator_id":an,"prop":"fill_amount","time":0,"value":0,"ease":"linear"}));
    c(&mut s, "set_animator_keyframe", json!({"clip_id":id,"animator_id":an,"prop":"fill_amount","time":1,"value":1,"ease":"linear"}));
    let plain = { let (mut s2, id2) = scene("HH"); c(&mut s2, "add_text_animator", json!({"clip_id":id2,"unit":"char","start":0,"end":0.5,"smoothness":0})); frame(&s2, 0.0) };
    assert!(frame(&s, 0.0) == plain, "fill_amount 0 must draw exactly what the same animator draws without a fill");
    let l = inked(&frame(&s, 0.5), 0, W / 2); assert!(l.len() > 20);
    let off: Vec<_> = l.iter().filter(|p| p[1] != p[2] || p[1] * 2.0 != p[0]).collect();
    assert_eq!(off.len(), 0, "t=0.5 -> amount 0.5: want (a, a/2, a/2); first off {:?}", off.first());
}

#[test]
fn unselected_fill_draws_the_same_bytes() {
    let (mut a, ida) = scene("HH");
    c(&mut a, "add_text_animator", json!({"clip_id":ida,"unit":"char","start":0.9,"end":0.95,"smoothness":0,"fill":[1,0,0]}));
    let (mut b, idb) = scene("HH");
    c(&mut b, "add_text_animator", json!({"clip_id":idb,"unit":"char","start":0.9,"end":0.95,"smoothness":0}));
    assert!(frame(&a, 0.5) == frame(&b, 0.5), "a fill whose selector picks nothing must not change a byte");
}

#[test]
fn selector_ease_reshapes_the_amount() {
    // 4 chars, ramp_up over 0..1: k = (i + 0.5) / 4 = 0.125, 0.375, 0.625, 0.875; ease "in" (u^3): 0.001953125, 0.052734375, 0.244140625, 0.669921875
    // fill red on white text: g / r = 1 - k
    for (ease, want) in [(None, [0.125f64, 0.375, 0.625, 0.875]), (Some("in"), [0.001953125, 0.052734375, 0.244140625, 0.669921875])] {
        let (mut s, id) = scene("HHHH");
        let mut a = json!({"clip_id":id,"unit":"char","shape":"ramp_up","start":0,"end":1,"fill":[1,0,0]});
        if let Some(e) = ease { a["ease"] = json!(e); }
        c(&mut s, "add_text_animator", a);
        let d = frame(&s, 0.5);
        // glyph columns from the untinted render
        let (s0, _) = scene("HHHH"); let d0 = frame(&s0, 0.5);
        let cols: Vec<usize> = (0..W).filter(|&x| (0..H).any(|y| px(&d0, x, y)[0] > 0.0)).collect();
        let mut runs: Vec<(usize, usize)> = vec![]; for &x in &cols { match runs.last_mut() { Some(r) if r.1 + 1 == x => r.1 = x, _ => runs.push((x, x)) } }
        assert_eq!(runs.len(), 4, "four separate glyph column runs, got {runs:?}");
        for (gi, &(x0, x1)) in runs.iter().enumerate() {
            let ink = inked(&d, x0, x1 + 1); assert!(!ink.is_empty());
            for p in &ink { let ratio = p[1] as f64 / p[0] as f64; assert!((ratio - (1.0 - want[gi])).abs() < 1e-6, "ease {ease:?} glyph {gi}: g/r {ratio} want {}", 1.0 - want[gi]); }
        }
    }
}

#[test]
fn bad_shape_ease_fill_are_refused() {
    let (mut s, id) = scene("HH");
    assert!(dispatch(&mut s, "add_text_animator", &json!({"clip_id":id,"shape":"zigzag"})).unwrap_err().contains("zigzag"), "bad shape must be refused");
    assert!(dispatch(&mut s, "add_text_animator", &json!({"clip_id":id,"ease":"wobbly"})).is_err(), "bad ease must be refused");
    assert!(dispatch(&mut s, "add_text_animator", &json!({"clip_id":id,"fill":"notacolour"})).is_err(), "bad fill must be refused");
    let an = c(&mut s, "add_text_animator", json!({"clip_id":id}))["animator_id"].clone();
    assert!(dispatch(&mut s, "set_animator_keyframe", &json!({"clip_id":id,"animator_id":an,"prop":"fill_amount","time":0,"value":1})).is_err(), "fill_amount on an animator with no fill colour must be refused");
    for sh in ["square", "ramp_up", "ramp_down", "triangle", "round", "smooth"] { c(&mut s, "add_text_animator", json!({"clip_id":id,"shape":sh})); }
}

#[test]
fn schema_lists_fill_and_the_shared_easings() {
    let t = kimo_core::commands::find_command("add_text_animator").unwrap().schema.to_string();
    assert!(t.contains("\"fill\"") && t.contains("fill_amount") && t.contains("in_out_bounce"), "add_text_animator schema: {t}");
    let k = kimo_core::commands::find_command("set_animator_keyframe").unwrap().description;
    assert!(k.contains("fill_amount"), "set_animator_keyframe description must name fill_amount: {k}");
}
