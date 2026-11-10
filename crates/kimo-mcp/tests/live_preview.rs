//! Live preview frames, end to end against the real kimo-mcp binary: with a spectator connected, every call that
//! changes the project is followed by a `frame_rendered` (auto) at the playhead, without any `render_frame`;
//! read-only calls render nothing; a burst renders once after the burst; nothing renders (and no preview thread
//! exists) with telemetry off, with nobody listening, or with KIMO_SPECTATOR_FRAMES=0; a spectator that connects
//! late gets the current frame at once.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

struct Mcp { ch: Child, stdin: ChildStdin, out: BufReader<ChildStdout>, id: u64 }

impl Mcp {
    fn spawn(envs: &[(&str, &str)]) -> Mcp {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .env("KIMO_SPECTATOR", "off").env_remove("KIMO_TOOLS").env_remove("KIMO_SPECTATOR_FRAMES").env_remove("KIMO_SPECTATOR_FRAME_PX");
        for (k, v) in envs { cmd.env(k, v); }
        let mut ch = cmd.spawn().expect("spawn kimo-mcp");
        let stdin = ch.stdin.take().unwrap();
        let out = BufReader::new(ch.stdout.take().unwrap());
        let mut m = Mcp { ch, stdin, out, id: 0 };
        m.rpc("initialize", json!({}));
        m
    }
    fn send(&mut self, method: &str, params: Value) {
        self.id += 1;
        writeln!(self.stdin, "{}", json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})).unwrap();
        self.stdin.flush().unwrap();
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.out.read_line(&mut l).unwrap();
        serde_json::from_str(&l).expect("one JSON-RPC response per line")
    }
    fn rpc(&mut self, method: &str, params: Value) -> Value { self.send(method, params); self.recv() }
    fn call(&mut self, name: &str, args: Value) -> Value {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        assert_eq!(r["result"]["isError"], false, "{name} failed: {r}");
        r
    }
    fn threads(&self) -> Vec<String> { thread_names(self.ch.id()) }
}

impl Drop for Mcp { fn drop(&mut self) { let _ = self.ch.kill(); let _ = self.ch.wait(); } }

#[cfg(target_os = "linux")]
fn thread_names(pid: u32) -> Vec<String> {
    std::fs::read_dir(format!("/proc/{pid}/task")).map(|d| d.filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok()).map(|s| s.trim().to_string()).collect()).unwrap_or_default()
}
#[cfg(not(target_os = "linux"))]
fn thread_names(_pid: u32) -> Vec<String> { vec![] }

fn tmp(name: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("kimo-live-{}-{name}", std::process::id())) }

/// Complete lines of the sink file (a line still being appended is skipped).
fn events(p: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(p).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}
fn auto_frames(evs: &[Value]) -> Vec<Value> { evs.iter().filter(|e| e["type"] == "frame_rendered" && e["auto"] == true).cloned().collect() }

fn wait_for<T>(secs: f64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    loop { if let Some(v) = f() { return Some(v); } if Instant::now() > end { return None; } std::thread::sleep(Duration::from_millis(20)); }
}

/// call_id of the newest complete event for `tool`.
fn last_call_id(p: &std::path::Path, tool: &str) -> String {
    wait_for(10.0, || events(p).iter().rev().find(|e| e["tool_name"] == tool && e["phase"] == "complete").and_then(|e| e["call_id"].as_str().map(String::from)))
        .unwrap_or_else(|| panic!("no complete event for {tool}"))
}

fn frame_for(p: &std::path::Path, call_id: &str) -> Value {
    wait_for(30.0, || auto_frames(&events(p)).into_iter().find(|f| f["call_id"] == call_id))
        .unwrap_or_else(|| panic!("no live frame after {call_id}: {:#?}", events(p).iter().filter(|e| e.get("type").is_some()).map(|e| e["type"].clone()).collect::<Vec<_>>()))
}

fn check_png(f: &Value, max_w: u64) {
    assert!(f["png_b64"].as_str().map_or(false, |s| s.starts_with("iVBORw0KGgo")), "base64 PNG: {}", f["png_b64"].as_str().map(|s| &s[..s.len().min(40)]).unwrap_or("none"));
    let (w, h) = (f["w"].as_u64().unwrap(), f["h"].as_u64().unwrap());
    assert!(w > 0 && w <= max_w && h > 0, "preview size {w}x{h}");
    assert!(f["time"].as_f64().is_some() && f["tc"].as_str().is_some() && f["frame"].as_i64().is_some());
    // an independent decoder reads the in-process PNG at the stated size
    let path = tmp(&format!("probe-{}.png", f["gen"]));
    std::fs::write(&path, b64_decode(f["png_b64"].as_str().unwrap())).unwrap();
    let o = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=width,height,codec_name", "-of", "csv=p=0"]).arg(&path).output().expect("ffprobe");
    let _ = std::fs::remove_file(&path);
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), format!("png,{w},{h}"), "ffprobe: {}", String::from_utf8_lossy(&o.stderr));
}

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 { match c { b'A'..=b'Z' => (c - b'A') as u32, b'a'..=b'z' => (c - b'a' + 26) as u32, b'0'..=b'9' => (c - b'0' + 52) as u32, b'+' => 62, b'/' => 63, _ => 0 } };
    let mut out = vec![];
    for q in s.as_bytes().chunks(4) {
        let n = q.iter().fold(0u32, |a, &c| a << 6 | val(c));
        let pad = q.iter().filter(|&&c| c == b'=').count();
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    out
}

const SETUP: &[(&str, &str)] = &[
    ("project_set", r#"{"width":1280,"height":720,"fps":24}"#),
    ("add_track", r#"{}"#),
    ("add_clip", r#"{"track_id":"track1","source_type":"solid","color":[0.9,0.2,0.1],"start":0,"duration":4}"#),
];

#[test]
fn live_frame_after_every_mutating_call_and_none_for_reads() {
    let p = tmp("live.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display()))]);
    let steps: Vec<(&str, Value)> = vec![
        ("project_set", json!({"width":1280,"height":720,"fps":24})),
        ("add_track", json!({})),
        ("add_clip", json!({"track_id":"track1","source_type":"solid","color":[0.9,0.2,0.1],"start":0,"duration":4})),
        ("set_property", json!({"clip_id":"clip2","property":"x","value":200})),
        ("add_effect", json!({"clip_id":"clip2","kind":"vignette"})),
        ("split_clip", json!({"clip_id":"clip2","at":1.5})),
    ];
    let reads: Vec<(&str, Value)> = vec![("get_project", json!({})), ("list_effects", json!({})), ("get_transform", json!({"clip_id":"clip2"}))];
    let mut frames = vec![];
    for (i, (tool, args)) in steps.iter().enumerate() {
        m.call(tool, args.clone());
        let id = last_call_id(&p, tool);
        let f = frame_for(&p, &id);
        check_png(&f, 640);
        assert_eq!((f["src_w"].as_u64(), f["src_h"].as_u64()), (Some(1280), Some(720)), "{tool}: {f}");
        frames.push(f);
        // reads after the frame landed: no more frames
        if i >= 2 {
            let n = auto_frames(&events(&p)).len();
            for (r, a) in &reads { m.call(r, a.clone()); }
            m.rpc("tools/list", json!({}));
            std::thread::sleep(Duration::from_millis(700));
            assert_eq!(auto_frames(&events(&p)).len(), n, "read-only calls after {tool} rendered a frame");
        }
    }
    assert_eq!(frames.len(), steps.len());
    assert!(frames[1]["png_b64"] != frames[2]["png_b64"], "add_clip changed the picture");
    assert!(frames[2]["png_b64"] != frames[3]["png_b64"], "set_property x changed the picture");
    assert!(frames[3]["png_b64"] != frames[4]["png_b64"], "the vignette changed the picture");
    assert_eq!(frames[5]["time"], 1.5, "split moves the playhead to the cut");
    assert_eq!(frames[5]["frame"], 36);
    // the agent's own render_frame is reported as before and is not followed by an extra live frame
    let n = auto_frames(&events(&p)).len();
    let png = tmp("agent.png");
    m.call("render_frame", json!({"time": 1.5, "path": png.to_str().unwrap(), "inline": "never"}));
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(auto_frames(&events(&p)).len(), n);
    assert!(events(&p).iter().any(|e| e["type"] == "frame_rendered" && e.get("auto").is_none()), "render_frame still streams its own frame");
    drop(m);
    let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&png);
}

#[test]
fn burst_renders_once_after_the_burst() {
    let p = tmp("burst.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAME_PX", "320")]);
    for (t, a) in SETUP { m.call(t, serde_json::from_str(a).unwrap()); }
    let id = last_call_id(&p, "add_clip");
    check_png(&frame_for(&p, &id), 320);
    let before = auto_frames(&events(&p)).len();
    // 12 calls written at once (pipelined), answered back to back
    let n = 12;
    for k in 0..n { m.send("tools/call", json!({"name":"set_property","arguments":{"clip_id":"clip2","property":"x","value":10 * (k + 1)}})); }
    for _ in 0..n { let r = m.recv(); assert_eq!(r["result"]["isError"], false); }
    // the sink is written by a background thread, so the last responses can arrive before their events: wait for them
    // (like last_call_id) instead of reading the file once
    let completes = || -> Vec<String> { events(&p).iter().filter(|e| e["tool_name"] == "set_property" && e["phase"] == "complete").filter_map(|e| e["call_id"].as_str().map(String::from)).collect() };
    let ids: Vec<String> = wait_for(10.0, || { let ids = completes(); (ids.len() >= n as usize).then_some(ids) }).unwrap_or_else(completes);
    assert_eq!(ids.len(), n as usize);
    let last = ids.last().unwrap().clone();
    let f = frame_for(&p, &last);
    std::thread::sleep(Duration::from_millis(700));
    let burst: Vec<Value> = auto_frames(&events(&p))[before..].to_vec();
    assert!(!burst.is_empty() && burst.len() <= 2, "a burst of {n} calls renders once after it (at most one more mid-burst): {} frames", burst.len());
    assert_eq!(burst.last().unwrap()["call_id"], json!(last), "latest wins");
    assert_eq!(f["w"], 320, "KIMO_SPECTATOR_FRAME_PX caps the preview width");
    drop(m);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn steady_stream_of_calls_still_updates_the_viewer() {
    // one call every 50 ms for 3 s: each call cancels a frame in progress, but a frame pending for 400 ms renders
    // to the end, so the viewer keeps updating instead of waiting for the stream to stop
    let p = tmp("steady.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display()))]);
    for (t, a) in SETUP { m.call(t, serde_json::from_str(a).unwrap()); }
    let id = last_call_id(&p, "add_clip");
    frame_for(&p, &id);
    let before = auto_frames(&events(&p)).len();
    let t0 = Instant::now();
    let mut k = 0;
    while t0.elapsed() < Duration::from_secs(3) { k += 1; m.call("set_property", json!({"clip_id":"clip2","property":"x","value":k})); std::thread::sleep(Duration::from_millis(50)); }
    let during = auto_frames(&events(&p)).len() - before;
    assert!(during >= 2, "{k} calls over 3 s produced {during} frames during the stream");
    assert!(during < k / 2, "frames coalesce: {during} frames for {k} calls");
    drop(m);
    let _ = std::fs::remove_file(&p);
}

fn mutate(m: &mut Mcp) { for (t, a) in SETUP { m.call(t, serde_json::from_str(a).unwrap()); } m.call("set_property", json!({"clip_id":"clip2","property":"x","value":5})); }

#[test]
fn no_preview_work_when_off_unconnected_or_disabled() {
    // telemetry off: no preview thread, no spectator thread
    let mut off = Mcp::spawn(&[]);
    mutate(&mut off);
    std::thread::sleep(Duration::from_millis(300));
    let th = off.threads();
    if cfg!(target_os = "linux") { assert!(!th.is_empty()); }
    assert!(!th.iter().any(|t| t == "kimo-preview" || t == "kimo-spectator"), "telemetry off starts nothing: {th:?}");

    // configured but nobody listening: events are dropped and nothing renders
    let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let mut un = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{dead}"))]);
    mutate(&mut un);
    std::thread::sleep(Duration::from_millis(300));
    let th = un.threads();
    assert!(!th.iter().any(|t| t == "kimo-preview"), "no listener, no renderer: {th:?}");

    // a connected sink with live frames turned off: tool events, zero frames
    let p = tmp("noframes.jsonl"); let _ = std::fs::remove_file(&p);
    let mut nf = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAMES", "0")]);
    mutate(&mut nf);
    std::thread::sleep(Duration::from_millis(700));
    let evs = events(&p);
    assert!(evs.iter().any(|e| e["phase"] == "complete"));
    assert_eq!(evs.iter().filter(|e| e["type"] == "frame_rendered").count(), 0);
    assert!(!nf.threads().iter().any(|t| t == "kimo-preview"));

    // and the control: a connected sink renders, on its own thread
    let q = tmp("control.jsonl"); let _ = std::fs::remove_file(&q);
    let mut on = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", q.display()))]);
    mutate(&mut on);
    let id = last_call_id(&q, "set_property");
    frame_for(&q, &id);
    if cfg!(target_os = "linux") { assert!(on.threads().iter().any(|t| t == "kimo-preview"), "{:?}", on.threads()); }
    drop((off, un, nf, on));
    let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&q);
}

#[test]
fn spectator_opened_late_gets_the_current_frame() {
    let port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{port}"))]);
    mutate(&mut m); // nobody listening yet: nothing renders
    let l = std::net::TcpListener::bind(("127.0.0.1", port)).expect("rebind the port");
    let rx = std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
        for line in BufReader::new(s).lines().map_while(Result::ok) {
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["type"] == "frame_rendered" { return Some(v); }
        }
        None
    });
    // read-only calls only: the next event makes the writer reconnect (after its backoff), which sends the frame
    let t0 = Instant::now();
    while !rx.is_finished() && t0.elapsed() < Duration::from_secs(20) { m.call("get_project", json!({})); std::thread::sleep(Duration::from_millis(150)); }
    let f = rx.join().unwrap().expect("a live frame once the spectator connected");
    assert_eq!(f["auto"], true);
    check_png(&f, 640);
    assert_eq!(f["gen"], 4, "the frame shows the state after all four mutating calls");
}
