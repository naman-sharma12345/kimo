//! Telemetry on by default: with KIMO_SPECTATOR unset kimo-mcp scans 127.0.0.1:7421..=7430 and streams only to a peer
//! that greets it with the versioned spectator hello (crates/kimo-gui/src/main.rs). These tests run with the variable
//! fully unset (or set to the values under test). They need those loopback ports free (no spectator open on this
//! machine); a test that finds one taken prints "skipped" and returns without asserting.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HELLO: &[u8] = b"KIMO-SPECTATOR/1\n";
/// The tests below share the fixed port range: one at a time.
static PORTS: Mutex<()> = Mutex::new(());
fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> { PORTS.lock().unwrap_or_else(|e| e.into_inner()) }

fn req(id: u64, name: &str, args: Value) -> Value { json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}) }
fn calls() -> Vec<Vec<Value>> {
    vec![
        vec![json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}), json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
             req(3, "project_set", json!({"width":320,"height":180,"fps":24})), req(4, "add_track", json!({}))],
        vec![req(5, "add_text", json!({"track_id":"track1","text":"kimo","start":0,"duration":1})), req(6, "set_property", json!({"clip_id":"clip2","property":"x","value":40}))],
        vec![req(7, "set_property", json!({"clip_id":"clip2","property":"y","value":12})), req(8, "get_project", json!({}))],
    ]
}

type Step = Box<dyn FnOnce()>;
/// One kimo-mcp session in phases: before each phase its step runs, then a pause of `gap`. `spec` None = unset.
/// Returns (stdout bytes, stderr bytes).
fn session(spec: Option<&str>, phases: Vec<(Step, Duration, Vec<Value>)>) -> (Vec<u8>, Vec<u8>) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_kimo-mcp"));
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, _) in std::env::vars() { if k.starts_with("KIMO_") { cmd.env_remove(k); } }
    if let Some(s) = spec { cmd.env("KIMO_SPECTATOR", s); }
    let mut ch = cmd.spawn().expect("spawn kimo-mcp");
    let mut stdin = ch.stdin.take().unwrap();
    let mut out = BufReader::new(ch.stdout.take().unwrap());
    let mut err = ch.stderr.take().unwrap();
    let errt = std::thread::spawn(move || { let mut v = Vec::new(); let _ = err.read_to_end(&mut v); v });
    let mut stdout = Vec::new();
    for (step, gap, reqs) in phases {
        step();
        std::thread::sleep(gap);
        for r in reqs {
            writeln!(stdin, "{r}").unwrap(); stdin.flush().unwrap();
            let mut l = String::new(); out.read_line(&mut l).unwrap(); stdout.extend_from_slice(l.as_bytes());
        }
    }
    drop(stdin);
    out.read_to_end(&mut stdout).unwrap();
    assert!(ch.wait().unwrap().success());
    (stdout, errt.join().unwrap())
}
fn plain(reqs: Vec<Value>) -> Vec<(Step, Duration, Vec<Value>)> { vec![(Box::new(|| {}), Duration::ZERO, reqs)] }

/// A loopback listener on `port` until stopped (`stop` closes the port and every connection it accepted). With `hello`
/// it greets each connection with it (a spectator); `got` is every line it was sent, `bytes` the byte count.
#[derive(Clone)]
struct Peer { stop: Arc<AtomicBool>, conns: Arc<Mutex<Vec<TcpStream>>>, got: Arc<Mutex<Vec<String>>>, bytes: Arc<Mutex<usize>>, accepted: Arc<Mutex<usize>> }
impl Peer {
    fn start(l: TcpListener, hello: Option<&'static [u8]>) -> Peer {
        let p = Peer { stop: Arc::default(), conns: Arc::default(), got: Arc::default(), bytes: Arc::default(), accepted: Arc::default() };
        l.set_nonblocking(true).unwrap();
        let q = p.clone();
        std::thread::spawn(move || while !q.stop.load(Ordering::SeqCst) {
            match l.accept() {
                Ok((mut c, _)) => {
                    c.set_nonblocking(false).unwrap();
                    *q.accepted.lock().unwrap() += 1;
                    if let Some(h) = hello { let _ = c.write_all(h); }
                    q.conns.lock().unwrap().push(c.try_clone().unwrap());
                    let r = q.clone();
                    std::thread::spawn(move || {
                        let mut rd = BufReader::new(c);
                        let mut line = String::new();
                        while let Ok(n) = rd.read_line(&mut line) { if n == 0 { break; } *r.bytes.lock().unwrap() += n; r.got.lock().unwrap().push(line.trim_end().to_string()); line.clear(); }
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        });
        p
    }
    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        for c in self.conns.lock().unwrap().iter() { let _ = c.shutdown(Shutdown::Both); }
        std::thread::sleep(Duration::from_millis(50)); // the accept loop sees the flag and drops the listener
    }
    fn events(&self) -> Vec<Value> { self.got.lock().unwrap().iter().filter_map(|l| serde_json::from_str(l).ok()).collect() }
}

fn bind(port: u16) -> TcpListener { TcpListener::bind(("127.0.0.1", port)).unwrap() }
fn ports_free() -> bool {
    let ok = (7421..=7430u16).all(|p| TcpListener::bind(("127.0.0.1", p)).is_ok());
    if !ok { println!("skipped: a port in 127.0.0.1:7421..=7430 is taken (a spectator open on this machine?)"); }
    ok
}
fn rpc_ids(evs: &[Value], phase: &str) -> Vec<u64> { evs.iter().filter(|e| e["phase"] == phase).filter_map(|e| e["rpc_id"].as_u64()).collect() }

#[test]
fn default_skips_squatters_silently_and_moves_to_a_real_spectator() {
    let _one = one_at_a_time();
    if !ports_free() { return; }
    let silent = Peer::start(bind(7421), None);                                   // accepts, says nothing
    let garbage = Peer::start(bind(7423), Some(b"SSH-2.0-OpenSSH_9.2\r\n"));      // accepts, says something else
    let gui22: Arc<Mutex<Option<Peer>>> = Arc::default();
    let gui21: Arc<Mutex<Option<Peer>>> = Arc::default();
    let (g22a, g22b, g21, s21) = (gui22.clone(), gui22.clone(), gui21.clone(), silent.clone());
    let mut c = calls().into_iter();
    let phases: Vec<(Step, Duration, Vec<Value>)> = vec![
        // A: only squatters (7421 silent, 7423 garbage): nothing is streamed, nothing is printed
        (Box::new(|| {}), Duration::ZERO, c.next().unwrap()),
        // B: a spectator appears on 7422: the next backoff pass finds it
        (Box::new(move || { std::thread::sleep(Duration::from_millis(700)); *g22a.lock().unwrap() = Some(Peer::start(bind(7422), Some(HELLO))); }), Duration::from_millis(1600), c.next().unwrap()),
        // C: the 7421 squatter goes, a spectator takes 7421, the 7422 one closes: the engine moves to 7421 (no blacklist)
        (Box::new(move || {
            s21.stop();
            *g21.lock().unwrap() = Some(Peer::start(bind(7421), Some(HELLO)));
            g22b.lock().unwrap().as_ref().unwrap().stop();
        }), Duration::from_millis(1600), c.next().unwrap()),
    ];
    let (stdout, stderr) = session(None, phases);
    std::thread::sleep(Duration::from_millis(300));
    let (on22, on21) = (gui22.lock().unwrap().clone().unwrap(), gui21.lock().unwrap().clone().unwrap());
    assert!(stderr.is_empty(), "silent means silent: {:?}", String::from_utf8_lossy(&stderr));
    assert_eq!(*silent.bytes.lock().unwrap(), 0, "the silent squatter on 7421 got telemetry");
    assert_eq!(*garbage.bytes.lock().unwrap(), 0, "the squatter that answers something else got telemetry");
    assert!(*silent.accepted.lock().unwrap() >= 1 && *garbage.accepted.lock().unwrap() >= 1, "both squatters were tried");
    assert_eq!(rpc_ids(&on22.events(), "start"), vec![5, 6], "7422 gets the calls made while it was up, nothing replayed");
    assert_eq!(rpc_ids(&on22.events(), "complete"), vec![5, 6]);
    assert_eq!(rpc_ids(&on21.events(), "start"), vec![7, 8], "the engine moved to the spectator that took 7421 from the squatter");
    assert_eq!(rpc_ids(&on21.events(), "complete"), vec![7, 8]);
    // stdout is the JSON-RPC responses only, byte-identical to a run with telemetry off
    let (off, off_err) = session(Some("off"), plain(calls().concat()));
    assert!(off_err.is_empty());
    assert_eq!(String::from_utf8_lossy(&stdout), String::from_utf8_lossy(&off), "default-on with squatters and spectators changes nothing on stdout");
    assert_eq!(stdout.iter().filter(|b| **b == b'\n').count(), calls().concat().len());
    garbage.stop(); on21.stop();
}

#[test]
fn default_with_no_app_and_unusable_values_are_quiet_and_change_nothing() {
    let _one = one_at_a_time();
    if !ports_free() { return; }
    let all = calls().concat();
    let (off, _) = session(Some("off"), plain(all.clone()));
    for spec in [None, Some(""), Some("Off"), Some("nonsense"), Some("tcp://0.0.0.0:7421"), Some("tcp://10.1.2.3:7421")] {
        let (out, err) = session(spec, plain(all.clone()));
        assert!(err.is_empty(), "KIMO_SPECTATOR={spec:?} printed {:?}", String::from_utf8_lossy(&err));
        assert_eq!(out, off, "KIMO_SPECTATOR={spec:?} changes stdout");
    }
}

#[test]
fn an_older_or_other_hello_is_a_failed_handshake() {
    let _one = one_at_a_time();
    if !ports_free() { return; }
    let old = Peer::start(bind(7421), Some(b"kimo-spectator 0\n"));
    let next = Peer::start(bind(7422), Some(b"KIMO-SPECTATOR/2\n"));
    let (_, err) = session(None, vec![(Box::new(|| {}), Duration::from_millis(300), calls().concat())]);
    std::thread::sleep(Duration::from_millis(200));
    assert!(err.is_empty());
    assert_eq!((*old.bytes.lock().unwrap(), *next.bytes.lock().unwrap()), (0, 0), "a peer with another hello gets nothing");
    old.stop(); next.stop();
}
