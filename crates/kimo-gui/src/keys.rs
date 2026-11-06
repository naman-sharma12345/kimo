//! Bring-your-own API key: the provider list, a redacting secret type, the credential store, and "Test connection".
//! Keys live in memory for the session by default. "Remember on this computer" stores a key in the OS credential
//! store (Windows Credential Manager, macOS Keychain, Linux Secret Service) through the `keyring` crate; nothing is
//! ever written to a plain file or a log. A key is only ever sent to the base URL of the provider it was entered for.
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// A secret string whose Debug/Display never shows the value.
#[derive(Clone, PartialEq)]
pub struct Secret(String);
impl Secret {
    pub fn new(s: impl Into<String>) -> Secret { Secret(s.into().trim().to_string()) }
    pub fn expose(&self) -> &str { &self.0 }
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
    /// "sk-…a1b2": enough for the user to recognise which key is set, never the key.
    pub fn hint(&self) -> String {
        let n = self.0.chars().count();
        if n <= 8 { return "••••".into(); }
        let tail: String = self.0.chars().skip(n - 4).collect();
        let head: String = self.0.chars().take(3).collect();
        format!("{head}…{tail}")
    }
}
impl std::fmt::Debug for Secret { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("Secret(<redacted>)") } }

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wire { OpenAi, Anthropic }

#[derive(Clone, Debug, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub wire: Wire,
    pub base_url: &'static str,
    /// A starting point only (REFERENCE ONLY): the user can type any model their key allows.
    pub default_model: &'static str,
    pub key_page: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider { id: "openai", name: "OpenAI", wire: Wire::OpenAi, base_url: "https://api.openai.com/v1", default_model: "gpt-4o-mini", key_page: "https://platform.openai.com/api-keys" },
    Provider { id: "anthropic", name: "Anthropic", wire: Wire::Anthropic, base_url: "https://api.anthropic.com", default_model: "claude-sonnet-4-5", key_page: "https://console.anthropic.com/settings/keys" },
    Provider { id: "gemini", name: "Google Gemini", wire: Wire::OpenAi, base_url: "https://generativelanguage.googleapis.com/v1beta/openai", default_model: "gemini-2.0-flash", key_page: "https://aistudio.google.com/apikey" },
    Provider { id: "openrouter", name: "OpenRouter", wire: Wire::OpenAi, base_url: "https://openrouter.ai/api/v1", default_model: "openai/gpt-4o-mini", key_page: "https://openrouter.ai/keys" },
    Provider { id: "groq", name: "Groq", wire: Wire::OpenAi, base_url: "https://api.groq.com/openai/v1", default_model: "llama-3.3-70b-versatile", key_page: "https://console.groq.com/keys" },
    Provider { id: "custom", name: "Custom (OpenAI-compatible, e.g. a local server)", wire: Wire::OpenAi, base_url: "http://127.0.0.1:11434/v1", default_model: "", key_page: "" },
];

pub fn provider(id: &str) -> Option<&'static Provider> { PROVIDERS.iter().find(|p| p.id == id) }

/// Where remembered keys go. The GUI uses `OsKeychain`; tests use `MemoryStore`.
pub trait SecretStore: Send + Sync {
    fn load(&self, provider: &str) -> Result<Option<Secret>, String>;
    fn save(&self, provider: &str, key: &Secret) -> Result<(), String>;
    fn delete(&self, provider: &str) -> Result<(), String>;
    fn label(&self) -> &'static str;
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, Secret>>);
impl SecretStore for MemoryStore {
    fn load(&self, p: &str) -> Result<Option<Secret>, String> { Ok(self.0.lock().unwrap().get(p).cloned()) }
    fn save(&self, p: &str, k: &Secret) -> Result<(), String> { self.0.lock().unwrap().insert(p.into(), k.clone()); Ok(()) }
    fn delete(&self, p: &str) -> Result<(), String> { self.0.lock().unwrap().remove(p); Ok(()) }
    fn label(&self) -> &'static str { "memory" }
}

pub struct OsKeychain;
const SERVICE: &str = "dev.kimo.app";
impl SecretStore for OsKeychain {
    fn load(&self, p: &str) -> Result<Option<Secret>, String> {
        let e = keyring::Entry::new(SERVICE, &format!("api-key:{p}")).map_err(|e| e.to_string())?;
        match e.get_password() { Ok(s) => Ok(Some(Secret::new(s))), Err(keyring::Error::NoEntry) => Ok(None), Err(e) => Err(e.to_string()) }
    }
    fn save(&self, p: &str, k: &Secret) -> Result<(), String> {
        keyring::Entry::new(SERVICE, &format!("api-key:{p}")).and_then(|e| e.set_password(k.expose())).map_err(|e| e.to_string())
    }
    fn delete(&self, p: &str) -> Result<(), String> {
        let e = keyring::Entry::new(SERVICE, &format!("api-key:{p}")).map_err(|e| e.to_string())?;
        match e.delete_credential() { Ok(()) | Err(keyring::Error::NoEntry) => Ok(()), Err(e) => Err(e.to_string()) }
    }
    fn label(&self) -> &'static str { if cfg!(windows) { "Windows Credential Manager" } else if cfg!(target_os = "macos") { "macOS Keychain" } else { "Secret Service keyring" } }
}

/// Session keys (memory) with optional persistence through a `SecretStore`.
pub struct KeyRing { session: Mutex<HashMap<String, Secret>>, store: Box<dyn SecretStore> }
#[derive(Serialize, Debug, PartialEq)]
pub struct KeyStatus { pub provider: String, pub set: bool, pub hint: Option<String>, pub remembered: bool, pub store: &'static str, pub note: Option<String> }

impl KeyRing {
    pub fn new(store: Box<dyn SecretStore>) -> KeyRing { KeyRing { session: Mutex::new(HashMap::new()), store } }
    pub fn store_label(&self) -> &'static str { self.store.label() }
    /// Load a remembered key (called once at startup for providers the settings say were remembered).
    pub fn restore(&self, provider: &str) -> Result<bool, String> {
        match self.store.load(provider)? { Some(k) if !k.is_empty() => { self.session.lock().unwrap().insert(provider.into(), k); Ok(true) } _ => Ok(false) }
    }
    pub fn set(&self, provider: &str, key: Secret, remember: bool) -> KeyStatus {
        let mut note = None;
        let mut remembered = false;
        if remember {
            match self.store.save(provider, &key) { Ok(()) => remembered = true, Err(e) => note = Some(format!("Could not save to the {}: {e}. The key is kept for this session only.", self.store.label())) }
        } else {
            let _ = self.store.delete(provider);
        }
        let hint = key.hint();
        self.session.lock().unwrap().insert(provider.into(), key);
        KeyStatus { provider: provider.into(), set: true, hint: Some(hint), remembered, store: self.store.label(), note }
    }
    pub fn clear(&self, provider: &str) -> KeyStatus {
        self.session.lock().unwrap().remove(provider);
        let note = self.store.delete(provider).err();
        KeyStatus { provider: provider.into(), set: false, hint: None, remembered: false, store: self.store.label(), note }
    }
    pub fn get(&self, provider: &str) -> Option<Secret> { self.session.lock().unwrap().get(provider).cloned() }
}

pub fn agent(timeout_s: u64) -> ureq::Agent { ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_s)).build() }

/// Turn a ureq error into one plain sentence. Never includes request headers (the key travels only in a header).
pub fn plain_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body).ok()
                .and_then(|v| v.pointer("/error/message").or_else(|| v.get("message")).or_else(|| v.get("error")).and_then(|m| m.as_str().map(String::from)))
                .unwrap_or_else(|| body.chars().take(160).collect());
            match code {
                401 | 403 => format!("The provider refused the key ({code}): {msg}"),
                404 => format!("Not found at that address ({code}). Check the base URL. {msg}"),
                429 => format!("Rate limited or out of credit ({code}): {msg}"),
                _ => format!("The provider answered {code}: {msg}"),
            }
        }
        ureq::Error::Transport(t) => format!("Could not reach the provider: {}", t.kind()),
    }
}

/// "Test connection": the cheapest authenticated read each API has (its model list). Returns "OK, N models".
pub fn test_connection(wire: Wire, base_url: &str, key: &Secret) -> Result<String, String> {
    let base = base_url.trim_end_matches('/');
    let a = agent(20);
    let req = match wire {
        Wire::OpenAi => a.get(&format!("{base}/models")).set("Authorization", &format!("Bearer {}", key.expose())),
        Wire::Anthropic => a.get(&format!("{base}/v1/models")).set("x-api-key", key.expose()).set("anthropic-version", "2023-06-01"),
    };
    let v: Value = req.call().map_err(plain_error)?.into_json().map_err(|e| format!("Unreadable answer: {e}"))?;
    let n = v.get("data").and_then(|d| d.as_array()).map(|a| a.len()).unwrap_or(0);
    Ok(format!("Connected. The key works ({n} models visible)."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{serve, Resp};
    use serde_json::json;
    #[test]
    fn secret_never_prints() {
        let s = Secret::new("  sk-abcdefghijklmnop1234 ");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert_eq!(s.hint(), "sk-…1234");
        assert_eq!(s.expose(), "sk-abcdefghijklmnop1234");
        assert_eq!(Secret::new("short").hint(), "••••");
    }
    #[test]
    fn keyring_session_and_remember() {
        let kr = KeyRing::new(Box::new(MemoryStore::default()));
        let st = kr.set("openai", Secret::new("sk-session-only-123456"), false);
        assert!(st.set && !st.remembered);
        assert!(!kr.restore("openai").unwrap(), "not remembered: nothing in the store");
        let st = kr.set("openai", Secret::new("sk-remember-me-987654"), true);
        assert!(st.remembered);
        let kr2 = KeyRing { session: Mutex::new(HashMap::new()), store: Box::new(MemoryStore::default()) };
        assert!(kr2.get("openai").is_none());
        assert!(kr.restore("openai").unwrap());
        kr.clear("openai");
        assert!(kr.get("openai").is_none() && !kr.restore("openai").unwrap());
    }
    struct Failing;
    impl SecretStore for Failing {
        fn load(&self, _: &str) -> Result<Option<Secret>, String> { Err("no keyring daemon".into()) }
        fn save(&self, _: &str, _: &Secret) -> Result<(), String> { Err("no keyring daemon".into()) }
        fn delete(&self, _: &str) -> Result<(), String> { Ok(()) }
        fn label(&self) -> &'static str { "test store" }
    }
    #[test]
    fn remember_failure_keeps_session_key_and_says_so() {
        let kr = KeyRing::new(Box::new(Failing));
        let st = kr.set("anthropic", Secret::new("sk-ant-xxxxxxxxxxxx"), true);
        assert!(st.set && !st.remembered);
        assert!(st.note.unwrap().contains("this session only"));
        assert!(kr.get("anthropic").is_some());
    }
    #[test]
    fn test_connection_openai_and_anthropic_against_fixture() {
        let fx = serve(|r| {
            match (r.path.as_str(), r.header("authorization"), r.header("x-api-key")) {
                ("/v1/models", Some("Bearer good-key-123456"), _) => Resp::json(200, &json!({"data":[{"id":"a"},{"id":"b"}]})),
                ("/v1/v1/models", _, Some("good-key-123456")) if r.header("anthropic-version").is_some() => Resp::json(200, &json!({"data":[{"id":"c"}]})),
                _ => Resp::json(401, &json!({"error":{"message":"invalid api key"}})),
            }
        });
        let base = format!("{}/v1", fx.base);
        assert_eq!(test_connection(Wire::OpenAi, &base, &Secret::new("good-key-123456")).unwrap(), "Connected. The key works (2 models visible).");
        let e = test_connection(Wire::OpenAi, &base, &Secret::new("bad-key-0000000")).unwrap_err();
        assert!(e.contains("refused the key (401)") && e.contains("invalid api key") && !e.contains("bad-key"), "{e}");
        assert!(test_connection(Wire::Anthropic, &base, &Secret::new("good-key-123456")).unwrap().contains("1 models"));
        // the key went only in a header to the fixture, never in a URL
        assert!(fx.log.lock().unwrap().iter().all(|r| !r.path.contains("key")));
    }
    #[test]
    fn unreachable_is_plain() {
        let e = test_connection(Wire::OpenAi, "http://127.0.0.1:9/v1", &Secret::new("k-123456789")).unwrap_err();
        assert!(e.starts_with("Could not reach the provider"), "{e}");
    }
}
