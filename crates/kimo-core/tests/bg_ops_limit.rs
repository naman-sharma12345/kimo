//! V1c-D: bg_grid and bg_orbs name set_limits when their expression exceeds expr_max_ops. Own file: the limit is process-global, so it must not race other rows.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
#[test] fn expression_over_the_limit_is_a_typed_error_naming_set_limits() {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":64,"height":64,"fps":25})).unwrap(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "bg_grid", &json!({"track":t,"duration":2})).unwrap(); dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":2,"orbs":[{"cx":10,"cy":10,"radius":10}]})).unwrap();
    dispatch(&mut s, "set_limits", &json!({"expr_max_ops":20})).unwrap();
    for (tool, a) in [("bg_grid", json!({"track":t,"duration":2})), ("bg_orbs", json!({"track":t,"duration":2,"orbs":[{"cx":10,"cy":10,"radius":10}]}))] {
        let e = dispatch(&mut s, tool, &a).unwrap_err(); assert!(e.starts_with("bg_expr_ops_limit:"), "{tool}: {e}"); assert!(e.contains("set_limits(expr_max_ops=40)") && e.contains("fix_hint"), "{tool}: {e}");
    }
}
