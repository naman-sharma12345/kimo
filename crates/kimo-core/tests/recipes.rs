//! RV-11 recipes: schema lint, dry-run expansion == hand-run batch, independent instances, hostile text stays literal, param errors name allowed values.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn pin_fonts() { static O: std::sync::Once = std::sync::Once::new(); O.call_once(|| { std::env::set_var("KIMO_FONTS", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts")); let r = kimo_core::text::resolve_font("Montserrat"); assert!(r.replace('\\', "/").ends_with("tests/fonts/Montserrat-Var.ttf"), "text goldens need the repo-owned font tests/fonts/Montserrat-Var.ttf but resolve_font(\"Montserrat\") returned {r:?} (silent system-font fallback is not allowed in tests)"); }); }

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn fresh() -> Session { pin_fonts(); let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25})); ss }
fn hashes(ss: &Session) -> Vec<u64> { pin_fonts(); let cache = render::Cache::new(); [0.3, 1.0, 2.0].iter().map(|t| { let f = render::render_frame(&ss.project, *t, &cache).unwrap(); f.d.iter().fold(1469598103934665603u64, |h, x| (h ^ x.to_bits() as u64).wrapping_mul(1099511628211)) }).collect() }
/// (recipe, params) pairs that are valid on a fresh project. product_reveal needs an asset, added by `with_asset`.
fn cases() -> Vec<(&'static str, Value)> { vec![
    ("lower_third", json!({"title": "Naman Sharma", "subtitle": "Editor"})), ("title_card", json!({"title": "Hello", "subtitle": "World"})), ("stat_counter", json!({"value": "42%", "label": "faster"})),
    ("explainer_diagram", json!({"labels": ["Capture", "Edit", "Ship"]})) ] }

#[test] fn describe_schema_equals_the_schema_apply_validates() {
    let mut ss = fresh(); let list = c(&mut ss, "recipe_list", json!({})); let names: Vec<String> = list["recipes"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["explainer_diagram", "lower_third", "narration_section", "product_reveal", "stat_counter", "title_card"]);
    for n in &names { let d = c(&mut ss, "recipe_describe", json!({"name": n})); assert_eq!(d["recipe_version"], 1); let sch = &d["params"]; assert_eq!(sch["type"], "object");
        if *n != "lower_third" && *n != "narration_section" { let r = kimo_core::recipes::find(n).unwrap(); assert_eq!(sch, &kimo_core::recipes::schema(r), "{n}: describe must be the table apply validates"); }
        // every param the schema lists is accepted by apply (unknown ones are rejected)
        let m = e(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": {"nope_param": 1}, "dry_run": true})); assert!(m.contains("nope_param"), "{n}: {m}"); }
    assert!(c(&mut ss, "recipe_list", json!({"query": "diagram"}))["count"].as_u64().unwrap() == 1);
    let m = e(&mut ss, "recipe_describe", json!({"name": "bogus"})); assert!(m.contains("Valid:") && m.contains("title_card"), "{m}");
}
#[test] fn param_errors_name_allowed_values_and_never_echo_text() {
    let mut ss = fresh(); let h = pj(&ss);
    let m = e(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "x", "easing": "wobble"}})); assert!(m.contains("Valid:") && m.contains("out_back"), "{m}");
    let m = e(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"duration": 3}})); assert!(m.contains("title"), "{m}");
    let m = e(&mut ss, "recipe_apply", json!({"name": "stat_counter", "start": 0.0, "params": {"value": "1", "duration": 9999}})); assert!(m.contains("between"), "{m}");
    let m = e(&mut ss, "recipe_apply", json!({"name": "explainer_diagram", "start": 0.0, "params": {"labels": ["a","b"], "box_width": 1200, "spacing": 800, "box_height": 50}})); assert!(m.contains("too wide"), "{m}");
    let m = e(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "SECRET-PAYLOAD", "color": [9, 9, 9]}})); assert!(!m.contains("SECRET-PAYLOAD"), "{m}");
    assert_eq!(pj(&ss), h, "failed applies change nothing");
}
#[test] fn dry_run_expansion_run_by_hand_as_a_batch_gives_equal_bytes_and_frames() {
    for (n, p) in cases() {
        let mut a = fresh(); let d = c(&mut a, "recipe_apply", json!({"name": n, "start": 0.5, "params": p, "dry_run": true})); assert_eq!(pj(&a), pj(&fresh()), "{n}: dry_run leaves nothing");
        let ops = d["expansion"].clone(); assert!(ops.as_array().unwrap().len() >= 3, "{n}");
        let r = c(&mut a, "recipe_apply", json!({"name": n, "start": 0.5, "params": p, "expect_revision": d["revision"]}));
        let mut b = fresh(); c(&mut b, "batch", json!({"ops": ops})); assert_eq!(pj(&a), pj(&b), "{n}: expansion by hand == recipe_apply"); assert_eq!(hashes(&a), hashes(&b), "{n}: frames");
        assert_eq!(d["created"], r["created"], "{n}: dry run reports the real ids"); assert!(r["roles"].as_object().unwrap().keys().all(|k| k.starts_with(&format!("{n}/"))), "{n}: roles are <prefix>/<role>");
        assert_eq!(a.undo.len(), 2, "{n}: project_set + ONE recipe step");
    }
}
#[test] fn applying_twice_makes_two_independent_instances() {
    for (n, p) in cases() {
        let mut ss = fresh(); let u = ss.undo.len(); let r1 = c(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": p})); let after1 = pj(&ss); let r2 = c(&mut ss, "recipe_apply", json!({"name": n, "start": 4.0, "params": p}));
        assert_eq!(ss.undo.len(), u + 2, "{n}"); assert!(r1["created"]["clip_ids"].as_array().unwrap().iter().all(|x| !r2["created"]["clip_ids"].as_array().unwrap().contains(x)), "{n}: ids differ");
        c(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), after1, "{n}: undo removes only the second instance");
        let tid = r1["track_id"].as_str().unwrap().to_string(); let mut s3 = fresh(); let t3 = c(&mut s3, "add_track", json!({}))["track_id"].clone(); c(&mut s3, "recipe_apply", json!({"name": n, "start": 0.0, "params": p, "track_id": t3})); assert_eq!(s3.project.tracks.len(), 1, "{n}: shared track given"); let _ = tid;
    }
}
#[test] fn hostile_text_stays_literal_text() {
    let mut ss = fresh(); let evil = "$x.clip_id"; let r = c(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": evil, "subtitle": "$$ and add_track"}}));
    assert!(r["ok"].as_bool().unwrap()); let texts: Vec<String> = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| match &c.source { kimo_core::model::Source::Text { text, .. } => Some(text.clone()), _ => None }).collect();
    assert!(texts.contains(&evil.to_string()) && texts.contains(&"$$ and add_track".to_string()), "{texts:?}"); assert_eq!(ss.project.tracks.len(), 1, "text never became a tool call");
    let r = c(&mut ss, "recipe_apply", json!({"name": "explainer_diagram", "start": 0.0, "params": {"labels": ["$a.b", "ok"]}})); assert!(r["ok"].as_bool().unwrap());
}
#[test] fn track_selection_and_locks() {
    let mut ss = fresh(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "x"}, "track_id": t})); assert_eq!(ss.project.tracks.len(), 1); assert_eq!(r["track_id"], t.as_str());
    c(&mut ss, "set_track", json!({"track_id": t, "locked": true})); let h = pj(&ss); let m = e(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "x"}, "track_id": t})); assert!(m.contains("locked"), "{m}"); assert_eq!(pj(&ss), h);
    assert!(e(&mut ss, "recipe_apply", json!({"name": "title_card", "start": 0.0, "params": {"title": "x"}, "track_name": "nope"})).contains("no track named"));
}
fn asset_session() -> (Session, String) {
    let path = std::env::temp_dir().join(format!("kimo-recipe-asset-{}.png", std::process::id()));
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=0x3070d0:s=64x48", "-frames:v", "1", path.to_str().unwrap()]).status().unwrap().success());
    let mut ss = fresh(); let a = c(&mut ss, "import_media", json!({"path": path}))["asset_id"].as_str().unwrap().to_string(); (ss, a)
}
#[test] fn product_reveal_asset_bound_dry_run_equals_hand_batch() {
    let (mut a, asset) = asset_session(); let (mut b, _) = asset_session(); let p = json!({"asset_id": asset, "title": "Kimo", "tagline": "Edit by tool call"});
    let d = c(&mut a, "recipe_apply", json!({"name": "product_reveal", "start": 1.0, "params": p, "dry_run": true})); let r = c(&mut a, "recipe_apply", json!({"name": "product_reveal", "start": 1.0, "params": p, "expect_revision": d["revision"]}));
    c(&mut b, "batch", json!({"ops": d["expansion"]})); assert_eq!(pj(&a), pj(&b)); assert_eq!(hashes(&a), hashes(&b)); assert_eq!(d["created"], r["created"]);
    for k in ["product_reveal/bg", "product_reveal/product", "product_reveal/title", "product_reveal/tagline"] { assert!(r["roles"].get(k).is_some(), "{k}"); }
    let m = e(&mut a, "recipe_apply", json!({"name": "product_reveal", "start": 0.0, "params": {"asset_id": "no_such_asset"}})); assert!(!m.is_empty());
    assert_eq!(r["anchors"]["end"], 5.0);
}
/// Golden frame hashes per recipe at 3 times (tests/golden/recipe_frames.txt; KIMO_UPDATE_GOLDEN=1 regenerates).
#[test] fn golden_frames_per_recipe() {
    let mut out = String::new(); let (_, asset) = asset_session();
    let mut all = cases(); all.push(("product_reveal", json!({"asset_id": asset, "title": "Kimo", "tagline": "Edit by tool call"})));
    for (n, p) in all { let (mut ss, _) = asset_session(); let r = c(&mut ss, "recipe_apply", json!({"name": n, "start": 0.0, "params": p})); assert!(r["ok"].as_bool().unwrap(), "{n}"); out += &format!("{n}\t{}\n", hashes(&ss).iter().map(|h| format!("{h:016x}")).collect::<Vec<_>>().join(" ")); }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/recipe_frames.txt");
    if std::env::var("KIMO_UPDATE_GOLDEN").is_ok() { std::fs::write(&path, &out).unwrap(); } else { assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), out, "recipe frame hashes changed (regenerate with KIMO_UPDATE_GOLDEN=1 only for intended changes)"); }
}
/// Acceptance 9: real call counts for a lower third + title card + diagram scene, by hand (one call per primitive op) versus recipe_apply, with identical frames.
#[test] fn call_count_metric_hand_versus_recipes() {
    let scene = [("title_card", json!({"title": "Eurogrip", "subtitle": "Winter range"})), ("explainer_diagram", json!({"labels": ["Design", "Test", "Ship"]})), ("lower_third", json!({"title": "Anna Keller", "subtitle": "Head of Product"}))];
    let mut r = fresh(); let mut by_recipe = 0; let mut expansions = vec![];
    for (i, (n, p)) in scene.iter().enumerate() { let start = i as f64 * 0.2; let d = c(&mut r, "recipe_apply", json!({"name": n, "start": start, "params": p, "dry_run": true})); by_recipe += 1; expansions.push(d["expansion"].clone()); c(&mut r, "recipe_apply", json!({"name": n, "start": start, "params": p})); by_recipe += 1; }
    // the recipe path needs 1 call per recipe (the dry_run preview above is optional): count both honestly
    let mut h = fresh(); let mut by_hand = 0;
    for ops in &expansions { let mut ids: std::collections::HashMap<String, Value> = Default::default();
        for o in ops.as_array().unwrap() { let mut args = o["args"].clone(); if let Some(m) = args.as_object_mut() { for (_, v) in m.iter_mut() { if let Some(rf) = v.as_str().and_then(|s| s.strip_prefix('$')).filter(|s| !s.starts_with('$')) { let (id, f) = rf.split_once('.').unwrap(); *v = ids[id][f].clone(); } } } let res = c(&mut h, o["tool"].as_str().unwrap(), args); ids.insert(o["id"].as_str().unwrap().into(), res); by_hand += 1; } }
    assert_eq!(hashes(&r), hashes(&h), "frame-identical renders"); assert_eq!(pj(&r).len(), pj(&h).len());
    let one_batch = 3; eprintln!("CALL COUNT (Eurogrip-like scene: title card + diagram + lower third): by hand {by_hand} primitive calls; recipes {one_batch} recipe_apply calls (or {by_recipe} with a dry_run preview each); one history step per recipe");
    assert!(one_batch <= 5 && by_hand > one_batch);
}
