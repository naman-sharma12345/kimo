//! v7.2 additions: check_color_consistency -- the read-only diagnosis that match_color (one clip) and
//! match_shots (every clip, mutating) have no counterpart for: which clips in a group actually disagree
//! in brightness/color before an agent reaches for either fix. No new pixel analysis -- each clip's own
//! midpoint is sampled through the existing get_scopes (dispatch_nested).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).unwrap_err() }

fn solid_fixture(name: &str, color: &str, secs: f64) -> String {
    let p = format!("/tmp/kimo-colorconsist-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c={color}:size=64x48:rate=25:duration={secs}"), "-pix_fmt", "yuv420p", &p])
        .status().unwrap().success());
    p
}
fn base() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 48, "fps": 25})); ss }
fn import(ss: &mut Session, path: &str) -> String { c(ss, "import_media", json!({"path": path}))["asset_id"].as_str().unwrap().to_string() }
fn clip(ss: &mut Session, t: &str, aid: &str, start: f64, dur: f64) -> String { c(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aid, "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string() }

#[test] fn no_clips_reports_ok_empty_not_an_error() {
    let mut ss = base();
    let r = c(&mut ss, "check_color_consistency", json!({}));
    assert_eq!(r["ok"], true, "{r}"); assert_eq!(r["checked"], 0, "{r}"); assert_eq!(r["clips"], json!([]));
}

#[test] fn three_gray_clips_agree_with_each_other_by_default() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let mut paths = vec![];
    for (i, s) in ["gray", "gray", "gray"].iter().enumerate() {
        let p = solid_fixture(&format!("agree{i}"), s, 1.0); let a = import(&mut ss, &p);
        clip(&mut ss, &t, &a, i as f64, 1.0);
        paths.push(p); // get_scopes decodes lazily at check time, not at import -- keep the files until after
    }
    let r = c(&mut ss, "check_color_consistency", json!({}));
    assert_eq!(r["ok"], true, "{r}"); assert_eq!(r["checked"], 3); assert_eq!(r["flagged"], 0, "{r}");
    for p in paths { std::fs::remove_file(&p).ok(); }
}

#[test] fn one_clip_far_off_from_the_rest_is_flagged_and_the_others_are_not() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let mut ids = vec![]; let mut paths = vec![];
    for (i, s) in ["gray", "gray", "white", "gray"].iter().enumerate() { // index 2 is the odd one out
        let p = solid_fixture(&format!("oddone{i}"), s, 1.0); let a = import(&mut ss, &p);
        ids.push(clip(&mut ss, &t, &a, i as f64, 1.0));
        paths.push(p);
    }
    let r = c(&mut ss, "check_color_consistency", json!({}));
    assert_eq!(r["flagged"], 1, "{r}");
    let flagged_ids: Vec<&str> = r["clips"].as_array().unwrap().iter().filter(|c| c["flagged"] == true).map(|c| c["clip_id"].as_str().unwrap()).collect();
    assert_eq!(flagged_ids, vec![ids[2].as_str()], "{r}");
    for p in paths { std::fs::remove_file(&p).ok(); }
}

#[test] fn ref_clip_id_compares_everyone_against_the_named_hero_shot() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p0 = solid_fixture("hero", "gray", 1.0); let a0 = import(&mut ss, &p0);
    let hero = clip(&mut ss, &t, &a0, 0.0, 1.0);
    let p1 = solid_fixture("agrees", "gray", 1.0); let a1 = import(&mut ss, &p1);
    let agrees = clip(&mut ss, &t, &a1, 1.0, 1.0);
    let p2 = solid_fixture("disagrees", "white", 1.0); let a2 = import(&mut ss, &p2);
    let disagrees = clip(&mut ss, &t, &a2, 2.0, 1.0);
    let r = c(&mut ss, "check_color_consistency", json!({"ref_clip_id": hero}));
    assert!(r["reference"].as_str().unwrap().contains(&hero), "{r}");
    let get_flag = |id: &str| -> bool { r["clips"].as_array().unwrap().iter().find(|c| c["clip_id"] == id).unwrap()["flagged"].as_bool().unwrap() };
    assert!(!get_flag(&hero), "{r}"); assert!(!get_flag(&agrees), "{r}"); assert!(get_flag(&disagrees), "{r}");
    std::fs::remove_file(&p0).ok(); std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn unknown_ref_clip_id_errors_plainly() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p = solid_fixture("refcheck", "gray", 1.0); let a = import(&mut ss, &p);
    clip(&mut ss, &t, &a, 0.0, 1.0);
    assert!(e(&mut ss, "check_color_consistency", json!({"ref_clip_id": "nope"})).contains("not among the clips being checked"));
    std::fs::remove_file(&p).ok();
}

#[test] fn explicit_clip_ids_scopes_the_check_to_just_those_clips() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("scope1", "gray", 1.0); let a1 = import(&mut ss, &p1);
    let c1 = clip(&mut ss, &t, &a1, 0.0, 1.0);
    let p2 = solid_fixture("scope2", "white", 1.0); let a2 = import(&mut ss, &p2); // would be flagged by default
    clip(&mut ss, &t, &a2, 1.0, 1.0);
    let r = c(&mut ss, "check_color_consistency", json!({"clip_ids": [c1]}));
    assert_eq!(r["checked"], 1, "{r}"); assert_eq!(r["flagged"], 0, "{r}"); // alone, it trivially matches its own median
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn solid_color_generator_clips_are_excluded_by_default_media_only() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9,0.9,0.9], "start": 0.0, "duration": 1.0}));
    let p = solid_fixture("onlymedia", "gray", 1.0); let a = import(&mut ss, &p);
    clip(&mut ss, &t, &a, 1.0, 1.0);
    let r = c(&mut ss, "check_color_consistency", json!({}));
    assert_eq!(r["checked"], 1, "the generator clip is not a shot and should be skipped by default: {r}");
    std::fs::remove_file(&p).ok();
}

#[test] fn tolerances_are_adjustable() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("tol1", "gray", 1.0); let a1 = import(&mut ss, &p1);
    clip(&mut ss, &t, &a1, 0.0, 1.0);
    let p2 = solid_fixture("tol2", "white", 1.0); let a2 = import(&mut ss, &p2);
    clip(&mut ss, &t, &a2, 1.0, 1.0);
    let loose = c(&mut ss, "check_color_consistency", json!({"luma_tolerance": 1.0, "rgb_tolerance": 1.0}));
    assert_eq!(loose["flagged"], 0, "{loose}");
    let tight = c(&mut ss, "check_color_consistency", json!({"luma_tolerance": 0.01, "rgb_tolerance": 0.01}));
    assert!(tight["flagged"].as_u64().unwrap() >= 1, "{tight}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn is_read_only_on_the_project() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p = solid_fixture("ro", "gray", 1.0); let a = import(&mut ss, &p);
    clip(&mut ss, &t, &a, 0.0, 1.0);
    let h = serde_json::to_string(&ss.project).unwrap(); let u = ss.undo.len();
    c(&mut ss, "check_color_consistency", json!({}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), h); assert_eq!(ss.undo.len(), u);
    std::fs::remove_file(&p).ok();
}

