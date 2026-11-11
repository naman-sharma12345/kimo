//! Non-secret settings, kept as JSON in the app's config dir. API keys are NEVER stored here (see keys.rs).
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// MCP switch: off by default. On = the engine was verified and the external-activity listener runs.
    pub mcp_enabled: bool,
    pub provider: String,
    /// Per-provider model and base URL overrides; empty = the provider's default (REFERENCE ONLY defaults).
    pub models: HashMap<String, String>,
    pub base_urls: HashMap<String, String>,
    /// Providers whose key the user asked to remember (the key itself is in the OS credential store).
    pub remembered: Vec<String>,
    /// The first-run setup was finished or skipped.
    pub setup_done: bool,
    /// Which agent the Agent tab talks to: "claude-code" | "codex" | "antigravity" | "api" (own key).
    pub agent: String,
    /// Optional model per subscription agent ("" = the CLI's own default).
    pub agent_models: HashMap<String, String>,
    /// Optional reasoning effort per subscription agent ("" = the CLI's own default).
    pub agent_efforts: HashMap<String, String>,
    /// Recently opened or saved project files, newest first.
    pub recent: Vec<String>,
    /// The welcome tour was finished or skipped.
    pub onboarded: bool,
}
impl Default for Settings {
    fn default() -> Self { Settings { mcp_enabled: false, provider: "openai".into(), models: HashMap::new(), base_urls: HashMap::new(), remembered: vec![], setup_done: false,
        agent: "claude-code".into(), agent_models: HashMap::new(), agent_efforts: HashMap::new(), recent: vec![], onboarded: false } }
}
impl Settings {
    pub fn load(p: &Path) -> Settings { std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() }
    /// Put a project path at the top of the recent list (max 8, no duplicates).
    pub fn touch_recent(&mut self, path: &str) {
        self.recent.retain(|r| r != path);
        self.recent.insert(0, path.to_string());
        self.recent.truncate(8);
    }
    pub fn save(&self, p: &Path) -> Result<(), String> {
        if let Some(d) = p.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, p).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_off_and_round_trip() {
        let d = std::env::temp_dir().join(format!("kimo-settings-{}", std::process::id()));
        let p = d.join("settings.json");
        let s = Settings::load(&p);
        assert!(!s.mcp_enabled, "MCP is off by default");
        let mut s2 = s.clone(); s2.mcp_enabled = true; s2.remembered.push("openai".into());
        s2.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s2);
        assert!(!std::fs::read_to_string(&p).unwrap().contains("sk-"));
        std::fs::write(&p, "{\"provider\":\"groq\"}").unwrap();
        assert_eq!(Settings::load(&p).provider, "groq");
        assert_eq!(Settings::load(&p).agent, "claude-code", "old settings files get the new defaults");
        let mut r = Settings::default();
        for i in 0..10 { r.touch_recent(&format!("p{i}")); }
        r.touch_recent("p5");
        assert_eq!(r.recent.len(), 8);
        assert_eq!(r.recent[0], "p5");
        let _ = std::fs::remove_dir_all(&d);
    }
}
