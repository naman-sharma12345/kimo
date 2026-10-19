//! v5.x bug (HEX_COLOR_BUG_v5x.md, 2026-10-17): add_clip solid with "color":"#ffffff" was accepted and rendered
//! black, because the shared colour reader took only arrays and fell back to the default. Hex now means the colour
//! it names; anything else that is not a colour is refused with an error naming the argument.
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

/// Naman m:408fe2ee: black, white and a mid grey each render their exact colour (x/255 in the working buffer).
#[test]
fn hex_black_white_grey_render_their_exact_colour() {
    for (h, v) in [("#000000", 0u8), ("#ffffff", 255), ("#808080", 128)] {
        let px = solid(json!(h)).unwrap_or_else(|e| panic!("{h}: {e}"));
        let want = (v as f64 / 255.0) as f32;
        let off = px.chunks_exact(4).filter(|p| p[0] != want || p[1] != want || p[2] != want || p[3] != 1.0).count();
        eprintln!("{h}: {} px, want rgb {want}, first px {:?}, off {off}", px.len() / 4, &px[..4]);
        assert_eq!(off, 0, "{h}: every pixel is rgb {want}, alpha 1");
        assert_eq!(px, solid(json!([v as f64 / 255.0, v as f64 / 255.0, v as f64 / 255.0])).unwrap(), "{h} == its [r,g,b] array");
    }
}

#[test]
fn hex_white_solid_renders_white_not_black() {
    let hex = solid(json!("#ffffff")).expect("hex white is accepted");
    let arr = solid(json!([1, 1, 1])).unwrap();
    let black = hex.chunks_exact(4).filter(|p| p[0] == 0.0 && p[1] == 0.0 && p[2] == 0.0).count();
    eprintln!("#ffffff: {} px, {} black, first px {:?}", hex.len() / 4, black, &hex[..4]);
    assert_eq!(black, 0, "no black pixel in a #ffffff solid");
    assert_eq!(hex, arr, "#ffffff renders bit-identical to [1,1,1]");
}

#[test]
fn hex_forms_match_their_arrays() {
    for (h, a) in [("#3366cc", json!([0.2, 0.4, 0.8])), ("#36c", json!([0.2, 0.4, 0.8])), ("#3366CC", json!([0.2, 0.4, 0.8])), ("#ff000080", json!([1, 0, 0])), ("#000", json!([0, 0, 0]))] {
        assert_eq!(solid(json!(h)).unwrap_or_else(|e| panic!("{h}: {e}")), solid(a).unwrap(), "{h}");
    }
}

#[test]
fn non_colours_are_refused_and_name_the_argument() {
    for bad in [json!("white"), json!("#12345"), json!("#gggggg"), json!("ffffff"), json!(7), json!({"r":1})] {
        let e = solid(bad.clone()).expect_err(&format!("{bad} must be refused"));
        assert!(e.contains("color"), "error names the argument for {bad}: {e}");
    }
    let mut s = Session::new();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let e = dispatch(&mut s, "add_shape", &json!({"track_id":t,"shape":"rect","fill":"white","start":0,"duration":1})).map(|_| ()).unwrap_err();
    assert!(e.contains("fill"), "shape fill error names the argument: {e}");
}

#[test]
fn absent_colour_keeps_its_default() {
    assert_eq!(solid(Value::Null).unwrap(), solid(json!([0, 0, 0])).unwrap(), "no color -> the solid default stays black");
}
