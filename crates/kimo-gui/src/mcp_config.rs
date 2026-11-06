//! Exact connection config for external MCP clients (Claude Code, Codex, any `mcpServers` JSON client).
//! Pure functions: the GUI shows these strings and copies them verbatim. Syntax verified 2026-11-05 against
//!   Claude Code: https://docs.claude.com/en/docs/claude-code/mcp.md  (`claude mcp add [options] <name> -- <command> [args...]`,
//!                `claude mcp add-json <name> '<json>'`, `--scope user`, `--env KEY=value` must not sit right before the name)
//!   Codex:       https://developers.openai.com/codex/mcp  (`[mcp_servers.<name>]` with command/args/env in ~/.codex/config.toml,
//!                `codex mcp add <name> --env K=V -- <command>`)
use serde::Serialize;
use serde_json::{json, Map, Value};

/// The stdio launch line an external client needs: the client starts this process itself.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Launch {
    pub command: String,
    pub args: Vec<String>,
    /// Ordered (key, value) pairs; order is kept so snippets are stable.
    pub env: Vec<(String, String)>,
}

pub const SERVER_NAME: &str = "kimo";

/// Quote one word for a POSIX shell: bare when it is safe, single quotes otherwise.
pub fn sh_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c)) { return s.to_string(); }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Quote one word for Windows PowerShell / cmd: bare when safe, double quotes otherwise (`"` doubled for PowerShell).
pub fn win_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@\\+,".contains(c)) { return s.to_string(); }
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn q(s: &str, windows: bool) -> String { if windows { win_quote(s) } else { sh_quote(s) } }

/// `mcpServers` JSON (Claude Desktop, Cursor, Claude Code's .mcp.json and most clients).
pub fn mcp_servers_json(l: &Launch) -> Value {
    json!({"mcpServers": {SERVER_NAME: server_object(l, false)}})
}

fn server_object(l: &Launch, typed: bool) -> Value {
    let mut o = Map::new();
    if typed { o.insert("type".into(), json!("stdio")); }
    o.insert("command".into(), json!(l.command));
    o.insert("args".into(), json!(l.args));
    if !l.env.is_empty() {
        let env: Map<String, Value> = l.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        o.insert("env".into(), Value::Object(env));
    }
    Value::Object(o)
}

/// The argv for `claude mcp add-json --scope user kimo '<json>'` (what "Add for me" runs; no shell involved).
pub fn claude_add_json_argv(l: &Launch) -> Vec<String> {
    vec!["mcp".into(), "add-json".into(), "--scope".into(), "user".into(), SERVER_NAME.into(), server_object(l, true).to_string()]
}

/// The copyable `claude mcp add` line. `--transport stdio` and `--scope user` sit between the `--env` pairs and the
/// name, as the docs require, and `--` separates Claude's options from the server command.
pub fn claude_add_command(l: &Launch, windows: bool) -> String {
    let mut parts: Vec<String> = vec!["claude".into(), "mcp".into(), "add".into()];
    for (k, v) in &l.env { parts.push("--env".into()); parts.push(q(&format!("{k}={v}"), windows)); }
    parts.extend(["--transport", "stdio", "--scope", "user", SERVER_NAME, "--"].map(String::from));
    parts.push(q(&l.command, windows));
    for a in &l.args { parts.push(q(a, windows)); }
    parts.join(" ")
}

/// The copyable `codex mcp add` line.
pub fn codex_add_command(l: &Launch, windows: bool) -> String {
    let mut parts: Vec<String> = vec!["codex".into(), "mcp".into(), "add".into(), SERVER_NAME.into()];
    for (k, v) in &l.env { parts.push("--env".into()); parts.push(q(&format!("{k}={v}"), windows)); }
    parts.push("--".into());
    parts.push(q(&l.command, windows));
    for a in &l.args { parts.push(q(a, windows)); }
    parts.join(" ")
}

/// A TOML basic string: backslashes and quotes escaped (Windows paths stay valid).
pub fn toml_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04X}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn toml_key(k: &str) -> String {
    if !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') { k.to_string() } else { toml_str(k) }
}

/// The `[mcp_servers.kimo]` table for ~/.codex/config.toml.
pub fn codex_toml(l: &Launch) -> String {
    let mut s = format!("[mcp_servers.{SERVER_NAME}]\ncommand = {}\n", toml_str(&l.command));
    let args: Vec<String> = l.args.iter().map(|a| toml_str(a)).collect();
    s.push_str(&format!("args = [{}]\n", args.join(", ")));
    if !l.env.is_empty() {
        let env: Vec<String> = l.env.iter().map(|(k, v)| format!("{} = {}", toml_key(k), toml_str(v))).collect();
        s.push_str(&format!("env = {{ {} }}\n", env.join(", ")));
    }
    s
}

/// Everything the MCP panel and the agent cards show, in one serializable bundle.
#[derive(Serialize)]
pub struct Snippets {
    pub launch: Launch,
    pub json: String,
    pub claude_add: String,
    pub claude_add_json: String,
    pub codex_add: String,
    pub codex_toml: String,
    pub claude_doc: &'static str,
    pub codex_doc: &'static str,
}

pub const CLAUDE_DOC: &str = "https://docs.claude.com/en/docs/claude-code/mcp.md";
pub const CODEX_DOC: &str = "https://developers.openai.com/codex/mcp";

pub fn snippets(l: &Launch, windows: bool) -> Snippets {
    let argv = claude_add_json_argv(l);
    let claude_add_json = format!("claude mcp add-json --scope user {SERVER_NAME} {}", q(&argv[5], windows));
    Snippets {
        launch: l.clone(),
        json: serde_json::to_string_pretty(&mcp_servers_json(l)).unwrap_or_default(),
        claude_add: claude_add_command(l, windows),
        claude_add_json,
        codex_add: codex_add_command(l, windows),
        codex_toml: codex_toml(l),
        claude_doc: CLAUDE_DOC,
        codex_doc: CODEX_DOC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn win() -> Launch {
        Launch { command: r"C:\Program Files\kimo\kimo-mcp.exe".into(), args: vec![],
                 env: vec![("KIMO_FONTS".into(), r"C:\Program Files\kimo\fonts".into())] }
    }
    fn nix() -> Launch {
        Launch { command: "/home/a b/.local/share/kimo/bin/kimo-mcp".into(), args: vec!["--x".into()], env: vec![] }
    }
    #[test]
    fn json_round_trips_windows_paths() {
        let v = mcp_servers_json(&win());
        assert_eq!(v["mcpServers"]["kimo"]["command"], r"C:\Program Files\kimo\kimo-mcp.exe");
        assert_eq!(v["mcpServers"]["kimo"]["env"]["KIMO_FONTS"], r"C:\Program Files\kimo\fonts");
        assert!(v["mcpServers"]["kimo"].get("type").is_none());
    }
    #[test]
    fn claude_add_orders_env_before_transport_and_name() {
        let s = claude_add_command(&win(), true);
        assert_eq!(s, r#"claude mcp add --env "KIMO_FONTS=C:\Program Files\kimo\fonts" --transport stdio --scope user kimo -- "C:\Program Files\kimo\kimo-mcp.exe""#);
        let s = claude_add_command(&nix(), false);
        assert_eq!(s, "claude mcp add --transport stdio --scope user kimo -- '/home/a b/.local/share/kimo/bin/kimo-mcp' --x");
    }
    #[test]
    fn claude_add_json_argv_is_typed_stdio() {
        let a = claude_add_json_argv(&win());
        assert_eq!(&a[..5], &["mcp", "add-json", "--scope", "user", "kimo"]);
        let v: Value = serde_json::from_str(&a[5]).unwrap();
        assert_eq!(v["type"], "stdio");
        assert_eq!(v["command"], r"C:\Program Files\kimo\kimo-mcp.exe");
    }
    #[test]
    fn codex_toml_escapes_backslashes() {
        let t = codex_toml(&win());
        assert_eq!(t, "[mcp_servers.kimo]\ncommand = \"C:\\\\Program Files\\\\kimo\\\\kimo-mcp.exe\"\nargs = []\nenv = { KIMO_FONTS = \"C:\\\\Program Files\\\\kimo\\\\fonts\" }\n");
        let t = codex_toml(&nix());
        assert!(t.contains("args = [\"--x\"]") && !t.contains("env ="));
    }
    #[test]
    fn codex_add_line() {
        assert_eq!(codex_add_command(&nix(), false), "codex mcp add kimo -- '/home/a b/.local/share/kimo/bin/kimo-mcp' --x");
    }
    #[test]
    fn quoting() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote(""), "''");
        assert_eq!(win_quote(r#"a "b""#), r#""a ""b""""#);
        assert_eq!(toml_str("a\"b\\c"), r#""a\"b\\c""#);
    }
}
