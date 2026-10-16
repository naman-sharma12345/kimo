//! Spectator telemetry, end to end against the real kimo-mcp binary:
//! start/complete/error with matching call_id over a file sink and over the GUI's TCP transport,
//! and MCP stdout byte-identical with telemetry off, on, and pointed at a dead listener.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn requests() -> Vec<Value> {
    vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"project_set","arguments":{"width":320,"height":180,"fps":24}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_track","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"add_text","arguments":{"track_id":"track1","text":"kimo","start":0,"duration":1}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"set_property","arguments":{"clip_id":"clip2","property":"x","value":40}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"set_property","arguments":{"clip_id":"clip404","property":"x","value":1}}}),
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_project","arguments":{}}}),
    ]
}

/// Run kimo-mcp over `reqs` with KIMO_SPECTATOR = `spec` (None = unset). Returns stdout bytes.
fn run(spec: Option<&str>, reqs: &[Value]) -> Vec<u8> {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).env_remove("KIMO_SPECTATOR").env_remove("KIMO_TOOLS");
    if let Some(s) = spec { cmd.env("KIMO_SPECTATOR", s); }
    let mut ch = cmd.spawn().expect("spawn kimo-mcp");
    // Drain stdout on its own thread while stdin is fed: with thousands of requests the responses overflow the OS pipe
    // buffer, and a harness that writes everything before reading anything deadlocks with the server (both block in
    // pipe_write). A write error below is therefore a real early exit of the server, not back-pressure.
    let mut stdout = ch.stdout.take().unwrap();
    let reader = std::thread::spawn(move || { let mut buf = Vec::new(); std::io::Read::read_to_end(&mut stdout, &mut buf).map(|_| buf) });
    {
        let mut stdin = ch.stdin.take().unwrap();
        for r in reqs { writeln!(stdin, "{r}").unwrap(); }
    }
    let stdout = reader.join().expect("stdout reader thread").expect("read kimo-mcp stdout");
    let status = ch.wait().unwrap();
    assert!(status.success(), "kimo-mcp exited {status:?}");
    stdout
}

fn events_of(text: &str) -> Vec<Value> { text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).expect("each telemetry line is JSON")).collect() }

fn tool_events<'a>(evs: &'a [Value], tool: &str) -> Vec<&'a Value> { evs.iter().filter(|e| e["tool_name"] == tool).collect() }

fn check_events(evs: &[Value]) {
    // every call: exactly one start then one terminal event with the same call_id
    let starts: Vec<&Value> = evs.iter().filter(|e| e["phase"] == "start").collect();
    assert_eq!(starts.len(), 7, "seven tools/call requests -> seven starts: {evs:#?}");
    for s in &starts {
        let id = s["call_id"].as_str().unwrap();
        let ends: Vec<&Value> = evs.iter().filter(|e| e["call_id"] == id && (e["phase"] == "complete" || e["phase"] == "error")).collect();
        assert_eq!(ends.len(), 1, "one terminal event for {id}");
        let e = ends[0];
        assert!(e["duration_ms"].as_f64().unwrap() >= 0.0);
        assert_eq!(e["payload"]["dur_ms"], e["duration_ms"]);
        assert!(e["ts_ms"].as_u64().unwrap() >= s["ts_ms"].as_u64().unwrap());
        assert!(e["ts"].as_f64().unwrap() >= s["ts"].as_f64().unwrap());
        // the GUI router treats any event with `type` as legacy, so tool events must not carry one
        assert!(s.get("type").is_none() && e.get("type").is_none());
        assert!(s["target"].is_object() && s["payload"]["args"].is_object());
    }
    assert_eq!(evs[0]["type"], "session", "a session reset leads the stream");
    // project size in pixels on the session event (the fresh project) and on every project_changed (after the call)
    assert_eq!((evs[0]["width"].as_u64(), evs[0]["height"].as_u64()), (Some(1280), Some(720)), "session carries the starting project size: {}", evs[0]);
    let pcs: Vec<&Value> = evs.iter().filter(|e| e["type"] == "project_changed").collect();
    assert!(pcs.len() >= 3, "project_set, add_track, add_text and set_property change the project: {pcs:#?}");
    for pc in &pcs {
        assert_eq!((pc["width"].as_u64(), pc["height"].as_u64()), (Some(320), Some(180)), "project_changed carries the size after the call: {pc}");
    }
    let set_id = starts.iter().find(|s| s["tool_name"] == "project_set").unwrap()["call_id"].clone();
    assert!(pcs.iter().any(|pc| pc["call_id"] == set_id), "a size change alone is a project change");
    for e in evs { assert!(e.get("tool_name").is_some() != e.get("type").is_some(), "tool event xor engine event: {e}"); }
    let ids: std::collections::HashSet<&str> = starts.iter().map(|s| s["call_id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 7, "call ids are unique");

    let at = tool_events(evs, "add_track");
    assert_eq!(at[1]["phase"], "complete");
    assert_eq!(at[1]["payload"]["result"]["track_id"], "track1");
    assert_eq!(at[1]["payload"]["changed"]["tracks_added"], json!(["track1"]));

    let tx = tool_events(evs, "add_text");
    assert_eq!(tx[0]["target"]["track_id"], "track1");
    assert_eq!(tx[0]["payload"]["args"]["text"], "kimo");
    assert_eq!(tx[1]["payload"]["changed"]["clips_added"][0]["clip_id"], "clip2");
    // router vocabulary for an insert
    assert_eq!(tx[1]["payload"]["clip"], "clip2"); assert_eq!(tx[1]["payload"]["track"], "track1");
    assert_eq!(tx[1]["payload"]["s"], 0.0); assert_eq!(tx[1]["payload"]["e"], 1.0); assert_eq!(tx[1]["payload"]["label"], "kimo");

    let sp = tool_events(evs, "set_property");
    assert_eq!(sp[0]["target"], json!({"track_id":"track1","clip_id":"clip2","param":"x"}), "track resolved from the project");
    assert_eq!(sp[1]["phase"], "complete");
    assert_eq!(sp[1]["payload"]["mutated"], true);
    assert!(sp[1]["payload"]["changed"]["clip"].as_object().map_or(false, |o| !o.is_empty()), "clip field diff: {:#}", sp[1]);
    assert_eq!((sp[1]["payload"]["from"].as_f64(), sp[1]["payload"]["to"].as_f64()), (Some(0.0), Some(40.0)), "from/to for the inspector: {:#}", sp[1]);

    let bad = tool_events(evs, "no_such_tool");
    assert_eq!(bad[1]["phase"], "error");
    assert!(bad[1]["payload"]["error"].as_str().unwrap().contains("no_such_tool"));
    assert_eq!(bad[1]["payload"]["code"], "unknown_tool");
    assert_eq!(bad[1]["call_id"], bad[0]["call_id"]);

    assert_eq!(sp[2]["phase"], "start"); assert_eq!(sp[3]["phase"], "error");
    assert_eq!(sp[3]["payload"]["code"], "not_found");

    let gp = tool_events(evs, "get_project");
    assert_eq!(gp[1]["payload"]["mutated"], false);
    assert_eq!(evs.iter().filter(|e| e["type"] == "project_changed").count(), 4, "project_set, add_track, add_text, set_property mutate");
}

fn tmp(name: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("kimo-spectator-{}-{name}", std::process::id())) }

#[test]
fn file_sink_start_complete_error() {
    let p = tmp("file.jsonl"); let _ = std::fs::remove_file(&p);
    run(Some(&format!("file:{}", p.display())), &requests());
    check_events(&events_of(&std::fs::read_to_string(&p).unwrap()));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn tcp_sink_feeds_the_gui_transport() {
    // the GUI shell listens; the engine connects (crates/kimo-gui/src/main.rs)
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let rx = std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        BufReader::new(s).lines().map_while(Result::ok).collect::<Vec<_>>().join("\n")
    });
    run(Some(&format!("tcp://127.0.0.1:{port}")), &requests());
    check_events(&events_of(&rx.join().unwrap()));
}

#[test]
fn stdout_is_byte_identical_with_telemetry_off_on_or_unreachable() {
    let reqs = requests();
    let off = run(None, &reqs);
    let p = tmp("ident.jsonl");
    let on_file = run(Some(&format!("file:{}", p.display())), &reqs);
    let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() }; // closed again: nobody listening
    let t = Instant::now();
    let on_dead = run(Some(&format!("tcp://127.0.0.1:{dead}")), &reqs);
    let dead_secs = t.elapsed().as_secs_f64();
    assert_eq!(String::from_utf8_lossy(&off), String::from_utf8_lossy(&on_file));
    assert_eq!(off, on_file, "file sink changes nothing on stdout");
    assert_eq!(off, on_dead, "dead listener changes nothing on stdout");
    assert!(dead_secs < 10.0, "no listener must not stall the server ({dead_secs:.1}s)");
    assert_eq!(String::from_utf8_lossy(&off).lines().count(), 8, "one JSON-RPC response per request, nothing else");
    assert!(std::fs::metadata(&p).map(|m| m.len() > 0).unwrap_or(false), "the sink did receive events");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn effect_param_change_is_a_from_to() {
    let p = tmp("fx.jsonl"); let _ = std::fs::remove_file(&p);
    let mut reqs = requests()[..4].to_vec();
    reqs.push(json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"add_effect","arguments":{"clip_id":"clip2","kind":"temperature","params":{"amount":0.0}}}}));
    reqs.push(json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"set_effect_param","arguments":{"clip_id":"clip2","effect_id":"fx3","param":"amount","value":0.35}}}));
    reqs.push(json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"effects","arguments":{"action":"set_effect_param","args":{"clip_id":"clip2","effect_id":"fx3","param":"amount","value":0.5}}}}));
    run(Some(&format!("file:{}", p.display())), &reqs);
    let evs = events_of(&std::fs::read_to_string(&p).unwrap());
    let add = tool_events(&evs, "add_effect");
    assert_eq!(add[1]["payload"]["effect"], "temperature");
    assert_eq!(add[1]["payload"]["result"]["effect_id"], "fx3");
    let set = tool_events(&evs, "set_effect_param");
    assert_eq!(set.len(), 4, "direct call and group call both report under the resolved tool name");
    assert_eq!(set[1]["payload"]["from"], 0.0); assert_eq!(set[1]["payload"]["to"], 0.35);
    assert_eq!(set[1]["payload"]["changed"]["clip"]["effects[fx3].params.amount.base"], json!([0.0, 0.35]));
    assert_eq!(set[2]["group"], "effects");
    assert_eq!(set[3]["payload"]["from"], 0.35); assert_eq!(set[3]["payload"]["to"], 0.5);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn render_frame_reaches_the_viewer() {
    let p = tmp("frame.jsonl"); let _ = std::fs::remove_file(&p);
    let png = tmp("frame.png");
    let mut reqs = requests()[..4].to_vec();
    reqs.push(json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"render_frame","arguments":{"time":0.5,"path": png.display().to_string(),"inline":"never"}}}));
    run(Some(&format!("file:{}", p.display())), &reqs);
    let evs = events_of(&std::fs::read_to_string(&p).unwrap());
    let f = evs.iter().find(|e| e["type"] == "frame_rendered").expect("frame_rendered after render_frame");
    let b = f["png_b64"].as_str().unwrap();
    assert!(b.starts_with("iVBORw0KGgo"), "a PNG, base64");
    assert_eq!(b.len(), std::fs::metadata(&png).unwrap().len().div_ceil(3) as usize * 4);
    let rf = tool_events(&evs, "render_frame");
    assert_eq!(f["call_id"], rf[0]["call_id"]);
    let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&png);
}

#[test]
fn export_reports_progress() {
    if Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| !s.success()).unwrap_or(true) {
        eprintln!("ffmpeg not on PATH; skipping the export progress check");
        return;
    }
    let p = tmp("export.jsonl"); let _ = std::fs::remove_file(&p);
    let mp4 = tmp("export.mp4");
    let mut reqs = requests()[..4].to_vec();
    reqs.push(json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"export","arguments":{"path": mp4.display().to_string(),"audio":false}}}));
    let out = String::from_utf8(run(Some(&format!("file:{}", p.display())), &reqs)).unwrap();
    let evs = events_of(&std::fs::read_to_string(&p).unwrap());
    let ex = tool_events(&evs, "export");
    assert_eq!(ex.first().unwrap()["phase"], "start");
    assert_eq!(ex.last().unwrap()["phase"], "complete", "export failed? stdout: {out}");
    let prog: Vec<f64> = ex.iter().filter(|e| e["phase"] == "progress").map(|e| e["payload"]["progress"].as_f64().unwrap()).collect();
    assert!(ex.iter().filter(|e| e["phase"] == "progress").all(|e| (e["payload"]["pct"].as_f64().unwrap() - 100.0 * e["payload"]["progress"].as_f64().unwrap()).abs() < 0.06));
    assert!(!prog.is_empty(), "export emits progress events");
    assert!(prog.windows(2).all(|w| w[0] <= w[1]), "progress is monotonic: {prog:?}");
    assert!((prog.last().unwrap() - 1.0).abs() < 1e-9, "progress reaches 1.0: {prog:?}");
    assert!(ex.iter().all(|e| e["call_id"] == ex[0]["call_id"]));
    // frame counters (Naman's spec): current_frame, total_frames, fps (instantaneous), pct, elapsed_ms, eta_ms
    let pe: Vec<&Value> = ex.iter().copied().filter(|e| e["phase"] == "progress").collect();
    let total = pe[0]["payload"]["total_frames"].as_u64().expect("total_frames on every progress event");
    assert_eq!(total, 24, "1 s at 24 fps");
    let frames: Vec<u64> = pe.iter().map(|e| e["payload"]["current_frame"].as_u64().expect("current_frame on every progress event")).collect();
    assert_eq!(frames[0], 0, "an opening report before any frame: {frames:?}");
    assert_eq!(*frames.last().unwrap(), total, "the last report has every frame done: {frames:?}");
    assert!(frames.windows(2).all(|w| w[0] <= w[1]), "current_frame is monotonic: {frames:?}");
    let els: Vec<u64> = pe.iter().map(|e| e["payload"]["elapsed_ms"].as_u64().expect("elapsed_ms")).collect();
    assert!(els.windows(2).all(|w| w[0] <= w[1]), "elapsed_ms is monotonic: {els:?}");
    for e in &pe {
        let pl = &e["payload"];
        assert_eq!(pl["total_frames"].as_u64(), Some(total), "total_frames is constant: {pl}");
        let f = pl["current_frame"].as_u64().unwrap();
        assert!((pl["progress"].as_f64().unwrap() - f as f64 / total as f64).abs() < 1e-9, "progress = current_frame / total_frames: {pl}");
        assert!((pl["pct"].as_f64().unwrap() - 100.0 * f as f64 / total as f64).abs() < 0.06, "pct: {pl}");
        let fps = pl["fps"].as_f64().expect("fps on every progress event");
        assert!(fps.is_finite() && fps >= 0.0, "fps is a finite rate: {pl}");
        assert!(pl.get("eta_ms").is_some(), "eta_ms is always present (null when unknown): {pl}");
        if f == 0 {
            assert_eq!(fps, 0.0, "no frame finished yet, no rate: {pl}");
            assert!(pl["eta_ms"].is_null(), "no frame finished yet, no ETA: {pl}");
        } else {
            assert!(fps > 0.0, "frames finished since the last report give a rate: {pl}");
            assert!(pl["eta_ms"].as_u64().is_some(), "an ETA once frames are done: {pl}");
        }
    }
    assert_eq!(pe.last().unwrap()["payload"]["eta_ms"], 0, "nothing left at the end");
    // the Renders history row reads the output size from the complete event (telemetry only; stdout unchanged)
    let size = std::fs::metadata(&mp4).unwrap().len();
    let done = ex.last().unwrap();
    assert_eq!(done["payload"]["bytes"].as_u64(), Some(size), "{done}");
    assert_eq!(done["payload"]["result"]["bytes"].as_u64(), Some(size));
    assert!(!out.contains("\"bytes\""), "the tool result itself is unchanged: {out}");
    let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&mp4);
}

/// Collects every line the spectator connection sends, until the peer goes away.
fn listen_once(l: std::net::TcpListener) -> (std::thread::JoinHandle<()>, std::sync::Arc<std::sync::Mutex<Vec<String>>>, std::sync::mpsc::Receiver<std::net::TcpStream>) {
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (stx, srx) = std::sync::mpsc::channel();
    let g = got.clone();
    let h = std::thread::spawn(move || {
        let Ok((s, _)) = l.accept() else { return };
        drop(l); // one connection per listener lifetime; the port is closed again once it is taken
        let _ = stx.send(s.try_clone().unwrap());
        for line in BufReader::new(s).lines().map_while(Result::ok) { g.lock().unwrap().push(line); }
    });
    (h, got, srx)
}

fn wait_for(got: &std::sync::Arc<std::sync::Mutex<Vec<String>>>, within: Duration, pred: impl Fn(&Value) -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < within {
        if got.lock().unwrap().iter().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|e| pred(&e)) { return true; }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn spectator_closing_and_reopening_under_load() {
    // one long-lived server under continuous tool-call load; the GUI's listener dies mid-session and comes back on
    // the same port while calls keep flowing
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    let l1 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l1.local_addr().unwrap().port();
    let (h1, got1, conn1) = listen_once(l1);

    let mut ch = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .env_remove("KIMO_TOOLS").env("KIMO_SPECTATOR", format!("tcp://127.0.0.1:{port}"))
        .spawn().expect("spawn kimo-mcp");
    let mut stdin = ch.stdin.take().unwrap();
    let stdout = ch.stdout.take().unwrap();
    let (otx, orx) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || { for l in BufReader::new(stdout).lines().map_while(Result::ok) { if otx.send(l).is_err() { break; } } });
    let setp = |id: u64, v: u64| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"set_property","arguments":{"clip_id":"clip2","property":"x","value":v}}});
    let start_value = |e: &Value| -> Option<u64> { (e["tool_name"] == "set_property" && e["phase"] == "start").then(|| e["payload"]["args"]["value"].as_u64()).flatten() };

    // load driver: one call after another as fast as the server answers, each under a 10 s deadline (a server
    // blocked on telemetry fails here instead of hanging the suite); records (value, sent at, answered at)
    let stop = Arc::new(AtomicBool::new(false));
    let next = Arc::new(AtomicU64::new(1));
    let (st, nx) = (stop.clone(), next.clone());
    let driver = std::thread::spawn(move || {
        let mut sent: Vec<Value> = Vec::new(); let mut out: Vec<String> = Vec::new(); let mut log: Vec<(u64, Instant, Instant)> = Vec::new();
        for r in &requests()[..4] {
            writeln!(stdin, "{r}").unwrap(); stdin.flush().unwrap();
            out.push(orx.recv_timeout(Duration::from_secs(10)).expect("setup response")); sent.push(r.clone());
        }
        while !st.load(Ordering::SeqCst) {
            let v = nx.fetch_add(1, Ordering::SeqCst);
            let req = setp(100 + v, v);
            let t0 = Instant::now();
            writeln!(stdin, "{req}").unwrap(); stdin.flush().unwrap();
            let resp = orx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| panic!("no response within 10 s to call {v}: the server hung"));
            log.push((v, t0, Instant::now())); sent.push(req); out.push(resp);
        }
        drop(stdin);
        (sent, out, log)
    });

    // 1. connected under load: events arrive
    assert!(wait_for(&got1, Duration::from_secs(10), |e| start_value(e).is_some()), "first listener receives events");
    let s1 = conn1.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    // 2. the spectator dies mid-load: connection shut, listener already gone, calls keep flowing
    let killed_at = Instant::now();
    let v_at_kill = next.load(Ordering::SeqCst);
    s1.shutdown(std::net::Shutdown::Both).unwrap();
    h1.join().unwrap();
    std::thread::sleep(Duration::from_millis(700));
    let v_before_bind = next.load(Ordering::SeqCst);
    assert!(v_before_bind > v_at_kill + 5, "calls kept flowing while the spectator was gone ({v_at_kill} -> {v_before_bind})");
    // 3. the spectator comes back on the same port, still under load: the writer reconnects within the backoff ceiling
    let l2 = std::net::TcpListener::bind(("127.0.0.1", port)).expect("rebind the spectator port");
    let bound_at = Instant::now();
    let (h2, got2, conn2) = listen_once(l2);
    assert!(wait_for(&got2, Duration::from_secs(5), |e| start_value(e).is_some()), "the writer reconnected to the reopened spectator");
    let reconnect = bound_at.elapsed();
    assert!(reconnect < Duration::from_millis(1500), "reconnect inside the 1000 ms backoff ceiling plus slack ({reconnect:?})");
    // later calls arrive whole (start then complete under one call_id)
    let v_after = next.load(Ordering::SeqCst) + 2;
    assert!(wait_for(&got2, Duration::from_secs(5), |e| start_value(e).map_or(false, |v| v >= v_after)), "events after the reconnect keep arriving");
    stop.store(true, Ordering::SeqCst);
    let (sent, out, log) = driver.join().expect("load driver: every call answered");
    let st = ch.wait().unwrap();
    assert!(st.success(), "kimo-mcp exited {st:?}");
    reader.join().unwrap();
    if let Ok(s2) = conn2.recv_timeout(Duration::from_secs(1)) { let _ = s2.shutdown(std::net::Shutdown::Both); }
    h2.join().unwrap();

    let evs2: Vec<Value> = got2.lock().unwrap().iter().map(|l| serde_json::from_str(l).expect("whole JSON lines after the reconnect")).collect();
    let first = evs2.iter().find(|e| start_value(e).map_or(false, |v| v >= v_after)).unwrap();
    assert!(evs2.iter().any(|e| e["call_id"] == first["call_id"] && e["phase"] == "complete"), "complete follows start for {}", first["call_id"]);
    // nothing emitted while disconnected was queued and replayed: calls answered well before the rebind never show up
    let gone: Vec<u64> = log.iter().filter(|(_, _, done)| *done > killed_at && *done + Duration::from_millis(200) < bound_at).map(|x| x.0).collect();
    assert!(gone.len() > 5, "calls answered while disconnected: {}", gone.len());
    for v in &gone { assert!(!evs2.iter().any(|e| start_value(e) == Some(*v)), "event from while disconnected was replayed: call {v}"); }
    // no call slowed down by the spectator going away or coming back
    let worst = log.iter().filter(|(_, t0, _)| *t0 > killed_at - Duration::from_millis(200)).map(|(_, t0, t1)| *t1 - *t0).max().unwrap();
    assert!(worst < Duration::from_secs(1), "a call took {worst:?} while the spectator came and went");
    eprintln!("load: {} calls, {} while disconnected, reconnect {:?} after rebind, worst call {:?}", log.len(), gone.len(), reconnect, worst);

    // MCP stdout through all of it is exactly what the same requests give with telemetry off
    let off = String::from_utf8(run(None, &sent)).unwrap();
    let on = out.join("\n") + "\n";
    assert_eq!(on, off, "stdout unchanged by the spectator coming and going");
}
