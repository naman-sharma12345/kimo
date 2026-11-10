//! search_tools ranking: "remove um and uh" and "cut the ums" are what a newcomer actually types for
//! trim_filler_words -- a fresh-clone readiness check (KQ4, 561297e) found trim_filler_words ranked #21 of 153 for
//! "remove um and uh" (outranked by every remove_* tool, which score higher purely by having "remove" in their own
//! name), so a client with a default 8-result search_tools window never sees it. "remove filler words" already finds
//! it immediately; this test pins the natural filler-word phrasing too, in the top 3.
use kimo_core::commands::{dispatch, Session};
use kimo_core::toolgroups::search_tools;
use serde_json::{json, Value};

fn catalogs(ss: &mut Session) -> (Value, Value) {
    (dispatch(ss, "list_effects", &json!({})).unwrap_or(json!([])), dispatch(ss, "list_transitions", &json!({})).unwrap_or(json!([])))
}

fn top_names(ss: &mut Session, q: &str, limit: usize) -> Vec<String> {
    let (ef, tr) = catalogs(ss);
    let r = search_tools(q, limit, None, &ef, &tr);
    r["matches"].as_array().unwrap().iter().filter_map(|m| m["name"].as_str().map(str::to_string)).collect()
}

#[test] fn trim_filler_words_ranks_in_the_top_3_for_natural_filler_phrasing() {
    let mut ss = Session::new();
    for q in ["remove um and uh", "cut the ums"] {
        let names = top_names(&mut ss, q, 8);
        assert!(names.iter().take(3).any(|n| n == "trim_filler_words"),
            "query {q:?}: trim_filler_words not in top 3 of default 8-result window: {names:?}");
    }
}

/// KQ4 also confirmed fit_clip_duration is the #1 hit for "make it shorter than 30 seconds"; pin that so a future
/// ranking change can't quietly lose it while fixing trim_filler_words.
#[test] fn fit_clip_duration_stays_the_top_hit_for_a_duration_cap_query() {
    let mut ss = Session::new();
    let names = top_names(&mut ss, "make it shorter than 30 seconds", 8);
    assert_eq!(names.first().map(String::as_str), Some("fit_clip_duration"), "{names:?}");
}

/// The exact phrasing that already worked (KQ4) must keep working.
#[test] fn remove_filler_words_still_finds_it_immediately() {
    let mut ss = Session::new();
    let names = top_names(&mut ss, "remove filler words", 8);
    assert!(names.iter().take(3).any(|n| n == "trim_filler_words"), "{names:?}");
}
