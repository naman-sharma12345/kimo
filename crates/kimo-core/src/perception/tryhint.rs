//! P4: every perception tool error ends with "; try <tool>(<args>)", a call that works on any project, so the next step is never a guess.
use serde_json::{json, Value};
/// Append the recovery call to an error from a perception tool. Idempotent.
pub fn with_try(tool: &str, e: String) -> String {
    if e.contains("; try ") { return e; }
    let l = e.to_lowercase();
    let (t, args) = if l.contains("clip") || l.contains("isolate") || l.contains("hide") || l.contains("track") || l.starts_with("size_mismatch") { ("get_project", json!({})) } else { match tool {
        "describe_frame" => ("describe_frame", json!({"time": 0})),
        "lint_frame" => ("lint_frame", json!({"time": 0, "annotate": false})),
        "diff_frames" => ("diff_frames", json!({"a": {"time": 0}, "b": {"time": 0}})),
        "detect_cuts" => ("detect_cuts", json!({})),
        _ => ("get_project", json!({})),
    } };
    format!("{}; try {t}({args})", e.trim_end_matches('.'))
}
pub fn wrap(tool: &str, r: Result<Value, String>) -> Result<Value, String> { r.map_err(|e| with_try(tool, e)) }
