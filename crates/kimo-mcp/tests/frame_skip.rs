//! Headless live-frame skips, end to end against the real kimo-mcp binary (preview.rs, "Headless skips"): every live
//! frame a mutating call queues ends as exactly one auto `frame_rendered` or one `frame_skipped` with an explicit
//! `skip_reason`. A pipelined burst coalesces; the agent's own render_frame supersedes a pending frame; an HDR output
//! transfer and a render error are refused with the engine's words (`refused` + `reason`, which the GUI's out-of-date
//! chip reads); an agent image over the forward limit is reported; a spectator that stops reading drops frames off the
//! latest-only lane, each counted (`frame_lane_drops`) and reported. Benign skips never carry `reason`.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Mcp { ch: Child, stdin: ChildStdin, out: BufReader<ChildStdout>, id: u64 }

impl Mcp {
    fn spawn(envs: &[(&str, &str)]) -> Mcp {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .env("KIMO_SPECTATOR", "off").env_remove("KIMO_TOOLS").env_remove("KIMO_SPECTATOR_FRAMES").env_remove("KIMO_SPECTATOR_FRAME_PX")
            .env_remove("KIMO_SPECTATOR_DEMO");
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
    /// Every request written before any response is read: each call starts the moment the previous one answered.
    fn pipeline(&mut self, calls: &[(&str, Value)]) {
        for (n, a) in calls { self.send("tools/call", json!({"name": n, "arguments": a})); }
        for (n, _) in calls { let r = self.recv(); assert_eq!(r["result"]["isError"], false, "{n} failed: {r}"); }
    }
}

impl Drop for Mcp { fn drop(&mut self) { let _ = self.ch.kill(); let _ = self.ch.wait(); } }

fn tmp(name: &str) -> std::path::PathBuf { std::env::temp_dir().join(format!("kimo-fskip-{}-{name}", std::process::id())) }

fn events(p: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(p).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn wait_for<T>(secs: f64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    loop { if let Some(v) = f() { return Some(v); } if Instant::now() > end { return None; } std::thread::sleep(Duration::from_millis(20)); }
}

/// The writer has opened the file sink (its first line, the session event, is written once it is connected): from
/// here on every mutating call queues a live frame. Before that a call has nobody listening and queues none.
fn sink_open(p: &std::path::Path) { wait_for(10.0, || events(p).first().filter(|e| e["type"] == "session").map(|_| ())).expect("the sink never opened"); }

/// The JSON object a tool returned.
fn result_of(r: &Value) -> Value { serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap() }

/// A 4 s solid clip on a fresh project; returns its clip id.
fn setup(m: &mut Mcp, w: u32, h: u32) -> String {
    m.call("project_set", json!({"width": w, "height": h, "fps": 24}));
    let t = result_of(&m.call("add_track", json!({})))["track_id"].clone();
    let r = m.call("add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5, 0.5, 0.5], "start": 0, "duration": 4}));
    result_of(&r)["clip_id"].as_str().unwrap().to_string()
}

/// call_ids of the calls that changed the project (one project_changed each), in order.
fn mutating(evs: &[Value]) -> Vec<String> {
    evs.iter().filter(|e| e["type"] == "project_changed").map(|e| e["call_id"].as_str().unwrap().to_string()).collect()
}
/// A headless live frame's skip (not demo mode's, not the agent's own image).
fn live_skip(e: &Value) -> bool { e["type"] == "frame_skipped" && e.get("demo").is_none() && e.get("agent").is_none() }
/// What became of the live frame `id` queued: its auto frame_rendered and/or its frame_skipped events.
fn outcomes<'a>(evs: &'a [Value], id: &str) -> Vec<&'a Value> {
    evs.iter().filter(|e| e["call_id"] == id && ((e["type"] == "frame_rendered" && e["auto"] == true) || live_skip(e))).collect()
}
fn unaccounted(evs: &[Value]) -> Vec<String> { mutating(evs).into_iter().filter(|id| outcomes(evs, id).is_empty()).collect() }
/// Every mutating call so far has an outcome; then a short wait for anything late (a duplicate would show).
fn settled(mut read: impl FnMut() -> Vec<Value>) -> Vec<Value> {
    wait_for(45.0, || { let e = read(); (!mutating(&e).is_empty() && unaccounted(&e).is_empty()).then_some(()) })
        .unwrap_or_else(|| panic!("live frames never accounted for: {:?}", unaccounted(&read())));
    std::thread::sleep(Duration::from_millis(600));
    read()
}
fn assert_one_each(evs: &[Value]) {
    for id in mutating(evs) {
        let o = outcomes(evs, &id);
        assert_eq!(o.len(), 1, "call {id}: exactly one frame_rendered or frame_skipped, got {o:#?}");
    }
}
fn completes(evs: &[Value], tool: &str) -> usize { evs.iter().filter(|e| e["tool_name"] == tool && e["phase"] == "complete").count() }
/// Wait until `tool` has completed `n` times on the sink (the response can arrive before its telemetry is written).
fn wait_calls(mut read: impl FnMut() -> Vec<Value>, tool: &str, n: usize) -> String {
    wait_for(30.0, || { let e = read(); (completes(&e, tool) >= n).then(|| last_call_id(&e, tool)) }).unwrap_or_else(|| panic!("{tool} never completed {n} times"))
}
fn last_call_id(evs: &[Value], tool: &str) -> String {
    evs.iter().rev().find(|e| e["tool_name"] == tool && e["phase"] == "complete").and_then(|e| e["call_id"].as_str()).unwrap_or_else(|| panic!("no {tool}")).to_string()
}
/// A benign skip: the frame is replaced by a newer one, so it carries no refusal for the GUI's chip.
fn assert_benign(s: &Value) {
    assert!(s.get("reason").is_none() && s.get("refused").is_none(), "a benign skip must not read as a refusal: {s}");
    assert!(s.get("auto").is_none(), "`auto` stays the mark of a drawn live frame: {s}");
    assert!(s["call_id"].is_string() && s["rev"].is_u64() && s["gen"].is_u64() && s["frame_lane_drops"].is_u64() && s["ts_ms"].is_u64(), "shape: {s}");
}

#[test]
fn a_burst_coalesces_and_every_queued_frame_is_accounted_for() {
    let p = tmp("burst.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAME_PX", "320")]);
    sink_open(&p);
    let clip = setup(&mut m, 640, 360);
    settled(|| events(&p));
    let burst: Vec<(&str, Value)> = (0..16).map(|i| ("set_property", json!({"clip_id": clip, "property": "x", "value": 10 * (i + 1)}))).collect();
    m.pipeline(&burst);
    wait_calls(|| events(&p), "set_property", 16);
    let evs = settled(|| events(&p));
    assert_one_each(&evs);
    let ids = mutating(&evs);
    let last = ids.last().unwrap();
    assert_eq!(outcomes(&evs, last)[0]["type"], "frame_rendered", "the newest state reaches the screen");
    let mut coalesced = 0;
    for (i, e) in evs.iter().enumerate() {
        if e["type"] != "frame_skipped" { continue; }
        assert_eq!(e["skip_reason"], "coalesced", "only coalescing happens in a plain burst: {e}");
        assert_benign(e);
        assert!(e["superseded_by"]["call_id"].is_string() && e["superseded_by"]["gen"].as_u64() > e["gen"].as_u64(), "names the newer frame: {e}");
        // state-lane order: a frame is reported skipped only after its own call completed, and before the newer call's frame
        assert!(evs[..i].iter().any(|x| x["call_id"] == e["call_id"] && x["phase"] == "complete"), "skip before its call completed: {e}");
        assert!(e.get("render_cancelled").is_none() || e["render_cancelled"] == true);
        coalesced += 1;
    }
    assert!(coalesced >= 8, "a pipelined burst of 16 coalesces: {coalesced} coalesced");
    eprintln!("burst: {} mutating calls, {coalesced} coalesced, {} drawn", ids.len(), evs.iter().filter(|e| e["type"] == "frame_rendered" && e["auto"] == true).count());
    drop(m); let _ = std::fs::remove_file(&p);
}

#[test]
fn the_agents_render_frame_supersedes_a_pending_live_frame() {
    let p = tmp("agent.jsonl"); let _ = std::fs::remove_file(&p);
    let png = tmp("agent.png");
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAME_PX", "320")]);
    sink_open(&p);
    let clip = setup(&mut m, 640, 360);
    settled(|| events(&p));
    let mut hit = None;
    // the render_frame request is already waiting when the set_property answers, so it normally starts inside QUIET;
    // a scheduler hiccup can let the live frame start first, so a few tries are allowed
    for i in 0..5 {
        m.pipeline(&[("set_property", json!({"clip_id": clip, "property": "x", "value": 50 + i})),
            ("render_frame", json!({"time": 1.0, "path": png.to_str().unwrap(), "inline": "never"}))]);
        let set = wait_calls(|| events(&p), "set_property", i as usize + 1);
        let rf = wait_calls(|| events(&p), "render_frame", i as usize + 1);
        let evs = settled(|| events(&p));
        let o = outcomes(&evs, &set)[0].clone();
        if o["type"] == "frame_skipped" { hit = Some((o, rf)); break; }
    }
    let (s, rf) = hit.expect("the live frame was never superseded by the agent's render_frame in 5 tries");
    assert_eq!(s["skip_reason"], "superseded_by_render_frame", "{s}");
    assert_eq!(s["superseded_by"]["call_id"], json!(rf), "names the render_frame call: {s}");
    assert_benign(&s);
    let evs = events(&p);
    assert!(evs.iter().any(|e| e["type"] == "frame_rendered" && e.get("auto").is_none() && e["call_id"] == json!(rf)), "the agent's frame reached the viewer");
    assert_one_each(&evs);
    drop(m); let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&png);
}

#[test]
fn hdr_output_and_render_errors_are_refused_with_the_engines_words() {
    let p = tmp("refused.jsonl"); let _ = std::fs::remove_file(&p);
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAME_PX", "320")]);
    sink_open(&p);
    let clip = setup(&mut m, 640, 360);
    settled(|| events(&p));
    let outcome = |p: &std::path::Path, id: &str| wait_for(30.0, || events(p).into_iter().find(|e| e["call_id"] == id && ((e["type"] == "frame_rendered" && e["auto"] == true) || live_skip(e))))
        .unwrap_or_else(|| panic!("no live-frame outcome for {id}"));
    for (n, (transfer, why)) in [(6, "pq_output"), (7, "hlg_output")].into_iter().enumerate() {
        m.call("set_color_pipeline", json!({"settings": {"enabled": true, "output_gamut": 1, "output_transfer": transfer, "reference_white": 100}}));
        let id = wait_calls(|| events(&p), "set_color_pipeline", n + 1);
        let s = outcome(&p, &id);
        assert_eq!((s["type"].as_str(), s["skip_reason"].as_str()), (Some("frame_skipped"), Some(why)), "transfer {transfer}: {s}");
        assert_eq!((s["refused"].clone(), s["reason"].clone()), (json!(true), json!("HDR output")), "the demo-mode words, for the GUI chip: {s}");
        assert!(s["rev"].is_u64() && s["gen"].is_u64() && s["frame_lane_drops"].is_u64());
    }
    // back to SDR: the picture renders again
    m.call("set_color_pipeline", json!({"settings": {"enabled": false}}));
    let id = wait_calls(|| events(&p), "set_color_pipeline", 3);
    assert_eq!(outcome(&p, &id)["type"], "frame_rendered");
    // a mixer selected as the grade output with no inputs: the engine refuses the render (the GUI's inferred "item 4")
    m.call("add_node", json!({"clip_id": clip, "node_id": "m", "kind": "parallel"}));
    m.call("set_grade_output", json!({"clip_id": clip, "node_id": "m"}));
    let id = wait_calls(|| events(&p), "set_grade_output", 1);
    let s = outcome(&p, &id);
    assert_eq!((s["type"].as_str(), s["skip_reason"].as_str()), (Some("frame_skipped"), Some("render_error")), "{s}");
    assert_eq!(s["refused"], true);
    assert!(s["reason"].as_str().unwrap().contains("incomplete reachable node m"), "the engine's own words: {s}");
    drop(m); let _ = std::fs::remove_file(&p);
}

#[test]
fn an_agent_image_over_the_forward_limit_is_reported() {
    let p = tmp("oversize.jsonl"); let _ = std::fs::remove_file(&p);
    let png = tmp("oversize.png");
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("file:{}", p.display())), ("KIMO_SPECTATOR_FRAME_PX", "320")]);
    sink_open(&p);
    let clip = setup(&mut m, 1920, 1080);
    m.call("add_effect", json!({"clip_id": clip, "kind": "grain", "params": {"amount": 1.0}}));
    settled(|| events(&p));
    m.call("render_frame", json!({"time": 1.0, "path": png.to_str().unwrap(), "inline": "never"}));
    let size = std::fs::metadata(&png).unwrap().len();
    assert!(size > 1_500_000, "the noise frame must be over the 1.5 MB forward limit to test it: {size}");
    let rf = wait_calls(|| events(&p), "render_frame", 1);
    let s = wait_for(10.0, || events(&p).into_iter().find(|e| e["type"] == "frame_skipped" && e["call_id"] == json!(rf))).expect("no frame_skipped for the oversize image");
    assert_eq!(s["skip_reason"], "oversize_agent_frame");
    assert_eq!((s["agent"].clone(), s["bytes"].as_u64(), s["max_bytes"].as_u64()), (json!(true), Some(size), Some(1_500_000)), "{s}");
    assert!(s.get("reason").is_none() && s.get("refused").is_none() && s.get("auto").is_none(), "the live preview still covers it: {s}");
    assert!(!events(&p).iter().any(|e| e["type"] == "frame_rendered" && e["call_id"] == json!(rf)), "an oversize image is not forwarded");
    drop(m); let _ = std::fs::remove_file(&p); let _ = std::fs::remove_file(&png);
}

#[test]
fn a_spectator_that_stops_reading_drops_frames_off_the_lane_counted_and_reported() {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mut m = Mcp::spawn(&[("KIMO_SPECTATOR", &format!("tcp://127.0.0.1:{port}")), ("KIMO_SPECTATOR_FRAME_PX", "1920")]);
    let (sock, _) = l.accept().unwrap();
    std::thread::sleep(Duration::from_millis(300)); // the writer marks itself connected right after its connect returns
    let (go, got) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(Vec::<Value>::new())));
    let reader = { let (go, got) = (go.clone(), got.clone()); std::thread::spawn(move || {
        while !go.load(Ordering::SeqCst) { std::thread::sleep(Duration::from_millis(10)); }
        for line in BufReader::new(sock).lines() {
            let Ok(line) = line else { break };
            // keep the events small: a full-size frame's payload is replaced by its length
            if let Ok(mut v) = serde_json::from_str::<Value>(&line) {
                if let Some(n) = v.get("png_b64").and_then(|b| b.as_str()).map(|b| b.len()) { v["png_b64"] = json!(n); }
                got.lock().unwrap().push(v);
            }
        }
    }) };
    // full-size noise frames: the first fills the socket while nobody reads, the rest queue on the 2-deep frame lane
    let clip = setup(&mut m, 1920, 1080);
    m.call("add_effect", json!({"clip_id": clip, "kind": "grain", "params": {"amount": 1.0}}));
    for i in 0..16 { m.call("set_property", json!({"clip_id": clip, "property": "x", "value": 10 * (i + 1)})); std::thread::sleep(Duration::from_millis(300)); }
    go.store(true, Ordering::SeqCst);
    wait_calls(|| got.lock().unwrap().clone(), "set_property", 16);
    let evs = settled(|| got.lock().unwrap().clone());
    assert_eq!(evs[0]["type"], "session", "nothing was lost from the state lane");
    assert_one_each(&evs);
    let drops: Vec<&Value> = evs.iter().filter(|e| e["type"] == "frame_skipped" && e["skip_reason"] == "frame_lane_dropped").collect();
    assert!(!drops.is_empty(), "a stalled spectator drops frames off the lane: {:?}", evs.iter().filter(|e| e["type"] == "frame_skipped").map(|e| e["skip_reason"].clone()).collect::<Vec<_>>());
    for d in &drops { assert_benign(d); }
    let frames: Vec<&Value> = evs.iter().filter(|e| e["type"] == "frame_rendered" && e["auto"] == true).collect();
    assert!(frames.iter().all(|f| f["frame_lane_drops"].is_u64()), "every live frame carries the lane drop counter");
    assert_eq!(frames.last().unwrap()["frame_lane_drops"].as_u64(), Some(drops.len() as u64), "the counter matches the reported drops");
    eprintln!("stalled spectator: {} mutating calls, {} frames drawn, {} dropped off the lane, skips {:?}", mutating(&evs).len(), frames.len(), drops.len(),
        evs.iter().filter(|e| live_skip(e)).map(|e| e["skip_reason"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>());
    drop(m);
    let _ = reader.join();
}
