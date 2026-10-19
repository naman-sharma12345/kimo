//! Naman m:796d7bd1 (follow-up to fix 2, 92222e0): every hex form a person would type for a colour draws that colour.
//! Short "#fff"/"#FFF", upper case "#FFFFFF", 8-digit "#FFFFFFFF" / "#ff000080", and a value with a stray leading or
//! trailing space (" #ffffff", "#ffffff "). On main 05cfbaab every hex string silently drew black; at 92222e0 the
//! spaced forms were refused. The 8-digit alpha is kept where kimo has an alpha for that argument (shape fill, text
//! colour); a solid clip has no alpha, exactly as an [r,g,b,a] array on a solid.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};

fn solid(color: Value) -> Result<Vec<f32>, String> {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":64,"height":36,"fps":24}))?;
    let t = dispatch(&mut s, "add_track", &json!({}))?["track_id"].clone();
    dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"solid","color":color,"start":0,"duration":1}))?;
    Ok(render_frame(&s.project, 0.5, &Cache::new())?.d)
}
/// A 300x200 rect (covers the 64x36 canvas) with the given fill, over a black solid.
fn shape(fill: Value) -> Result<Vec<f32>, String> {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":64,"height":36,"fps":24}))?;
    let t0 = dispatch(&mut s, "add_track", &json!({}))?["track_id"].clone();
    dispatch(&mut s, "add_clip", &json!({"track_id":t0,"source_type":"solid","color":[0,0,0],"start":0,"duration":1}))?;
    let t1 = dispatch(&mut s, "add_track", &json!({}))?["track_id"].clone();
    dispatch(&mut s, "add_shape", &json!({"track_id":t1,"shape":"rect","fill":fill,"start":0,"duration":1}))?;
    Ok(render_frame(&s.project, 0.5, &Cache::new())?.d)
}
fn every_px(px: &[f32], want: [f32; 4]) -> usize { px.chunks_exact(4).filter(|p| p[0] != want[0] || p[1] != want[1] || p[2] != want[2] || p[3] != want[3]).count() }

#[test]
fn hex_short_upper_and_8_digit_forms_draw_their_colour_on_a_solid() {
    for (h, want) in [("#fff", [1.0, 1.0, 1.0, 1.0]), ("#FFF", [1.0, 1.0, 1.0, 1.0]), ("#FFFFFF", [1.0, 1.0, 1.0, 1.0]), ("#FFFFFFFF", [1.0, 1.0, 1.0, 1.0]), ("#ff000080", [1.0, 0.0, 0.0, 1.0])] {
        let px = solid(json!(h)).unwrap_or_else(|e| panic!("{h} must be accepted: {e}"));
        let off = every_px(&px, want);
        eprintln!("solid {h:>10}: first px {:?}, want {want:?}, off {off}/{}", &px[..4], px.len() / 4);
        assert_eq!(off, 0, "solid {h}: every pixel must be {want:?}");
    }
}

#[test]
fn hex_8_digit_alpha_is_kept_on_a_shape_fill() {
    let hex = shape(json!("#ff000080")).unwrap_or_else(|e| panic!("#ff000080 fill must be accepted: {e}"));
    let arr = shape(json!([1.0, 0.0, 0.0, 128.0 / 255.0])).unwrap();
    eprintln!("shape fill #ff000080 over black: first px {:?}; [1,0,0,128/255] gives {:?}", &hex[..4], &arr[..4]);
    assert!(hex[0] > 0.0 && hex[0] < 1.0 && hex[1] == 0.0 && hex[2] == 0.0, "half-transparent red over black is a part red: {:?}", &hex[..4]);
    assert_eq!(hex, arr, "#ff000080 fill == [1,0,0,128/255] fill, pixel for pixel");
    let opaque = shape(json!("#FFFFFFFF")).unwrap();
    assert_eq!(opaque, shape(json!([1, 1, 1])).unwrap(), "#FFFFFFFF fill == opaque white");
}

#[test]
fn hex_with_a_leading_or_trailing_space_draws_its_colour() {
    for h in [" #ffffff", "#ffffff ", " #FFF ", "\t#ffffff\n"] {
        let px = solid(json!(h)).unwrap_or_else(|e| panic!("{h:?} must be accepted (spaces trimmed): {e}"));
        let off = every_px(&px, [1.0, 1.0, 1.0, 1.0]);
        eprintln!("solid {h:?}: first px {:?}, off {off}", &px[..4]);
        assert_eq!(off, 0, "{h:?}: every pixel white");
    }
    // trimming does not open the door to non-colours
    for bad in [" white ", "# ffffff", "#ff ffff", " "] {
        assert!(solid(json!(bad)).is_err(), "{bad:?} must still be refused");
    }
}
