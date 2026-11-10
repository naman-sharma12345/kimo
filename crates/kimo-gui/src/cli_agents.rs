//! Coding agents on the user's OWN subscription: kimo runs the official CLIs the user installed and signed in to
//! (Claude Code with a Claude Pro/Max plan, Codex with a ChatGPT plan, Antigravity with a Google account) as headless
//! child processes, with the kimo engine attached over MCP through `kimo-gui mcp` (which joins the open app's engine).
//! kimo never reads, copies or stores their login tokens: the CLIs authenticate themselves exactly as in a terminal.
//! Sign-in is detected only by whether the CLI's own credential file EXISTS (never opened) or by the CLI's own status
//! command, and signing in happens in a terminal window running the CLI's own login.
//!
//! Each CLI's live output is a JSON-lines stream; the pure `parse_*` functions below turn one line into neutral
//! `Ev`ents (text, tool start/end, session id, final reply, error) so the chat UI is the same for all three.
use crate::agents::{home_dir, which};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind { ClaudeCode, Codex, Antigravity }

impl Kind {
    pub fn all() -> [Kind; 3] { [Kind::ClaudeCode, Kind::Codex, Kind::Antigravity] }
    pub fn id(self) -> &'static str { match self { Kind::ClaudeCode => "claude-code", Kind::Codex => "codex", Kind::Antigravity => "antigravity" } }
    pub fn from_id(s: &str) -> Option<Kind> { Kind::all().into_iter().find(|k| k.id() == s) }
    pub fn name(self) -> &'static str { match self { Kind::ClaudeCode => "Claude Code", Kind::Codex => "Codex", Kind::Antigravity => "Antigravity" } }
    pub fn plan(self) -> &'static str { match self { Kind::ClaudeCode => "Claude Pro or Max plan", Kind::Codex => "ChatGPT Plus, Pro, Business or Enterprise plan", Kind::Antigravity => "Google account" } }
    pub fn bin_name(self) -> &'static str { match self { Kind::ClaudeCode => "claude", Kind::Codex => "codex", Kind::Antigravity => "agy" } }
    /// The terminal command that installs the CLI (npm), or None when it ships as its own installer.
    pub fn install_cmd(self) -> Option<&'static str> {
        match self { Kind::ClaudeCode => Some("npm install -g @anthropic-ai/claude-code"), Kind::Codex => Some("npm install -g @openai/codex"), Kind::Antigravity => None }
    }
    pub fn install_url(self) -> &'static str {
        match self { Kind::ClaudeCode => "https://docs.claude.com/en/docs/claude-code/setup", Kind::Codex => "https://developers.openai.com/codex/cli", Kind::Antigravity => "https://antigravity.google/download" }
    }
    /// What the sign-in terminal runs: the CLI's own login.
    pub fn login_cmd(self) -> &'static str { match self { Kind::ClaudeCode => "claude /login", Kind::Codex => "codex login", Kind::Antigravity => "agy" } }
}

/// Where each CLI usually lives besides PATH (GUI apps on macOS/Linux often start with a short PATH).
fn extra_dirs(k: Kind) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(h) = home_dir() {
        v.push(h.join(".local").join("bin"));
        v.push(h.join(".npm-global").join("bin"));
        v.push(h.join(".bun").join("bin"));
        v.push(h.join(".volta").join("bin"));
        if k == Kind::ClaudeCode { v.push(h.join(".claude").join("local")); }
        if k == Kind::Antigravity { v.push(h.join(".antigravity").join("bin")); v.push(h.join(".gemini").join("antigravity-cli").join("bin")); }
    }
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") { v.push(PathBuf::from(a).join("npm")); }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            let l = PathBuf::from(l);
            v.push(l.join("agy").join("bin"));
            v.push(l.join("Programs").join("Antigravity").join("bin"));
            v.push(l.join("Programs").join("claude"));
        }
    } else {
        v.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    }
    v
}

pub fn find(k: Kind) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(format!("KIMO_{}_BIN", k.bin_name().to_ascii_uppercase())).map(PathBuf::from).filter(|p| p.is_file()) { return Some(p); }
    if let Some(p) = which(k.bin_name()) { return Some(p); }
    let dirs: Vec<String> = extra_dirs(k).iter().map(|d| d.display().to_string()).collect();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let pathext = if cfg!(windows) { Some(".EXE;.CMD;.BAT") } else { None };
    crate::agents::which_in(k.bin_name(), &dirs.join(sep), pathext)
}

/// A child with no console window on Windows (the app is a GUI; a flashing console would be alarming).
fn quiet(c: &mut Command) -> &mut Command {
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
    c
}

/// Run `bin args` with a timeout; Some((exit ok, stdout+stderr)).
fn run_short(bin: &Path, args: &[&str], secs: u64) -> Option<(bool, String)> {
    let mut c = Command::new(bin);
    quiet(&mut c).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().ok()?;
    let t0 = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                let mut out = String::new();
                if let Some(mut o) = child.stdout.take() { let _ = o.read_to_string(&mut out); }
                if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut out); }
                return Some((st.success(), out));
            }
            Ok(None) if t0.elapsed() < Duration::from_secs(secs) => std::thread::sleep(Duration::from_millis(50)),
            _ => { let _ = child.kill(); let _ = child.wait(); return None; }
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct Status {
    pub id: &'static str,
    pub name: &'static str,
    pub plan: &'static str,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// Some(true): signed in. Some(false): definitely not. None: cannot tell without running it (Test checks for real).
    pub signed_in: Option<bool>,
    pub sign_in_note: String,
    pub install_cmd: Option<&'static str>,
    pub install_url: &'static str,
    pub login_cmd: &'static str,
    /// kimo is registered in the CLI's own MCP config (so the user's own sessions can use it too).
    pub registered: bool,
}

fn first_line(s: &str) -> Option<String> { s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(80).collect()) }

/// Read-only: never opens a credential file, only checks that it exists.
pub fn status(k: Kind) -> Status {
    let path = find(k);
    let version = path.as_ref().and_then(|p| run_short(p, &["--version"], 8)).and_then(|(_, o)| first_line(&o));
    let home = home_dir();
    let exists = |rel: &[&str]| home.as_ref().map(|h| rel.iter().fold(h.clone(), |p, s| p.join(s)).is_file()).unwrap_or(false);
    let (signed_in, note) = match k {
        Kind::ClaudeCode => {
            if std::env::var_os("ANTHROPIC_API_KEY").is_some() { (Some(true), "Signed in through ANTHROPIC_API_KEY in your environment (that bills the API, not your plan).".to_string()) }
            else if exists(&[".claude", ".credentials.json"]) { (Some(true), "Signed in (Claude Code's own login).".into()) }
            else if cfg!(target_os = "macos") && path.is_some() { (None, "Claude Code keeps its login in the macOS Keychain; press Test to check.".into()) }
            else { (Some(false), "Not signed in yet. Sign in once with your Claude plan.".into()) }
        }
        Kind::Codex => match path.as_ref().and_then(|p| run_short(p, &["login", "status"], 10)) {
            Some((true, out)) => (Some(true), first_line(&out).unwrap_or_else(|| "Signed in".into())),
            Some((false, out)) => (Some(false), first_line(&out).unwrap_or_else(|| "Not signed in".into())),
            None if exists(&[".codex", "auth.json"]) => (Some(true), "Signed in (Codex's own login).".into()),
            None => (Some(false), "Not signed in yet. Sign in once with ChatGPT.".into()),
        },
        Kind::Antigravity => {
            if exists(&[".gemini", "oauth_creds.json"]) || exists(&[".gemini", "google_accounts.json"]) { (Some(true), "Signed in with your Google account.".into()) }
            else { (Some(false), "Not signed in yet. Open Antigravity once and sign in with Google.".into()) }
        }
    };
    Status {
        id: k.id(), name: k.name(), plan: k.plan(), installed: path.is_some(), path: path.map(|p| p.display().to_string()), version,
        signed_in: if path_missing(&k) { Some(false) } else { signed_in }, sign_in_note: note,
        install_cmd: k.install_cmd(), install_url: k.install_url(), login_cmd: k.login_cmd(), registered: registered(k),
    }
}
fn path_missing(k: &Kind) -> bool { find(*k).is_none() }

/// Is a `kimo` server in the CLI's user-level MCP config? (read-only, text match on the server name)
pub fn registered(k: Kind) -> bool {
    let Some(h) = home_dir() else { return false };
    match k {
        Kind::ClaudeCode => std::fs::read_to_string(h.join(".claude.json")).ok().and_then(|s| crate::agents::claude_entry(&s)).is_some(),
        Kind::Codex => std::fs::read_to_string(crate::agents::codex_config_path(&h)).ok().and_then(|s| crate::agents::codex_entry(&s)).is_some(),
        Kind::Antigravity => [h.join(".gemini").join("config").join("mcp_config.json"), h.join(".gemini").join("antigravity").join("mcp_config.json"), h.join(".gemini").join("antigravity-cli").join("mcp_config.json"), h.join(".gemini").join("settings.json")]
            .iter().filter_map(|p| std::fs::read_to_string(p).ok()).any(|s| serde_json::from_str::<Value>(&s).ok().and_then(|v| v.get("mcpServers").and_then(|m| m.get("kimo")).cloned()).is_some()),
    }
}

/// The launch every agent uses for kimo: this app's own binary in `mcp` mode (joins the open app, else standalone).
#[derive(Clone, Debug)]
pub struct KimoLaunch { pub command: String, pub env: Vec<(String, String)> }
impl KimoLaunch {
    pub fn args(&self, k: Kind) -> Vec<String> { vec!["mcp".into(), "--client".into(), k.id().into()] }
}

/// Register kimo in the CLI's own user-level MCP config, using the CLI's own `mcp add` (argv, no shell).
pub fn register(k: Kind, bin: &Path, l: &KimoLaunch) -> Result<String, String> {
    let mut args: Vec<String> = match k {
        Kind::ClaudeCode => {
            let mut o = serde_json::Map::new();
            o.insert("type".into(), json!("stdio")); o.insert("command".into(), json!(l.command)); o.insert("args".into(), json!(l.args(k)));
            if !l.env.is_empty() { o.insert("env".into(), json!(l.env.iter().cloned().collect::<std::collections::BTreeMap<_, _>>())); }
            return run_register(bin, &["mcp".into(), "add-json".into(), "--scope".into(), "user".into(), "kimo".into(), Value::Object(o).to_string()]);
        }
        Kind::Codex => vec!["mcp".into(), "add".into(), "kimo".into()],
        Kind::Antigravity => vec!["mcp".into(), "add".into()],
    };
    for (key, v) in &l.env { args.push("--env".into()); args.push(format!("{key}={v}")); }
    if k == Kind::Antigravity { args.push("kimo".into()); }
    args.push("--".into());
    args.push(l.command.clone());
    args.extend(l.args(k));
    run_register(bin, &args)
}
fn run_register(bin: &Path, args: &[String]) -> Result<String, String> {
    let mut c = Command::new(bin);
    let out = quiet(&mut c).args(args).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if out.status.success() { Ok(if text.is_empty() { "Added kimo.".into() } else { text }) } else { Err(if text.is_empty() { format!("exited with {}", out.status) } else { text }) }
}

/// Open a visible terminal running `cmdline` (sign in / install). The user sees and drives it.
pub fn open_terminal(cmdline: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        // `start` opens a new console window; /k keeps it open so the user can read the result. cmd.exe does its own
        // quote parsing, so the line is passed raw, wrapped in one extra pair of quotes that /k strips.
        use std::os::windows::process::CommandExt;
        Command::new("cmd").raw_arg(format!("/c start \"kimo\" cmd /k \"{cmdline}\"")).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!("tell application \"Terminal\" to do script \"{}\"\ntell application \"Terminal\" to activate", cmdline.replace('\\', "\\\\").replace('"', "\\\""));
        Command::new("osascript").args(["-e", &script]).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let hold = format!("{cmdline}; echo; read -p 'Press Enter to close'");
        for (t, pre) in [("x-terminal-emulator", vec!["-e"]), ("gnome-terminal", vec!["--"]), ("konsole", vec!["-e"]), ("xfce4-terminal", vec!["-x"]), ("kitty", vec![]), ("alacritty", vec!["-e"]), ("xterm", vec!["-e"])] {
            if which(t).is_some() {
                let mut c = Command::new(t);
                c.args(pre).args(["sh", "-c", &hold]);
                return c.spawn().map(|_| ()).map_err(|e| e.to_string());
            }
        }
        Err("No terminal app found. Run this yourself: ".to_string() + cmdline)
    }
}

/// Open an https URL in the user's browser.
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") { return Err("only https links".into()); }
    #[cfg(windows)]
    { let mut c = Command::new("rundll32"); quiet(&mut c).args(["url.dll,FileProtocolHandler", url]).spawn().map(|_| ()).map_err(|e| e.to_string()) }
    #[cfg(target_os = "macos")]
    { Command::new("open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string()) }
    #[cfg(all(unix, not(target_os = "macos")))]
    { Command::new("xdg-open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string()) }
}

/// Show a file in the system file manager.
pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    { use std::os::windows::process::CommandExt; Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn().map(|_| ()).map_err(|e| e.to_string()) }
    #[cfg(target_os = "macos")]
    { Command::new("open").arg("-R").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string()) }
    #[cfg(all(unix, not(target_os = "macos")))]
    { Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn().map(|_| ()).map_err(|e| e.to_string()) }
}

// ------------------------------------------------------------------------------------------------------------------
// One turn

/// Neutral events the chat UI renders, whichever CLI produced them.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ev {
    Session { id: String },
    /// Streaming text (append).
    Delta { text: String },
    /// A whole text block (replace the streamed block with it).
    Text { text: String },
    Thinking { text: String },
    ToolStart { id: String, tool: String, input: Value },
    ToolEnd { id: String, ok: bool, output: String },
    Status { text: String },
    Done { text: String, cost_usd: Option<f64>, turns: Option<u64> },
    Error { message: String },
}

pub const SYSTEM: &str = "You are the editing agent inside kimo, an AI video editor app open on the user's computer. \
The user is watching their project live: every change you make shows up in their timeline and viewer at once and can be \
undone there. Edit ONLY through the kimo MCP tools (server kimo): each tool is a group taking an object with an action name and its args. \
Start with project.timeline_summary (or project.get_project) to see the edit, and use search_tools / kimo_describe when you \
are unsure of an action or its arguments. Make exactly the change asked for, the smallest edit that does it. Do not read, \
write or run anything on the computer other than kimo tools, and do not export or delete files unless asked. When done, \
reply in one or two plain sentences saying what you changed.";

/// Display name of a tool: `mcp__kimo__timeline` → `timeline`.
pub fn short_tool(name: &str) -> String {
    name.strip_prefix("mcp__kimo__").or_else(|| name.strip_prefix("kimo__")).or_else(|| name.strip_prefix("kimo.")).unwrap_or(name).to_string()
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|b| b.get("text").and_then(|t| t.as_str()).map(String::from).unwrap_or_else(|| b.as_str().map(String::from).unwrap_or_default())).collect::<Vec<_>>().join("\n"),
        Value::Null => String::new(),
        o => o.get("text").and_then(|t| t.as_str()).map(String::from).unwrap_or_else(|| o.to_string()),
    }
}
fn cut(s: &str, n: usize) -> String { if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() } }

/// Claude Code `-p --output-format stream-json --verbose --include-partial-messages`.
pub fn parse_claude(v: &Value) -> Vec<Ev> {
    let mut out = vec![];
    match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
        "system" if v["subtype"] == "init" => {
            if let Some(id) = v["session_id"].as_str() { out.push(Ev::Session { id: id.into() }); }
            if let Some(servers) = v["mcp_servers"].as_array() {
                for s in servers.iter().filter(|s| s["name"] == "kimo") {
                    let st = s["status"].as_str().unwrap_or("");
                    if st != "connected" { out.push(Ev::Error { message: format!("Claude Code could not start the kimo tools ({st}). Restart kimo and try again.") }); }
                }
            }
        }
        "stream_event" => {
            let e = &v["event"];
            if e["type"] == "content_block_delta" {
                if let Some(t) = e["delta"]["text"].as_str() { out.push(Ev::Delta { text: t.into() }); }
                if let Some(t) = e["delta"]["thinking"].as_str() { out.push(Ev::Thinking { text: t.into() }); }
            }
        }
        "assistant" => for b in v["message"]["content"].as_array().into_iter().flatten() {
            match b["type"].as_str() {
                Some("text") => out.push(Ev::Text { text: b["text"].as_str().unwrap_or("").into() }),
                Some("tool_use") => out.push(Ev::ToolStart { id: b["id"].as_str().unwrap_or("").into(), tool: short_tool(b["name"].as_str().unwrap_or("")), input: b["input"].clone() }),
                _ => {}
            }
        },
        "user" => for b in v["message"]["content"].as_array().into_iter().flatten() {
            if b["type"] == "tool_result" {
                out.push(Ev::ToolEnd { id: b["tool_use_id"].as_str().unwrap_or("").into(), ok: !b["is_error"].as_bool().unwrap_or(false), output: cut(&text_of(&b["content"]), 600) });
            }
        },
        "result" => {
            if let Some(id) = v["session_id"].as_str() { out.push(Ev::Session { id: id.into() }); }
            let text = v["result"].as_str().unwrap_or("").to_string();
            if v["is_error"].as_bool().unwrap_or(false) || v["subtype"].as_str().map(|s| s.starts_with("error")).unwrap_or(false) {
                out.push(Ev::Error { message: if text.is_empty() { format!("Claude Code stopped: {}", v["subtype"].as_str().unwrap_or("error")) } else { text } });
            } else {
                out.push(Ev::Done { text, cost_usd: v["total_cost_usd"].as_f64(), turns: v["num_turns"].as_u64() });
            }
        }
        _ => {}
    }
    out
}

/// Codex `exec --json` (thread/turn/item events).
pub fn parse_codex(v: &Value) -> Vec<Ev> {
    let mut out = vec![];
    let item = &v["item"];
    let id = item["id"].as_str().unwrap_or("").to_string();
    match v["type"].as_str().unwrap_or("") {
        "thread.started" => if let Some(t) = v["thread_id"].as_str() { out.push(Ev::Session { id: t.into() }); },
        "item.started" | "item.updated" if item["type"] == "mcp_tool_call" && v["type"] == "item.started" => {
            out.push(Ev::ToolStart { id, tool: short_tool(item["tool"].as_str().unwrap_or("")), input: item["arguments"].clone() });
        }
        "item.started" if item["type"] == "command_execution" => {
            out.push(Ev::ToolStart { id, tool: "shell".into(), input: json!({"command": item["command"]}) });
        }
        "item.completed" => match item["type"].as_str().unwrap_or("") {
            "agent_message" => out.push(Ev::Text { text: item["text"].as_str().unwrap_or("").into() }),
            "reasoning" => out.push(Ev::Thinking { text: item["text"].as_str().unwrap_or("").into() }),
            "mcp_tool_call" => {
                let failed = item["status"] == "failed" || !item["error"].is_null() && item.get("error").is_some();
                let output = if !item["error"].is_null() && item.get("error").is_some() { text_of(&item["error"]["message"]) } else { text_of(&item["result"]["content"]) };
                out.push(Ev::ToolEnd { id, ok: !failed, output: cut(&output, 600) });
            }
            "command_execution" => out.push(Ev::ToolEnd { id, ok: item["exit_code"].as_i64() == Some(0), output: cut(item["aggregated_output"].as_str().unwrap_or(""), 600) }),
            "error" => out.push(Ev::Error { message: item["message"].as_str().unwrap_or("error").into() }),
            _ => {}
        },
        "turn.failed" => out.push(Ev::Error { message: v["error"]["message"].as_str().unwrap_or("Codex stopped with an error").into() }),
        "error" => out.push(Ev::Error { message: v["message"].as_str().unwrap_or("Codex error").into() }),
        "turn.completed" => out.push(Ev::Status { text: "turn complete".into() }),
        _ => {}
    }
    out
}

/// Antigravity CLI `-p --output-format stream-json` (init / step_update / result).
pub fn parse_agy(v: &Value) -> Vec<Ev> {
    let mut out = vec![];
    match v["event"].as_str().unwrap_or("") {
        "init" => if let Some(id) = v["conversation_id"].as_str() { out.push(Ev::Session { id: id.into() }); },
        "step_update" => {
            let s = &v["step_update"];
            let ty = s["step_type"].as_str().unwrap_or("");
            let state = s["state"].as_str().unwrap_or("");
            let id = format!("s{}", s["step_index"].as_u64().unwrap_or(0));
            match ty {
                "user_input" => {}
                "agent_response" => { if let Some(t) = s["text_delta"].as_str() { if !t.is_empty() { out.push(Ev::Delta { text: t.into() }); } } }
                "thinking" | "planner_thinking" => { if let Some(t) = s["text_delta"].as_str() { out.push(Ev::Thinking { text: t.into() }); } }
                "tool" => {
                    // agy 1.4+: {"step_type":"tool","tool_info":{"name","parameters","output"|"error"}}; MCP calls are
                    // name=call_mcp_tool with parameters {ServerName, ToolName, Arguments}
                    let ti = &s["tool_info"];
                    let name = ti["name"].as_str().or(s["tool_name"].as_str()).unwrap_or("tool");
                    let p = &ti["parameters"];
                    let (tool, input) = if name == "call_mcp_tool" {
                        let server = p["ServerName"].as_str().unwrap_or("");
                        let t = short_tool(p["ToolName"].as_str().unwrap_or("mcp"));
                        (if server.is_empty() || server == "kimo" { t } else { format!("{server}.{t}") }, p["Arguments"].clone())
                    } else { (name.to_string(), p.clone()) };
                    match state {
                        "ACTIVE" | "RUNNING" | "PENDING" => out.push(Ev::ToolStart { id, tool, input }),
                        "DONE" => {
                            out.push(Ev::ToolStart { id: id.clone(), tool, input });
                            out.push(Ev::ToolEnd { id, ok: true, output: cut(&text_of(&ti["output"]), 600) });
                        }
                        "ERROR" | "FAILED" | "CANCELED" | "CANCELLED" => {
                            out.push(Ev::ToolStart { id: id.clone(), tool, input });
                            let msg = ti["error"]["message"].as_str().map(String::from).unwrap_or_else(|| text_of(&ti["error"]));
                            out.push(Ev::ToolEnd { id, ok: false, output: cut(&msg, 600) });
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        "result" => {
            let r = &v["result"];
            if let Some(id) = r["conversation_id"].as_str() { out.push(Ev::Session { id: id.into() }); }
            let text = r["response"].as_str().unwrap_or("").trim_end().to_string();
            if r["status"] == "SUCCESS" { out.push(Ev::Done { text, cost_usd: None, turns: r["num_turns"].as_u64() }); }
            else { out.push(Ev::Error { message: if text.is_empty() { format!("Antigravity stopped: {}", r["status"].as_str().unwrap_or("error")) } else { text } }); }
        }
        "error" => out.push(Ev::Error { message: v["error"]["message"].as_str().or(v["message"].as_str()).unwrap_or("Antigravity error").into() }),
        _ => {}
    }
    out
}

pub struct Turn<'a> {
    pub kind: Kind,
    pub bin: &'a Path,
    pub prompt: &'a str,
    pub session: Option<&'a str>,
    pub model: Option<&'a str>,
    pub launch: &'a KimoLaunch,
    /// The agent's working directory: an empty kimo workspace folder, never the user's files.
    pub workdir: &'a Path,
}

/// The argv for one turn, plus what goes on stdin (the prompt is piped, never put on a command line, except for
/// Antigravity, whose print mode takes it as an argument).
pub fn command(t: &Turn) -> Result<(Vec<String>, Option<String>), String> {
    let first = t.session.is_none();
    let prompt = if first && t.kind != Kind::ClaudeCode { format!("{SYSTEM}\n\nUser request: {}", t.prompt) } else { t.prompt.to_string() };
    let mut a: Vec<String> = vec![];
    match t.kind {
        Kind::ClaudeCode => {
            let cfg = t.workdir.join(".kimo-mcp.json");
            let mut server = serde_json::Map::new();
            server.insert("type".into(), json!("stdio")); server.insert("command".into(), json!(t.launch.command)); server.insert("args".into(), json!(t.launch.args(t.kind)));
            if !t.launch.env.is_empty() { server.insert("env".into(), json!(t.launch.env.iter().cloned().collect::<std::collections::BTreeMap<_, _>>())); }
            std::fs::write(&cfg, json!({"mcpServers": {"kimo": server}}).to_string()).map_err(|e| format!("could not write {}: {e}", cfg.display()))?;
            a.extend(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--strict-mcp-config", "--mcp-config"].map(String::from));
            a.push(cfg.display().to_string());
            a.extend(["--allowedTools", "mcp__kimo", "--disallowedTools", "Bash,Edit,Write,MultiEdit,NotebookEdit,WebFetch,WebSearch,Task", "--append-system-prompt", SYSTEM].map(String::from));
            if let Some(s) = t.session { a.push("--resume".into()); a.push(s.into()); }
            if let Some(m) = t.model.filter(|m| !m.is_empty()) { a.push("--model".into()); a.push(m.into()); }
            Ok((a, Some(prompt)))
        }
        Kind::Codex => {
            a.push("exec".into());
            if let Some(s) = t.session { a.push("resume".into()); a.push(s.into()); }
            a.extend(["--json", "--skip-git-repo-check"].map(String::from));
            if t.session.is_none() { a.extend(["--sandbox".into(), "read-only".into(), "-C".into(), t.workdir.display().to_string()]); }
            // TOML literal strings ('...') keep Windows backslashes as they are
            a.push("-c".into()); a.push(format!("mcp_servers.kimo.command='{}'", t.launch.command));
            a.push("-c".into()); a.push(format!("mcp_servers.kimo.args=[{}]", t.launch.args(t.kind).iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(",")));
            for (k, v) in &t.launch.env { a.push("-c".into()); a.push(format!("mcp_servers.kimo.env.{k}='{v}'")); }
            a.extend(["-c", "mcp_servers.kimo.startup_timeout_sec=30", "-c", "mcp_servers.kimo.tool_timeout_sec=900", "-c", "approval_policy=\"never\""].map(String::from));
            if let Some(m) = t.model.filter(|m| !m.is_empty()) { a.push("-m".into()); a.push(m.into()); }
            a.push("-".into()); // prompt from stdin
            Ok((a, Some(prompt)))
        }
        Kind::Antigravity => {
            a.extend(["-p".into(), prompt, "--output-format".into(), "stream-json".into(), "--disable-slash-commands".into()]);
            if let Some(s) = t.session { a.push("--conversation".into()); a.push(s.into()); }
            if let Some(m) = t.model.filter(|m| !m.is_empty()) { a.push("--model".into()); a.push(m.into()); }
            Ok((a, None))
        }
    }
}

/// The running child of the current turn, so Stop can end it.
pub type Slot = Arc<Mutex<Option<Child>>>;

pub struct Outcome { pub session: Option<String>, pub reply: String, pub error: Option<String> }

/// Run one turn to the end, streaming events. Blocking; call it on a worker thread.
pub fn run(t: &Turn, slot: &Slot, emit: &mut dyn FnMut(Ev)) -> Outcome {
    let (args, stdin) = match command(t) { Ok(x) => x, Err(e) => return Outcome { session: None, reply: String::new(), error: Some(e) } };
    let mut c = Command::new(t.bin);
    quiet(&mut c).args(&args).current_dir(t.workdir).stdout(Stdio::piped()).stderr(Stdio::piped())
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .env("NO_COLOR", "1").env("CI", "1");
    let mut child = match c.spawn() { Ok(ch) => ch, Err(e) => return Outcome { session: None, reply: String::new(), error: Some(format!("Could not start {}: {e}", t.kind.name())) } };
    if let (Some(p), Some(mut w)) = (stdin, child.stdin.take()) { let _ = w.write_all(p.as_bytes()); drop(w); }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *slot.lock().unwrap() = Some(child);
    let err_buf = Arc::new(Mutex::new(String::new()));
    let eb = err_buf.clone();
    let et = std::thread::spawn(move || { if let Some(e) = stderr { for l in BufReader::new(e).lines().map_while(Result::ok) { let mut b = eb.lock().unwrap(); if b.len() < 8000 { b.push_str(&l); b.push('\n'); } } } });
    let mut o = Outcome { session: t.session.map(String::from), reply: String::new(), error: None };
    let mut streamed = String::new();
    if let Some(out) = stdout {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
            let evs = match t.kind { Kind::ClaudeCode => parse_claude(&v), Kind::Codex => parse_codex(&v), Kind::Antigravity => parse_agy(&v) };
            for ev in evs {
                match &ev {
                    Ev::Session { id } => o.session = Some(id.clone()),
                    Ev::Delta { text } => streamed.push_str(text),
                    Ev::Text { text } => { o.reply = text.clone(); streamed.clear(); }
                    Ev::Done { text, .. } => if !text.is_empty() { o.reply = text.clone(); } else if o.reply.is_empty() { o.reply = streamed.clone(); },
                    Ev::Error { message } => o.error = Some(message.clone()),
                    _ => {}
                }
                emit(ev);
            }
        }
    }
    let status = slot.lock().unwrap().take().map(|mut ch| ch.wait());
    let _ = et.join();
    if o.reply.is_empty() { o.reply = streamed; }
    let killed = status.is_none();
    if o.error.is_none() {
        match status {
            Some(Ok(st)) if !st.success() && o.reply.is_empty() => {
                let e = err_buf.lock().unwrap().clone();
                o.error = Some(explain(t.kind, &e).unwrap_or_else(|| format!("{} exited with {st}. {}", t.kind.name(), cut(e.trim(), 400))));
            }
            None if killed => o.error = Some("Stopped.".into()),
            _ => {}
        }
    }
    o
}

/// Turn common CLI failures into a plain next step.
pub fn explain(k: Kind, stderr: &str) -> Option<String> {
    let s = stderr.to_ascii_lowercase();
    if s.contains("login") || s.contains("log in") || s.contains("not logged") || s.contains("unauthorized") || s.contains("401") || s.contains("authenticat") || s.contains("sign in") {
        return Some(format!("{} is not signed in. Open Connections and press Sign in, then try again.", k.name()));
    }
    if s.contains("rate limit") || s.contains("usage limit") || s.contains("quota") || s.contains("429") {
        return Some(format!("Your {} plan's usage limit was reached. Try again later or switch agents.", k.name()));
    }
    None
}

/// Stop the running turn (if any).
pub fn stop(slot: &Slot) -> bool {
    if let Some(ch) = slot.lock().unwrap().as_mut() { let _ = ch.kill(); return true; }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_stream() {
        let lines = [
            json!({"type":"system","subtype":"init","session_id":"S1","mcp_servers":[{"name":"kimo","status":"connected"}]}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Adding "}}}),
            json!({"type":"assistant","message":{"content":[{"type":"text","text":"Adding a title."},{"type":"tool_use","id":"tu1","name":"mcp__kimo__text","input":{"action":"add_text","args":{"text":"Hi"}}}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":[{"type":"text","text":"{\"clip_id\":\"clip3\"}"}]}]}}),
            json!({"type":"result","subtype":"success","is_error":false,"result":"Added a title.","session_id":"S1","total_cost_usd":0.01,"num_turns":2}),
        ];
        let evs: Vec<Ev> = lines.iter().flat_map(parse_claude).collect();
        assert_eq!(evs[0], Ev::Session { id: "S1".into() });
        assert_eq!(evs[1], Ev::Delta { text: "Adding ".into() });
        assert_eq!(evs[3], Ev::ToolStart { id: "tu1".into(), tool: "text".into(), input: json!({"action":"add_text","args":{"text":"Hi"}}) });
        assert_eq!(evs[4], Ev::ToolEnd { id: "tu1".into(), ok: true, output: "{\"clip_id\":\"clip3\"}".into() });
        assert!(matches!(&evs[6], Ev::Done { text, turns: Some(2), .. } if text == "Added a title."));
        let bad = parse_claude(&json!({"type":"system","subtype":"init","session_id":"S","mcp_servers":[{"name":"kimo","status":"failed"}]}));
        assert!(matches!(&bad[1], Ev::Error { .. }));
        let err = parse_claude(&json!({"type":"result","subtype":"error_max_turns","is_error":true,"session_id":"S"}));
        assert!(matches!(&err[1], Ev::Error { message } if message.contains("error_max_turns")));
    }

    #[test]
    fn codex_stream() {
        let lines = [
            json!({"type":"thread.started","thread_id":"T1"}),
            json!({"type":"turn.started"}),
            json!({"type":"item.completed","item":{"id":"i0","type":"reasoning","text":"Plan"}}),
            json!({"type":"item.started","item":{"id":"i1","type":"mcp_tool_call","server":"kimo","tool":"timeline","arguments":{"action":"add_track"},"status":"in_progress"}}),
            json!({"type":"item.completed","item":{"id":"i1","type":"mcp_tool_call","server":"kimo","tool":"timeline","arguments":{"action":"add_track"},"result":{"content":[{"type":"text","text":"{\"track_id\":\"track1\"}"}]},"error":null,"status":"completed"}}),
            json!({"type":"item.completed","item":{"id":"i2","type":"agent_message","text":"Added a track."}}),
            json!({"type":"turn.completed","usage":{}}),
        ];
        let evs: Vec<Ev> = lines.iter().flat_map(parse_codex).collect();
        assert_eq!(evs[0], Ev::Session { id: "T1".into() });
        assert_eq!(evs[1], Ev::Thinking { text: "Plan".into() });
        assert_eq!(evs[2], Ev::ToolStart { id: "i1".into(), tool: "timeline".into(), input: json!({"action":"add_track"}) });
        assert_eq!(evs[3], Ev::ToolEnd { id: "i1".into(), ok: true, output: "{\"track_id\":\"track1\"}".into() });
        assert_eq!(evs[4], Ev::Text { text: "Added a track.".into() });
        let failed = parse_codex(&json!({"type":"item.completed","item":{"id":"i9","type":"mcp_tool_call","tool":"timeline","error":{"message":"no such clip"},"status":"failed"}}));
        assert_eq!(failed, vec![Ev::ToolEnd { id: "i9".into(), ok: false, output: "no such clip".into() }]);
        assert!(matches!(&parse_codex(&json!({"type":"turn.failed","error":{"message":"usage limit"}}))[0], Ev::Error { .. }));
    }

    #[test]
    fn antigravity_stream_as_captured_from_agy_1_4_6() {
        // exact lines from `agy -p ... --output-format stream-json` (agy 1.4.6)
        let lines = [
            r#"{"event":"init","conversation_id":"66e9","init":{"cwd":"C:\\x","tools":["call_mcp_tool"],"permission_mode":"always-proceed"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"66e9","step_index":0,"state":"DONE","step_type":"user_input"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"66e9","step_index":1,"state":"ACTIVE","step_type":"agent_response","text_delta":"ok"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"66e9","step_index":1,"state":"DONE","step_type":"agent_response","text_delta":"\n","duration_seconds":4.7}}"#,
            r#"{"event":"result","result":{"conversation_id":"66e9","status":"SUCCESS","response":"ok\n","duration_seconds":5.4,"num_turns":1}}"#,
        ];
        let evs: Vec<Ev> = lines.iter().flat_map(|l| parse_agy(&serde_json::from_str(l).unwrap())).collect();
        assert_eq!(evs[0], Ev::Session { id: "66e9".into() });
        assert_eq!(evs[1], Ev::Delta { text: "ok".into() });
        assert_eq!(evs[2], Ev::Delta { text: "\n".into() });
        assert_eq!(evs[4], Ev::Done { text: "ok".into(), cost_usd: None, turns: Some(1) });
        // tool steps exactly as agy 1.5.5 streams them
        let start = parse_agy(&serde_json::from_str(r#"{"event":"step_update","step_update":{"step_index":4,"state":"ACTIVE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{"action":"add_track"},"ServerName":"kimo","ToolName":"timeline"}}}}"#).unwrap());
        assert_eq!(start, vec![Ev::ToolStart { id: "s4".into(), tool: "timeline".into(), input: json!({"action":"add_track"}) }]);
        let done = parse_agy(&serde_json::from_str(r#"{"event":"step_update","step_update":{"step_index":4,"state":"DONE","step_type":"tool","tool_name":"call_mcp_tool","tool_info":{"name":"call_mcp_tool","parameters":{"Arguments":{"action":"add_track"},"ServerName":"kimo","ToolName":"timeline"},"output":"{\"track_id\":\"track1\"}"}}}"#).unwrap());
        assert_eq!(done[1], Ev::ToolEnd { id: "s4".into(), ok: true, output: "{\"track_id\":\"track1\"}".into() });
        let err = parse_agy(&serde_json::from_str(r#"{"event":"step_update","step_update":{"step_index":4,"state":"ERROR","step_type":"tool","tool_info":{"name":"call_mcp_tool","parameters":{"ServerName":"kimo","ToolName":"project"},"error":{"type":"TOOL_ERROR","message":"server name kimo failed to load"}}}}"#).unwrap());
        assert_eq!(err[1], Ev::ToolEnd { id: "s4".into(), ok: false, output: "server name kimo failed to load".into() });
        let other = parse_agy(&serde_json::from_str(r#"{"event":"step_update","step_update":{"step_index":2,"state":"ACTIVE","step_type":"tool","tool_info":{"name":"read_file","parameters":{"Path":"x"}}}}"#).unwrap());
        assert!(matches!(&other[0], Ev::ToolStart { tool, .. } if tool == "read_file"));
        assert!(parse_agy(&json!({"event":"step_update","step_update":{"step_index":13,"state":"DONE","step_type":"system_message"}})).is_empty());
    }

    fn turn<'a>(k: Kind, l: &'a KimoLaunch, w: &'a Path, s: Option<&'a str>) -> Turn<'a> { Turn { kind: k, bin: Path::new("x"), prompt: "make it warmer", session: s, model: None, launch: l, workdir: w } }

    #[test]
    fn commands_attach_kimo_and_keep_the_prompt_off_the_command_line() {
        let w = std::env::temp_dir().join(format!("kimo-cli-{}", std::process::id()));
        std::fs::create_dir_all(&w).unwrap();
        let l = KimoLaunch { command: r"C:\Program Files\kimo\kimo-gui.exe".into(), env: vec![] };
        let (a, stdin) = command(&turn(Kind::ClaudeCode, &l, &w, None)).unwrap();
        assert!(a.windows(2).any(|p| p[0] == "--allowedTools" && p[1] == "mcp__kimo"));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert!(!a.iter().any(|x| x.contains("warmer")), "prompt goes on stdin");
        assert_eq!(stdin.as_deref(), Some("make it warmer"));
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(w.join(".kimo-mcp.json")).unwrap()).unwrap();
        assert_eq!(cfg["mcpServers"]["kimo"]["args"], json!(["mcp", "--client", "claude-code"]));
        let (a, _) = command(&turn(Kind::ClaudeCode, &l, &w, Some("S1"))).unwrap();
        assert!(a.windows(2).any(|p| p[0] == "--resume" && p[1] == "S1"));

        let (a, stdin) = command(&turn(Kind::Codex, &l, &w, None)).unwrap();
        assert_eq!(a[0], "exec");
        assert!(a.contains(&r"mcp_servers.kimo.command='C:\Program Files\kimo\kimo-gui.exe'".to_string()));
        assert!(a.contains(&"mcp_servers.kimo.args=['mcp','--client','codex']".to_string()));
        assert_eq!(a.last().unwrap(), "-");
        assert!(stdin.unwrap().contains("User request: make it warmer"), "first turn carries the kimo instructions");
        let (a, _) = command(&turn(Kind::Codex, &l, &w, Some("T1"))).unwrap();
        assert_eq!(&a[..3], ["exec", "resume", "T1"]);

        let (a, stdin) = command(&turn(Kind::Antigravity, &l, &w, Some("C1"))).unwrap();
        assert!(stdin.is_none());
        assert_eq!(a[1], "make it warmer");
        assert!(a.windows(2).any(|p| p[0] == "--conversation" && p[1] == "C1"));
        let _ = std::fs::remove_dir_all(&w);
    }

    #[test]
    fn explains_sign_in_and_limits() {
        assert!(explain(Kind::Codex, "Error: Not logged in. Run codex login").unwrap().contains("Sign in"));
        assert!(explain(Kind::ClaudeCode, "API Error: 429 rate limit").unwrap().contains("usage limit"));
        assert!(explain(Kind::Antigravity, "boom").is_none());
        assert_eq!(short_tool("mcp__kimo__timeline"), "timeline");
    }
}
