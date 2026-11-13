//! The ledger: every engine call the app makes or relays goes through here, tagged with WHO made it ("you" in the
//! editor, "claude-code" / "codex" / "antigravity" driving the app's engine through the bridge, "kimo-chat" for the
//! API-key agent, "app" for the window's own quiet reads). One engine, one project, one history:
//!   * every call becomes a Live entry (start + end, with duration and a one-line summary);
//!   * every call that grew the engine's undo history becomes a Change card that can be undone from the app;
//!   * every mutating call announces `project-changed`, so the editor, viewer and timeline refresh themselves.
//! Mutating calls are serialized so the before/after history depth of each card is exact even with several agents.
use crate::agent::{history, summarize, Tools};
use crate::engine::{is_error, result_text};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

/// One undoable change, whoever made it.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Change {
    pub id: u64,
    pub actor: String,
    /// The engine's undo depth right after this change (history length).
    pub depth: usize,
    /// How many engine undo steps this one call made.
    pub steps: usize,
    pub tool: String,
    pub action: String,
    pub summary: String,
    pub t: u128,
    pub undone: bool,
    /// What the engine found wrong with this edit (its `observe.findings`): code, severity, message and the fix call.
    pub findings: Vec<Value>,
}

/// Reads never change the project. Anything not listed is treated as possibly mutating (history tells the truth).
pub fn is_read_only(tool: &str, action: &str) -> bool {
    if matches!(tool, "search_tools" | "kimo_describe" | "kimo_enable_group") { return true; }
    let a = action;
    a.starts_with("get_") || a.starts_with("list_") || a.starts_with("find") || a.starts_with("check_")
        || a.starts_with("read_") || a.starts_with("cache_stats") || a.ends_with("_status") || a.ends_with("_info")
        || matches!(a, "timeline_summary" | "history_list" | "render_frame" | "preview_range" | "snap_time" | "project_diff"
            | "kimo_info" | "kimo_search" | "describe" | "job_status" | "list" | "scope" | "waveform" | "vectorscope"
            | "analyze_exposure" | "export_edl" | "proxy_jobs")
}

/// The findings an engine result carries in its `observe` field (worst first, at most 6, compact) and their counts.
pub fn observed(text: &str) -> (Vec<Value>, Value) {
    let Some(obs) = serde_json::from_str::<Value>(text).ok().and_then(|v| v.get("observe").cloned()) else { return (vec![], Value::Null) };
    let f: Vec<Value> = obs.get("findings").and_then(|x| x.as_array()).map(|a| a.iter().take(6).map(|x| json!({
        "code": x["code"], "severity": x["severity"], "message": x["message"], "at": x["at"], "clip_id": x["clip_id"], "fix": x["fix"]})).collect()).unwrap_or_default();
    (f, obs.get("counts").cloned().unwrap_or(Value::Null))
}

/// Calls the Live tab does not list (the window's own refresh reads, and history bookkeeping).
fn quiet(actor: &str, action: &str) -> bool { actor == "app" || action == "history_list" }

pub fn now_ms() -> u128 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) }

fn clip(s: &str, n: usize) -> String { if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() } }

pub struct Ledger {
    tools: Arc<dyn Tools + Send + Sync>,
    changes: Mutex<Vec<Change>>,
    seq: AtomicU64,
    card_seq: AtomicU64,
    write: Mutex<()>,
    emit: Emit,
    /// Counts successful possibly-mutating calls: autosave saves when it moves.
    rev: AtomicU64,
}

impl Ledger {
    pub fn new(tools: Arc<dyn Tools + Send + Sync>, emit: Emit) -> Ledger {
        Ledger { tools, changes: Mutex::new(vec![]), seq: AtomicU64::new(1), card_seq: AtomicU64::new(1), write: Mutex::new(()), emit, rev: AtomicU64::new(0) }
    }

    pub fn tools(&self) -> Vec<Value> { self.tools.list() }
    /// How many edits have landed (any actor). Autosave compares this with the revision it last wrote.
    pub fn revision(&self) -> u64 { self.rev.load(Ordering::SeqCst) }
    pub fn changes(&self) -> Vec<Change> { self.changes.lock().unwrap().clone() }
    /// A fresh project (engine restart / project load): the old cards can no longer be undone.
    pub fn reset(&self) { self.changes.lock().unwrap().clear(); (self.emit)("changes", json!([])); }

    /// tools/call as `actor`. Returns the MCP result object exactly as the engine sent it.
    pub fn call(&self, actor: &str, tool: &str, args: &Value) -> Result<Value, String> {
        // the person's own edits stay instant: findings without rendering before/after frames (agents get the full look)
        let owned;
        let args = if (actor == "you" || actor == "app") && args.get("observe").is_none() && args.is_object() {
            owned = { let mut a = args.clone(); a["observe"] = json!("numbers"); a }; &owned
        } else { args };
        let action = args.get("action").and_then(|a| a.as_str()).unwrap_or("").to_string();
        let ro = is_read_only(tool, &action);
        let loud = !quiet(actor, &action);
        let id = self.seq.fetch_add(1, Ordering::SeqCst);
        let summary = summarize(tool, args);
        if loud {
            (self.emit)("live-event", json!({"id": id, "phase": "start", "actor": actor, "tool": tool, "action": action,
                "summary": summary, "args": args.get("args").cloned().unwrap_or(Value::Null), "read_only": ro, "t": now_ms() as u64}));
        }
        let t0 = std::time::Instant::now();
        let (res, change) = if ro {
            (self.tools.call(tool, args), None)
        } else {
            let _w = self.write.lock().unwrap_or_else(|e| e.into_inner());
            let before = history(self.tools.as_ref()).map(|h| h.0);
            let r = self.tools.call(tool, args);
            let ok = matches!(&r, Ok(v) if !is_error(v));
            let change = if ok { self.after_write(actor, tool, &action, &summary, before) } else { None };
            (r, change)
        };
        let ms = t0.elapsed().as_millis() as u64;
        let (ok, text) = match &res { Ok(v) => (!is_error(v), result_text(v)), Err(e) => (false, e.clone()) };
        // the engine's own verdict on this edit (kimo_core::observe): keep the worst few findings with their fixes
        let (findings, counts) = observed(&text);
        if let (Some(c), false) = (&change, findings.is_empty()) {
            let mut cs = self.changes.lock().unwrap();
            if let Some(x) = cs.iter_mut().find(|x| x.id == c.id) { x.findings = findings.clone(); }
            (self.emit)("changes", serde_json::to_value(&*cs).unwrap_or(json!([])));
        }
        if loud {
            (self.emit)("live-event", json!({"id": id, "phase": "end", "actor": actor, "tool": tool, "action": action, "summary": summary,
                "ok": ok, "ms": ms, "result": clip(&text, 600), "change": change, "read_only": ro, "t": now_ms() as u64,
                "findings": findings, "counts": counts}));
        }
        if !ro && ok && !matches!(action.as_str(), "save_project" | "export_job_start" | "export") { self.rev.fetch_add(1, Ordering::SeqCst); }
        if !ro && ok { (self.emit)("project-changed", json!({"actor": actor, "tool": tool, "action": action, "change": change})); }
        res
    }

    /// After a successful possibly-mutating call: compare history depth, make or re-sync cards.
    fn after_write(&self, actor: &str, tool: &str, action: &str, summary: &str, before: Option<usize>) -> Option<Change> {
        let (after, _label) = history(self.tools.as_ref())?;
        let mut cs = self.changes.lock().unwrap();
        let mut changed = false;
        // undo / redo / restore move the depth without adding a card: re-mark what is (un)done
        for c in cs.iter_mut() {
            let undone = c.depth > after;
            if c.undone != undone && (matches!(action, "undo" | "redo" | "restore" | "load_project") || undone) { c.undone = undone; changed = true; }
        }
        let made = match before { Some(b) if after > b && !matches!(action, "redo") => {
            // history branched: anything at or above the new depth that was undone is gone for good
            cs.retain(|c| !(c.undone && c.depth >= after));
            let c = Change { id: self.card_seq.fetch_add(1, Ordering::SeqCst), actor: actor.into(), depth: after, steps: after - b,
                tool: tool.into(), action: action.into(), summary: summary.into(), t: now_ms(), undone: false, findings: vec![] };
            cs.push(c.clone());
            changed = true;
            Some(c)
        } _ => None };
        if changed { (self.emit)("changes", serde_json::to_value(&*cs).unwrap_or(json!([]))); }
        made
    }

    /// Undo one card: the engine's history is linear, so every change made after it is undone too.
    pub fn undo_change(&self, actor: &str, id: u64) -> Result<Value, String> {
        let depth = self.changes.lock().unwrap().iter().find(|c| c.id == id).map(|c| (c.depth, c.steps)).ok_or("That change is no longer in the history.")?;
        let (now, _) = history(self.tools.as_ref()).ok_or("Could not read the engine's history.")?;
        let first = depth.0 + 1 - depth.1.max(1);
        if now < first { return Err("That change is already undone.".into()); }
        let steps = now - first + 1;
        let r = self.call(actor, "project", &json!({"action": "undo", "args": {"steps": steps}}))?;
        if is_error(&r) { return Err(result_text(&r)); }
        Ok(json!({"steps": steps}))
    }
}

/// Lets anything that takes `Tools` (the viewer, the API-key chat) run through the ledger as one actor.
pub struct As { pub ledger: Arc<Ledger>, pub actor: &'static str }
impl Tools for As {
    fn list(&self) -> Vec<Value> { self.ledger.tools() }
    fn call(&self, name: &str, args: &Value) -> Result<Value, String> { self.ledger.call(self.actor, name, args) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An engine with a real linear undo stack: add_track pushes, undo pops `steps`.
    #[derive(Default)]
    struct Fake { undo: Mutex<Vec<String>>, redo: Mutex<Vec<String>> }
    impl Tools for Fake {
        fn list(&self) -> Vec<Value> { vec![json!({"name":"timeline"}), json!({"name":"project"})] }
        fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
            let a = args["action"].as_str().unwrap_or("");
            let text = match (name, a) {
                ("project", "history_list") => json!({"undo": self.undo.lock().unwrap().iter().map(|t| json!({"tool": t})).collect::<Vec<_>>()}).to_string(),
                ("project", "undo") => { let n = args["args"]["steps"].as_u64().unwrap_or(1) as usize; let mut u = self.undo.lock().unwrap(); for _ in 0..n { if let Some(x) = u.pop() { self.redo.lock().unwrap().push(x); } } "{}".into() }
                ("project", "redo") => { if let Some(x) = self.redo.lock().unwrap().pop() { self.undo.lock().unwrap().push(x); } "{}".into() }
                ("project", "get_project") => "{}".into(),
                ("timeline", "add_track") => { self.undo.lock().unwrap().push("add_track".into()); self.redo.lock().unwrap().clear(); "{\"track_id\":\"t\"}".into() }
                ("timeline", "bad") => return Ok(json!({"content":[{"type":"text","text":"no such clip"}],"isError":true})),
                _ => "{}".into(),
            };
            Ok(json!({"content":[{"type":"text","text": text}]}))
        }
    }
    fn ledger() -> (Arc<Ledger>, Arc<Mutex<Vec<(String, Value)>>>) {
        let got = Arc::new(Mutex::new(vec![]));
        let g = got.clone();
        (Arc::new(Ledger::new(Arc::new(Fake::default()), Arc::new(move |k, v| g.lock().unwrap().push((k.to_string(), v))))), got)
    }

    #[test]
    fn cards_carry_the_actor_and_undo_is_linear() {
        let (l, got) = ledger();
        l.call("you", "timeline", &json!({"action":"add_track"})).unwrap();
        l.call("claude-code", "timeline", &json!({"action":"add_track"})).unwrap();
        l.call("codex", "timeline", &json!({"action":"add_track"})).unwrap();
        let cs = l.changes();
        assert_eq!(cs.iter().map(|c| c.actor.as_str()).collect::<Vec<_>>(), ["you", "claude-code", "codex"]);
        assert_eq!(cs.iter().map(|c| c.depth).collect::<Vec<_>>(), [1, 2, 3]);
        // undoing claude's change takes codex's with it
        assert_eq!(l.undo_change("you", cs[1].id).unwrap()["steps"], 2);
        let cs = l.changes();
        assert_eq!(cs.iter().map(|c| c.undone).collect::<Vec<_>>(), [false, true, true]);
        assert!(l.undo_change("you", cs[2].id).is_err(), "already undone");
        // a new edit branches the history: the undone cards are dropped
        l.call("antigravity", "timeline", &json!({"action":"add_track"})).unwrap();
        let cs = l.changes();
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[1].actor, "antigravity");
        let g = got.lock().unwrap();
        assert!(g.iter().any(|(k, v)| k == "live-event" && v["actor"] == "codex" && v["phase"] == "end" && v["ok"] == true));
        assert!(g.iter().any(|(k, _)| k == "project-changed"));
    }

    #[test]
    fn reads_are_not_cards_and_app_reads_are_quiet() {
        let (l, got) = ledger();
        l.call("app", "project", &json!({"action":"get_project"})).unwrap();
        l.call("claude-code", "project", &json!({"action":"get_project"})).unwrap();
        assert!(l.changes().is_empty());
        let g = got.lock().unwrap();
        let live: Vec<&Value> = g.iter().filter(|(k, _)| k == "live-event").map(|(_, v)| v).collect();
        assert_eq!(live.len(), 2, "only claude's read is listed (start + end): {live:?}");
        assert!(!g.iter().any(|(k, _)| k == "project-changed"), "reads never announce a change");
    }

    #[test]
    fn failed_edits_make_no_card_and_report_the_error() {
        let (l, got) = ledger();
        let r = l.call("you", "timeline", &json!({"action":"bad"})).unwrap();
        assert!(is_error(&r));
        assert!(l.changes().is_empty());
        assert!(got.lock().unwrap().iter().any(|(k, v)| k == "live-event" && v["ok"] == false && v["result"] == "no such clip"));
    }

    #[test]
    fn engine_undo_and_redo_resync_cards() {
        let (l, _) = ledger();
        l.call("you", "timeline", &json!({"action":"add_track"})).unwrap();
        l.call("you", "timeline", &json!({"action":"add_track"})).unwrap();
        l.call("codex", "project", &json!({"action":"undo","args":{"steps":1}})).unwrap();
        assert_eq!(l.changes().iter().map(|c| c.undone).collect::<Vec<_>>(), [false, true]);
        l.call("you", "project", &json!({"action":"redo"})).unwrap();
        assert_eq!(l.changes().iter().map(|c| c.undone).collect::<Vec<_>>(), [false, false], "redo brings the card back");
        assert_eq!(l.changes().len(), 2, "redo makes no new card");
    }

    #[test]
    fn observe_findings_are_kept_on_the_card() {
        let text = json!({"clip_id": "c1", "observe": {"counts": {"warning": 1}, "findings": [{"code": "flash_clip", "severity": "warning", "message": "m", "fix": {"tool": "delete_clip", "args": {"clip_id": "c1"}}}]}}).to_string();
        let (f, c) = observed(&text);
        assert_eq!(f[0]["code"], "flash_clip");
        assert_eq!(f[0]["fix"]["tool"], "delete_clip");
        assert_eq!(c["warning"], 1);
        assert!(observed("not json").0.is_empty());
    }

    #[test]
    fn read_only_classification() {
        for (t, a) in [("project", "get_project"), ("project", "timeline_summary"), ("output", "job_status"), ("output", "render_frame"), ("search_tools", ""), ("project", "find_assets")] {
            assert!(is_read_only(t, a), "{t}.{a}");
        }
        for (t, a) in [("timeline", "add_clip"), ("project", "import_media"), ("project", "undo"), ("text", "add_text"), ("output", "export_job_start")] {
            assert!(!is_read_only(t, a), "{t}.{a}");
        }
    }
}
