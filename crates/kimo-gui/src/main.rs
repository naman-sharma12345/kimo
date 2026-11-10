//! kimo: the desktop app people download. One window: a video editor (media bin, viewer, timeline, inspector, export)
//! on top of the kimo engine (kimo-mcp, launched and supervised as a sidecar over stdio), an Agent tab that runs Claude
//! Code, Codex or Antigravity on the user's OWN subscription (or an API key), and a Live view of every edit any agent
//! makes. While the app is open, `kimo-gui mcp` (what those agents launch) joins this app's engine through the
//! loopback bridge, so there is one project, one history and one Undo for everyone.
//! Loopback only; the webview can call only the commands listed in build.rs, granted to the main window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kimo_gui::agent::{self, Chat};
use kimo_gui::bridge;
use kimo_gui::cli_agents::{self, Ev, Kind, KimoLaunch, Slot};
use kimo_gui::download::{self, Part};
use kimo_gui::engine::{self, Status, Supervisor};
use kimo_gui::keys::{self, KeyRing, Secret};
use kimo_gui::ledger::{self, As, Ledger};
use kimo_gui::mcp_config::{self, Launch};
use kimo_gui::settings::Settings;
use kimo_gui::telemetry::{self, External, Source};
use kimo_gui::viewer::{self, Viewer};
use kimo_gui::{agents, engine_env, find_fonts, part_satisfied, PARTS_JSON};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    sup: Arc<Supervisor>,
    ledger: Arc<Ledger>,
    keys: KeyRing,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    chat: Mutex<Chat>,
    external: Mutex<Option<External>>,
    data_dir: PathBuf,
    fonts: Option<PathBuf>,
    parts: Vec<Part>,
    cancel_dl: Arc<AtomicBool>,
    viewer: Arc<Viewer>,
    bridge: Mutex<Option<bridge::Server>>,
    /// Subscription agents: the running turn (for Stop), whether one is running, and each agent's session id.
    turn: Slot,
    busy: AtomicBool,
    sessions: Mutex<HashMap<String, String>>,
    /// Agents currently attached through the bridge (label → open connections).
    attached: Mutex<HashMap<String, u32>>,
    project_path: Mutex<Option<String>>,
}

impl AppState {
    fn launch(&self) -> Option<Launch> {
        let bin = self.sup.bin.clone()?;
        Some(Launch { command: bin.display().to_string(), args: vec![], env: engine_env(self.fonts.as_deref(), &self.data_dir, &self.parts) })
    }
    /// What agents launch for kimo: this very app binary in `mcp` mode (joins this app's engine).
    fn kimo_launch(&self) -> KimoLaunch {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "kimo-gui".into());
        KimoLaunch { command: exe, env: vec![] }
    }
    fn save(&self) { let _ = self.settings.lock().unwrap().save(&self.settings_path); }
    fn provider_cfg(&self, id: &str) -> Option<(&'static keys::Provider, String, String)> {
        let p = keys::provider(id)?;
        let s = self.settings.lock().unwrap();
        let base = s.base_urls.get(id).filter(|b| !b.is_empty()).cloned().unwrap_or_else(|| p.base_url.to_string());
        let model = s.models.get(id).filter(|m| !m.is_empty()).cloned().unwrap_or_else(|| p.default_model.to_string());
        Some((p, base, model))
    }
    fn app_tools(&self) -> As { As { ledger: self.ledger.clone(), actor: "app" } }
    fn workspace(&self) -> PathBuf { let w = self.data_dir.join("agent-workspace"); let _ = std::fs::create_dir_all(&w); w }
    fn remember_project(&self, path: &str) {
        *self.project_path.lock().unwrap() = Some(path.to_string());
        self.settings.lock().unwrap().touch_recent(path);
        self.save();
    }
}

fn blocking<T: Send + 'static>(app: tauri::AppHandle, f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static) -> impl std::future::Future<Output = Result<T, String>> {
    async move { tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>())).await.map_err(|e| e.to_string())? }
}

/// An engine result for the webview: ok flag, the text, and the text parsed as JSON when it is JSON.
fn shaped(r: Value) -> Value {
    let text = engine::result_text(&r);
    let data = serde_json::from_str::<Value>(&text).ok();
    json!({"ok": !engine::is_error(&r), "text": text, "data": data})
}

// ---------- state ----------
#[tauri::command]
fn app_state(st: State<'_, AppState>) -> Value {
    let s = st.settings.lock().unwrap().clone();
    let keys: Vec<Value> = keys::PROVIDERS.iter().map(|p| {
        let k = st.keys.get(p.id);
        json!({"provider": p.id, "set": k.is_some(), "hint": k.map(|k| k.hint()), "remembered": s.remembered.iter().any(|r| r == p.id)})
    }).collect();
    let launch = st.launch();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "engine": st.sup.status(),
        "mcp_enabled": s.mcp_enabled,
        "external_port": st.external.lock().unwrap().as_ref().map(|e| e.port),
        "bridge_port": st.bridge.lock().unwrap().as_ref().map(|b| b.info.port),
        "attached": *st.attached.lock().unwrap(),
        "snippets": launch.as_ref().map(|l| mcp_config::snippets(&Launch { command: st.kimo_launch().command, args: vec!["mcp".into()], env: l.env.clone() }, cfg!(windows))),
        "kimo_launch": {"command": st.kimo_launch().command, "args": ["mcp"]},
        "settings": s,
        "providers": keys::PROVIDERS,
        "keys": keys,
        "key_store": st.keys.store_label(),
        "data_dir": st.data_dir.display().to_string(),
        "project_path": *st.project_path.lock().unwrap(),
        "busy": st.busy.load(Ordering::SeqCst),
        "os": std::env::consts::OS,
        "fonts": st.fonts.as_ref().map(|f| f.display().to_string()),
    })
}

#[tauri::command]
async fn engine_restart(app: tauri::AppHandle) -> Result<Status, String> {
    blocking(app, |st| { let s = st.sup.start(); st.ledger.reset(); *st.project_path.lock().unwrap() = None; Ok(s) }).await
}

// ---------- the engine, for the editor ----------
/// An edit or read the user makes in the editor (Live and Changes show it as "you").
#[tauri::command]
async fn engine_call(app: tauri::AppHandle, tool: String, args: Value) -> Result<Value, String> {
    blocking(app, move |st| st.ledger.call("you", &tool, &args).map(shaped)).await
}

/// The window's own refresh reads (not listed in Live).
#[tauri::command]
async fn engine_read(app: tauri::AppHandle, tool: String, args: Value) -> Result<Value, String> {
    blocking(app, move |st| {
        let action = args.get("action").and_then(|a| a.as_str()).unwrap_or("");
        if !ledger::is_read_only(&tool, action) { return Err(format!("{tool}.{action} changes the project; use engine_call")); }
        st.ledger.call("app", &tool, &args).map(shaped)
    }).await
}

#[tauri::command]
fn changes_list(st: State<'_, AppState>) -> Vec<ledger::Change> { st.ledger.changes() }

#[tauri::command]
async fn change_undo(app: tauri::AppHandle, id: u64) -> Result<Value, String> {
    blocking(app, move |st| st.ledger.undo_change("you", id)).await
}

// ---------- projects ----------
#[tauri::command]
async fn project_open(app: tauri::AppHandle, path: String) -> Result<String, String> {
    blocking(app, move |st| {
        let r = st.ledger.call("you", "project", &json!({"action": "load_project", "args": {"path": path}}))?;
        if engine::is_error(&r) { return Err(engine::result_text(&r)); }
        st.ledger.reset();
        *st.chat.lock().unwrap() = Chat::default();
        st.remember_project(&path);
        Ok(engine::result_text(&r))
    }).await
}

#[tauri::command]
async fn project_save(app: tauri::AppHandle, path: String) -> Result<String, String> {
    blocking(app, move |st| {
        let path = if Path::new(&path).extension().is_none() { format!("{path}.kimo") } else { path };
        let r = st.ledger.call("you", "project", &json!({"action": "save_project", "args": {"path": path}}))?;
        if engine::is_error(&r) { return Err(engine::result_text(&r)); }
        st.remember_project(&path);
        Ok(path)
    }).await
}

/// A fresh, empty project (the engine restarts; agents attached through the bridge see the new project too).
#[tauri::command]
async fn project_new(app: tauri::AppHandle) -> Result<Status, String> {
    let a2 = app.clone();
    let s = blocking(app, |st| { let s = st.sup.start(); st.ledger.reset(); *st.project_path.lock().unwrap() = None; *st.chat.lock().unwrap() = Chat::default(); st.sessions.lock().unwrap().clear(); Ok(s) }).await?;
    let _ = a2.emit("project-changed", json!({"actor": "you", "action": "new_project"}));
    Ok(s)
}

// ---------- native dialogs ----------
const MEDIA_EXT: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mxf", "mts", "m2ts", "mpg", "mpeg", "wmv", "flv", "3gp",
    "mp3", "wav", "aac", "m4a", "flac", "ogg", "opus", "aiff", "aif", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "exr", "svg"];

/// kind: "media" (several files) | "project" (open) | "save_project" | "export" (save). Returns absolute paths.
#[tauri::command]
async fn pick(app: tauri::AppHandle, kind: String, suggest: Option<String>) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let d = app.dialog().file();
        let to = |p: tauri_plugin_dialog::FilePath| p.into_path().ok().map(|p| p.display().to_string());
        let out: Vec<String> = match kind.as_str() {
            "media" => d.set_title("Import media").add_filter("Video, audio and images", MEDIA_EXT).add_filter("All files", &["*"])
                .blocking_pick_files().unwrap_or_default().into_iter().filter_map(to).collect(),
            "project" => d.set_title("Open a kimo project").add_filter("kimo project", &["kimo", "json"]).blocking_pick_file().and_then(to).into_iter().collect(),
            "save_project" => d.set_title("Save project").add_filter("kimo project", &["kimo"]).set_file_name(suggest.unwrap_or_else(|| "project.kimo".into()))
                .blocking_save_file().and_then(to).into_iter().collect(),
            "export" => d.set_title("Export video").add_filter("MP4 video", &["mp4"]).set_file_name(suggest.unwrap_or_else(|| "export.mp4".into()))
                .blocking_save_file().and_then(to).into_iter().collect(),
            _ => return Err("unknown dialog".into()),
        };
        Ok(out)
    }).await.map_err(|e| e.to_string())?
}

/// A small JPEG of a media file (first second) for the media bin, made with ffmpeg into the viewer cache.
#[tauri::command]
async fn media_thumb(app: tauri::AppHandle, path: String) -> Result<Option<String>, String> {
    blocking(app, move |st| {
        use sha2::{Digest, Sha512};
        let p = Path::new(&path);
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(ext.as_str(), "mp3" | "wav" | "aac" | "m4a" | "flac" | "ogg" | "opus" | "aiff" | "aif") { return Ok(None); }
        let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok().map(|t| format!("{t:?}")).unwrap_or_default();
        let h: String = Sha512::digest(format!("{path}|{mtime}").as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect();
        let name = format!("{h}.jpg");
        let out = st.viewer.dir().join(&name);
        if !out.is_file() {
            let Some(ff) = viewer::find_ffmpeg(&st.data_dir) else { return Ok(None) };
            let _ = std::fs::create_dir_all(st.viewer.dir());
            let mut c = std::process::Command::new(ff);
            c.args(["-v", "error", "-y", "-ss", "0.8", "-i"]).arg(p).args(["-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "5"]).arg(&out)
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            #[cfg(windows)]
            { use std::os::windows::process::CommandExt; c.creation_flags(0x0800_0000); }
            let ok = c.status().map(|s| s.success()).unwrap_or(false) && out.is_file();
            if !ok {
                // very short clips: take the first frame instead
                let mut c2 = std::process::Command::new(viewer::find_ffmpeg(&st.data_dir).unwrap());
                c2.args(["-v", "error", "-y", "-i"]).arg(p).args(["-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "5"]).arg(&out)
                    .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
                #[cfg(windows)]
                { use std::os::windows::process::CommandExt; c2.creation_flags(0x0800_0000); }
                if !(c2.status().map(|s| s.success()).unwrap_or(false) && out.is_file()) { return Ok(None); }
            }
        }
        Ok(Some(viewer::cache_url(&name)))
    }).await
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> { cli_agents::open_url(&url) }

#[tauri::command]
fn reveal(path: String) -> Result<(), String> { cli_agents::reveal(Path::new(&path)) }

// ---------- subscription agents ----------
#[tauri::command]
async fn cli_status(app: tauri::AppHandle) -> Result<Vec<cli_agents::Status>, String> {
    blocking(app, |_| {
        let hs: Vec<_> = Kind::all().into_iter().map(|k| std::thread::spawn(move || cli_agents::status(k))).collect();
        Ok(hs.into_iter().filter_map(|h| h.join().ok()).collect())
    }).await
}

/// Opens a terminal running the CLI's own login (the user signs in there; kimo never sees the credentials).
#[tauri::command]
fn cli_signin(id: String) -> Result<String, String> {
    let k = Kind::from_id(&id).ok_or("unknown agent")?;
    let bin = cli_agents::find(k).ok_or_else(|| format!("{} is not installed yet.", k.name()))?;
    let cmd = match k { Kind::ClaudeCode => format!("\"{}\"", bin.display()), Kind::Codex => format!("\"{}\" login", bin.display()), Kind::Antigravity => format!("\"{}\"", bin.display()) };
    cli_agents::open_terminal(&cmd)?;
    Ok(match k {
        Kind::ClaudeCode => "A terminal opened with Claude Code. Type /login, pick your Claude subscription and finish in the browser, then close the terminal.".into(),
        Kind::Codex => "A terminal opened with `codex login`. Choose “Sign in with ChatGPT”, finish in the browser, then close the terminal.".into(),
        Kind::Antigravity => "A terminal opened with Antigravity. Sign in with your Google account when it asks, then close the terminal.".into(),
    })
}

/// Opens a terminal running the npm install (visible to the user), or the download page.
#[tauri::command]
fn cli_install(id: String) -> Result<String, String> {
    let k = Kind::from_id(&id).ok_or("unknown agent")?;
    match k.install_cmd() {
        Some(c) if agents::which("npm").is_some() => { cli_agents::open_terminal(c)?; Ok(format!("Installing {} in a terminal window. When it finishes, press Check again.", k.name())) }
        _ => { cli_agents::open_url(k.install_url())?; Ok(format!("Opened the {} download page in your browser.", k.name())) }
    }
}

/// Add kimo to the CLI's own MCP list too, so the user's own sessions (terminal/IDE) can use it.
#[tauri::command]
async fn cli_register(app: tauri::AppHandle, id: String) -> Result<String, String> {
    blocking(app, move |st| {
        let k = Kind::from_id(&id).ok_or("unknown agent")?;
        let bin = cli_agents::find(k).ok_or_else(|| format!("{} is not installed.", k.name()))?;
        cli_agents::register(k, &bin, &st.kimo_launch())
    }).await
}

#[tauri::command]
fn agent_select(st: State<'_, AppState>, agent: String, model: Option<String>, select: Option<bool>) -> Result<(), String> {
    if agent != "api" && Kind::from_id(&agent).is_none() { return Err("unknown agent".into()); }
    let mut s = st.settings.lock().unwrap();
    if select != Some(false) { s.agent = agent.clone(); }
    if let Some(m) = model { s.agent_models.insert(agent, m.trim().to_string()); }
    drop(s);
    st.save();
    Ok(())
}

fn emit_ev(app: &tauri::AppHandle, agent: &str, turn: u64, ev: &Ev) {
    let _ = app.emit("agent-event", json!({"agent": agent, "turn": turn, "ev": ev}));
}

/// One message to the selected agent. Streams `agent-event`s; returns the final reply.
#[tauri::command]
async fn agent_send(app: tauri::AppHandle, text: String, turn: u64) -> Result<Value, String> {
    let a2 = app.clone();
    blocking(app, move |st| {
        if text.trim().is_empty() { return Err("Type what you want to change.".into()); }
        if st.busy.swap(true, Ordering::SeqCst) { return Err("The agent is still working on the last message. Press Stop to end it.".into()); }
        let _guard = scopeguard(&st.busy);
        let agent = st.settings.lock().unwrap().agent.clone();
        if agent == "api" { return api_chat(&a2, st, &text, turn); }
        let k = Kind::from_id(&agent).ok_or("Pick an agent first.")?;
        let bin = cli_agents::find(k).ok_or_else(|| format!("{} is not installed on this computer. Open Connections to install it.", k.name()))?;
        if k == Kind::Antigravity && !cli_agents::registered(k) {
            emit_ev(&a2, &agent, turn, &Ev::Status { text: "Adding kimo to Antigravity's MCP servers (one time)…".into() });
            cli_agents::register(k, &bin, &st.kimo_launch()).map_err(|e| format!("Could not add kimo to Antigravity: {e}"))?;
        }
        let model = st.settings.lock().unwrap().agent_models.get(&agent).cloned().filter(|m| !m.is_empty());
        let session = st.sessions.lock().unwrap().get(&agent).cloned();
        let launch = st.kimo_launch();
        let workdir = st.workspace();
        let t = cli_agents::Turn { kind: k, bin: &bin, prompt: &text, session: session.as_deref(), model: model.as_deref(), launch: &launch, workdir: &workdir };
        emit_ev(&a2, &agent, turn, &Ev::Status { text: format!("{} is working on your {}", k.name(), k.plan()) });
        let mut o = cli_agents::run(&t, &st.turn, &mut |ev| emit_ev(&a2, &agent, turn, &ev));
        // a resumed session the CLI no longer knows: start a fresh one once
        if o.error.as_deref().map(|e| e.to_ascii_lowercase().contains("session") || e.to_ascii_lowercase().contains("conversation")).unwrap_or(false) && session.is_some() && o.reply.is_empty() {
            st.sessions.lock().unwrap().remove(&agent);
            let t2 = cli_agents::Turn { session: None, ..t };
            o = cli_agents::run(&t2, &st.turn, &mut |ev| emit_ev(&a2, &agent, turn, &ev));
        }
        if let Some(s) = &o.session { st.sessions.lock().unwrap().insert(agent.clone(), s.clone()); }
        if let Some(e) = o.error { return Err(e); }
        Ok(json!({"reply": o.reply, "session": o.session}))
    }).await
}

struct Release<'a>(&'a AtomicBool);
impl Drop for Release<'_> { fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); } }
fn scopeguard(b: &AtomicBool) -> Release<'_> { Release(b) }

/// The API-key agent (kept for people without a subscription CLI), speaking the same agent-event stream.
fn api_chat(app: &tauri::AppHandle, st: &AppState, text: &str, turn: u64) -> Result<Value, String> {
    let provider = st.settings.lock().unwrap().provider.clone();
    let (p, base, model) = st.provider_cfg(&provider).ok_or("Pick a provider in Connections first.")?;
    let key = st.keys.get(&provider).ok_or_else(|| format!("Add your {} key in Connections, or pick Claude Code, Codex or Antigravity to use your subscription.", p.name))?;
    if model.is_empty() { return Err("Type a model name for this provider in Connections.".into()); }
    let llm = agent::Llm { wire: p.wire, base_url: base, model, key };
    let mut chat = st.chat.lock().map_err(|_| "chat is busy")?;
    let tools = As { ledger: st.ledger.clone(), actor: "kimo-chat" };
    let reply = chat.send(&llm, &tools, text, &mut |ev| {
        let e = match ev {
            agent::Event::Thinking { step } => Ev::Status { text: format!("thinking (step {})", step + 1) },
            agent::Event::ToolStart { call, tool, args, .. } => Ev::ToolStart { id: call, tool, input: args },
            agent::Event::ToolEnd { call, ok, result, .. } => Ev::ToolEnd { id: call, ok, output: result },
            agent::Event::Reply { text } => Ev::Done { text, cost_usd: None, turns: None },
            agent::Event::Error { message } => Ev::Error { message },
        };
        emit_ev(app, "api", turn, &e);
    })?;
    Ok(json!({"reply": reply}))
}

#[tauri::command]
fn agent_stop(st: State<'_, AppState>) -> bool { cli_agents::stop(&st.turn) }

/// Forget the conversation (the next message starts a new session with the agent).
#[tauri::command]
fn agent_reset(st: State<'_, AppState>) {
    st.sessions.lock().unwrap().clear();
    *st.chat.lock().unwrap() = Chat::default();
}

// ---------- MCP switch (external engines) ----------
#[tauri::command]
async fn mcp_probe(app: tauri::AppHandle) -> Result<engine::ProbeInfo, String> {
    blocking(app, |st| {
        let l = st.launch().ok_or("The kimo engine (kimo-mcp) was not found next to the app.")?;
        Ok(engine::probe(Path::new(&l.command), &l.args, &l.env))
    }).await
}

fn start_external(app: &tauri::AppHandle) -> Result<u16, String> {
    let st = app.state::<AppState>();
    if let Some(e) = st.external.lock().unwrap().as_ref() { return Ok(e.port); }
    let (a1, a2) = (app.clone(), app.clone());
    let ext = telemetry::listen_external(
        Arc::new(move |src, ev| forward(&a1, src, ev)),
        Arc::new(move |s| { let _ = a2.emit("spectator-status", s); }),
    )?;
    let port = ext.port;
    *st.external.lock().unwrap() = Some(ext);
    Ok(port)
}

#[tauri::command]
async fn mcp_set(app: tauri::AppHandle, enabled: bool) -> Result<Value, String> {
    let a2 = app.clone();
    blocking(app, move |st| {
        if !enabled {
            if let Some(e) = st.external.lock().unwrap().take() { e.stop(); }
            st.settings.lock().unwrap().mcp_enabled = false;
            st.save();
            return Ok(json!({"enabled": false}));
        }
        let l = st.launch().ok_or("The kimo engine (kimo-mcp) was not found next to the app, so there is nothing to connect to.")?;
        let probe = engine::probe(Path::new(&l.command), &l.args, &l.env);
        if !probe.ok { return Ok(json!({"enabled": false, "probe": probe})); }
        let port = start_external(&a2)?;
        st.settings.lock().unwrap().mcp_enabled = true;
        st.save();
        Ok(json!({"enabled": true, "probe": probe, "external_port": port, "snippets": mcp_config::snippets(&l, cfg!(windows))}))
    }).await
}

// ---------- API keys (optional) ----------
#[tauri::command]
fn key_set(st: State<'_, AppState>, provider: String, key: String, remember: bool) -> Result<keys::KeyStatus, String> {
    keys::provider(&provider).ok_or("unknown provider")?;
    let k = Secret::new(key);
    if k.is_empty() { return Err("Paste a key first.".into()); }
    let status = st.keys.set(&provider, k, remember);
    {
        let mut s = st.settings.lock().unwrap();
        s.remembered.retain(|p| p != &provider);
        if status.remembered { s.remembered.push(provider.clone()); }
        s.provider = provider;
    }
    st.save();
    Ok(status)
}

#[tauri::command]
fn key_clear(st: State<'_, AppState>, provider: String) -> keys::KeyStatus {
    let r = st.keys.clear(&provider);
    st.settings.lock().unwrap().remembered.retain(|p| p != &provider);
    st.save();
    r
}

#[tauri::command]
async fn key_test(app: tauri::AppHandle, provider: String, key: Option<String>) -> Result<String, String> {
    blocking(app, move |st| {
        let (p, base, _) = st.provider_cfg(&provider).ok_or("unknown provider")?;
        let k = key.map(Secret::new).filter(|k| !k.is_empty()).or_else(|| st.keys.get(&provider)).ok_or("Paste a key first.")?;
        keys::test_connection(p.wire, &base, &k)
    }).await
}

#[tauri::command]
fn settings_set(st: State<'_, AppState>, provider: String, model: Option<String>, base_url: Option<String>) -> Result<(), String> {
    keys::provider(&provider).ok_or("unknown provider")?;
    let mut s = st.settings.lock().unwrap();
    s.provider = provider.clone();
    if let Some(m) = model { s.models.insert(provider.clone(), m.trim().to_string()); }
    if let Some(b) = base_url { s.base_urls.insert(provider, b.trim().to_string()); }
    drop(s);
    st.save();
    Ok(())
}

#[tauri::command]
fn onboarded_set(st: State<'_, AppState>) { st.settings.lock().unwrap().onboarded = true; st.save(); }

// ---------- viewer ----------
#[tauri::command]
async fn viewer_frame(app: tauri::AppHandle, time: f64) -> Result<viewer::Shown, String> {
    blocking(app, move |st| st.viewer.frame(&st.app_tools(), time)).await
}

#[tauri::command]
async fn viewer_draft(app: tauri::AppHandle, start: f64, seconds: f64) -> Result<viewer::Draft, String> {
    blocking(app, move |st| st.viewer.draft(&st.app_tools(), start, seconds)).await
}

#[tauri::command]
fn open_spectator(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("spectator") { return w.set_focus().map_err(|e| e.to_string()); }
    tauri::WebviewWindowBuilder::new(&app, "spectator", tauri::WebviewUrl::App("index.html".into()))
        .title("kimo \u{b7} live graph").inner_size(1376.0, 768.0).min_inner_size(1200.0, 700.0)
        .build().map(|_| ()).map_err(|e| e.to_string())
}

// ---------- first-run setup ----------
#[tauri::command]
fn setup_status(st: State<'_, AppState>) -> Value {
    let parts: Vec<Value> = st.parts.iter().map(|p| json!({
        "id": p.id, "name": p.name, "note": p.note, "size": p.size, "optional": p.optional,
        "pinned": download::pinned(p), "installed": download::installed(&st.data_dir, p), "satisfied": part_satisfied(&st.data_dir, p),
    })).collect();
    let ready = st.parts.iter().filter(|p| !p.optional).all(|p| part_satisfied(&st.data_dir, p).is_some());
    json!({"done": st.settings.lock().unwrap().setup_done, "ready": ready, "parts": parts, "dir": st.data_dir.display().to_string()})
}

#[tauri::command]
async fn setup_start(app: tauri::AppHandle, skip_optional: bool) -> Result<Value, String> {
    let a2 = app.clone();
    blocking(app, move |st| {
        st.cancel_dl.store(false, Ordering::SeqCst);
        let mut results = vec![];
        for p in st.parts.iter().filter(|p| !(skip_optional && p.optional)) {
            if download::installed(&st.data_dir, p) { results.push(json!({"id": p.id, "ok": true, "already": true})); continue; }
            if !download::pinned(p) {
                let sys = part_satisfied(&st.data_dir, p);
                let _ = a2.emit("setup-progress", json!({"id": p.id, "phase": if sys.is_some() { "system" } else { "unavailable" }}));
                results.push(json!({"id": p.id, "ok": sys.is_some(), "system": sys.is_some(), "error": if sys.is_some() { Value::Null } else { json!("not published for download yet") }}));
                continue;
            }
            let a = a2.clone();
            match download::fetch(&st.data_dir, p, &st.cancel_dl, &mut |pr| { let _ = a.emit("setup-progress", &pr); }) {
                Ok(_) => results.push(json!({"id": p.id, "ok": true})),
                Err(e) => { let msg = e.to_string(); let _ = a2.emit("setup-progress", json!({"id": p.id, "phase": "error", "error": msg})); results.push(json!({"id": p.id, "ok": false, "error": msg})); if e == download::Failure::Cancelled { break; } }
            }
        }
        let ready = st.parts.iter().filter(|p| !p.optional).all(|p| part_satisfied(&st.data_dir, p).is_some());
        if ready { st.settings.lock().unwrap().setup_done = true; st.save(); st.sup.start(); st.ledger.reset(); }
        Ok(json!({"results": results, "ready": ready}))
    }).await
}

#[tauri::command]
fn setup_pause(st: State<'_, AppState>) { st.cancel_dl.store(true, Ordering::SeqCst); }

#[tauri::command]
fn setup_skip(st: State<'_, AppState>) { st.settings.lock().unwrap().setup_done = true; st.save(); }

// ---------- live feed ----------
/// Engine telemetry: raw to the live-graph window; activity from OTHER engines (an agent that started its own
/// kimo-mcp because it was configured before this app existed) is listed in Live as an external engine.
fn forward(app: &tauri::AppHandle, src: Source, ev: Value) {
    if let Source::External { conn } = src { let _ = app.emit("external-event", json!({"conn": conn, "t": ledger::now_ms() as u64, "ev": ev})); }
    let _ = app.emit("engine-event", ev);
}

// ---------- `kimo-gui mcp` ----------
/// `kimo-gui mcp [--client NAME]`: what Claude Code / Codex / Antigravity launch. Joins the open kimo app's engine
/// (same project, live, undoable); with no app open, runs a standalone engine over stdio exactly as before.
fn run_mcp(args: &[String]) -> ! {
    let mut client = "agent".to_string();
    let mut rest = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--client" { if let Some(c) = it.next() { client = c.clone(); } } else { rest.push(a.clone()); }
    }
    // a small diagnostic log next to bridge.json: which way this agent connected, and why it stopped
    let log = |msg: &str| {
        if let Some(d) = bridge::data_dir() {
            let _ = std::fs::create_dir_all(&d);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("mcp-clients.log")) {
                use std::io::Write;
                let _ = writeln!(f, "{} pid={} client={client} {msg}", ledger::now_ms(), std::process::id());
            }
        }
    };
    log("start");
    if std::env::var_os("KIMO_STANDALONE").is_none() {
        match bridge::data_dir() {
            Some(dir) => match bridge::connect(&dir, &client) {
                Ok(s) => { log("attached to the open kimo app"); let code = bridge::relay_stdio(s); log("detached"); std::process::exit(code); }
                Err(e) => log(&format!("standalone engine ({e})")),
            },
            None => log("standalone engine (no data dir)"),
        }
    }
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let Some(bin) = engine::find_engine(exe_dir.as_deref(), None) else {
        eprintln!("kimo: the engine (kimo-mcp) was not found next to this app or on PATH");
        std::process::exit(127);
    };
    let mut c = std::process::Command::new(&bin);
    c.args(&rest);
    if std::env::var_os("KIMO_FONTS").is_none() { if let Some(f) = find_fonts(exe_dir.as_deref(), None, Some(&bin)) { c.env("KIMO_FONTS", f); } }
    let st = c.status();
    log(&format!("standalone engine ended: {st:?}"));
    std::process::exit(st.ok().and_then(|s| s.code()).unwrap_or(1));
}

fn bridge_handler(app: tauri::AppHandle) -> bridge::Handler {
    Arc::new(move |client, method, params| {
        let st = app.state::<AppState>();
        match method {
            "initialize" => Ok(bridge::initialize_result(&params, env!("CARGO_PKG_VERSION"))),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": st.ledger.tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).ok_or((-32602, "missing tool name".to_string()))?;
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                st.ledger.call(client, name, &args).map_err(|e| (-32603, e))
            }
            "resources/list" => Ok(json!({"resources": []})),
            "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            _ => Err((-32601, format!("method not found: {method}"))),
        }
    })
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(|a| a == "mcp").unwrap_or(false) { run_mcp(&argv[1..]); }
    if argv.first().map(|a| a == "--version").unwrap_or(false) { println!("kimo-gui {}", env!("CARGO_PKG_VERSION")); return; }

    tauri::Builder::default()
        // must be the first plugin: a second launch hands its arguments here and exits
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") { let _ = w.unminimize(); let _ = w.show(); let _ = w.set_focus(); }
            if let Some(f) = argv.iter().skip(1).find(|a| a.to_ascii_lowercase().ends_with(".kimo")) { let _ = app.emit("open-file", f); }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = bridge::data_dir().or_else(|| app.path().app_data_dir().ok()).unwrap_or_else(|| std::env::temp_dir().join("kimo"));
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| data_dir.clone());
            let _ = std::fs::create_dir_all(&data_dir);
            let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let res_dir = app.path().resource_dir().ok();
            let bin = engine::find_engine(exe_dir.as_deref(), res_dir.as_deref());
            let fonts = find_fonts(exe_dir.as_deref(), res_dir.as_deref(), bin.as_deref());
            let parts = download::for_this_os(download::load_parts(PARTS_JSON).unwrap_or_default());
            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);

            // the app's own engine streams telemetry to a private loopback port (for the live-graph window)
            let h2 = handle.clone();
            let private = telemetry::listen_private(Arc::new(move |src, ev| forward(&h2, src, ev)))?;
            let mut env = engine_env(fonts.as_deref(), &data_dir, &parts);
            env.push(("KIMO_SPECTATOR".into(), format!("tcp://127.0.0.1:{private}")));
            env.push(("KIMO_INLINE_IMAGES".into(), "0".into()));
            let sup = Arc::new(Supervisor::new(bin, env, Some(data_dir.join("engine.log"))));
            let h3 = handle.clone();
            let ledger = Arc::new(Ledger::new(sup.clone(), Arc::new(move |kind, v| { let _ = h3.emit(kind, v); })));

            let keyring = KeyRing::new(Box::new(keys::OsKeychain));
            for p in &settings.remembered { let _ = keyring.restore(p); }
            let mcp_on = settings.mcp_enabled;
            let vdir = data_dir.join("viewer-cache");
            let _ = std::fs::remove_dir_all(&vdir);
            let viewer = Arc::new(Viewer::new(vdir).with_ffmpeg(viewer::find_ffmpeg(&data_dir)));
            app.manage(AppState {
                sup: sup.clone(), ledger, keys: keyring, settings: Mutex::new(settings), settings_path,
                chat: Mutex::new(Chat::default()), external: Mutex::new(None), data_dir: data_dir.clone(), fonts, parts,
                cancel_dl: Arc::new(AtomicBool::new(false)), viewer, bridge: Mutex::new(None),
                turn: Arc::new(Mutex::new(None)), busy: AtomicBool::new(false), sessions: Mutex::new(HashMap::new()),
                attached: Mutex::new(HashMap::new()), project_path: Mutex::new(None),
            });
            // the bridge: agents launched with `kimo-gui mcp` join this engine
            let h4 = handle.clone();
            let on_conn: bridge::OnConn = Arc::new(move |label, up| {
                let st = h4.state::<AppState>();
                let mut a = st.attached.lock().unwrap();
                let n = a.entry(label.to_string()).or_insert(0);
                if up { *n += 1 } else { *n = n.saturating_sub(1) }
                if *n == 0 { a.remove(label); }
                let snapshot = a.clone();
                drop(a);
                let _ = h4.emit("agent-connection", json!({"client": label, "up": up, "attached": snapshot}));
            });
            match bridge::serve(Some(&data_dir), bridge_handler(handle.clone()), on_conn) {
                Ok(s) => *app.state::<AppState>().bridge.lock().unwrap() = Some(s),
                Err(e) => eprintln!("kimo: bridge not started: {e}"),
            }
            if mcp_on { if let Err(e) = start_external(&handle) { eprintln!("kimo: {e}"); } }
            let h5 = handle.clone();
            std::thread::spawn(move || { let s = sup.start(); let _ = h5.emit("engine-status", &s); });
            Ok(())
        })
        // kimo://localhost/<hex>.png|mp4 (http://kimo.localhost on Windows): the viewer cache, nothing else
        .register_uri_scheme_protocol("kimo", |ctx, req| {
            let dir = ctx.app_handle().state::<AppState>().viewer.dir().to_path_buf();
            let range = req.headers().get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
            let (status, headers, body) = viewer::respond(&dir, req.uri().path(), range.as_deref());
            let mut b = tauri::http::Response::builder().status(status);
            for (k, v) in headers { b = b.header(k, v); }
            b.body(body).unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
        })
        .invoke_handler(tauri::generate_handler![
            app_state, engine_restart, engine_call, engine_read, changes_list, change_undo,
            project_open, project_save, project_new, pick, media_thumb, open_url, reveal,
            cli_status, cli_signin, cli_install, cli_register, agent_select, agent_send, agent_stop, agent_reset,
            mcp_set, mcp_probe, key_set, key_clear, key_test, settings_set, onboarded_set,
            open_spectator, setup_status, setup_start, setup_pause, setup_skip, viewer_frame, viewer_draft
        ])
        .build(tauri::generate_context!())
        .expect("kimo failed to start")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev {
                let st = app.state::<AppState>();
                cli_agents::stop(&st.turn);
                if let Some(b) = st.bridge.lock().unwrap().take() { b.remove_file(); }
                st.sup.stop();
            }
        });
}
