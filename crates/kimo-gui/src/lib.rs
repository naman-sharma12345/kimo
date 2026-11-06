//! kimo GUI library: everything testable without a window. `main.rs` is the Tauri glue.
pub mod agent;
pub mod agents;
pub mod download;
pub mod engine;
pub mod keys;
pub mod mcp_config;
pub mod settings;
pub mod telemetry;
#[cfg(test)]
mod testutil;

impl agent::Tools for engine::Supervisor {
    fn list(&self) -> Vec<serde_json::Value> { self.tools() }
    fn call(&self, name: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> { engine::Supervisor::call(self, name, args) }
}

/// The parts the first-run window offers (REFERENCE ONLY pins; the release workflow rewrites them).
pub const PARTS_JSON: &str = include_str!("../parts.json");

/// The environment the engine needs on this machine, for both the GUI's own engine and external clients' configs.
/// `tools_dir` holds downloaded ffmpeg/whisper; `fonts` is the bundled font dir.
pub fn engine_env(fonts: Option<&std::path::Path>, data_dir: &std::path::Path, parts: &[download::Part]) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(f) = fonts.filter(|f| f.is_dir()) { env.push(("KIMO_FONTS".into(), f.display().to_string())); }
    for p in parts {
        let path = data_dir.join(&p.dest);
        if !path.is_file() { continue; }
        match p.id.as_str() {
            "whisper" => env.push(("KIMO_WHISPER_BIN".into(), path.display().to_string())),
            "model" => env.push(("KIMO_WHISPER_MODEL".into(), path.display().to_string())),
            _ => {}
        }
    }
    let tools = data_dir.join("tools");
    if tools.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }).is_file() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        env.push(("PATH".into(), format!("{}{sep}{}", tools.display(), std::env::var("PATH").unwrap_or_default())));
    }
    env
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_parts_manifest_parses() {
        let p = super::download::for_this_os(super::download::load_parts(super::PARTS_JSON).unwrap());
        assert!(p.iter().any(|x| x.id == "ffmpeg"));
        assert_eq!(p.iter().filter(|x| x.id == "ffmpeg").count(), 1, "one ffmpeg per OS");
        assert!(p.iter().any(|x| x.id == "model" && super::download::pinned(x)));
        assert!(p.iter().all(|x| x.sha512.len() == 128 || x.sha512.starts_with("PIN")), "every part is pinned or visibly unpinned");
    }
    #[test]
    fn engine_env_uses_downloaded_parts_only_when_present() {
        let d = std::env::temp_dir().join(format!("kimo-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let parts = super::download::load_parts(super::PARTS_JSON).unwrap();
        assert!(super::engine_env(None, &d, &parts).is_empty());
        let model = parts.iter().find(|p| p.id == "model").unwrap();
        std::fs::create_dir_all(d.join(&model.dest).parent().unwrap()).unwrap();
        std::fs::write(d.join(&model.dest), b"x").unwrap();
        let env = super::engine_env(None, &d, &parts);
        assert_eq!(env[0].0, "KIMO_WHISPER_MODEL");
        let _ = std::fs::remove_dir_all(&d);
    }
}
