//! kimo-mcp: Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//! Tools are generated from kimo_core::commands::registry().
//! Every tool call runs on a worker thread under the sandbox layer (kimo_core::guard): argument sanitizer, project limits with
//! rollback, memory budget, panic catching, and a wall-clock time limit with cooperative cancel (hard exit as the last resort).
//! Every tools/call is also reported to the spectator GUI (spectator.rs; on by default, found on 127.0.0.1:7421..=7430,
//! KIMO_SPECTATOR=off turns it off): out of band, never on stdout.
//! Command line: `--spectator-demo` (spectator demo mode at the default pace) or `--demo-pace-ms <50..=100>` turn on
//! spectator demo mode, which gives every call its own preview frame and paces the calls (preview.rs); they override
//! `KIMO_SPECTATOR_DEMO`. Default: headless (off).
use kimo_core::commands::{dispatch, Session};
mod inline;
mod png;
mod preview;
mod spectator;
use kimo_core::toolgroups;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

#[global_allocator]
static ALLOC: kimo_core::guard::Counting = kimo_core::guard::Counting;

fn run_guarded(ss: &Arc<Mutex<Session>>, name: &str, args: &Value, out: &mut impl Write, id: &Option<Value>) -> Value {
    let r = match kimo_core::guard::run_tool_guarded(ss, name, args) {
        kimo_core::guard::Guarded::Done(r) => r,
        kimo_core::guard::Guarded::Hung { limit } => { // worker did not stop: report, then exit so the machine is never held hostage; the client restarts the server
            let text = format!("tool '{name}' hung past {limit}s and could not be cancelled; the server is exiting safely. Restart it.");
            spectator::call_abort(&text);
            let msg = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":text}],"isError":true}});
            let _ = writeln!(out, "{msg}"); let _ = out.flush(); std::process::exit(124); }
    };
    match r {
        Ok(v) => { let media = { let mut g = ss.lock().unwrap_or_else(|e| e.into_inner()); std::mem::take(&mut g.media) };
            let mut content = vec![json!({"type":"text","text":serde_json::to_string(&v).unwrap()})];
            if media.is_empty() { return json!({"content":content,"isError":false}); }
            let (blk, ent, notes) = inline::blocks(&media, args); content.extend(blk);
            if !notes.is_empty() { content.push(json!({"type":"text","text":notes.join("; ")})); }
            let mut sc = ent[0].clone(); sc["time"] = args.get("time").cloned().unwrap_or(Value::Null); if ent.len() > 1 { sc = json!({"images": ent}); }
            json!({"content":content,"structuredContent":sc,"isError":false}) }
        Err(e) => json!({"content":[{"type":"text","text":e.message}],"isError":true,"structuredContent":{"code":e.code,"param":e.param,"hint":e.hint}}),
    }
}

/// The demo-mode setting given on the command line, if any: `--demo-pace-ms N`, `--demo-pace-ms=N` or `--spectator-demo`.
fn demo_arg(args: &[String]) -> Option<String> {
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--spectator-demo" { out = Some("on".to_string()); }
        else if a == "--demo-pace-ms" { out = Some(it.next().cloned().unwrap_or_default()); }
        else if let Some(v) = a.strip_prefix("--demo-pace-ms=") { out = Some(v.to_string()); }
    }
    out
}

fn main() {
    kimo_core::guard::init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    preview::configure_demo(demo_arg(&argv).as_deref()); // spectator demo mode: off unless asked for
    spectator::init(); // telemetry for the spectator GUI, on by default (KIMO_SPECTATOR); KIMO_SPECTATOR=off = no thread, no work
    std::panic::set_hook(Box::new(|i| { if std::thread::current().name() == Some("kimo-preview") { return; } eprintln!("kimo: {i}"); })); // a cancelled live preview is not news
    let ss = Arc::new(Mutex::new(Session::new()));
    let mut enabled: std::collections::HashSet<String> = std::collections::HashSet::new();
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let Ok(req): Result<Value, _> = serde_json::from_str(&line) else { continue };
        let id = req.get("id").cloned(); let mut notify_changed = false;
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));
        let result: Option<Result<Value, (i64, String)>> = match method {
            "initialize" => Some(Ok(json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":true}},"serverInfo":{"name":"kimo","version":kimo_core::buildinfo::VERSION,"commit":kimo_core::buildinfo::COMMIT,"schema_version":kimo_core::buildinfo::SCHEMA_VERSION}}))),
            "ping" => Some(Ok(json!({}))),
            "tools/list" => {
                // grouped by default (about 16 entry points); mode "flat" or KIMO_TOOLS=flat lists every tool. Every tool stays callable by name either way.
                let profile = params.get("mode").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| std::env::var("KIMO_TOOLS").unwrap_or_default());
                Some(Ok(json!({"tools": toolgroups::profile_list(&profile, &enabled)})))
            }
            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                spectator::call_start(name, &args, id.as_ref(), &ss);
                let text = |r: Result<Value, String>| match r { Ok(v) => json!({"content":[{"type":"text","text":serde_json::to_string(&v).unwrap()}],"isError":false}), Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}) };
                let r = match name {
                    "kimo_search" => { let qs = args.get("query").and_then(|v| v.as_str()).unwrap_or(""); let mut r = toolgroups::search(qs, args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize);
                        let (ef, tr) = { let mut g = ss.lock().unwrap_or_else(|e| e.into_inner()); (dispatch(&mut g, "list_effects", &json!({})).unwrap_or(json!([])), dispatch(&mut g, "list_transitions", &json!({})).unwrap_or(json!([]))) };
                        r["effects_and_transitions"] = json!(toolgroups::search_catalog(qs, &ef, &tr, 8)); text(Ok(r)) }
                    "search_tools" => { let qs = args.get("query").and_then(|v| v.as_str()).unwrap_or(""); let lim = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(8) as usize; let grp = args.get("group").and_then(|v| v.as_str());
                        let (ef, tr) = { let mut g = ss.lock().unwrap_or_else(|e| e.into_inner()); (dispatch(&mut g, "list_effects", &json!({})).unwrap_or(json!([])), dispatch(&mut g, "list_transitions", &json!({})).unwrap_or(json!([]))) };
                        text(Ok(toolgroups::search_tools(qs, lim, grp, &ef, &tr))) }
                    "kimo_enable_group" => { let n = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        if toolgroups::GROUPS.iter().any(|g| g.name == n && !toolgroups::members(n).is_empty()) { enabled.insert(n.to_string()); notify_changed = true; text(Ok(json!({"enabled": n, "tools": toolgroups::members(n).len(), "note": "tools/list now lists this group's tools individually"}))) }
                        else { text(Err(format!("unknown group '{n}'; groups: {}", toolgroups::GROUPS.iter().filter(|g| !toolgroups::members(g.name).is_empty()).map(|g| g.name).collect::<Vec<_>>().join(", ")))) } }
                    "kimo_describe" => text(toolgroups::describe(&args)),
                    n if toolgroups::is_group(n) => match toolgroups::resolve_group_call(n, &args) { Ok((a, ia)) => run_guarded(&ss, &a, &ia, &mut out, &id), Err(e) => text(Err(e)) },
                    _ => run_guarded(&ss, name, &args, &mut out, &id),
                };
                spectator::call_end(&r, &ss);
                Some(Ok(r))
            }
            m if m.starts_with("notifications/") => None,
            _ => Some(Err((-32601, format!("method not found: {method}")))),
        };
        if let (Some(id), Some(r)) = (id, result) {
            let msg = match r { Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}), Err((c, m)) => json!({"jsonrpc":"2.0","id":id,"error":{"code":c,"message":m}}) };
            writeln!(out, "{msg}").ok(); out.flush().ok();
            if notify_changed { writeln!(out, "{}", json!({"jsonrpc":"2.0","method":"notifications/tools/list_changed"})).ok(); out.flush().ok(); }
        }
        if method == "tools/call" { spectator::pace(); } // spectator demo mode only: this call's frame on screen before the next call
    }
    spectator::flush(std::time::Duration::from_millis(500));
}

#[cfg(test)]
mod tests {
    use super::demo_arg;
    fn v(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }
    #[test]
    fn demo_flags() {
        assert_eq!(demo_arg(&v(&[])), None, "headless by default");
        assert_eq!(demo_arg(&v(&["--spectator-demo"])).as_deref(), Some("on"));
        assert_eq!(demo_arg(&v(&["--demo-pace-ms", "60"])).as_deref(), Some("60"));
        assert_eq!(demo_arg(&v(&["--demo-pace-ms=90"])).as_deref(), Some("90"));
        assert_eq!(demo_arg(&v(&["--other", "x"])), None);
    }
}
