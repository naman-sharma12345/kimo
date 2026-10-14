//! Visual-tier gate rule (Pass 2 registry). tests/visual_manifest.json lists the pixel-rendering families and their status.
//! Strict cycle correctness_pass -> premium_pending -> premium_approved; premium_approved needs the owner's recorded sign-off.
//! Only tools added after the manifest was written (not in legacy_tools) and living in a pixel group must be classified; nothing here renders or writes.
use kimo_core::commands::registry_ref;
use kimo_core::toolgroups::group_of;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
const CYCLE: [&str; 3] = ["correctness_pass", "premium_pending", "premium_approved"];
fn manifest() -> Value { serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/visual_manifest.json")).expect("tests/visual_manifest.json")).expect("valid json") }
fn strs(v: &Value) -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }
#[test]
fn every_new_pixel_tool_is_classified_and_every_family_follows_the_status_cycle() {
    let m = manifest(); let mut errs: Vec<String> = vec![];
    let registry: HashSet<&str> = registry_ref().iter().map(|c| c.name).collect();
    let legacy: HashSet<String> = strs(&m["legacy_tools"]).into_iter().collect(); let pixel: HashSet<String> = strs(&m["pixel_groups"]).into_iter().collect();
    let mut listed: HashMap<String, String> = HashMap::new();
    let scene_src = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/visual_pass.py")).unwrap_or_default();
    let golden = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/golden_registry.txt")).unwrap_or_default();
    let hash_of = |t: &str| golden.lines().find_map(|l| { let mut p = l.split_whitespace(); (p.next() == Some(t)).then(|| p.next().unwrap_or("").to_string()) });
    for f in m["families"].as_array().expect("families") {
        let name = f["family"].as_str().unwrap_or("?"); let status = f["status"].as_str().unwrap_or("");
        let Some(si) = CYCLE.iter().position(|s| *s == status) else { errs.push(format!("{name}: unknown status '{status}'")); continue };
        for t in strs(&f["tools"]) { if !registry.contains(t.as_str()) { errs.push(format!("{name}: tool {t} is not in the registry")); } if let Some(o) = listed.insert(t.clone(), name.to_string()) { errs.push(format!("{t} is listed in both {o} and {name}")); } }
        // the history must walk the cycle in order, one step at a time, and end at the current status
        let hist: Vec<&Value> = f["history"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
        let seq: Vec<&str> = hist.iter().filter_map(|h| h["status"].as_str()).collect();
        if seq != CYCLE[..=si].to_vec() { errs.push(format!("{name}: history {seq:?} must be exactly {:?} (no skipped step)", &CYCLE[..=si])); }
        if si >= 1 {
            if f["scene"].as_str().map(|s| scene_src.contains(&format!("@scene(\"{s}\")"))).unwrap_or(false) == false { errs.push(format!("{name}: {status} needs a scene that exists in tests/visual_pass.py")); }
            let n = strs(&f["numbers"]); if n.len() != 3 || n.iter().any(|x| x.trim().is_empty()) { errs.push(format!("{name}: {status} needs exactly 3 named numbers, has {}", n.len())); }
        }
        if si == 2 {
            let last = hist.last().copied().unwrap_or(&Value::Null);
            for k in ["by", "at", "via"] { if last[k].as_str().map(|s| s.trim().is_empty()).unwrap_or(true) { errs.push(format!("{name}: premium_approved needs the sign-off field '{k}' in its last history entry")); } }
            // a stale approval: the description/schema/mutates hash of a family tool moved since the sign-off
            for t in strs(&f["tools"]) { let rec = f["registry_hash"][&t].as_str().unwrap_or(""); match hash_of(&t) { Some(now) if now == rec => {}, Some(now) => errs.push(format!("{name}: approval is stale, {t} registry hash {rec} -> {now}; re-review and move the family back to premium_pending")), None => errs.push(format!("{name}: {t} missing from golden_registry.txt")) } }
        }
    }
    for n in m["n_a"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let t = n["tool"].as_str().unwrap_or("?"); if n["reason"].as_str().map(|r| r.trim().is_empty()).unwrap_or(true) { errs.push(format!("n_a {t} has no reason")); }
        if let Some(o) = listed.insert(t.to_string(), "n_a".into()) { errs.push(format!("{t} is listed in both {o} and n_a")); }
    }
    for c in registry_ref() { if pixel.contains(group_of(c.name)) && !legacy.contains(c.name) && !listed.contains_key(c.name) { errs.push(format!("new pixel tool {} (group {}) needs a visual_manifest family or an n_a reason", c.name, group_of(c.name))); } }
    let counts = CYCLE.iter().map(|s| (*s, m["families"].as_array().unwrap().iter().filter(|f| f["status"] == *s).count())).collect::<Vec<_>>();
    eprintln!("visual families by status: {counts:?}");
    assert!(errs.is_empty(), "visual manifest: {errs:#?}");
}
