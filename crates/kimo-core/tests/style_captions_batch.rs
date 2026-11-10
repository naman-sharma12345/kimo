//! v7.5 feature: style_captions_batch -- apply one style to every caption GROUP in the project (or a named
//! list of them) in one call. set_caption_style already applies a style to every CUE inside one caption
//! group (its own description: "Apply caption preset to every cue"); a project can hold more than one
//! caption GROUP (one add_captions call makes one group, another call -- another speaker, another track,
//! a caption_from_words pass -- makes a second), and restyling all of them today is one set_caption_style
//! call per group. style_captions_batch reuses set_caption_style itself (dispatch_nested, one call per
//! group) rather than reimplementing the per-cue styling math.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(ss: &mut Session, n: &str, v: Value) -> Result<Value, String> { dispatch(ss, n, &v) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 640, "height": 360, "fps": 25}));
    ss
}

/// A caption group on its own track, from inline cues (no file needed), styled "subtitle" (the default
/// when style is omitted -- same default add_captions/add_caption_cues already apply).
fn add_caption_group(ss: &mut Session, text: &str, start: f64) -> String {
    let t = c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let cues = json!([{"start": start, "end": start + 2.0, "text": text}]);
    c(ss, "add_caption_cues", json!({"track_id": t, "cues": cues}))["caption_id"].as_str().unwrap().to_string()
}

fn group_style(ss: &Session, caption_id: &str) -> String { ss.project.editorial.captions.get(caption_id).unwrap().style.clone() }

#[test]
fn styles_every_caption_group_in_the_project_when_none_are_named() {
    let mut ss = new_proj();
    let g1 = add_caption_group(&mut ss, "hello there", 0.0);
    let g2 = add_caption_group(&mut ss, "general kenobi", 3.0);
    assert_eq!(group_style(&ss, &g1), "subtitle");
    let r = c(&mut ss, "style_captions_batch", json!({"style": "boxed"}));
    assert_eq!(r["count"], 2, "{r}");
    assert_eq!(group_style(&ss, &g1), "boxed");
    assert_eq!(group_style(&ss, &g2), "boxed");
}

#[test]
fn styles_only_the_named_groups_when_caption_ids_is_given() {
    let mut ss = new_proj();
    let g1 = add_caption_group(&mut ss, "styled", 0.0);
    let g2 = add_caption_group(&mut ss, "left alone", 3.0);
    let r = c(&mut ss, "style_captions_batch", json!({"caption_ids": [g1], "style": "yellow"}));
    assert_eq!(r["count"], 1, "{r}");
    assert_eq!(group_style(&ss, &g1), "yellow");
    assert_eq!(group_style(&ss, &g2), "subtitle", "the un-named group must be left alone");
}

#[test]
fn size_and_y_overrides_pass_through_to_every_styled_group() {
    let mut ss = new_proj();
    let g1 = add_caption_group(&mut ss, "a", 0.0);
    let g2 = add_caption_group(&mut ss, "b", 3.0);
    c(&mut ss, "style_captions_batch", json!({"style": "boxed", "size": 40.0, "y": 300.0}));
    for g in [&g1, &g2] {
        let cue_ids = ss.project.editorial.captions.get(g.as_str()).unwrap().clips.clone();
        for cid in cue_ids {
            let clip = ss.project.find_clip(&cid).unwrap();
            let y = clip.props.get("y").map(|p| p.base);
            assert_eq!(y, Some(300.0), "caption cue should carry the overridden y");
        }
    }
}

#[test]
fn an_empty_project_with_no_captions_and_no_caption_ids_is_a_harmless_no_op() {
    let mut ss = new_proj();
    let r = c(&mut ss, "style_captions_batch", json!({"style": "boxed"}));
    assert_eq!(r["count"], 0, "{r}");
}

#[test]
fn errors_on_an_unknown_named_caption_id() {
    let mut ss = new_proj();
    add_caption_group(&mut ss, "real", 0.0);
    let err = try_c(&mut ss, "style_captions_batch", json!({"caption_ids": ["nope"], "style": "boxed"})).unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn errors_on_an_unknown_style_name() {
    let mut ss = new_proj();
    add_caption_group(&mut ss, "real", 0.0);
    assert!(try_c(&mut ss, "style_captions_batch", json!({"style": "not_a_real_style"})).is_err());
}

#[test]
fn style_captions_batch_is_undoable_in_one_step() {
    let mut ss = new_proj();
    let g1 = add_caption_group(&mut ss, "a", 0.0);
    let g2 = add_caption_group(&mut ss, "b", 3.0);
    c(&mut ss, "style_captions_batch", json!({"style": "yellow"}));
    assert_eq!(group_style(&ss, &g1), "yellow");
    assert_eq!(group_style(&ss, &g2), "yellow");
    c(&mut ss, "undo", json!({}));
    assert_eq!(group_style(&ss, &g1), "subtitle", "undo should restore both groups in one step");
    assert_eq!(group_style(&ss, &g2), "subtitle");
}

#[test]
fn an_empty_caption_ids_array_styles_nothing_not_everything() {
    let mut ss = new_proj();
    let g1 = add_caption_group(&mut ss, "a", 0.0);
    let r = c(&mut ss, "style_captions_batch", json!({"caption_ids": [], "style": "boxed"}));
    assert_eq!(r["count"], 0, "{r}: an explicit empty list means style nothing, not fall back to all");
    assert_eq!(group_style(&ss, &g1), "subtitle");
}
