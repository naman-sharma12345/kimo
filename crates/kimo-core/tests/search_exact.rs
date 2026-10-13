//! V4-1 P0a: a query that is exactly a tool name (underscores as spaces) must rank that tool first, ahead of description matches and effect or transition catalog hits.
use kimo_core::commands::{dispatch, registry_ref, Session};
use kimo_core::toolgroups::search_tools;
use serde_json::{json, Value};
fn catalogs() -> (Value, Value) { let mut ss = Session::new(); (dispatch(&mut ss, "list_effects", &json!({})).unwrap(), dispatch(&mut ss, "list_transitions", &json!({})).unwrap()) }
fn names(r: &Value) -> Vec<String> { r["matches"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap().to_string()).collect() }
#[test] fn every_tool_name_query_ranks_that_tool_first_in_the_top_8() {
    let (ef, tr) = catalogs(); let mut miss = vec![]; let mut not_first = vec![]; let all = registry_ref();
    for c in all { let q = c.name.replace('_', " "); let n = names(&search_tools(&q, 8, None, &ef, &tr));
        if !n.contains(&c.name.to_string()) { miss.push(c.name); } else if n[0] != c.name { not_first.push((c.name, n[0].clone())); } }
    assert!(miss.is_empty(), "not in top 8: {miss:?}"); assert!(not_first.is_empty(), "in top 8 but not rank 1 ({}): {:?}", not_first.len(), &not_first[..not_first.len().min(10)]);
}
#[test] fn exact_name_is_found_in_any_case_with_underscores_or_spaces_and_respects_the_group_filter() {
    let (ef, tr) = catalogs();
    for q in ["SET AUDIO", "set_audio", "  set   audio "] { assert_eq!(names(&search_tools(q, 8, None, &ef, &tr))[0], "set_audio", "{q}"); }
    let g = names(&search_tools("set path", 8, Some("animate"), &ef, &tr)); assert!(!g.is_empty() && g.iter().all(|n| n != "set_path" || kimo_core::toolgroups::group_of("set_path") == "animate"), "{g:?}");
    let r = names(&search_tools("set audio", 8, Some("effects"), &ef, &tr)); assert!(!r.contains(&"set_audio".to_string()), "a tool outside the requested group must not be forced in: {r:?}");
}
