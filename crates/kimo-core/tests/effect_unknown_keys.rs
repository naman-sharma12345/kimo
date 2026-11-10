//! Naman (task brief, item b): an add_effect key the effect does not read was ignored in silence. Example on main 05cfbaab and on
//! v51/silent-input up to 00a9b2f: add_effect chroma_key strings {"screen":"green"} -> Ok, nothing keyed. A typo in params
//! ({"tolerence":0.5}) was stored and never read. Both must now be refused with an error naming the tool, the effect and the key.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn session() -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":32,"height":18,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let clip = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0,1,0],"start":0,"duration":1}))["clip_id"].clone();
    (s, clip)
}

fn effects_on(s: &Session) -> usize { s.project.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.effects.len()).sum() }

#[test]
fn unknown_effect_keys_are_refused_naming_tool_effect_and_key() {
    let (mut s, clip) = session();
    let r = dispatch(&mut s, "add_effect", &json!({"clip_id":clip,"kind":"chroma_key","strings":{"screen":"green"}}));
    let e = r.expect_err("chroma_key strings {\"screen\":\"green\"} must be refused, not ignored");
    eprintln!("strings screen -> {e}");
    assert!(e.contains("add_effect") && e.contains("chroma_key") && e.contains("'screen'"), "{e}");
    let r = dispatch(&mut s, "add_effect", &json!({"clip_id":clip,"kind":"chroma_key","params":{"tolerence":0.5}}));
    let e = r.expect_err("chroma_key params {\"tolerence\":0.5} (typo) must be refused, not stored unread");
    eprintln!("params tolerence -> {e}");
    assert!(e.contains("add_effect") && e.contains("chroma_key") && e.contains("'tolerence'") && e.contains("tolerance"), "{e}");
    let r = dispatch(&mut s, "add_effect", &json!({"clip_id":clip,"kind":"blur","params":{"radius":2,"amount":1}}));
    let e = r.expect_err("blur params amount must be refused (blur reads radius only)");
    eprintln!("blur amount -> {e}");
    assert!(e.contains("'amount'") && e.contains("blur"), "{e}");
    assert_eq!(effects_on(&s), 0, "a refused add_effect must not add an effect");
}

#[test]
fn known_effect_keys_still_work() {
    let (mut s, clip) = session();
    c(&mut s, "add_effect", json!({"clip_id":clip,"kind":"chroma_key","params":{"g":1,"tolerance":0.4,"softness":0.1}}));
    c(&mut s, "add_effect", json!({"clip_id":clip,"kind":"chroma_key"}));
    c(&mut s, "add_effect", json!({"clip_id":clip,"kind":"pixel_expr","strings":{"expr":"vec4(r,g,b,a)"}}));
    c(&mut s, "add_effect", json!({"clip_id":clip,"kind":"exposure","strings":{"mask_layer":""}}));
    assert_eq!(effects_on(&s), 4);
}
