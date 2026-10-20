//! Task brief items (c)+(d), Naman: the four colour readers left after 2717cb1 must refuse a bad colour instead of drawing a
//! wrong one, the same way `color` does ("{key}: expected 3 or 4 numbers, got N" / "... element I is V"):
//!   set_text_style shadow_color / glow_color / gradient_top / gradient_bottom (tools_1.rs `rgb`), set_text_style bg,
//!   add_transition color (tools_2.rs), preview_material color (ext/lighting_tools.rs).
//! On 00a9b2f: a non-number element read as 0 (or 0.8), five values were cut to three, [1,1] / a hex string were ignored.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn session() -> (Session, Value, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let text = c(&mut s, "add_text", json!({"track_id":t,"text":"H","font":"DejaVuSans","size":12,"start":0,"duration":2}))["clip_id"].clone();
    let solid = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0,0,1],"start":1,"duration":2}))["clip_id"].clone();
    (s, text, solid)
}

fn clip<'a>(s: &'a Session, id: &Value) -> &'a kimo_core::model::Clip {
    s.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == id.as_str().unwrap()).expect("clip").as_ref()
}

fn refused(s: &mut Session, tool: &str, a: Value, want: &str) {
    let e = dispatch(s, tool, &a).expect_err(&format!("{tool} {a} must be refused, not drawn as a wrong colour"));
    eprintln!("{tool} {a} -> {e}");
    assert!(e.contains(want), "{tool} {a}: error must contain {want:?}, got: {e}");
}

#[test]
fn set_text_style_rgb_refuses_bad_colours() {
    let (mut s, text, _) = session();
    refused(&mut s, "set_text_style", json!({"clip_id":text,"shadow_color":["a",0,0]}), "shadow_color: expected 3 or 4 numbers, element 0 is \"a\"");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"glow_color":[1,1]}), "glow_color: expected 3 or 4 numbers, got 2");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"gradient_top":[1,0,0,0,1],"gradient_bottom":[0,0,1]}), "gradient_top: expected 3 or 4 numbers, got 5");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"gradient_top":[1,0,0],"gradient_bottom":[0,null,1]}), "gradient_bottom: expected 3 or 4 numbers, element 1 is null");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"shadow_color":"blue"}), "shadow_color: \"blue\" is not a colour");
    assert!(clip(&s, &text).text_fx.as_ref().map_or(true, |fx| fx.shadow_color == [0.0; 3] && fx.glow_color == [1.0; 3] && fx.gradient.is_none()), "a refused call must change nothing");
}

#[test]
fn set_text_style_bg_refuses_bad_colours() {
    let (mut s, text, _) = session();
    refused(&mut s, "set_text_style", json!({"clip_id":text,"bg":[0,"x",0,1]}), "bg: expected 3 or 4 numbers, element 1 is \"x\"");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"bg":[0,0]}), "bg: expected 3 or 4 numbers, got 2");
    refused(&mut s, "set_text_style", json!({"clip_id":text,"bg":[0,0,0,1,1]}), "bg: expected 3 or 4 numbers, got 5");
    assert!(clip(&s, &text).text_fx.as_ref().map_or(true, |fx| fx.bg.is_none()), "a refused call must change nothing");
}

#[test]
fn add_transition_colour_refuses_bad_elements() {
    let (mut s, _, solid) = session();
    refused(&mut s, "add_transition", json!({"clip_id":solid,"kind":"flash","duration":0.5,"color":[1,"x",1]}), "color: expected 3 or 4 numbers, element 1 is \"x\"");
    refused(&mut s, "add_transition", json!({"clip_id":solid,"kind":"flash","duration":0.5,"color":[1,1]}), "color: expected 3 or 4 numbers, got 2");
    assert!(clip(&s, &solid).transition_in.is_none(), "a refused call must change nothing");
}

#[test]
fn preview_material_colour_refuses_bad_elements() {
    let (mut s, _, _) = session();
    let p = std::env::temp_dir().join(format!("kimo_prev_strict_{}.png", std::process::id()));
    refused(&mut s, "preview_material", json!({"path":p,"material":"plastic","size":64,"color":[0.2,"x",0.2]}), "color: expected 3 or 4 numbers, element 1 is \"x\"");
    refused(&mut s, "preview_material", json!({"path":p,"material":"plastic","size":64,"color":[0.2,0.2]}), "color: expected 3 or 4 numbers, got 2");
}

#[test]
fn right_colours_still_work() {
    let (mut s, text, solid) = session();
    c(&mut s, "set_text_style", json!({"clip_id":text,"shadow_color":[1,0,0],"glow_color":"#00ff00","gradient_top":[1,1,1,1],"gradient_bottom":"#000000","bg":[0,0,0,0.5]}));
    { let fx = clip(&s, &text).text_fx.as_ref().unwrap();
      assert_eq!(fx.shadow_color, [1.0, 0.0, 0.0]); assert_eq!(fx.glow_color, [0.0, 1.0, 0.0]);
      assert_eq!(fx.gradient, Some(([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]))); assert_eq!(fx.bg, Some([0.0, 0.0, 0.0, 0.5])); }
    c(&mut s, "set_text_style", json!({"clip_id":text,"bg":[0,0,1]}));
    assert_eq!(clip(&s, &text).text_fx.as_ref().unwrap().bg, Some([0.0, 0.0, 1.0, 1.0]), "[r,g,b] bg keeps alpha 1");
    c(&mut s, "set_text_style", json!({"clip_id":text,"bg":[],"gradient_top":[]}));
    { let fx = clip(&s, &text).text_fx.as_ref().unwrap(); assert_eq!(fx.bg, None, "[] clears bg"); assert_eq!(fx.gradient, None, "[] clears the gradient"); }
    c(&mut s, "add_transition", json!({"clip_id":solid,"kind":"flash","duration":0.5,"color":[1,0.5,0]}));
    assert_eq!(clip(&s, &solid).transition_in.as_ref().unwrap().color, [1.0, 0.5, 0.0]);
    c(&mut s, "add_transition", json!({"clip_id":solid,"kind":"flash","duration":0.5}));
    assert_eq!(clip(&s, &solid).transition_in.as_ref().unwrap().color, [0.0; 3], "no color keeps the black default");
    let p = std::env::temp_dir().join(format!("kimo_prev_ok_{}.png", std::process::id()));
    c(&mut s, "preview_material", json!({"path":p,"material":"plastic","size":64,"color":[0.9,0.1,0.1,1]}));
    let _ = std::fs::remove_file(&p);
}
