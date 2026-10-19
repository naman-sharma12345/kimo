//! Naman m:48b79971: a colour array of the wrong length ([1,1], five values, []) must ERROR, naming the key and the
//! length ("color: expected 3 or 4 numbers, got 2"), not fall back silently to the default colour. A non-number element
//! (it used to read as 0) is refused the same way. On main 05cfbaab, and on this branch up to a539345, [1,1] was accepted.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn session() -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    (s, t)
}

#[test]
fn wrong_length_colour_arrays_error_naming_key_and_length() {
    let (mut s, t) = session();
    for (arr, n) in [(json!([1, 1]), 2), (json!([1, 0, 0, 1, 1]), 5), (json!([]), 0), (json!([0.5]), 1)] {
        let r = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"solid","color":arr,"start":0,"duration":1}));
        let e = r.expect_err(&format!("solid color {arr} must be refused, not drawn as the default colour"));
        eprintln!("solid color {arr} -> {e}");
        assert!(e.contains(&format!("color: expected 3 or 4 numbers, got {n}")), "{e}");
    }
    let r = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"gradient","color":[0,0,0],"color2":[1,1],"start":0,"duration":1}));
    let e = r.expect_err("gradient color2 [1,1] must be refused");
    eprintln!("gradient color2 [1,1] -> {e}");
    assert!(e.contains("color2: expected 3 or 4 numbers, got 2"), "{e}");
    let r = dispatch(&mut s, "add_text", &json!({"track_id":t,"text":"H","font":"DejaVuSans","size":12,"color":[1,1],"start":0,"duration":1}));
    let e = r.expect_err("text color [1,1] must be refused");
    eprintln!("text color [1,1] -> {e}");
    assert!(e.contains("color: expected 3 or 4 numbers, got 2"), "{e}");
    let r = dispatch(&mut s, "add_text", &json!({"track_id":t,"text":"H","font":"DejaVuSans","size":12,"color":[1,1,1],"stroke_color":[0,0,0,1,1,1],"start":0,"duration":1}));
    let e = r.expect_err("text stroke_color with 6 values must be refused");
    eprintln!("text stroke_color 6 values -> {e}");
    assert!(e.contains("stroke_color: expected 3 or 4 numbers, got 6"), "{e}");
    let r = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"solid","color":["a",1,1],"start":0,"duration":1}));
    let e = r.expect_err("a non-number element must be refused, not read as 0");
    eprintln!("solid color [\"a\",1,1] -> {e}");
    assert!(e.contains("color: expected 3 or 4 numbers, element 0 is \"a\""), "{e}");
}

#[test]
fn right_length_colours_still_work() {
    let (mut s, t) = session();
    for col in [json!([1, 0, 0]), json!([1, 0, 0, 0.5]), json!("#ff0000"), Value::Null] {
        c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":col,"start":0,"duration":1}));
    }
    c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","start":1,"duration":1}));
    c(&mut s, "add_text", json!({"track_id":t,"text":"H","font":"DejaVuSans","size":12,"color":[1,1,1,1],"stroke_color":[0,0,0],"start":0,"duration":1}));
}
