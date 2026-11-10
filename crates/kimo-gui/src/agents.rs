//! Coding-agent linking: read-only detection of the Claude Code / Codex CLIs and of an existing `kimo` entry in their
//! config, plus the "Add for me" actions that run ONLY when the user clicks (the GUI calls `add_*` from a button).
use crate::mcp_config::{self, Launch, SERVER_NAME};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct AgentState {
    pub id: &'static str,
    pub name: &'static str,
    /// Absolute path of the CLI found on PATH, if any.
    pub cli: Option<String>,
    /// The config file that was read (whether or not it exists).
    pub config_path: Option<String>,
    /// A `kimo` server entry is present in that config.
    pub linked: bool,
    /// The command the existing entry launches, when readable.
    pub linked_command: Option<String>,
    pub note: Option<String>,
}

/// Find an executable on a PATH string (PATHEXT-aware on Windows). Pure: PATH and PATHEXT are passed in.
pub fn which_in(name: &str, path_var: &str, pathext: Option<&str>) -> Option<PathBuf> {
    let sep = if cfg!(windows) { ';' } else { ':' };
    let exts: Vec<String> = match pathext {
        Some(pe) => std::iter::once(String::new()).chain(pe.split(';').filter(|e| !e.is_empty()).map(|e| e.to_ascii_lowercase())).collect(),
        None => vec![String::new()],
    };
    for dir in path_var.split(sep).filter(|d| !d.is_empty()) {
        for ext in &exts {
            let p = Path::new(dir).join(format!("{name}{ext}"));
            if is_executable(&p) { return Some(p); }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}
#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool { p.is_file() }

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let pathext = if cfg!(windows) { Some(std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())) } else { None };
    which_in(name, &path, pathext.as_deref())
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// Claude Code keeps user/local-scope servers in ~/.claude.json: top-level `mcpServers` (user scope) and
/// `projects.<dir>.mcpServers` (local scope). Returns the command of a `kimo` entry, if any.
pub fn claude_entry(claude_json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(claude_json).ok()?;
    let pick = |m: &Value| m.get("mcpServers").and_then(|s| s.get(SERVER_NAME)).map(|e| e.get("command").and_then(|c| c.as_str()).unwrap_or("").to_string());
    if let Some(c) = pick(&v) { return Some(c); }
    v.get("projects")?.as_object()?.values().find_map(pick)
}

/// Codex: a `[mcp_servers.kimo]` table in config.toml. A small line scan (no TOML dependency): good enough to detect
/// the table header and read its `command = "..."` line.
pub fn codex_entry(config_toml: &str) -> Option<String> {
    let mut inside = false;
    let mut found = false;
    let mut cmd = None;
    for raw in config_toml.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            let hdr = line.trim_matches(|c| c == '[' || c == ']').trim();
            inside = hdr == format!("mcp_servers.{SERVER_NAME}") || hdr == format!("mcp_servers.\"{SERVER_NAME}\"");
            if inside { found = true; }
            continue;
        }
        if inside {
            if let Some(rest) = line.strip_prefix("command") {
                let rest = rest.trim_start();
                if let Some(val) = rest.strip_prefix('=') {
                    let val = val.trim();
                    cmd = Some(unquote_toml(val));
                }
            }
        }
    }
    if found { Some(cmd.unwrap_or_default()) } else { None }
}

fn unquote_toml(v: &str) -> String {
    if let Some(s) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) { return s.replace("\\\\", "\\").replace("\\\"", "\""); }
    if let Some(s) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) { return s.to_string(); }
    v.to_string()
}

pub fn codex_config_path(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex")).join("config.toml")
}

/// Read-only detection of both agents. Never writes, never runs the CLIs.
pub fn detect(home: Option<&Path>) -> Vec<AgentState> {
    let mut out = Vec::new();
    let claude_cfg = home.map(|h| h.join(".claude.json"));
    let ce = claude_cfg.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| claude_entry(&s));
    out.push(AgentState {
        id: "claude", name: "Claude Code",
        cli: which("claude").map(|p| p.display().to_string()),
        config_path: claude_cfg.map(|p| p.display().to_string()),
        linked: ce.is_some(), linked_command: ce,
        note: None,
    });
    let codex_cfg = home.map(codex_config_path);
    let xe = codex_cfg.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| codex_entry(&s));
    out.push(AgentState {
        id: "codex", name: "Codex",
        cli: which("codex").map(|p| p.display().to_string()),
        config_path: codex_cfg.map(|p| p.display().to_string()),
        linked: xe.is_some(), linked_command: xe,
        note: None,
    });
    out
}

/// Append the `[mcp_servers.kimo]` table to a Codex config (creating it), after a `.bak` copy of the old file.
/// Refuses when a kimo table already exists (the user edits or removes it; nothing is overwritten).
pub fn add_codex(config: &Path, l: &Launch) -> Result<String, String> {
    let old = std::fs::read_to_string(config).unwrap_or_default();
    if codex_entry(&old).is_some() { return Err(format!("{} already has a [mcp_servers.{SERVER_NAME}] table; left unchanged", config.display())); }
    if let Some(dir) = config.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
    if !old.is_empty() {
        let bak = config.with_extension("toml.bak");
        std::fs::write(&bak, &old).map_err(|e| e.to_string())?;
    }
    let mut new = old.clone();
    if !new.is_empty() && !new.ends_with('\n') { new.push('\n'); }
    if !new.is_empty() { new.push('\n'); }
    new.push_str("# added by kimo GUI\n");
    new.push_str(&mcp_config::codex_toml(l));
    std::fs::write(config, new).map_err(|e| e.to_string())?;
    Ok(format!("wrote [mcp_servers.{SERVER_NAME}] to {}", config.display()))
}

/// Run `claude mcp add-json --scope user kimo '<json>'` (argv, no shell). Returns the CLI's own output.
pub fn add_claude(cli: &Path, l: &Launch) -> Result<String, String> {
    let out = std::process::Command::new(cli).args(mcp_config::claude_add_json_argv(l))
        .stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if out.status.success() { Ok(text) } else { Err(if text.is_empty() { format!("claude exited with {}", out.status) } else { text }) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kimo-gui-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    #[test]
    fn claude_json_user_and_local_scope() {
        assert_eq!(claude_entry(r#"{"mcpServers":{"kimo":{"type":"stdio","command":"/x/kimo-mcp"}}}"#).as_deref(), Some("/x/kimo-mcp"));
        assert_eq!(claude_entry(r#"{"projects":{"/p":{"mcpServers":{"kimo":{"command":"k"}}}}}"#).as_deref(), Some("k"));
        assert_eq!(claude_entry(r#"{"mcpServers":{"other":{}}}"#), None);
        assert_eq!(claude_entry("not json"), None);
    }
    #[test]
    fn codex_table_detection() {
        let t = "model = \"x\"\n[mcp_servers.other]\ncommand = \"o\"\n[mcp_servers.kimo]\ncommand = \"C:\\\\k\\\\kimo-mcp.exe\"\nargs = []\n";
        assert_eq!(codex_entry(t).as_deref(), Some("C:\\k\\kimo-mcp.exe"));
        assert_eq!(codex_entry("[mcp_servers.kimo_old]\ncommand=\"x\"\n"), None);
        assert_eq!(codex_entry(""), None);
    }
    #[test]
    fn add_codex_appends_with_backup_and_refuses_twice() {
        let d = tmp("codex");
        let cfg = d.join(".codex").join("config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, "model = \"m\"").unwrap();
        let l = Launch { command: "/opt/kimo/kimo-mcp".into(), args: vec![], env: vec![] };
        add_codex(&cfg, &l).unwrap();
        let s = std::fs::read_to_string(&cfg).unwrap();
        assert!(s.starts_with("model = \"m\"\n\n# added by kimo GUI\n[mcp_servers.kimo]\n"), "{s}");
        assert_eq!(codex_entry(&s).as_deref(), Some("/opt/kimo/kimo-mcp"));
        assert_eq!(std::fs::read_to_string(cfg.with_extension("toml.bak")).unwrap(), "model = \"m\"");
        assert!(add_codex(&cfg, &l).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
    #[cfg(unix)]
    #[test]
    fn which_finds_only_executables() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("which");
        let f = d.join("claude");
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        assert_eq!(which_in("claude", d.to_str().unwrap(), None), None, "not executable yet");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(which_in("claude", &format!("/nonexistent:{}", d.display()), None), Some(f));
        let _ = std::fs::remove_dir_all(&d);
    }
    #[cfg(unix)]
    #[test]
    fn add_claude_runs_argv_without_shell() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmp("claudecli");
        let f = d.join("claude");
        let log = d.join("argv.txt");
        std::fs::write(&f, format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\necho Added stdio MCP server kimo\n", log.display())).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        let l = Launch { command: "/a b/kimo-mcp".into(), args: vec![], env: vec![("K".into(), "v w".into())] };
        let out = add_claude(&f, &l).unwrap();
        assert!(out.contains("Added"));
        let argv = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = argv.lines().collect();
        assert_eq!(&lines[..5], &["mcp", "add-json", "--scope", "user", "kimo"]);
        let v: Value = serde_json::from_str(lines[5]).unwrap();
        assert_eq!(v["command"], "/a b/kimo-mcp");
        assert_eq!(v["env"]["K"], "v w");
        let _ = std::fs::remove_dir_all(&d);
    }
}
