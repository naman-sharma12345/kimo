//! v7.3: `apply_review_fixes` -- self_review deliberately stops at diagnosis ("lint_edit's own problems
//! already carry the exact fix call each needs; applying them is a deliberate separate step, not this
//! tool's"). This IS that separate step: calls lint_edit itself, and for every problem whose `fix` is a
//! concrete, fully-specified tool call (no placeholder value for a human to fill in, e.g. "<the text
//! without those characters>"), dispatches it. One batch call instead of an agent reading each problem,
//! deciding which ones it can act on unaided, and calling them one at a time. `dry_run` reports exactly
//! what would be applied/skipped without mutating anything. A problem whose fix needs a human decision
//! (a placeholder value, an "or" choice with no clear default, or no fix at all) is reported under
//! `skipped_manual` with why, never guessed at. Each attempted fix is applied independently: one failing
//! (e.g. a clip another fix already removed) is recorded in `failed` and does not stop the rest.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};

/// True if any string value anywhere in `v` is a human-fill-in placeholder (kimo's own convention in
/// lint_edit's `fix` payloads: angle-bracketed prose like "<the text without those characters>").
fn has_placeholder(v: &Value) -> bool {
    match v {
        Value::String(s) => s.contains('<') && s.contains('>'),
        Value::Object(m) => m.values().any(has_placeholder),
        Value::Array(a) => a.iter().any(has_placeholder),
        _ => false,
    }
}

/// Pulls a dispatchable (tool, args) pair out of a `fix` value, or None if it isn't one (no `tool` key,
/// non-string tool name, or a placeholder anywhere in its args -- those need a human, not a guess).
fn dispatchable<'a>(fix: &'a Value) -> Option<(&'a str, Value)> {
    let tool = fix.get("tool")?.as_str()?;
    let args = fix.get("args").cloned().unwrap_or_else(|| json!({}));
    if has_placeholder(&args) { return None; }
    Some((tool, args))
}

fn apply_review_fixes(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let platform = a.get("platform").and_then(|v| v.as_str()).unwrap_or("none").to_string();
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let codes: Option<Vec<String>> = a.get("codes").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|c| c.as_str().map(String::from)).collect());

    let lint = match dispatch_nested(ss, "lint_edit", &json!({"platform": platform})) {
        Ok(v) => v,
        Err(e) => return Ok(json!({"ok": true, "dry_run": dry_run, "problems_seen": 0, "applied": [], "skipped_manual": [], "failed": [],
            "note": format!("lint_edit had nothing to check: {e}")})),
    };
    let problems: Vec<Value> = lint["problems"].as_array().cloned().unwrap_or_default();

    let mut applied = vec![]; let mut skipped_manual = vec![]; let mut failed = vec![];
    for p in &problems {
        let code = p["code"].as_str().unwrap_or("").to_string();
        if let Some(allow) = &codes { if !allow.iter().any(|c| c == &code) { continue; } }
        let Some(fix) = p.get("fix").filter(|f| !f.is_null()) else {
            skipped_manual.push(json!({"code": code, "clip_id": p.get("clip_id"), "reason": "no fix supplied"})); continue;
        };
        let Some((tool, args)) = dispatchable(fix) else {
            skipped_manual.push(json!({"code": code, "clip_id": p.get("clip_id"), "reason": "fix needs a human decision (placeholder value or no direct tool call)", "fix": fix}));
            continue;
        };
        if dry_run { applied.push(json!({"code": code, "clip_id": p.get("clip_id"), "would_call": {"tool": tool, "args": args}})); continue; }

        match dispatch_nested(ss, tool, &args) {
            Ok(result) => {
                let mut entry = json!({"code": code, "clip_id": p.get("clip_id"), "tool": tool, "args": args, "result": result});
                // A "then" step (e.g. move_clip then trim_clip) is applied only if the primary succeeded and it is itself concrete.
                if let Some(then_fix) = fix.get("then") {
                    if let Some((then_tool, then_args)) = dispatchable(then_fix) {
                        match dispatch_nested(ss, then_tool, &then_args) {
                            Ok(tr) => entry["then_result"] = json!({"tool": then_tool, "args": then_args, "result": tr}),
                            Err(e) => entry["then_failed"] = json!({"tool": then_tool, "args": then_args, "error": e}),
                        }
                    }
                }
                applied.push(entry);
            }
            Err(e) => failed.push(json!({"code": code, "clip_id": p.get("clip_id"), "tool": tool, "args": args, "error": e})),
        }
    }

    Ok(json!({
        "ok": failed.is_empty(),
        "dry_run": dry_run,
        "problems_seen": problems.len(),
        "applied": applied, "skipped_manual": skipped_manual, "failed": failed,
        "lint_edit": lint,
        "note": "one pass over lint_edit's own problems, in the order lint_edit returned them; a fix is applied only when it is a concrete tool call with no placeholder value -- anything needing a human judgment call is reported in skipped_manual, not guessed at",
    }))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "apply_review_fixes", description: "Apply lint_edit's (and so self_review's) machine-actionable fixes in one batch call, instead of reading each problem and calling its fix by hand. Runs lint_edit itself (platform passes through, default none). For each problem: if `fix` names a concrete tool call with no placeholder value (lint_edit marks a human-fill-in value like \"<the text without those characters>\" this way), it is dispatched, including a chained `then` step when the primary succeeds; a fix needing a human decision (a placeholder, an \"or\" alternative, or no fix at all) is reported under skipped_manual with why, never guessed at. `codes` (optional) limits which problem codes to act on, e.g. [\"transition_from_black\"]; default every code lint_edit returns. `dry_run` reports exactly what would be applied/skipped without mutating anything -- run this first. One fix failing (e.g. a clip an earlier fix already removed) is recorded in `failed` and does not stop the rest. Returns problems_seen, applied, skipped_manual, failed and the full lint_edit result for context.", mutates: true,
        schema: obj(vec![("platform", s("passed through to lint_edit: tiktok|reels|shorts|none, default none")), ("codes", json!({"type":"array","items":{"type":"string"},"description":"only act on these lint_edit problem codes; default: all"})), ("dry_run", json!({"type":"boolean","description":"report what would be applied/skipped, mutate nothing"}))], &[]), run: apply_review_fixes },
] }
