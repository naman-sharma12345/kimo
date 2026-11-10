//! State telemetry is strictly one-to-one with tools/call, end to end against the real kimo-mcp binary: every call,
//! counted by its JSON-RPC id (`rpc_id`) and its engine `call_id`, gets exactly one `start` and exactly one terminal
//! event (`complete` or `error`), in call order, with nothing lost and nothing coalesced. Covered: timeline cuts
//! (split_clip), clip edits (add_clip, trim_clip, move_clip, set_speed), the property inspector (set_property x),
//! sliders (set_property opacity, set_effect_param), audio controls (set_audio volume/mute), reads and failing calls;
//! under a pipelined burst over a file sink, over the GUI's TCP transport, and with a spectator that stops reading for
//! 3 s in the middle of a 600-call and of an 8,000-call burst (a busy webview): before the state lane became lossless
//! (spectator.rs) the 8,000-call case lost about a third of the calls' events and cut a line in half.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Mcp { ch: Child, stdin: Option<ChildStdin>, rx: Receiver<String>, id: u64 }

impl Mcp {
    fn spawn(spec: &str) -> Mcp {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .env_remove("KIMO_TOOLS").env_remove("KIMO_SPECTATOR_FRAMES").env_remove("KIMO_SPECTATOR_DEMO").env("KIMO_SPECTATOR", spec);
        let mut ch = cmd.spawn().expect("spawn kimo-mcp");
        let stdin = ch.stdin.take();
        let out = ch.stdout.take().unwrap();
        // responses are read on their own thread, so a burst written in one go can never fill the pipes both ways
        let (tx, rx) = channel();
        std::thread::spawn(move || { for l in BufReader::new(out).lines().map_while(Result::ok) { if tx.send(l).is_err() { break; } } });
        let mut m = Mcp { ch, stdin, rx, id: 0 };
        m.send_raw("initialize", json!({}));
        m.recv();
        m
    }
    fn send_raw(&mut self, method: &str, params: Value) -> u64 {
        self.id += 1;
        let w = self.stdin.as_mut().unwrap();
        writeln!(w, "{}", json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})).unwrap();
        self.id
    }
    fn send(&mut self, tool: &str, args: Value) -> u64 { self.send_raw("tools/call", json!({"name": tool, "arguments": args})) }
    fn recv(&mut self) -> Value {
        let l = self.rx.recv_timeout(Duration::from_secs(120)).expect("a response within 120 s");
        serde_json::from_str(&l).expect("one JSON-RPC response per line")
    }
    /// Close stdin and wait for the server to exit (it drains the telemetry first).
    fn finish(mut self) {
        drop(self.stdin.take());
        let t0 = Instant::now();
        loop {
            if let Some(s) = self.ch.try_wait().unwrap() { assert!(s.success(), "kimo-mcp exited {s:?}"); return; }
            assert!(t0.elapsed() < Duration::from_secs(60), "kimo-mcp did not exit after stdin closed");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Mcp { fn drop(&mut self) { let _ = self.ch.kill(); let _ = self.ch.wait(); } }

const SETUP: &[(&str, &str)] = &[
    ("project_set", r#"{"width":640,"height":360,"fps":24}"#),
    ("add_track", r#"{}"#),
    ("add_clip", r#"{"track_id":"track1","source_type":"solid","color":[0.9,0.2,0.1],"start":0,"duration":30}"#),
    ("add_effect", r#"{"clip_id":"clip2","kind":"vignette"}"#),
];

/// Call `k` of a burst: cycles through the spectator's panels; every 100th call is a timeline cut.
fn mixed(k: u64) -> (&'static str, Value) {
    if k % 100 == 50 { return ("split_clip", json!({"clip_id": "clip2", "at": 1.0 + (k / 100) as f64})); }
    match k % 10 {
        0 => ("set_property", json!({"clip_id":"clip2","property":"x","value": k})),
        1 => ("set_property", json!({"clip_id":"clip2","property":"opacity","value": (k % 100) as f64 / 100.0})),
        2 => ("set_effect_param", json!({"clip_id":"clip2","effect_id":"fx3","param":"amount","value": (k % 50) as f64 / 50.0})),
        3 => ("set_audio", json!({"clip_id":"clip2","volume": (k % 40) as f64 / 10.0, "mute": k % 20 == 3})),
        4 => ("trim_clip", json!({"clip_id":"clip2","duration": 20 + k % 7})),
        5 => ("move_clip", json!({"clip_id":"clip2","start": (k % 3) as f64 / 4.0})),
        6 => ("set_speed", json!({"clip_id":"clip2","speed": 1.0 + (k % 4) as f64 / 4.0})),
        7 => ("get_project", json!({})),
        8 => ("set_property", json!({"clip_id":"clip404","property":"x","value": 1})), // fails: not_found
        _ => ("add_clip", json!({"track_id":"track1","source_type":"solid","color":[0.1,0.4,0.9],"start": 31 + k,"duration":0.5})),
    }
}

/// Setup, then `n` calls written back to back without waiting (pipelined), then every response.
/// Returns (rpc id -> (tool, isError)) for every tools/call.
fn burst(m: &mut Mcp, n: u64, call: fn(u64) -> (&'static str, Value)) -> Vec<(u64, String, bool)> {
    let mut sent = vec![];
    for (t, a) in SETUP { let id = m.send(t, serde_json::from_str(a).unwrap()); sent.push((id, t.to_string())); }
    for k in 0..n { let (t, a) = call(k); let id = m.send(t, a); sent.push((id, t.to_string())); }
    let mut answered: HashMap<u64, bool> = HashMap::new();
    for _ in 0..sent.len() {
        let r = m.recv();
        let id = r["id"].as_u64().expect("response id");
        assert!(answered.insert(id, r["result"]["isError"] == true).is_none(), "one response per request: {id}");
    }
    sent.into_iter().map(|(id, t)| { let e = answered[&id]; (id, t, e) }).collect()
}

/// The 1:1 contract, counted by rpc_id and by call_id.
fn check_one_to_one(calls: &[(u64, String, bool)], evs: &[Value]) {
    let tool_evs: Vec<(usize, &Value)> = evs.iter().enumerate().filter(|(_, e)| e.get("tool_name").is_some()).collect();
    let mut starts: HashMap<u64, Vec<(usize, &Value)>> = HashMap::new();
    let mut terms: HashMap<u64, Vec<(usize, &Value)>> = HashMap::new();
    for (i, e) in &tool_evs {
        let rpc = e["rpc_id"].as_u64().unwrap_or_else(|| panic!("every tool event carries its call's rpc_id: {e}"));
        match e["phase"].as_str().unwrap() {
            "start" => starts.entry(rpc).or_default().push((*i, e)),
            "complete" | "error" => terms.entry(rpc).or_default().push((*i, e)),
            "progress" => {}
            p => panic!("unknown phase {p}"),
        }
    }
    assert_eq!(starts.len(), calls.len(), "a start for every call and for nothing else");
    assert_eq!(terms.len(), calls.len(), "a terminal event for every call and for nothing else");
    let mut call_ids = std::collections::HashSet::new();
    let mut last_start = 0usize;
    for (rpc, tool, is_err) in calls {
        let s = starts.get(rpc).unwrap_or_else(|| panic!("call {rpc} ({tool}) has no start event"));
        let t = terms.get(rpc).unwrap_or_else(|| panic!("call {rpc} ({tool}) has no terminal event"));
        assert_eq!(s.len(), 1, "call {rpc} ({tool}): exactly one start, got {}", s.len());
        assert_eq!(t.len(), 1, "call {rpc} ({tool}): exactly one terminal, got {}", t.len());
        let ((si, s), (ti, t)) = (s[0], t[0]);
        assert_eq!(s["tool_name"], json!(tool), "call {rpc}: start names its tool");
        assert_eq!(t["tool_name"], json!(tool), "call {rpc}: terminal names its tool");
        assert_eq!(s["call_id"], t["call_id"], "call {rpc}: start and terminal share one call_id");
        assert!(call_ids.insert(s["call_id"].as_str().unwrap().to_string()), "call_id {} used twice", s["call_id"]);
        assert!(si < ti, "call {rpc}: start before its terminal");
        assert!(si > last_start, "starts arrive in call order (call {rpc})");
        last_start = si;
        assert_eq!(t["phase"] == "error", *is_err, "call {rpc} ({tool}): error event iff the response is an error: {t}");
    }
    // project_changed: one per mutating call, never coalesced
    let mutated: std::collections::HashSet<&str> = tool_evs.iter().filter(|(_, e)| e["phase"] == "complete" && e["payload"]["mutated"] == true).map(|(_, e)| e["call_id"].as_str().unwrap()).collect();
    let pcs: Vec<&str> = evs.iter().filter(|e| e["type"] == "project_changed").map(|e| e["call_id"].as_str().unwrap()).collect();
    assert_eq!(pcs.len(), mutated.len(), "one project_changed per mutating call");
    assert!(pcs.iter().all(|c| mutated.contains(c)));
    let revs: Vec<u64> = evs.iter().filter(|e| e["type"] == "project_changed").map(|e| e["rev"].as_u64().unwrap()).collect();
    assert!(revs.windows(2).all(|w| w[1] == w[0] + 1), "project revisions are consecutive: nothing skipped");
}

fn tmp(name: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("kimo-1to1-{}-{name}", std::process::id())) }

fn events_of(text: &str) -> (Vec<Value>, usize) {
    let mut bad = 0;
    let evs = text.lines().filter(|l| !l.trim().is_empty()).filter_map(|l| serde_json::from_str(l).map_err(|_| bad += 1).ok()).collect();
    (evs, bad)
}

#[test]
fn every_call_one_start_one_terminal_under_a_burst_file_sink() {
    let p = tmp("burst.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&format!("file:{}", p.display()));
    let calls = burst(&mut m, 600, mixed);
    m.finish();
    let (evs, bad) = events_of(&std::fs::read_to_string(&p).unwrap());
    assert_eq!(bad, 0);
    assert_eq!(calls.len(), 604);
    check_one_to_one(&calls, &evs);
    // each panel's vocabulary is on its own call's event
    let done = |tool: &str| -> Vec<&Value> { evs.iter().filter(|e| e["tool_name"] == tool && e["phase"] == "complete").collect() };
    let cuts = done("split_clip");
    assert_eq!(cuts.len(), 6, "six timeline cuts");
    assert!(cuts.iter().all(|e| e["payload"]["changed"]["clips_added"].as_array().map_or(false, |a| a.len() == 1)), "each cut adds one clip");
    assert!(done("set_property").iter().any(|e| e["target"]["param"] == "x" && e["payload"]["from"].is_number() && e["payload"]["to"].is_number()), "inspector from/to");
    assert_eq!(done("set_audio").len(), 60, "every audio-control call reports");
    assert_eq!(evs.iter().filter(|e| e["tool_name"] == "set_property" && e["phase"] == "error").count(), 60, "every failing call reports its error");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn every_call_one_start_one_terminal_over_the_gui_transport() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let rx = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = String::new();
        s.read_to_string(&mut buf).unwrap();
        buf
    });
    let mut m = Mcp::spawn(&format!("tcp://127.0.0.1:{port}"));
    let calls = burst(&mut m, 3000, mixed);
    m.finish();
    let (evs, bad) = events_of(&rx.join().unwrap());
    assert_eq!(bad, 0, "no line cut in half");
    check_one_to_one(&calls, &evs);
}

/// A listener shaped like the GUI shell (crates/kimo-gui/src/main.rs): accept, read lines until EOF, accept again.
/// The first connection stops reading for `stall` once it has read `after` bytes, the way a busy webview does.
fn gui_listener(stall: Duration, after: usize) -> (u16, Arc<Mutex<Vec<Vec<u8>>>>, Arc<std::sync::atomic::AtomicBool>, std::thread::JoinHandle<Vec<std::thread::JoinHandle<()>>>) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    let conns: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (c2, s2) = (conns.clone(), stop.clone());
    let acc = std::thread::spawn(move || {
        let mut readers = vec![];
        while !s2.load(std::sync::atomic::Ordering::SeqCst) {
            match l.accept() {
                Ok((mut s, _)) => {
                    s.set_nonblocking(false).unwrap();
                    let idx = { let mut g = c2.lock().unwrap(); g.push(vec![]); g.len() - 1 };
                    let c3 = c2.clone();
                    readers.push(std::thread::spawn(move || {
                        let (mut got, mut stalled, mut b) = (0usize, idx != 0, [0u8; 65536]);
                        loop {
                            let n = match s.read(&mut b) { Ok(0) | Err(_) => break, Ok(n) => n };
                            c3.lock().unwrap()[idx].extend_from_slice(&b[..n]);
                            got += n;
                            if !stalled && got >= after { stalled = true; std::thread::sleep(stall); }
                        }
                    }));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        readers
    });
    (port, conns, stop, acc)
}

fn stalled(n: u64, after: usize) {
    let (port, conns, stop, acc) = gui_listener(Duration::from_secs(3), after);
    let mut m = Mcp::spawn(&format!("tcp://127.0.0.1:{port}"));
    let t0 = Instant::now();
    let calls = burst(&mut m, n, |k| if k % 50 == 25 { mixed(k) } else { ("set_property", json!({"clip_id":"clip2","property": if k % 2 == 0 { "x" } else { "y" },"value": k})) });
    let answered = t0.elapsed();
    m.finish();
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    for r in acc.join().unwrap() { r.join().unwrap(); }
    let conns = conns.lock().unwrap();
    assert_eq!(conns.len(), 1, "the slow spectator was waited for, never disconnected");
    let (evs, bad) = events_of(&String::from_utf8_lossy(&conns[0]));
    assert_eq!(bad, 0, "no line cut in half");
    check_one_to_one(&calls, &evs);
    assert!(answered < Duration::from_secs(60), "{answered:?}");
}

#[test]
fn a_stalled_spectator_loses_nothing_600_calls() { stalled(600, 2_000); }

#[test]
fn a_stalled_spectator_loses_nothing_8000_calls() {
    // measured before the fix (047bd68): ~35% of the calls' events lost, one line cut in half, a reconnect
    stalled(8000, 20_000);
}
