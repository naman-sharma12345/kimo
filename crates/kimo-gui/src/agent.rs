//! The GUI's own editing agent (bring-your-own key). The user types a change ("captions bigger", "cut the pause at
//! 0:12"); the agent calls the engine's real MCP tools; every call that grows the engine's undo history becomes a
//! change card whose Undo uses the engine's own `undo` (project{action:"undo", args:{steps}}).
use crate::engine::{is_error, result_text};
use crate::keys::{agent as http, plain_error, Secret, Wire};
use serde::Serialize;
use serde_json::{json, Value};

pub const MAX_STEPS: usize = 12;
pub const MAX_RESULT_CHARS: usize = 4000;

pub const SYSTEM_PROMPT: &str = "You are kimo's editing agent inside the kimo desktop app. The user is looking at their \
video project and tells you what to change. Use the kimo tools: each tool is a group that takes {\"action\": ..., \"args\": {...}}. \
When you are unsure of an action's name or arguments, call search_tools or kimo_describe first. Make exactly the change the \
user asked for, the smallest edit that does it; do not export or delete files unless asked. Every change you make is shown \
to the user live and can be undone. When done, reply in one or two plain sentences saying what you changed. kimo shows you every edit: each call that changes the project returns `observe` (what it touched, before/after frames as an image, brightness numbers, and findings: mistakes with severity and the exact fix call). Read it after every edit and fix any error or warning before moving on. For anything that must hold, pass expect: [...] on the edit (visible/hidden at a time, inside title_safe, luma range, no_black_frames, readable, max_findings); if it fails the edit is rolled back and you are told why. Run review before export. Use observe: \"full\" for a sharper look, or render_frame/contact_sheet for any moment you want to see.";

/// What the agent drives: the supervised engine in the app, a fake in tests.
pub trait Tools {
    fn list(&self) -> Vec<Value>;
    fn call(&self, name: &str, args: &Value) -> Result<Value, String>;
}

/// Undo history depth and the newest label, read through project{action:"history_list"}.
pub fn history(t: &dyn Tools) -> Option<(usize, Option<String>)> {
    let r = t.call("project", &json!({"action": "history_list"})).ok()?;
    if is_error(&r) { return None; }
    let v: Value = serde_json::from_str(&result_text(&r)).ok()?;
    let undo = v.get("undo")?.as_array()?;
    let label = undo.last().map(|e| match e { Value::String(s) => s.clone(), o => o.get("label").or_else(|| o.get("tool")).and_then(|x| x.as_str()).unwrap_or("change").to_string() });
    Some((undo.len(), label))
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Card { pub id: usize, pub depth: usize, pub label: String, pub tool: String, pub summary: String, pub undone: bool }

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Thinking { step: usize },
    ToolStart { call: String, tool: String, args: Value, summary: String },
    ToolEnd { call: String, tool: String, ok: bool, result: String, card: Option<Card> },
    Reply { text: String },
    Error { message: String },
}

#[derive(Clone, Debug)]
enum Item { User(String), Assistant { text: String, calls: Vec<(String, String, Value)> }, Result { id: String, text: String, error: bool } }

pub struct Llm { pub wire: Wire, pub base_url: String, pub model: String, pub key: Secret }

#[derive(Default)]
pub struct Chat { items: Vec<Item>, pub cards: Vec<Card> }

/// "color_grade · gamma=1.1, lift=0.02" style one-liner for a tool call.
pub fn summarize(tool: &str, args: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(a) = args.get("action").and_then(|a| a.as_str()) { parts.push(a.to_string()); }
    let inner = args.get("args").unwrap_or(args);
    if let Some(o) = inner.as_object() {
        let kv: Vec<String> = o.iter().filter(|(k, _)| k.as_str() != "action").take(4).map(|(k, v)| {
            let s = match v { Value::String(s) => s.clone(), other => other.to_string() };
            let s: String = if s.chars().count() > 28 { format!("{}…", s.chars().take(27).collect::<String>()) } else { s };
            format!("{k}={s}")
        }).collect();
        if !kv.is_empty() { parts.push(kv.join(", ")); }
    }
    if parts.is_empty() { tool.to_string() } else { format!("{tool} · {}", parts.join(" · ")) }
}

fn clip(s: &str, n: usize) -> String { if s.chars().count() > n { format!("{}… [cut at {n} chars]", s.chars().take(n).collect::<String>()) } else { s.to_string() } }

impl Llm {
    fn body(&self, items: &[Item], tools: &[Value]) -> Value {
        match self.wire {
            Wire::OpenAi => {
                let mut msgs = vec![json!({"role": "system", "content": SYSTEM_PROMPT})];
                for it in items { match it {
                    Item::User(t) => msgs.push(json!({"role": "user", "content": t})),
                    Item::Assistant { text, calls } => {
                        let mut m = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
                        if !calls.is_empty() { m["tool_calls"] = json!(calls.iter().map(|(id, n, a)| json!({"id": id, "type": "function", "function": {"name": n, "arguments": a.to_string()}})).collect::<Vec<_>>()); }
                        msgs.push(m);
                    }
                    Item::Result { id, text, .. } => msgs.push(json!({"role": "tool", "tool_call_id": id, "content": text})),
                } }
                let tl: Vec<Value> = tools.iter().map(|t| json!({"type": "function", "function": {"name": t["name"], "description": t.get("description").cloned().unwrap_or(json!("")), "parameters": t.get("inputSchema").cloned().unwrap_or(json!({"type":"object"}))}})).collect();
                json!({"model": self.model, "messages": msgs, "tools": tl})
            }
            Wire::Anthropic => {
                let mut msgs: Vec<Value> = Vec::new();
                for it in items { match it {
                    Item::User(t) => msgs.push(json!({"role": "user", "content": t})),
                    Item::Assistant { text, calls } => {
                        let mut c = Vec::new();
                        if !text.is_empty() { c.push(json!({"type": "text", "text": text})); }
                        for (id, n, a) in calls { c.push(json!({"type": "tool_use", "id": id, "name": n, "input": a})); }
                        msgs.push(json!({"role": "assistant", "content": c}));
                    }
                    Item::Result { id, text, error } => {
                        let block = json!({"type": "tool_result", "tool_use_id": id, "content": text, "is_error": error});
                        // consecutive results share one user turn
                        match msgs.last_mut() {
                            Some(m) if m["role"] == "user" && m["content"].is_array() && m["content"][0]["type"] == "tool_result" => m["content"].as_array_mut().unwrap().push(block),
                            _ => msgs.push(json!({"role": "user", "content": [block]})),
                        }
                    }
                } }
                let tl: Vec<Value> = tools.iter().map(|t| json!({"name": t["name"], "description": t.get("description").cloned().unwrap_or(json!("")), "input_schema": t.get("inputSchema").cloned().unwrap_or(json!({"type":"object"}))})).collect();
                json!({"model": self.model, "max_tokens": 2048, "system": SYSTEM_PROMPT, "messages": msgs, "tools": tl})
            }
        }
    }

    fn send(&self, body: &Value) -> Result<Value, String> {
        let base = self.base_url.trim_end_matches('/');
        let a = http(120);
        let r = match self.wire {
            Wire::OpenAi => a.post(&format!("{base}/chat/completions")).set("Authorization", &format!("Bearer {}", self.key.expose())),
            Wire::Anthropic => a.post(&format!("{base}/v1/messages")).set("x-api-key", self.key.expose()).set("anthropic-version", "2023-06-01"),
        };
        r.send_json(body.clone()).map_err(plain_error)?.into_json().map_err(|e| format!("Unreadable answer: {e}"))
    }

    fn parse(&self, v: &Value) -> Result<(String, Vec<(String, String, Value)>), String> {
        match self.wire {
            Wire::OpenAi => {
                let m = v.pointer("/choices/0/message").ok_or("the provider sent no message")?;
                let text = m.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                let calls = m.get("tool_calls").and_then(|c| c.as_array()).map(|a| a.iter().map(|c| {
                    let args = c.pointer("/function/arguments").and_then(|s| s.as_str()).and_then(|s| serde_json::from_str(s).ok()).unwrap_or(json!({}));
                    (c["id"].as_str().unwrap_or("call").to_string(), c.pointer("/function/name").and_then(|n| n.as_str()).unwrap_or("").to_string(), args)
                }).collect()).unwrap_or_default();
                Ok((text, calls))
            }
            Wire::Anthropic => {
                let blocks = v.get("content").and_then(|c| c.as_array()).ok_or("the provider sent no content")?;
                let text = blocks.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n");
                let calls = blocks.iter().filter(|b| b["type"] == "tool_use").map(|b| (b["id"].as_str().unwrap_or("").to_string(), b["name"].as_str().unwrap_or("").to_string(), b.get("input").cloned().unwrap_or(json!({})))).collect();
                Ok((text, calls))
            }
        }
    }
}

impl Chat {
    /// One user message, run to the agent's final reply. `emit` sees every step live.
    pub fn send(&mut self, llm: &Llm, tools: &dyn Tools, text: &str, emit: &mut dyn FnMut(Event)) -> Result<String, String> {
        self.items.push(Item::User(text.to_string()));
        let list = tools.list();
        for step in 0..MAX_STEPS {
            emit(Event::Thinking { step });
            let v = match llm.send(&llm.body(&self.items, &list)) { Ok(v) => v, Err(e) => { emit(Event::Error { message: e.clone() }); self.items.pop_if_dangling(); return Err(e); } };
            let (reply, calls) = llm.parse(&v)?;
            self.items.push(Item::Assistant { text: reply.clone(), calls: calls.clone() });
            if calls.is_empty() {
                emit(Event::Reply { text: reply.clone() });
                return Ok(reply);
            }
            for (id, name, args) in calls {
                let summary = summarize(&name, &args);
                emit(Event::ToolStart { call: id.clone(), tool: name.clone(), args: args.clone(), summary: summary.clone() });
                let before = history(tools).map(|h| h.0);
                let (ok, out) = match tools.call(&name, &args) { Ok(r) => (!is_error(&r), result_text(&r)), Err(e) => (false, e) };
                let card = if ok { self.card_if_changed(tools, before, &name, &summary) } else { None };
                emit(Event::ToolEnd { call: id.clone(), tool: name.clone(), ok, result: clip(&out, 600), card });
                self.items.push(Item::Result { id, text: clip(&out, MAX_RESULT_CHARS), error: !ok });
            }
        }
        let msg = format!("Stopped after {MAX_STEPS} steps. Ask again to continue.");
        emit(Event::Reply { text: msg.clone() });
        Ok(msg)
    }

    fn card_if_changed(&mut self, tools: &dyn Tools, before: Option<usize>, tool: &str, summary: &str) -> Option<Card> {
        let (after, label) = history(tools)?;
        if after <= before.unwrap_or(after) { return None; }
        // anything at or past this depth from an earlier branch was replaced by this new change
        for c in self.cards.iter_mut() { if c.depth >= after && !c.undone { c.undone = true; } }
        let card = Card { id: self.cards.len() + 1, depth: after, label: label.unwrap_or_else(|| tool.to_string()), tool: tool.to_string(), summary: summary.to_string(), undone: false };
        self.cards.push(card.clone());
        Some(card)
    }

    /// Undo one card: the engine's history is linear, so undoing a card also undoes every later live change.
    /// Returns (steps undone, ids of the cards now undone).
    pub fn undo(&mut self, tools: &dyn Tools, card_id: usize) -> Result<(usize, Vec<usize>), String> {
        let depth = self.cards.iter().find(|c| c.id == card_id).ok_or("no such change")?.depth;
        let (now, _) = history(tools).ok_or("could not read the engine's history")?;
        if now < depth { return Err("that change is already undone".into()); }
        let steps = now - depth + 1;
        let r = tools.call("project", &json!({"action": "undo", "args": {"steps": steps}}))?;
        if is_error(&r) { return Err(result_text(&r)); }
        let mut ids = vec![];
        for c in self.cards.iter_mut() { if c.depth >= depth && !c.undone { c.undone = true; ids.push(c.id); } }
        Ok((steps, ids))
    }
}

trait PopDangling { fn pop_if_dangling(&mut self); }
impl PopDangling for Vec<Item> {
    /// A failed provider call leaves the user's message unanswered; drop it so a retry does not send it twice.
    fn pop_if_dangling(&mut self) { if matches!(self.last(), Some(Item::User(_))) { self.pop(); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{serve, Resp};
    use std::sync::Mutex;

    /// A fake engine with a real linear undo stack.
    #[derive(Default)]
    struct FakeEngine { undo: Mutex<Vec<String>>, calls: Mutex<Vec<String>> }
    impl Tools for FakeEngine {
        fn list(&self) -> Vec<Value> { vec![json!({"name":"text","description":"Text [9 actions]","inputSchema":{"type":"object","properties":{"action":{"type":"string"},"args":{"type":"object"}}}}), json!({"name":"project","description":"P"})] }
        fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
            let action = args["action"].as_str().unwrap_or("");
            if !(name == "project" && action == "history_list") { self.calls.lock().unwrap().push(format!("{name}.{action}")); }
            let mut u = self.undo.lock().unwrap();
            let text = match (name, action) {
                ("project", "history_list") => json!({"undo": *u, "redo": []}).to_string(),
                ("project", "undo") => { let n = args["args"]["steps"].as_u64().unwrap_or(1) as usize; for _ in 0..n { u.pop(); } format!("undid {n}") }
                ("text", "describe") => "read only".into(),
                ("text", a) => { u.push(format!("{a}")); format!("did {a}") }
                _ => return Ok(json!({"content":[{"type":"text","text":"unknown"}],"isError":true})),
            };
            Ok(json!({"content":[{"type":"text","text":text}],"isError":false}))
        }
    }

    fn openai_script() -> crate::testutil::Fixture {
        let turn = Mutex::new(0);
        serve(move |r| {
            assert_eq!(r.header("authorization"), Some("Bearer test-key-1234567"));
            let body: Value = serde_json::from_str(&r.body).unwrap();
            assert_eq!(body["tools"][0]["function"]["name"], "text");
            let mut t = turn.lock().unwrap(); *t += 1;
            match *t {
                1 => Resp::json(200, &json!({"choices":[{"message":{"content":null,"tool_calls":[
                        {"id":"c1","type":"function","function":{"name":"text","arguments":"{\"action\":\"describe\"}"}},
                        {"id":"c2","type":"function","function":{"name":"text","arguments":"{\"action\":\"set_caption_style\",\"args\":{\"size\":72}}"}}]}}]})),
                2 => {
                    // the tool results came back with the matching ids
                    let msgs = body["messages"].as_array().unwrap();
                    assert!(msgs.iter().any(|m| m["role"] == "tool" && m["tool_call_id"] == "c2" && m["content"] == "did set_caption_style"));
                    Resp::json(200, &json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"c3","type":"function","function":{"name":"text","arguments":"{\"action\":\"trim_pause\",\"args\":{\"at\":12}}"}}]}}]}))
                }
                _ => Resp::json(200, &json!({"choices":[{"message":{"content":"Made the captions bigger and cut the pause at 0:12."}}]})),
            }
        })
    }

    #[test]
    fn openai_loop_makes_cards_and_undo_is_linear() {
        let fx = openai_script();
        let llm = Llm { wire: Wire::OpenAi, base_url: format!("{}/v1", fx.base), model: "m".into(), key: Secret::new("test-key-1234567") };
        let eng = FakeEngine::default();
        let mut chat = Chat::default();
        let mut evs = vec![];
        let reply = chat.send(&llm, &eng, "captions bigger, cut the pause at 0:12", &mut |e| evs.push(e)).unwrap();
        assert!(reply.contains("captions bigger"));
        assert_eq!(chat.cards.len(), 2, "describe changed nothing; two real changes");
        assert_eq!((chat.cards[0].depth, chat.cards[1].depth), (1, 2));
        assert_eq!(chat.cards[0].summary, "text · set_caption_style · size=72");
        assert!(evs.iter().any(|e| matches!(e, Event::ToolStart { tool, .. } if tool == "text")));
        // undo the FIRST card: the engine's history is linear, so both changes go, in one undo call of 2 steps
        let (steps, ids) = chat.undo(&eng, 1).unwrap();
        assert_eq!((steps, ids), (2, vec![1, 2]));
        assert!(eng.undo.lock().unwrap().is_empty());
        assert!(chat.undo(&eng, 2).is_err(), "already undone");
        assert_eq!(eng.calls.lock().unwrap().last().unwrap(), "project.undo");
    }

    #[test]
    fn anthropic_wire_shape() {
        let turn = Mutex::new(0);
        let fx = serve(move |r| {
            assert_eq!(r.path, "/v1/messages");
            assert_eq!(r.header("x-api-key"), Some("ant-key-1234567"));
            assert_eq!(r.header("anthropic-version"), Some("2023-06-01"));
            let body: Value = serde_json::from_str(&r.body).unwrap();
            assert!(body["system"].as_str().unwrap().contains("kimo"));
            assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
            let mut t = turn.lock().unwrap(); *t += 1;
            if *t == 1 {
                Resp::json(200, &json!({"content":[{"type":"text","text":"On it."},{"type":"tool_use","id":"tu1","name":"text","input":{"action":"bigger"}}],"stop_reason":"tool_use"}))
            } else {
                let last = body["messages"].as_array().unwrap().last().unwrap().clone();
                assert_eq!(last["content"][0]["type"], "tool_result");
                assert_eq!(last["content"][0]["tool_use_id"], "tu1");
                Resp::json(200, &json!({"content":[{"type":"text","text":"Done."}],"stop_reason":"end_turn"}))
            }
        });
        let llm = Llm { wire: Wire::Anthropic, base_url: fx.base.clone(), model: "m".into(), key: Secret::new("ant-key-1234567") };
        let eng = FakeEngine::default();
        let mut chat = Chat::default();
        assert_eq!(chat.send(&llm, &eng, "bigger", &mut |_| {}).unwrap(), "Done.");
        assert_eq!(chat.cards.len(), 1);
    }

    #[test]
    fn provider_error_is_reported_and_message_not_duplicated() {
        let fx = serve(|_| Resp::json(401, &json!({"error":{"message":"bad key"}})));
        let llm = Llm { wire: Wire::OpenAi, base_url: fx.base.clone(), model: "m".into(), key: Secret::new("x-123456789") };
        let mut chat = Chat::default();
        let mut errs = vec![];
        assert!(chat.send(&llm, &FakeEngine::default(), "hi", &mut |e| if let Event::Error { message } = e { errs.push(message) }).is_err());
        assert!(errs[0].contains("refused the key (401)"));
        assert!(chat.items.is_empty());
    }

    #[test]
    fn summaries() {
        assert_eq!(summarize("timeline", &json!({"action":"split_clip","args":{"clip_id":"c1","at":12.5}})), "timeline · split_clip · at=12.5, clip_id=c1");
        assert_eq!(summarize("search_tools", &json!({})), "search_tools");
    }
}
