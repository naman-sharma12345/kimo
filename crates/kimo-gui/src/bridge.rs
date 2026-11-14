//! The shared-engine bridge. While the kimo app is open it owns ONE engine and ONE project; coding agents (Claude Code,
//! Codex, Antigravity — run by the app on the user's own subscription, or by the user in their own terminal/IDE) reach
//! that same engine through `kimo-gui mcp`, which attaches here instead of starting a private engine. So every agent edit
//! lands in the project the user is looking at, shows in the timeline and viewer, and gets an Undo card.
//!
//! Wire: loopback TCP only. The client's first line is `KIMO-BRIDGE/1 <token> <client-label>`; the app answers `OK`
//! (or closes). After that both sides speak newline-delimited JSON-RPC 2.0 (MCP over stdio, relayed byte for byte).
//! The port and the random token are in `bridge.json` in the app's data dir, readable only by this user; the file is
//! removed when the app exits. No app running → `kimo-gui mcp` runs a standalone engine exactly as before.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const HELLO: &str = "KIMO-BRIDGE/1";
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Info { pub port: u16, pub token: String, pub pid: u32, pub version: String }

/// The app's data directory, computed without Tauri so the `mcp` subcommand finds the same `bridge.json`
/// (Tauri's app_data_dir for identifier `dev.kimo.app`). `KIMO_DATA_DIR` overrides it (tests, portable installs).
pub fn data_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("KIMO_DATA_DIR") { return Some(PathBuf::from(d)); }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        crate::agents::home_dir().map(|h| h.join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| crate::agents::home_dir().map(|h| h.join(".local").join("share")))
    };
    base.map(|b| b.join("dev.kimo.app"))
}

pub fn info_path(dir: &Path) -> PathBuf { dir.join("bridge.json") }

/// 32 random hex chars from the OS (no extra dependency: SHA-512 over time, pid, address entropy and a fresh socket port).
pub fn token() -> String {
    use sha2::{Digest, Sha512};
    let mut h = Sha512::new();
    h.update(format!("{:?}{}", std::time::SystemTime::now(), std::process::id()));
    let local = 0u8; h.update(format!("{:p}", &local));
    if let Ok(l) = TcpListener::bind(("127.0.0.1", 0)) { if let Ok(a) = l.local_addr() { h.update(a.port().to_le_bytes()); } }
    for (k, v) in std::env::vars_os().take(8) { h.update(k.to_string_lossy().as_bytes()); h.update(v.to_string_lossy().as_bytes()); }
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// What the app answers for one JSON-RPC request from `client`: Ok(result) or Err((code, message)).
pub type Handler = Arc<dyn Fn(&str, &str, Value) -> Result<Value, (i64, String)> + Send + Sync>;
pub type OnConn = Arc<dyn Fn(&str, bool) + Send + Sync>;

pub struct Server { pub info: Info, file: Option<PathBuf> }
impl Server {
    pub fn remove_file(&self) { if let Some(f) = &self.file { let _ = std::fs::remove_file(f); } }
}
impl Drop for Server { fn drop(&mut self) { self.remove_file(); } }

/// Start listening on 127.0.0.1:<free port>; writes `bridge.json` into `dir` when given.
pub fn serve(dir: Option<&Path>, handler: Handler, on_conn: OnConn) -> std::io::Result<Server> {
    let l = TcpListener::bind(("127.0.0.1", 0))?;
    let info = Info { port: l.local_addr()?.port(), token: token(), pid: std::process::id(), version: env!("CARGO_PKG_VERSION").into() };
    let tok = info.token.clone();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let (h, t, c) = (handler.clone(), tok.clone(), on_conn.clone());
            std::thread::spawn(move || { let _ = session(s, &t, h, c); });
        }
    });
    let file = match dir {
        Some(d) => {
            std::fs::create_dir_all(d)?;
            let p = info_path(d);
            let tmp = p.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(&info).unwrap_or_default())?;
            restrict(&tmp);
            std::fs::rename(&tmp, &p)?;
            Some(p)
        }
        None => None,
    };
    Ok(Server { info, file })
}

#[cfg(unix)]
fn restrict(p: &Path) { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)); }
#[cfg(not(unix))]
fn restrict(_: &Path) {} // %APPDATA% is already per-user

/// A short, safe label for the Live tab: letters, digits, dash; "agent" when missing.
pub fn clean_label(s: &str) -> String {
    let c: String = s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(32).collect::<String>().to_ascii_lowercase();
    if c.is_empty() { "agent".into() } else { c }
}

fn session(s: TcpStream, token: &str, handler: Handler, on_conn: OnConn) -> std::io::Result<()> {
    // The hello must arrive quickly; after it, a client may stay quiet for minutes while its model thinks. The timeout
    // is set and cleared on the SAME handle the reader uses (a try_clone'd socket does not share it on Windows).
    let mut r = BufReader::new(s.try_clone()?);
    r.get_ref().set_read_timeout(Some(HELLO_TIMEOUT))?;
    let mut first = String::new();
    r.read_line(&mut first)?;
    let mut it = first.split_whitespace();
    let (Some(HELLO), Some(t), label) = (it.next(), it.next(), it.next()) else { return Ok(()) };
    if !constant_eq(t.as_bytes(), token.as_bytes()) { return Ok(()); }
    let label = clean_label(label.unwrap_or("agent"));
    r.get_ref().set_read_timeout(None)?;
    let w = Arc::new(Mutex::new(s.try_clone()?));
    w.lock().unwrap().write_all(b"OK\n")?;
    on_conn(&label, true);
    for line in r.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            write_line(&w, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}));
            continue;
        };
        let Some(id) = msg.get("id").cloned().filter(|i| !i.is_null()) else { continue }; // notifications need no answer
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let (h, w2, l2) = (handler.clone(), w.clone(), label.clone());
        // one thread per request: a long render does not block a quick read from the same client
        std::thread::spawn(move || {
            let out = match h(&l2, &method, params) {
                Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
            };
            write_line(&w2, &out);
        });
    }
    on_conn(&label, false);
    Ok(())
}

fn write_line(w: &Arc<Mutex<TcpStream>>, v: &Value) {
    let mut s = w.lock().unwrap();
    let _ = s.write_all(v.to_string().as_bytes()).and_then(|_| s.write_all(b"\n")).and_then(|_| s.flush());
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool { a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0 }

/// The MCP answers the app gives over the bridge (the engine's own tool list, routed through the ledger).
pub fn initialize_result(params: &Value, version: &str) -> Value {
    let proto = params.get("protocolVersion").and_then(|p| p.as_str()).unwrap_or("2024-11-05");
    json!({
        "protocolVersion": proto,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "kimo", "version": version},
        "instructions": "You are connected to the kimo video editor app that is open on this computer: one shared project the user is watching live. Every tool is a group taking {\"action\": ..., \"args\": {...}}. Call project.timeline_summary first to see the edit, and search_tools or kimo_describe when unsure of an action. Each change you make appears in the user's timeline at once and can be undone there. kimo shows you every edit: each call that changes the project returns `observe` (what it touched, before/after frames as an image, brightness numbers, and findings: mistakes with severity and the exact fix call). Read it after every edit and fix any error or warning before moving on. For anything that must hold, pass expect: [...] on the edit (visible/hidden at a time, inside title_safe, luma range, no_black_frames, readable, max_findings); if it fails the edit is rolled back and you are told why. Run review before export. Use observe: \"full\" for a sharper look, or render_frame/contact_sheet for any moment you want to see."
    })
}

/// Try to attach to a running app. Returns the connected stream after the `OK`, or why not.
pub fn connect(dir: &Path, label: &str) -> Result<TcpStream, String> {
    let raw = std::fs::read_to_string(info_path(dir)).map_err(|_| "the kimo app is not running".to_string())?;
    let info: Info = serde_json::from_str(&raw).map_err(|e| format!("bad bridge.json: {e}"))?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(800)).map_err(|_| "the kimo app is not running (stale bridge.json)".to_string())?;
    s.write_all(format!("{HELLO} {} {}\n", info.token, clean_label(label)).as_bytes()).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let mut ok = [0u8; 3];
    s.read_exact(&mut ok).map_err(|_| "the kimo app refused the connection".to_string())?;
    if &ok != b"OK\n" { return Err("the kimo app refused the connection".into()); }
    s.set_read_timeout(None).map_err(|e| e.to_string())?;
    Ok(s)
}

/// Relay stdin → app and app → stdout until either side closes. Used by `kimo-gui mcp` when the app is open.
pub fn relay_stdio(s: TcpStream) -> i32 {
    let mut up = match s.try_clone() { Ok(u) => u, Err(_) => return 1 };
    let t = std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let _ = std::io::copy(&mut stdin, &mut up);
        let _ = up.shutdown(std::net::Shutdown::Write);
    });
    let mut down = s;
    let mut stdout = std::io::stdout().lock();
    let mut buf = [0u8; 64 * 1024];
    loop {
        match down.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => { if stdout.write_all(&buf[..n]).and_then(|_| stdout.flush()).is_err() { break; } }
        }
    }
    drop(t);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(n: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("kimo-bridge-{n}-{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); d }

    fn echo_server(dir: &Path) -> (Server, Arc<Mutex<Vec<(String, bool)>>>) {
        let conns = Arc::new(Mutex::new(vec![]));
        let c2 = conns.clone();
        let h: Handler = Arc::new(|client, method, params| match method {
            "initialize" => Ok(initialize_result(&params, "9")),
            "tools/call" => Ok(json!({"content": [{"type": "text", "text": format!("{client} called {}", params["name"].as_str().unwrap_or(""))}]})),
            "boom" => Err((-32601, "no such method".into())),
            _ => Ok(json!({})),
        });
        (serve(Some(dir), h, Arc::new(move |l, up| c2.lock().unwrap().push((l.to_string(), up)))).unwrap(), conns)
    }

    fn rpc(s: &mut TcpStream, r: &mut BufReader<TcpStream>, v: Value) -> Value {
        s.write_all(format!("{v}\n").as_bytes()).unwrap();
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn attaches_with_token_and_answers_json_rpc() {
        let d = tmp("ok");
        let (srv, conns) = echo_server(&d);
        let info: Info = serde_json::from_str(&std::fs::read_to_string(info_path(&d)).unwrap()).unwrap();
        assert_eq!(info, srv.info);
        assert_eq!(info.token.len(), 32);
        let mut s = connect(&d, "Claude Code!").unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let init = rpc(&mut s, &mut r, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}));
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], "kimo");
        s.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").unwrap(); // no answer expected
        let call = rpc(&mut s, &mut r, json!({"jsonrpc":"2.0","id":"x","method":"tools/call","params":{"name":"timeline"}}));
        assert_eq!(call["id"], "x");
        assert_eq!(call["result"]["content"][0]["text"], "claudecode called timeline", "label is cleaned");
        let err = rpc(&mut s, &mut r, json!({"jsonrpc":"2.0","id":3,"method":"boom"}));
        assert_eq!(err["error"]["code"], -32601);
        // an agent thinks between calls: a quiet client must stay connected well past the hello timeout
        std::thread::sleep(HELLO_TIMEOUT + Duration::from_millis(700));
        let late = rpc(&mut s, &mut r, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"text"}}));
        assert_eq!(late["result"]["content"][0]["text"], "claudecode called text", "still attached after a quiet spell");
        drop((s, r));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(*conns.lock().unwrap(), vec![("claudecode".to_string(), true), ("claudecode".to_string(), false)]);
        drop(srv);
        assert!(!info_path(&d).exists(), "bridge.json is removed with the server");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn wrong_token_is_refused_and_missing_app_is_reported() {
        let d = tmp("bad");
        let (srv, _) = echo_server(&d);
        let mut s = TcpStream::connect(("127.0.0.1", srv.info.port)).unwrap();
        s.write_all(format!("{HELLO} 0000 x\n").as_bytes()).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut b = [0u8; 3];
        assert!(s.read_exact(&mut b).is_err(), "no OK for a wrong token");
        drop(srv);
        assert!(connect(&d, "x").unwrap_err().contains("not running"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn labels_are_sanitized() {
        assert_eq!(clean_label("codex"), "codex");
        assert_eq!(clean_label("../../evil name"), "evilname");
        assert_eq!(clean_label(""), "agent");
    }
}
