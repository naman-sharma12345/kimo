//! v7.4: track_mute_solo -- `muted` has existed on Track since before v7 and every audio-render and
//! analysis path already honoured it, but no tool could ever SET it (set_track's schema only took
//! `locked`/`name`/`index`) -- a track could never actually be muted. This closes that gap and adds the
//! standard NLE companion, `solo` (Premiere/Resolve/Reaper semantics: when any track in the project is
//! soloed, every non-soloed track is silent regardless of its own `muted` flag; with nothing soloed, a
//! track's own `muted` flag is the whole answer). Old project JSON lacking both fields still loads (serde
//! default = false for each), so this is purely additive.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn setup_two_track_project() -> (Session, String, String) {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (ss, t1, t2)
}

#[test] fn set_track_can_actually_mute_a_track_and_it_sticks_on_the_project() {
    let (mut ss, t1, _t2) = setup_two_track_project();
    let r = c(&mut ss, "set_track", json!({"track_id": t1, "muted": true}));
    assert_eq!(r["muted"], true, "{r}");
    let p = c(&mut ss, "get_project", json!({}));
    let tr = p["tracks"].as_array().unwrap().iter().find(|t| t["id"] == t1).unwrap();
    assert_eq!(tr["muted"], true, "{tr}");
}

#[test] fn set_track_can_unmute_again() {
    let (mut ss, t1, _t2) = setup_two_track_project();
    c(&mut ss, "set_track", json!({"track_id": t1, "muted": true}));
    let r = c(&mut ss, "set_track", json!({"track_id": t1, "muted": false}));
    assert_eq!(r["muted"], false, "{r}");
}

#[test] fn set_track_can_solo_a_track_independent_of_muted() {
    let (mut ss, t1, _t2) = setup_two_track_project();
    let r = c(&mut ss, "set_track", json!({"track_id": t1, "solo": true}));
    assert_eq!(r["solo"], true, "{r}");
    assert_eq!(r["muted"], false, "{r}");
}

#[test] fn solo_on_one_track_effectively_mutes_every_other_track_regardless_of_their_own_muted_flag() {
    let (mut ss, t1, t2) = setup_two_track_project();
    // t2 is explicitly un-muted, but once t1 is soloed, t2 must be silent anyway.
    c(&mut ss, "set_track", json!({"track_id": t2, "muted": false}));
    c(&mut ss, "set_track", json!({"track_id": t1, "solo": true}));
    let any_solo = true; let _ = any_solo;
    // Verified through the real effective_mute helper rather than re-deriving the rule here.
    let p_val = c(&mut ss, "get_project", json!({}));
    let tracks: Vec<kimo_core::model::Track> = serde_json::from_value(p_val["tracks"].clone()).unwrap();
    let t2_track = tracks.iter().find(|t| t.id == t2).unwrap();
    let t1_track = tracks.iter().find(|t| t.id == t1).unwrap();
    assert!(kimo_core::model::effective_mute(&tracks, t2_track), "non-soloed track must be effectively muted when another track is soloed");
    assert!(!kimo_core::model::effective_mute(&tracks, t1_track), "the soloed track itself must not be effectively muted");
}

#[test] fn with_nothing_soloed_effective_mute_is_just_the_tracks_own_muted_flag() {
    let (mut ss, t1, t2) = setup_two_track_project();
    c(&mut ss, "set_track", json!({"track_id": t1, "muted": true}));
    let p_val = c(&mut ss, "get_project", json!({}));
    let tracks: Vec<kimo_core::model::Track> = serde_json::from_value(p_val["tracks"].clone()).unwrap();
    let t1_track = tracks.iter().find(|t| t.id == t1).unwrap();
    let t2_track = tracks.iter().find(|t| t.id == t2).unwrap();
    assert!(kimo_core::model::effective_mute(&tracks, t1_track));
    assert!(!kimo_core::model::effective_mute(&tracks, t2_track));
}

#[test] fn soloing_a_track_that_is_also_marked_muted_still_plays_because_solo_wins() {
    let (mut ss, t1, _t2) = setup_two_track_project();
    c(&mut ss, "set_track", json!({"track_id": t1, "muted": true, "solo": true}));
    let p_val = c(&mut ss, "get_project", json!({}));
    let tracks: Vec<kimo_core::model::Track> = serde_json::from_value(p_val["tracks"].clone()).unwrap();
    let t1_track = tracks.iter().find(|t| t.id == t1).unwrap();
    assert!(!kimo_core::model::effective_mute(&tracks, t1_track), "solo overrides this track's own muted flag");
}

#[test] fn muting_a_track_with_a_media_clip_actually_silences_it_in_the_rendered_audio_mix() {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let dir = std::env::temp_dir().join(format!("kimo_mutesolo_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir); std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("tone.wav");
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-ar", "48000"]).arg(&wav).status().unwrap();
    assert!(status.success());
    let asset = c(&mut ss, "import_media", json!({"path": wav.to_str().unwrap()}));
    let asset_id = asset["asset_id"].as_str().unwrap().to_string();
    let tr = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": asset_id, "start": 0.0, "duration": 1.0}));
    let loud_before = kimo_core::audio::mix(&ss.project).iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(loud_before > 0.01, "sanity: the tone should be audible before muting, peak={loud_before}");
    c(&mut ss, "set_track", json!({"track_id": tr, "muted": true}));
    let loud_after = kimo_core::audio::mix(&ss.project).iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(loud_after < 1e-6, "muted track must be silent in the mix, peak={loud_after}");
}

#[test] fn an_old_project_json_with_neither_field_still_loads_with_both_false() {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    c(&mut ss, "add_track", json!({}));
    let dir = std::env::temp_dir().join(format!("kimo_mutesolo_old_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir); std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("old.kimo");
    // Hand-write a project JSON shaped like a pre-mute-solo file: no "muted", no "solo" key on the track at all.
    let mut v = serde_json::to_value(&ss.project).unwrap();
    for t in v["tracks"].as_array_mut().unwrap() { t.as_object_mut().unwrap().remove("muted"); t.as_object_mut().unwrap().remove("solo"); }
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();
    let r = c(&mut ss, "load_project", json!({"path": path.to_str().unwrap()}));
    assert!(r.get("error").is_none(), "{r:?}");
    let p = c(&mut ss, "get_project", json!({}));
    assert_eq!(p["tracks"][0]["muted"], false);
    assert_eq!(p["tracks"][0]["solo"], false);
}


