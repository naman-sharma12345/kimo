//! (c) colour schemas: the 16 colour properties in 9 tools said {"type":"array"} only, while every colour reader takes hex too
//! (92222e0, c9be19d). Anything that validates against the schema refused hex: kimo's own batch pre-check (ext/batch.rs check_args)
//! answered "argument 'color' must be a array" for "#ff0000". The schemas now say [r,g,b] / [r,g,b,a] 0..1 or hex.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

#[test]
fn batch_accepts_hex_colours_on_every_colour_property() {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":32,"height":18,"fps":25})).unwrap();
    let ops = json!([
        {"id":"t","tool":"add_track"},
        {"id":"s","tool":"add_clip","args":{"track_id":"$t.track_id","source_type":"solid","color":"#ff0000","start":0,"duration":1}},
        {"id":"g","tool":"add_clip","args":{"track_id":"$t.track_id","source_type":"gradient","color":"#000","color2":"#ffffff80","start":1,"duration":1}},
        {"id":"x","tool":"add_text","args":{"track_id":"$t.track_id","text":"hi","color":"#ffffff","stroke_color":"#000000","start":2,"duration":1}},
        {"id":"st","tool":"set_text_style","args":{"clip_id":"$x.clip_id","bg":"#00000080","shadow_color":"#111","glow_color":"#00ff00"}},
        {"id":"sh","tool":"add_shape","args":{"track_id":"$t.track_id","shape":"rect","width":4,"height":4,"fill":"#00ff0080","stroke_color":"#fff","start":3,"duration":1}},
        {"id":"tr","tool":"add_transition","args":{"clip_id":"$sh.clip_id","kind":"flash","duration":0.5,"color":"#ffcc00"}}
    ]);
    let r = dispatch(&mut s, "batch", &json!({"ops": ops}));
    assert!(r.is_ok(), "batch with hex colours must pass the schema pre-check: {}", r.unwrap_err());
    let px = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    assert_eq!(&px[..4], &[1.0, 0.0, 0.0, 1.0], "the hex solid draws red");
}
