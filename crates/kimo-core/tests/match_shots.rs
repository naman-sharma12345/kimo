//! match_shots (Resolve Shot Match to many clips): match every clip of a timeline to one hero clip in one call. "balance"
//! (default) takes the hero's neutral cast (the neutral cluster, as auto_color measures it) and its tone levels (Rec.709 luma
//! 2/50/98th percentiles) onto each clip as one editable primaries (model 1) effect; "lab" adds a full Lab covariance
//! lab_match like match_grade. Re-running replaces the earlier match effects (never stacks); strength 0 is an identity.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn png(ss: &mut Session, t: f64, tag: &str) -> Vec<[f32; 3]> {
    let path = std::env::temp_dir().join(format!("kimo-match-shots-{tag}-{}.png", std::process::id()));
    call(ss, "render_frame", json!({"time": t, "path": path, "inline": "never"}));
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&path).args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).output().unwrap(); let _ = std::fs::remove_file(&path);
    assert!(out.status.success()); out.stdout.chunks_exact(3).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]).collect() }
fn mean(px: &[[f32; 3]]) -> [f32; 3] { let mut m = [0.0f32; 3]; for c in px { for j in 0..3 { m[j] += c[j]; } } m.map(|v| v / px.len() as f32) }
fn luma_pct(px: &[[f32; 3]], q: f32) -> f32 { let mut y: Vec<f32> = px.iter().map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).collect(); y.sort_by(|a, b| a.total_cmp(b)); y[((y.len() - 1) as f32 * q) as usize] }
fn clip(ss: &mut Session, tr: &str, start: f64, c1: [f64; 3], c2: [f64; 3]) -> String {
    call(ss, "add_clip", json!({"track_id": tr, "source_type": "gradient", "color": c1, "color2": c2, "start": start, "duration": 1}))["clip_id"].as_str().unwrap().to_string() }
/// hero: neutral grey gradient; b: the same scene warmer and a stop darker; c: cooler and flatter
fn scene() -> (Session, String, String, String) {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 48, "height": 96, "fps": 25}));
    let tr = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let h = clip(&mut ss, &tr, 0.0, [0.04, 0.04, 0.04], [0.92, 0.92, 0.92]);
    let b = clip(&mut ss, &tr, 1.0, [0.03, 0.02, 0.012], [0.62, 0.45, 0.28]);
    let c = clip(&mut ss, &tr, 2.0, [0.20, 0.24, 0.30], [0.55, 0.66, 0.80]);
    (ss, h, b, c) }
fn cast(m: [f32; 3]) -> f32 { (m[0] - m[2]).abs() + (m[1] - 0.5 * (m[0] + m[2])).abs() }

#[test] fn match_shots_brings_every_clip_to_the_hero() {
    let (mut ss, h, b, c) = scene(); let (hp, bp, cp) = (png(&mut ss, 0.5, "h0"), png(&mut ss, 1.5, "b0"), png(&mut ss, 2.5, "c0"));
    assert!(cast(mean(&bp)) > 0.15 && cast(mean(&cp)) > 0.1, "the fixture starts mismatched");
    let r = call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b, c]}));
    assert_eq!(r["matched"].as_array().map(|a| a.len()), Some(2), "{r}");
    for (id, tag, t, before) in [(&b, "b1", 1.5, &bp), (&c, "c1", 2.5, &cp)] { let o = png(&mut ss, t, tag);
        assert!(cast(mean(&o)) < 0.035, "{id}: cast {} -> {} (hero {})", cast(mean(before)), cast(mean(&o)), cast(mean(&hp)));
        for q in [0.02f32, 0.5, 0.98] { assert!((luma_pct(&o, q) - luma_pct(&hp, q)).abs() < 0.04, "{id}: luma p{q} {} -> {} (hero {})", luma_pct(before, q), luma_pct(&o, q), luma_pct(&hp, q)); } }
    let hero_after = png(&mut ss, 0.5, "h1"); assert_eq!(hero_after, hp, "the hero is the reference and is not touched");
    let proj = call(&mut ss, "get_project", json!({})).to_string(); assert!(proj.contains("primaries") && proj.contains("matched_to"), "an editable primaries effect tagged with its hero");
}

#[test] fn rerun_replaces_and_strength_scales() {
    let (mut ss, h, b, _c) = scene(); let before = png(&mut ss, 1.5, "s0");
    call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b]})); let first = png(&mut ss, 1.5, "s1");
    call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b]})); let again = png(&mut ss, 1.5, "s2");
    assert_eq!(first, again, "re-running replaces the earlier match: same picture");
    let n = call(&mut ss, "get_project", json!({})).to_string().matches("matched_to").count(); assert_eq!(n, 1, "one match effect, not stacked");
    call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b], "strength": 0})); assert_eq!(png(&mut ss, 1.5, "s3"), before, "strength 0 is an identity");
    call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b], "strength": 0.5})); let half = mean(&png(&mut ss, 1.5, "s4"));
    let (m0, m1) = (mean(&before), mean(&first)); assert!(cast(half) < cast(m0) && cast(half) > cast(m1), "strength 0.5 lands between: {} {} {}", cast(m0), cast(half), cast(m1));
    let r = call(&mut ss, "match_shots", json!({"hero_clip_id": h, "clip_ids": [b], "mode": "lab"}));
    assert!(r.to_string().contains("lab_match"), "lab mode adds lab_match: {r}"); assert_eq!(call(&mut ss, "get_project", json!({})).to_string().matches("matched_to").count(), 1);
}

#[test] fn default_set_is_every_other_video_clip() {
    let dir = std::env::temp_dir(); let (a, w) = (dir.join(format!("kimo-ms-a-{}.mp4", std::process::id())), dir.join(format!("kimo-ms-w-{}.mp4", std::process::id())));
    for (p, f) in [(&a, "null"), (&w, "colorchannelmixer=rr=1.1:gg=0.85:bb=0.6")] {
        assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=64x64:rate=25:duration=1", "-vf", f, "-pix_fmt", "yuv420p"]).arg(p).status().unwrap().success()); }
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let tr = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let mut ids = vec![];
    for (p, s) in [(&a, 0.0), (&w, 1.0)] { let asset = call(&mut ss, "import_media", json!({"path": p}))["asset_id"].as_str().unwrap().to_string();
        ids.push(call(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": asset, "start": s, "duration": 1}))["clip_id"].as_str().unwrap().to_string()); }
    let t2 = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let solid = call(&mut ss, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [0.9, 0.1, 0.1], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    let r = call(&mut ss, "match_shots", json!({"hero_clip_id": ids[0]})); let got: Vec<String> = r["matched"].as_array().unwrap().iter().map(|m| m["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(got, vec![ids[1].clone()], "the default set is every other video clip (generators like {solid} are left alone): {r}");
    let _ = std::fs::remove_file(&a); let _ = std::fs::remove_file(&w);
}

#[test] fn bad_calls_are_refused_and_nothing_changes() {
    let (mut ss, h, b, _c) = scene(); let before = call(&mut ss, "get_project", json!({})).to_string();
    for a in [json!({"hero_clip_id": "nope"}), json!({"hero_clip_id": h, "clip_ids": [h]}), json!({"hero_clip_id": h, "clip_ids": [b, "ghost"]}), json!({"hero_clip_id": h, "clip_ids": [b], "strength": 1.5}),
        json!({"hero_clip_id": h, "clip_ids": [b], "mode": "magic"}), json!({"hero_clip_id": h, "clip_ids": []})] {
        assert!(dispatch(&mut ss, "match_shots", &a).is_err(), "{a} must be refused"); assert_eq!(call(&mut ss, "get_project", json!({})).to_string(), before, "{a}: project unchanged"); }
}
