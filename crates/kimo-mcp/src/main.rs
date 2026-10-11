//! kimo-mcp: Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//! Tools are generated from kimo_core::commands::registry().
//! Every tool call runs on a worker thread under the sandbox layer (kimo_core::guard): argument sanitizer, project limits with
//! rollback, memory budget, panic catching, and a wall-clock time limit with cooperative cancel (hard exit as the last resort).
use kimo_core::commands::{dispatch, registry, Session};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[global_allocator]
static ALLOC: kimo_core::guard::Counting = kimo_core::guard::Counting;

fn run_guarded(ss: &Arc<Mutex<Session>>, name: &str, args: &Value, out: &mut impl Write, id: &Option<Value>) -> Value {
    let l = kimo_core::guard::limits(); let limit = if name.starts_with("export") { l.export_timeout_s } else { l.timeout_s };
    kimo_core::guard::CANCEL.store(false, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = mpsc::channel(); let (s2, n2, a2) = (ss.clone(), name.to_string(), args.clone());
    std::thread::Builder::new().stack_size(64 << 20).spawn(move || { let mut g = s2.lock().unwrap_or_else(|e| e.into_inner()); let r = dispatch(&mut g, &n2, &a2); let _ = tx.send(r); }).ok();
    let r = match rx.recv_timeout(Duration::from_secs(limit)) {
        Ok(r) => r,
        Err(_) => {
            kimo_core::guard::CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
            match rx.recv_timeout(Duration::from_secs(15)) {
                Ok(_) => Err(format!("tool '{name}' stopped: exceeded the {limit}s time limit (KIMO_TIMEOUT_S); project rolled back where possible")),
                Err(_) => { // worker did not stop: report, then exit so the machine is never held hostage; the client restarts the server
                    let msg = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":format!("tool '{name}' hung past {limit}s and could not be cancelled; the server is exiting safely. Restart it.")}],"isError":true}});
                    let _ = writeln!(out, "{msg}"); let _ = out.flush(); std::process::exit(124); }
            }
        }
    };
    match r {
        Ok(v) => json!({"content":[{"type":"text","text":serde_json::to_string(&v).unwrap()}],"isError":false}),
        Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
    }
}

fn main() {
    kimo_core::guard::init();
    std::panic::set_hook(Box::new(|i| { eprintln!("kimo: {i}"); }));
    let ss = Arc::new(Mutex::new(Session::new()));
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let Ok(req): Result<Value, _> = serde_json::from_str(&line) else { continue };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(json!({}));
        let result: Option<Result<Value, (i64, String)>> = match method {
            "initialize" => Some(Ok(json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"kimo","version":env!("CARGO_PKG_VERSION")}}))),
            "ping" => Some(Ok(json!({}))),
            "tools/list" => Some(Ok(json!({"tools": registry().iter().map(|c| json!({"name":c.name,"description":c.description,"inputSchema":c.schema})).collect::<Vec<_>>()}))),
            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Some(Ok(run_guarded(&ss, name, &args, &mut out, &id)))
            }
            m if m.starts_with("notifications/") => None,
            _ => Some(Err((-32601, format!("method not found: {method}")))),
        };
        if let (Some(id), Some(r)) = (id, result) {
            let msg = match r { Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}), Err((c, m)) => json!({"jsonrpc":"2.0","id":id,"error":{"code":c,"message":m}}) };
            writeln!(out, "{msg}").ok(); out.flush().ok();
        }
    }
}
