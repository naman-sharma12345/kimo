//! kimo GUI library: everything testable without a window. `main.rs` is the Tauri glue.
pub mod agent;
pub mod agents;
pub mod download;
pub mod engine;
pub mod keys;
pub mod mcp_config;
pub mod settings;
pub mod telemetry;
pub mod viewer;
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

/// Against the real engine (KIMO_MCP_BIN): the Live feed and the chat's Undo cards, end to end. Skipped when unset.
#[cfg(test)]
mod real_engine {
    use crate::agent::{Chat, Llm};
    use crate::engine::Supervisor;
    use crate::keys::{Secret, Wire};
    use crate::telemetry::{listen_external, listen_private, Sink, Source};
    use crate::testutil::{serve, Resp};
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    fn bin() -> Option<PathBuf> { std::env::var_os("KIMO_MCP_BIN").map(PathBuf::from).filter(|p| p.is_file()) }
    fn wait_for(got: &Arc<Mutex<Vec<(Source, Value)>>>, f: impl Fn(&(Source, Value)) -> bool) -> bool {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(5) { if got.lock().unwrap().iter().any(&f) { return true; } std::thread::sleep(Duration::from_millis(50)); }
        false
    }
    fn names_tool(v: &Value, tool: &str) -> bool { v.to_string().contains(tool) }

    #[test]
    fn live_feed_tags_gui_and_external_engines() {
        let Some(bin) = bin() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        let _ports = crate::testutil::PORTS.lock().unwrap_or_else(|e| e.into_inner());
        let got: Arc<Mutex<Vec<(Source, Value)>>> = Arc::new(Mutex::new(vec![]));
        let g2 = got.clone();
        let sink: Sink = Arc::new(move |s, v| g2.lock().unwrap().push((s, v)));
        // the GUI's own engine: exact private port
        let port = listen_private(sink.clone()).unwrap();
        let sup = Supervisor::new(Some(bin.clone()), vec![("KIMO_SPECTATOR".into(), format!("tcp://127.0.0.1:{port}"))], None);
        sup.start();
        sup.call("timeline", &json!({"action": "add_track", "args": {}})).unwrap();
        assert!(wait_for(&got, |(s, v)| *s == Source::Gui && names_tool(v, "add_track")), "GUI engine's call not seen: {:?}", got.lock().unwrap());
        // an external client's engine: default KIMO_SPECTATOR (unset) finds the switch's listener on 7421-7430
        let ext = listen_external(sink, Arc::new(|_| {})).unwrap();
        let other = Supervisor::new(Some(bin), vec![], None);
        other.start();
        std::thread::sleep(Duration::from_millis(300));
        other.call("timeline", &json!({"action": "add_track", "args": {}})).unwrap();
        assert!(wait_for(&got, |(s, v)| matches!(s, Source::External { .. }) && names_tool(v, "add_track")), "external engine's call not seen");
        let n = got.lock().unwrap().len();
        eprintln!("live feed: {n} events from two real engines (one GUI, one external)");
        other.stop(); sup.stop(); ext.stop();
    }

    #[test]
    fn chat_cards_undo_through_the_real_engine_history() {
        let Some(bin) = bin() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        let sup = Supervisor::new(Some(bin), vec![("KIMO_SPECTATOR".into(), "off".into())], None);
        sup.start();
        let turn = Mutex::new(0);
        let fx = serve(move |_| {
            let mut t = turn.lock().unwrap(); *t += 1;
            match *t {
                1 => Resp::json(200, &json!({"choices":[{"message":{"content":null,"tool_calls":[
                    {"id":"a","type":"function","function":{"name":"timeline","arguments":"{\"action\":\"add_track\",\"args\":{}}"}},
                    {"id":"b","type":"function","function":{"name":"project","arguments":"{\"action\":\"kimo_info\"}"}},
                    {"id":"c","type":"function","function":{"name":"timeline","arguments":"{\"action\":\"add_track\",\"args\":{}}"}}]}}]})),
                _ => Resp::json(200, &json!({"choices":[{"message":{"content":"Added two tracks."}}]})),
            }
        });
        let llm = Llm { wire: Wire::OpenAi, base_url: format!("{}/v1", fx.base), model: "m".into(), key: Secret::new("test-key-1234567") };
        let mut chat = Chat::default();
        let mut events = 0;
        let reply = chat.send(&llm, &sup, "add two tracks", &mut |_| events += 1).unwrap();
        assert_eq!(reply, "Added two tracks.");
        assert_eq!(chat.cards.len(), 2, "two edits, the read-only kimo_info made no card: {:?}", chat.cards);
        assert_eq!(crate::agent::history(&sup).unwrap().0, 2);
        // Undo the newer card: one engine step
        assert_eq!(chat.undo(&sup, 2).unwrap(), (1, vec![2]));
        assert_eq!(crate::agent::history(&sup).unwrap().0, 1);
        assert!(chat.undo(&sup, 2).is_err(), "already undone");
        assert_eq!(chat.undo(&sup, 1).unwrap(), (1, vec![1]));
        assert_eq!(crate::agent::history(&sup).unwrap().0, 0);
        eprintln!("real engine chat: {events} live events, 2 cards, both undone through project.undo");
        sup.stop();
    }

    #[test]
    fn undoing_an_older_card_undoes_the_later_ones_too() {
        let Some(bin) = bin() else { eprintln!("KIMO_MCP_BIN unset: skipped"); return };
        let sup = Supervisor::new(Some(bin), vec![("KIMO_SPECTATOR".into(), "off".into())], None);
        sup.start();
        let fx = serve(move |r| {
            let body: Value = serde_json::from_str(&r.body).unwrap();
            if body["messages"].as_array().unwrap().last().unwrap()["role"] == "user" {
                Resp::json(200, &json!({"choices":[{"message":{"content":null,"tool_calls":[
                    {"id":"x","type":"function","function":{"name":"timeline","arguments":"{\"action\":\"add_track\",\"args\":{}}"}}]}}]}))
            } else { Resp::json(200, &json!({"choices":[{"message":{"content":"ok"}}]})) }
        });
        let llm = Llm { wire: Wire::OpenAi, base_url: format!("{}/v1", fx.base), model: "m".into(), key: Secret::new("test-key-1234567") };
        let mut chat = Chat::default();
        for _ in 0..3 { chat.send(&llm, &sup, "one more track", &mut |_| {}).unwrap(); }
        assert_eq!(chat.cards.len(), 3);
        assert_eq!(chat.undo(&sup, 1).unwrap(), (3, vec![1, 2, 3]), "linear history: card 1 takes 2 and 3 with it");
        assert_eq!(crate::agent::history(&sup).unwrap().0, 0);
        sup.stop();
    }
}
