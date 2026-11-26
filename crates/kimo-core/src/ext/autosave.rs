//! Auto-save and crash recovery. Once on (set_autosave, or KIMO_AUTOSAVE=1), every edit that changes the project
//! writes it to a rolling set of files at most once per interval (default 1 s, so at most a second of work can be
//! lost), keeping the newest `keep`. list_autosaves shows them; recover_autosave loads the newest (or a chosen one).
use super::kit::*;
use std::cell::RefCell;

struct State { on: bool, interval: f64, keep: usize, dir: Option<std::path::PathBuf>, last: Option<std::time::Instant>, written: u64 }
// per thread: one engine session per thread (the MCP server, the app's engine thread); parallel sessions never write
// each other's autosaves
thread_local! { static STATE: RefCell<State> = const { RefCell::new(State { on: false, interval: 1.0, keep: 20, dir: None, last: None, written: 0 }) }; }

fn default_dir() -> std::path::PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("XDG_STATE_HOME")).map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| std::path::PathBuf::from(h).join(".local").join("state"))).unwrap_or_else(std::env::temp_dir);
    base.join("kimo").join("autosave")
}
fn dir_of(st: &State) -> std::path::PathBuf { st.dir.clone().unwrap_or_else(default_dir) }
fn safe(name: &str) -> String { let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(40).collect(); if s.is_empty() { "untitled".into() } else { s } }
fn list(dir: &std::path::Path) -> Vec<(std::path::PathBuf, std::time::SystemTime)> {
    let mut v: Vec<_> = std::fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok()).filter(|e| e.path().extension().map_or(false, |x| x == "kimo")).filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?))).collect()).unwrap_or_default();
    v.sort_by(|a, b| b.1.cmp(&a.1)); v
}

/// Called after every successful tool call (commands::dispatch): writes the project when the tool changed it.
pub fn after(ss: &Session, tool: &str) { STATE.with(|cell| { let mut st = cell.borrow_mut();
    if !st.on && std::env::var("KIMO_AUTOSAVE").map_or(true, |v| v != "1") { return; }
    if !crate::commands::registry_ref().iter().any(|c| c.name == tool && c.mutates) { return; }
    if st.last.map_or(false, |t| t.elapsed().as_secs_f64() < st.interval) { return; }
    let dir = dir_of(&st); if std::fs::create_dir_all(&dir).is_err() { return; }
    let Ok(json) = serde_json::to_vec(&ss.project) else { return };
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = dir.join(format!("{}-{ms}.kimo", safe(&ss.project.name)));
    // write then rename, so a crash mid-write never leaves a torn file as the newest one
    let tmp = path.with_extension("tmp"); if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &path).is_ok() { st.written += 1; st.last = Some(std::time::Instant::now()); }
    let keep = st.keep; for (old, _) in list(&dir).into_iter().skip(keep) { let _ = std::fs::remove_file(old); }
}) }

fn set_autosave(_ss: &mut Session, a: &Value) -> Result<Value, String> {
    STATE.with(|cell| { let mut st = cell.borrow_mut();
    if let Some(on) = a.get("enabled").and_then(|v| v.as_bool()) { st.on = on; }
    if let Some(i) = a.get("interval_s").and_then(|v| v.as_f64()) { if !(0.0..=3600.0).contains(&i) { return Err("interval_s must be 0..3600".into()); } st.interval = i; }
    if let Some(k) = a.get("keep").and_then(|v| v.as_u64()) { if !(1..=1000).contains(&k) { return Err("keep must be 1..1000".into()); } st.keep = k as usize; }
    if let Some(d) = a.get("dir").and_then(|v| v.as_str()) { st.dir = Some(d.into()); }
    Ok(json!({"enabled": st.on, "interval_s": st.interval, "keep": st.keep, "dir": dir_of(&st).to_string_lossy(), "written": st.written})) })
}
fn list_autosaves(_ss: &mut Session, _a: &Value) -> Result<Value, String> {
    let (dir, on) = STATE.with(|c| { let st = c.borrow(); (dir_of(&st), st.on) });
    let v: Vec<Value> = list(&dir).into_iter().take(50).map(|(p, t)| json!({"path": p.to_string_lossy(), "age_s": t.elapsed().map_or(0.0, |d| d.as_secs_f64().round())})).collect();
    Ok(json!({"dir": dir.to_string_lossy(), "enabled": on, "autosaves": v}))
}
fn recover_autosave(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = match a.get("path").and_then(|v| v.as_str()) { Some(p) => p.to_string(), None => { let dir = STATE.with(|c| dir_of(&c.borrow())); list(&dir).first().map(|(p, _)| p.to_string_lossy().to_string()).ok_or("no autosaves to recover")? } };
    let r = crate::commands::dispatch_nested(ss, "load_project", &json!({"path": path}))?;
    Ok(json!({"recovered": path, "load": r}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "set_autosave", description: "Turn auto-save (crash recovery) on or off: after each edit the project is written to a rolling set of files, at most every interval_s seconds (default 1), keeping the newest `keep` (default 20) in `dir` (default the user's local app data kimo/autosave). Also on with KIMO_AUTOSAVE=1.", mutates: false,
        schema: obj(vec![("enabled", json!({"type":"boolean"})), ("interval_s", n("seconds between saves, default 1")), ("keep", n("files kept, default 20")), ("dir", s("folder"))], &[]),
        run: set_autosave },
    Command { name: "list_autosaves", description: "List auto-saved projects, newest first, with their age.", mutates: false, schema: obj(vec![], &[]), run: list_autosaves },
    Command { name: "recover_autosave", description: "Load an auto-saved project (default the newest) after a crash or a lost session.", mutates: true,
        schema: obj(vec![("path", s("autosave file (default: the newest)"))], &[]), run: recover_autosave },
] }
