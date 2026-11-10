//! v7.2 additions: self_review -- "self-review loops: render a check, lint it, auto-fix" (the north
//! star's own example of agent-superior "whole-project reasoning in one call"). Today an agent must call
//! lint_edit, validate_timeline and list_dependencies separately and manually correlate their times to
//! get one "what's wrong, and show me" answer. This orchestrates the three (pure composition through
//! dispatch_nested -- no new checking logic, no new dispatch path) and, when given a path, renders a
//! contact_sheet of every flagged timeline moment so the agent can actually look at what it found.
//! Deliberately NOT auto-fix: lint_edit's own problems already carry the exact fix call each one needs
//! (its own documented contract) -- applying them without the person's say-so is a mutation this tool
//! does not take; "auto-fix" stays a human/agent decision reading the report, same as lint_edit alone.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};

fn self_review(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let platform = a.get("platform").and_then(|v| v.as_str()).unwrap_or("none").to_string();
    let path = a.get("path").and_then(|v| v.as_str()).map(String::from);

    let lint = match dispatch_nested(ss, "lint_edit", &json!({"platform": platform})) {
        Ok(v) => v,
        Err(e) => json!({"skipped": true, "reason": e}), // e.g. "the timeline is empty" -- not a self_review failure
    };
    let timeline = dispatch_nested(ss, "validate_timeline", &json!({}))?;
    let deps = dispatch_nested(ss, "list_dependencies", &json!({}))?;

    let lint_problems: Vec<Value> = lint["problems"].as_array().cloned().unwrap_or_default();
    let timeline_issues: Vec<Value> = timeline["issues"].as_array().cloned().unwrap_or_default();
    let missing_deps: Vec<Value> = deps["items"].as_array().cloned().unwrap_or_default().into_iter().filter(|i| i["status"] == "missing").collect();

    // Every distinct flagged moment worth a look, sorted and capped to contact_sheet's own 16-tile limit.
    let mut times: Vec<f64> = vec![];
    for p in &lint_problems { if let Some(t) = p["at"].as_f64() { times.push(t); } }
    for i in &timeline_issues { if let Some(t) = i["after"]["end"].as_f64() { times.push(t); } if let Some(t) = i["first"]["end"].as_f64() { times.push(t); } if let Some(t) = i["clip"]["start"].as_f64() { times.push(t); } }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
    let times_truncated = times.len() > 16;
    times.truncate(16);

    let contact_sheet = match &path {
        None => json!({"rendered": false, "reason": "no path given -- pass path to also render a contact sheet of every flagged moment"}),
        Some(_) if times.is_empty() => json!({"rendered": false, "reason": "nothing flagged, nothing to show"}),
        Some(p) => match dispatch_nested(ss, "contact_sheet", &json!({"path": p, "times": times})) {
            Ok(mut v) => { v["rendered"] = json!(true); v }
            Err(e) => json!({"rendered": false, "reason": format!("contact_sheet failed: {e}")}),
        },
    };

    let ok = lint_problems.is_empty() && timeline["ok"] == true && missing_deps.is_empty();
    Ok(json!({
        "ok": ok,
        "summary": {"lint_problems": lint_problems.len(), "timeline_issues": timeline_issues.len(), "missing_media": missing_deps.len(), "flagged_moments": times.len(), "flagged_moments_truncated": times_truncated},
        "lint_edit": lint, "validate_timeline": timeline, "missing_media": missing_deps,
        "flagged_times": times, "contact_sheet": contact_sheet,
        "note": "orchestrates lint_edit + validate_timeline + list_dependencies (no new checking logic); fixes are not applied -- lint_edit's own problems already carry the exact fix call each one needs",
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "self_review", description: "One-call audit: runs lint_edit, validate_timeline and list_dependencies, and correlates their flagged moments. Saves calling all three separately and matching times up by hand. platform passes through to lint_edit (tiktok|reels|shorts|none, default none here since a general review shouldn't assume a platform); lint_edit erroring on an empty timeline is reported, not treated as a self_review failure. path (optional) also renders a contact_sheet of every flagged moment (capped at 16, contact_sheet's own limit) so the agent can look at what it found, not just read about it. Returns ok (nothing flagged anywhere), a summary count per source, each tool's own full result, and the merged flagged_times. Does NOT auto-fix: lint_edit's problems already carry the exact fix call each one needs; applying them is a deliberate separate step, not this tool's. Read-only (contact_sheet's own file write aside).", mutates: false,
        schema: obj(vec![("platform", s("passed through to lint_edit: tiktok|reels|shorts|none, default none")), ("path", s("optional: also render a contact sheet of every flagged moment to this .png path"))], &[]), run: self_review },
] }
