//! Spectator demo mode against the real kimo-mcp binary (preview.rs): every tools/call gets its own canvas frame,
//! tagged with that call (`demo`, `seq`, `call_id`, `rpc_id`, `tool`), delivered after the call's own events and
//! before the next call starts, at least the pace apart; on by `KIMO_SPECTATOR_DEMO` or the command line, off by
//! default. Headless (the default) keeps the cadence coalescing: the same burst renders at most a couple of frames
//! (the steady-stream and burst contracts themselves are in live_preview.rs).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

struct Mcp { ch: Child, stdin: Option<ChildStdin>, rx: Receiver<String>, id: u64 }

impl Mcp {
    fn spawn(envs: &[(&str, &str)], args: &[&str]) -> Mcp {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .env("KIMO_SPECTATOR", "off").env_remove("KIMO_TOOLS").env_remove("KIMO_SPECTATOR_FRAMES").env_remove("KIMO_SPECTATOR_FRAME_PX").env_remove("KIMO_SPECTATOR_DEMO");
        for (k, v) in envs { cmd.env(k, v); }
        let mut ch = cmd.spawn().expect("spawn kimo-mcp");
        let stdin = ch.stdin.take();
        let out = ch.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || { for l in BufReader::new(out).lines().map_while(Result::ok) { if tx.send(l).is_err() { break; } } });
        let mut m = Mcp { ch, stdin, rx, id: 0 };
        m.send_raw("initialize", json!({}));
        m.recv();
        m
    }
    fn send_raw(&mut self, method: &str, params: Value) -> u64 {
        self.id += 1;
        writeln!(self.stdin.as_mut().unwrap(), "{}", json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})).unwrap();
        self.id
    }
    fn recv(&mut self) -> Value { serde_json::from_str(&self.rx.recv_timeout(Duration::from_secs(120)).expect("a response within 120 s")).unwrap() }
    /// Write every call at once (pipelined), then read every response. Returns (rpc id, tool) per call.
    fn burst(&mut self, calls: &[(&str, Value)]) -> Vec<(u64, String)> {
        let sent: Vec<(u64, String)> = calls.iter().map(|(t, a)| (self.send_raw("tools/call", json!({"name": t, "arguments": a})), t.to_string())).collect();
        for _ in &sent { self.recv(); }
        sent
    }
    fn finish(mut self) {
        drop(self.stdin.take());
        let t0 = Instant::now();
        while self.ch.try_wait().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(60)); std::thread::sleep(Duration::from_millis(20)); }
    }
    fn threads(&self) -> Vec<String> {
        std::fs::read_dir(format!("/proc/{}/task", self.ch.id())).map(|d| d.filter_map(|e| std::fs::read_to_string(e.ok()?.path().join("comm")).ok()).map(|s| s.trim().to_string()).collect()).unwrap_or_default()
    }
}
impl Drop for Mcp { fn drop(&mut self) { let _ = self.ch.kill(); let _ = self.ch.wait(); } }

fn tmp(name: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("kimo-demo-{}-{name}", std::process::id())) }
fn events(p: &std::path::Path) -> Vec<Value> { std::fs::read_to_string(p).unwrap_or_default().lines().map(|l| serde_json::from_str(l).expect("JSON line")).collect() }

/// Setup plus a mix of every kind of call: clip edits, inspector, slider, audio, a timeline cut, reads, a failing call.
fn calls() -> Vec<(&'static str, Value)> {
    let mut v: Vec<(&str, Value)> = vec![
        ("project_set", json!({"width":640,"height":360,"fps":24})),
        ("add_track", json!({})),
        ("add_clip", json!({"track_id":"track1","source_type":"solid","color":[0.9,0.2,0.1],"start":0,"duration":8})),
        ("add_effect", json!({"clip_id":"clip2","kind":"vignette"})),
    ];
    for k in 0..26u64 {
        v.push(match k % 7 {
            0 => ("set_property", json!({"clip_id":"clip2","property":"x","value": 20 * (k + 1)})),
            1 => ("set_property", json!({"clip_id":"clip2","property":"opacity","value": 0.3 + (k % 5) as f64 / 10.0})),
            2 => ("set_audio", json!({"clip_id":"clip2","volume": (k % 4) as f64 / 2.0})),
            3 => ("get_project", json!({})),
            4 => ("set_property", json!({"clip_id":"clip404","property":"x","value":1})),
            5 => ("split_clip", json!({"clip_id":"clip2","at": 4.0 + k as f64 / 10.0})),
            _ => ("set_property", json!({"clip_id":"clip2","property":"y","value": 10 * k})),
        });
    }
    v
}

/// The demo contract on one event stream: one frame per call, in call order, each tied to its call, each after the
/// call's terminal event and before the next call's start, at least `pace` ms apart, and no coalesced extras.
fn check_demo(sent: &[(u64, String)], evs: &[Value], pace: u64) -> Vec<Value> {
    let starts: Vec<(usize, &Value)> = evs.iter().enumerate().filter(|(_, e)| e["phase"] == "start").collect();
    let terms: Vec<(usize, &Value)> = evs.iter().enumerate().filter(|(_, e)| e["phase"] == "complete" || e["phase"] == "error").collect();
    let frames: Vec<(usize, &Value)> = evs.iter().enumerate().filter(|(_, e)| e["demo"] == true).collect();
    assert_eq!(starts.len(), sent.len());
    assert_eq!(terms.len(), sent.len());
    assert_eq!(frames.len(), sent.len(), "{} calls -> {} demo frames", sent.len(), frames.len());
    assert_eq!(evs.iter().filter(|e| e["auto"] == true && e["demo"] != true).count(), 0, "demo mode adds no coalesced frames");
    for (k, (rpc, tool)) in sent.iter().enumerate() {
        let ((si, s), (ti, t), (fi, f)) = (starts[k], terms[k], frames[k]);
        assert_eq!(s["rpc_id"], json!(rpc));
        assert_eq!(t["call_id"], s["call_id"]);
        assert_eq!(f["type"], "frame_rendered", "call {rpc} ({tool}) got a real frame: {}", f["reason"]);
        assert_eq!(f["call_id"], s["call_id"], "frame {k} is call {rpc}'s");
        assert_eq!(f["rpc_id"], json!(rpc));
        assert_eq!(f["tool"], json!(tool));
        assert_eq!(f["seq"], json!(k + 1));
        assert_eq!(f["pace_ms"], json!(pace));
        assert!(f["png_b64"].as_str().map_or(false, |b| b.starts_with("iVBORw0KGgo")), "frame {k} carries a PNG");
        assert!(si < ti && ti < fi, "call {rpc}: start, terminal, then its frame");
        if let Some((ni, _)) = starts.get(k + 1) { assert!(fi < *ni, "call {rpc}'s frame lands before the next call starts"); }
    }
    for w in frames.windows(2) {
        let gap = w[1].1["ts_ms"].as_u64().unwrap() - w[0].1["ts_ms"].as_u64().unwrap();
        assert!(gap + 1 >= pace, "frames at least the pace apart: {gap} ms < {pace} ms");
    }
    frames.into_iter().map(|(_, f)| f.clone()).collect()
}

#[test]
fn demo_mode_gives_every_call_its_own_frame() {
    let p = tmp("demo.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_DEMO", "50")], &[]);
    let calls = calls();
    let sent = m.burst(&calls);
    m.finish();
    let frames = check_demo(&sent, &events(&p), 50);
    assert_eq!(frames.len(), 30);
    // the frame shows the state that call left: a move changes the picture, a read or a failed call re-sends it
    for (k, (tool, args)) in calls.iter().enumerate().skip(1) {
        let (a, b) = (&frames[k - 1], &frames[k]);
        let changes = *tool == "set_property" && args["clip_id"] == "clip2" && ["x", "y"].contains(&args["property"].as_str().unwrap());
        if changes { assert_ne!(a["png_b64"], b["png_b64"], "call {k} ({tool} {args}) changed the picture"); assert!(b.get("same").is_none()); }
        if *tool == "get_project" || args["clip_id"] == "clip404" { assert_eq!(b["same"], true, "call {k} ({tool}) left the picture as it was"); assert_eq!(a["png_b64"], b["png_b64"]); }
    }
    let _ = std::fs::remove_file(&p);
}

#[test]
fn demo_mode_over_the_gui_transport() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let rx = std::thread::spawn(move || { let (mut s, _) = l.accept().unwrap(); let mut b = String::new(); s.read_to_string(&mut b).unwrap(); b });
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{port}")), ("KIMO_SPECTATOR_DEMO", "on")], &[]);
    let sent = m.burst(&calls());
    m.finish();
    let evs: Vec<Value> = rx.join().unwrap().lines().map(|l| serde_json::from_str(l).expect("whole JSON lines")).collect();
    check_demo(&sent, &evs, 75);
}

#[test]
fn a_stalled_spectator_loses_no_demo_frame() {
    // the app stops reading for 2 s once it has 40 KB (a busy webview): every call's frame still arrives, in order,
    // tied to its call, and no line is cut or dropped
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let rx = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let (mut buf, mut b, mut stalled) = (Vec::new(), [0u8; 65536], false);
        loop {
            let n = match s.read(&mut b) { Ok(0) | Err(_) => break, Ok(n) => n };
            buf.extend_from_slice(&b[..n]);
            if !stalled && buf.len() > 40_000 { stalled = true; std::thread::sleep(Duration::from_secs(2)); }
        }
        String::from_utf8(buf).unwrap()
    });
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{port}")), ("KIMO_SPECTATOR_DEMO", "50")], &[]);
    let sent = m.burst(&calls());
    m.finish();
    let evs: Vec<Value> = rx.join().unwrap().lines().map(|l| serde_json::from_str(l).expect("whole JSON lines")).collect();
    assert_eq!(check_demo(&sent, &evs, 50).len(), sent.len());
}

#[test]
fn demo_mode_from_the_command_line() {
    for (args, env, pace) in [(&["--spectator-demo"][..], None, 75), (&["--demo-pace-ms", "60"][..], Some("90"), 60), (&["--demo-pace-ms=100"][..], None, 100), (&["--demo-pace-ms", "10"][..], None, 50)] {
        let p = tmp(&format!("cli{pace}.jsonl")); let _ = std::fs::remove_file(&p);
        let spec = format!("file:{}", p.display());
        let mut envs = vec![("KIMO_SPECTATOR", spec.as_str())];
        if let Some(e) = env { envs.push(("KIMO_SPECTATOR_DEMO", e)); }
        let mut m = Mcp::spawn(&envs, args);
        let calls = calls()[..8].to_vec();
        let sent = m.burst(&calls);
        m.finish();
        check_demo(&sent, &events(&p), pace);
        let _ = std::fs::remove_file(&p);
    }
}

#[test]
fn headless_by_default_and_demo_costs_nothing_without_a_spectator() {
    // the same burst headless: no demo frames, the cadence coalescing renders at most a couple of frames
    let p = tmp("headless.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display()))], &[]);
    let sent = m.burst(&calls());
    std::thread::sleep(Duration::from_millis(700));
    m.finish();
    let evs = events(&p);
    assert_eq!(evs.iter().filter(|e| e["phase"] == "start").count(), sent.len());
    assert_eq!(evs.iter().filter(|e| e.get("demo").is_some()).count(), 0, "headless has no demo frames");
    let auto = evs.iter().filter(|e| e["auto"] == true).count();
    assert!(auto >= 1 && auto <= 2, "a headless burst coalesces: {auto} frames for {} calls", sent.len());
    let _ = std::fs::remove_file(&p);

    // demo asked for, but telemetry off or nobody listening: no pacing, no renderer
    let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    for envs in [vec![("KIMO_SPECTATOR_DEMO", "on")], vec![("KIMO_SPECTATOR_DEMO", "100"), ("KIMO_SPECTATOR", "off")]] {
        let mut m = Mcp::spawn(&envs, &[]);
        let many: Vec<(&str, Value)> = (0..200).map(|k| ("project_set", json!({"width": 320 + 2 * k, "height": 180}))).collect();
        let t0 = Instant::now();
        m.burst(&many);
        assert!(t0.elapsed() < Duration::from_secs(10), "no spectator, no pacing: {:?}", t0.elapsed());
        if cfg!(target_os = "linux") { assert!(!m.threads().iter().any(|t| t == "kimo-preview"), "{:?}", m.threads()); }
        m.finish();
    }
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR_DEMO", "on"), ("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{dead}"))], &[]);
    let many: Vec<(&str, Value)> = (0..200).map(|k| ("project_set", json!({"width": 320 + 2 * k, "height": 180}))).collect();
    let t0 = Instant::now();
    m.burst(&many);
    assert!(t0.elapsed() < Duration::from_secs(10), "dead listener, no pacing: {:?}", t0.elapsed());
    if cfg!(target_os = "linux") { assert!(!m.threads().iter().any(|t| t == "kimo-preview"), "{:?}", m.threads()); }
    m.finish();
}
