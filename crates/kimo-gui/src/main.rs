//! kimo: the main desktop app. Opens the kimo window, launches and supervises the engine (kimo-mcp) as a sidecar
//! child over stdio, and carries the MCP switch, coding-agent linking, bring-your-own-key chat, the Live agent view and
//! the first-run downloads. `kimo-gui mcp` runs the engine as a plain stdio MCP server (what external clients launch).
//! Loopback only; the webview can call only the commands listed in build.rs, granted to the main window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kimo_gui::agent::{self, Chat};
use kimo_gui::download::{self, Part};
use kimo_gui::engine::{self, Status, Supervisor};
use kimo_gui::keys::{self, KeyRing, Secret};
use kimo_gui::mcp_config::{self, Launch};
use kimo_gui::settings::Settings;
use kimo_gui::telemetry::{self, External, Source};
use kimo_gui::{agents, engine_env, PARTS_JSON};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

struct AppState {
    sup: Arc<Supervisor>,
    keys: KeyRing,
    settings: Mutex<Settings>,
    settings_path: PathBuf,
    chat: Mutex<Chat>,
    external: Mutex<Option<External>>,
    data_dir: PathBuf,
    fonts: Option<PathBuf>,
    parts: Vec<Part>,
    cancel_dl: Arc<AtomicBool>,
}

impl AppState {
    fn launch(&self) -> Option<Launch> {
        let bin = self.sup.bin.clone()?;
        Some(Launch { command: bin.display().to_string(), args: vec![], env: engine_env(self.fonts.as_deref(), &self.data_dir, &self.parts) })
    }
    fn save(&self) { let _ = self.settings.lock().unwrap().save(&self.settings_path); }
    fn provider_cfg(&self, id: &str) -> Option<(&'static keys::Provider, String, String)> {
        let p = keys::provider(id)?;
        let s = self.settings.lock().unwrap();
        let base = s.base_urls.get(id).filter(|b| !b.is_empty()).cloned().unwrap_or_else(|| p.base_url.to_string());
        let model = s.models.get(id).filter(|m| !m.is_empty()).cloned().unwrap_or_else(|| p.default_model.to_string());
        Some((p, base, model))
    }
}

fn now_ms() -> u128 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0) }

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
        "snippets": launch.as_ref().map(|l| mcp_config::snippets(l, cfg!(windows))),
        "settings": s,
        "providers": keys::PROVIDERS,
        "keys": keys,
        "key_store": st.keys.store_label(),
        "data_dir": st.data_dir.display().to_string(),
        "os": std::env::consts::OS,
    })
}

#[tauri::command]
async fn engine_restart(app: tauri::AppHandle) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().sup.start()).await.map_err(|e| e.to_string())
}

// ---------- MCP switch ----------
#[tauri::command]
async fn mcp_probe(app: tauri::AppHandle) -> Result<engine::ProbeInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let l = st.launch().ok_or("The kimo engine (kimo-mcp) was not found next to the app.")?;
        Ok(engine::probe(std::path::Path::new(&l.command), &l.args, &l.env))
    }).await.map_err(|e| e.to_string())?
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
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        if !enabled {
            if let Some(e) = st.external.lock().unwrap().take() { e.stop(); }
            st.settings.lock().unwrap().mcp_enabled = false;
            st.save();
            return Ok(json!({"enabled": false}));
        }
        let l = st.launch().ok_or("The kimo engine (kimo-mcp) was not found next to the app, so there is nothing to connect to.")?;
        let probe = engine::probe(std::path::Path::new(&l.command), &l.args, &l.env);
        if !probe.ok { return Ok(json!({"enabled": false, "probe": probe})); }
        let port = start_external(&app)?;
        st.settings.lock().unwrap().mcp_enabled = true;
        st.save();
        Ok(json!({"enabled": true, "probe": probe, "external_port": port, "snippets": mcp_config::snippets(&l, cfg!(windows))}))
    }).await.map_err(|e| e.to_string())?
}

// ---------- coding agents ----------
#[tauri::command]
fn agents_detect() -> Vec<agents::AgentState> { agents::detect(agents::home_dir().as_deref()) }

/// Runs only on the user's click (the button asks first and shows the exact command).
#[tauri::command]
async fn agent_add(app: tauri::AppHandle, id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        if !st.settings.lock().unwrap().mcp_enabled { return Err("Turn the MCP switch on first.".into()); }
        let l = st.launch().ok_or("engine not found")?;
        match id.as_str() {
            "claude" => agents::add_claude(&agents::which("claude").ok_or("The claude command is not on PATH.")?, &l),
            "codex" => agents::add_codex(&agents::codex_config_path(&agents::home_dir().ok_or("no home directory")?), &l),
            _ => Err("unknown agent".into()),
        }
    }).await.map_err(|e| e.to_string())?
}

// ---------- keys ----------
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
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let (p, base, _) = st.provider_cfg(&provider).ok_or("unknown provider")?;
        let k = key.map(Secret::new).filter(|k| !k.is_empty()).or_else(|| st.keys.get(&provider)).ok_or("Paste a key first.")?;
        keys::test_connection(p.wire, &base, &k)
    }).await.map_err(|e| e.to_string())?
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

// ---------- chat ----------
#[tauri::command]
async fn chat_send(app: tauri::AppHandle, text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let provider = st.settings.lock().unwrap().provider.clone();
        let (p, base, model) = st.provider_cfg(&provider).ok_or("Pick a provider in Connections first.")?;
        let key = st.keys.get(&provider).ok_or_else(|| format!("Add your {} key in Connections first, or link Claude Code / Codex and edit from there.", p.name))?;
        if model.is_empty() { return Err("Type a model name for this provider in Connections.".into()); }
        let llm = agent::Llm { wire: p.wire, base_url: base, model, key };
        let mut chat = st.chat.lock().map_err(|_| "chat is busy")?;
        let a = app.clone();
        chat.send(&llm, st.sup.as_ref(), &text, &mut |ev| { let _ = a.emit("chat-event", &ev); })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn chat_undo(app: tauri::AppHandle, card: usize) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let mut chat = st.chat.lock().map_err(|_| "chat is busy")?;
        let (steps, ids) = chat.undo(st.sup.as_ref(), card)?;
        Ok(json!({"steps": steps, "undone": ids}))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn chat_reset(st: State<'_, AppState>) { *st.chat.lock().unwrap() = Chat::default(); }

#[tauri::command]
async fn project_open(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        let r = st.sup.call("project", &json!({"action": "load_project", "args": {"path": path}}))?;
        if engine::is_error(&r) { Err(engine::result_text(&r)) } else { *st.chat.lock().unwrap() = Chat::default(); Ok(engine::result_text(&r)) }
    }).await.map_err(|e| e.to_string())?
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
        "pinned": download::pinned(p), "installed": download::installed(&st.data_dir, p),
    })).collect();
    json!({"done": st.settings.lock().unwrap().setup_done, "parts": parts, "dir": st.data_dir.display().to_string()})
}

#[tauri::command]
async fn setup_start(app: tauri::AppHandle, skip_optional: bool) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        st.cancel_dl.store(false, Ordering::SeqCst);
        let mut results = vec![];
        for p in st.parts.iter().filter(|p| !(skip_optional && p.optional)) {
            if !download::pinned(p) { results.push(json!({"id": p.id, "ok": false, "error": "not published yet"})); continue; }
            let a = app.clone();
            match download::fetch(&st.data_dir, p, &st.cancel_dl, &mut |pr| { let _ = a.emit("setup-progress", &pr); }) {
                Ok(_) => results.push(json!({"id": p.id, "ok": true})),
                Err(e) => { let msg = e.to_string(); let _ = app.emit("setup-progress", json!({"id": p.id, "phase": "error", "error": msg})); results.push(json!({"id": p.id, "ok": false, "error": msg})); if e == download::Failure::Cancelled { break; } }
            }
        }
        let all_required = st.parts.iter().filter(|p| !p.optional).all(|p| download::installed(&st.data_dir, p));
        if all_required { st.settings.lock().unwrap().setup_done = true; st.save(); st.sup.start(); }
        Ok(json!({"results": results, "ready": all_required}))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn setup_pause(st: State<'_, AppState>) { st.cancel_dl.store(true, Ordering::SeqCst); }

#[tauri::command]
fn setup_skip(st: State<'_, AppState>) { st.settings.lock().unwrap().setup_done = true; st.save(); }

// ---------- live feed ----------
/// Every engine event goes two ways: tagged for the Live tab, raw for the spectator graph window.
fn forward(app: &tauri::AppHandle, src: Source, ev: Value) {
    let _ = app.emit("live-event", json!({"source": src, "t": now_ms() as u64, "ev": ev}));
    let _ = app.emit("engine-event", ev);
}

/// `kimo-gui mcp`: the engine as a stdio MCP server, stdio inherited (what external clients run).
fn run_mcp_passthrough(args: &[String]) -> ! {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let Some(bin) = engine::find_engine(exe_dir.as_deref(), None) else {
        eprintln!("kimo: the engine (kimo-mcp) was not found next to this app or on PATH");
        std::process::exit(127);
    };
    let st = std::process::Command::new(bin).args(args).status();
    std::process::exit(st.ok().and_then(|s| s.code()).unwrap_or(1));
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(|a| a == "mcp").unwrap_or(false) { run_mcp_passthrough(&argv[1..]); }
    if argv.first().map(|a| a == "--version").unwrap_or(false) { println!("kimo-gui {}", env!("CARGO_PKG_VERSION")); return; }

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("kimo"));
            let config_dir = app.path().app_config_dir().unwrap_or_else(|_| data_dir.clone());
            let _ = std::fs::create_dir_all(&data_dir);
            let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
            let res_dir = app.path().resource_dir().ok();
            let bin = engine::find_engine(exe_dir.as_deref(), res_dir.as_deref());
            let fonts = std::env::var_os("KIMO_FONTS").map(PathBuf::from)
                .or_else(|| [exe_dir.as_ref().map(|d| d.join("fonts")), res_dir.as_ref().map(|d| d.join("fonts")), bin.as_ref().and_then(|b| b.parent()).map(|d| d.join("..").join("fonts"))]
                    .into_iter().flatten().find(|d| d.is_dir()));
            let parts = download::for_this_os(download::load_parts(PARTS_JSON).unwrap_or_default());
            let settings_path = config_dir.join("settings.json");
            let settings = Settings::load(&settings_path);

            // the GUI's own engine streams to a private loopback port: its events are tagged as the GUI agent's
            let h2 = handle.clone();
            let private = telemetry::listen_private(Arc::new(move |src, ev| forward(&h2, src, ev)))?;
            let mut env = engine_env(fonts.as_deref(), &data_dir, &parts);
            env.push(("KIMO_SPECTATOR".into(), format!("tcp://127.0.0.1:{private}")));
            let sup = Arc::new(Supervisor::new(bin, env, Some(data_dir.join("engine.log"))));

            let keyring = KeyRing::new(Box::new(keys::OsKeychain));
            for p in &settings.remembered { let _ = keyring.restore(p); }
            let mcp_on = settings.mcp_enabled;
            app.manage(AppState {
                sup: sup.clone(), keys: keyring, settings: Mutex::new(settings), settings_path,
                chat: Mutex::new(Chat::default()), external: Mutex::new(None), data_dir, fonts, parts,
                cancel_dl: Arc::new(AtomicBool::new(false)),
            });
            if mcp_on { if let Err(e) = start_external(&handle) { eprintln!("kimo: {e}"); } }
            let h3 = handle.clone();
            std::thread::spawn(move || { let s = sup.start(); let _ = h3.emit("engine-status", &s); });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state, engine_restart, mcp_set, mcp_probe, agents_detect, agent_add,
            key_set, key_clear, key_test, settings_set, chat_send, chat_undo, chat_reset,
            project_open, open_spectator, setup_status, setup_start, setup_pause, setup_skip
        ])
        .build(tauri::generate_context!())
        .expect("kimo failed to start")
        .run(|app, ev| {
            if let tauri::RunEvent::Exit = ev { app.state::<AppState>().sup.stop(); }
        });
}
