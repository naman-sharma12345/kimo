//! Spectator telemetry: out-of-band JSON-lines events for the Kimo spectator GUI, on by default.
//!
//! `KIMO_SPECTATOR` picks the sink:
//!   unset or empty (the default; `on`, `1` and `gui` mean the same): the spectator GUI. The engine tries
//!     127.0.0.1:7421..=7430 in order and keeps only a peer that greets it with the spectator's hello line
//!     (`HELLO`, a versioned line, `KIMO-SPECTATOR/1`; any other line, an older or newer version included, is a failed
//!     handshake; sent by crates/kimo-gui/src/main.rs as soon as it accepts), so a foreign app holding one of those ports
//!     never gets a byte of telemetry; with no spectator there it retries quietly (nothing is ever printed);
//!   `tcp://127.0.0.1:PORT` (loopback only): exactly that port, no scanning and no hello (tests, relays, recordings);
//!   `file:/path/events.jsonl` (append; tests and recordings);
//!   `off`, `0`, `false`: telemetry off (no thread, no work). So is any value it cannot use (a non-loopback host, an
//!     unparsable one): quietly, since nothing about telemetry is ever printed.
//! Guarantees: nothing is ever written to stdout (the MCP channel); tool arguments and results are only read,
//! never changed. Two lanes feed one background writer, the only place that touches the sink:
//!   * state telemetry (every tools/call's start and its one complete|error, project_changed, the agent's own
//!     frame_rendered, export progress, and spectator demo frames) is lossless and ordered: never coalesced, never
//!     dropped while a spectator is connected. Its memory bound is `STATE_BOUND` queued bytes; past it, emitting waits
//!     for the writer (bounded backpressure: the call that emits holds until the spectator has read enough) instead of
//!     dropping. Nothing waits while no spectator is connected, so a dead or absent GUI never slows a call;
//!   * live preview canvas frames (preview.rs) have their own latest-only queue of `FRAME_DEPTH`, oldest dropped first
//!     (the newest frame supersedes them), so frames never displace, drop or hold up state events. Each drop is
//!     counted (`frame_lane_drops`, carried as `frame_lane_drops` on every auto frame) and reported as a state-lane
//!     `frame_skipped` with `skip_reason: "frame_lane_dropped"` (preview.rs, "Headless skips").
//! The writer takes lines in the order they were emitted across both lanes and writes whole lines only: a spectator
//! that is open but slow to read (a busy webview) is waited for, never cut off, so no line is ever truncated on a live
//! connection. With no listener events are dropped before they are serialized (`listening`); the writer reconnects on its
//! own backoff timer when the GUI is closed and reopened mid-session, and events from while it was gone are not replayed
//! (a reopened GUI gets the current frame). With `KIMO_SPECTATOR=off` every hook below is a single `OnceLock` load.
//!
//! One event per line, the spectator's unified contract (crates/kimo-gui/README.md, "Unified tool events"):
//!   {tool_name, call_id, rpc_id, phase: start|progress|complete|error, target: {track_id, clip_id, param}, payload, ts, ts_ms}
//! (`rpc_id`: the JSON-RPC id of the tools/call request, so a client can tie its calls to their events)
//! plus `duration_ms` on complete/error. payload by phase:
//!   start    {args}
//!   progress {pct 0..100, progress 0..1, stage, elapsed_ms} plus, from frame-counting tools (export),
//!            {current_frame (frames finished = next frame index), total_frames, fps (instantaneous), eta_ms}
//!   complete {dur_ms, result, changed, mutated} plus `bytes` (output file size, also in result) for export
//!            tools, plus the router's vocabulary where the engine state shows it:
//!            from/to (+param) for a field change, clip/track/s/e/label for an insert, s/e/at for edits,
//!            effect, on (mute/solo), text/s/e (captions)
//!   error    {dur_ms, error, code, param, hint}
//! Engine-level events use the shell's plain `type` form, which the router passes through:
//!   {"type":"session","width","height"} once at startup, {"type":"project_changed","rev","width","height"} after a
//!   mutating call (project size in pixels, so the GUI can place x/y moves on the viewer),
//!   {"type":"frame_rendered","png_b64"|"jpg_b64"} when a call writes a small frame image.
use kimo_core::commands::Session;
use kimo_core::toolgroups;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The spectator shell's loopback ports (crates/kimo-gui/src/main.rs binds the first free one): the default sink scans them.
pub const GUI_PORTS: std::ops::RangeInclusive<u16> = 7421..=7430;
/// The line the spectator shell sends first on every connection it accepts; the default sink keeps only a peer that sends it.
pub const HELLO: &str = "KIMO-SPECTATOR/1";
/// How long the default sink waits for that hello from a peer on one of `GUI_PORTS` before trying the next port.
const HELLO_WAIT: Duration = Duration::from_millis(250);
/// Queued state-telemetry bytes past which emitting waits for the writer (bounded backpressure, never a drop).
/// Far above any burst a live spectator falls behind by: 8,000 calls against a spectator stalled for 3 s queue a few MB.
pub const STATE_BOUND: usize = 64 << 20;
/// Live canvas frames waiting for the writer: at most this many, the oldest dropped first.
pub const FRAME_DEPTH: usize = 2;
/// Largest image file a call wrote that is forwarded to the viewer as its `frame_rendered` (`frame_event`); a bigger
/// one is reported as `frame_skipped` with `skip_reason: "oversize_agent_frame"`.
pub const AGENT_FRAME_MAX: u64 = 1_500_000;
/// Longest `flush` keeps waiting past its own bound while the spectator is still connected.
const FLUSH_CEIL: Duration = Duration::from_secs(15);
const STR_CAP: usize = 240;
const ARR_CAP: usize = 8;
const DEPTH_CAP: usize = 4;
const DIFF_CAP: usize = 24;

/// A state-lane line: plain state telemetry, or a spectator demo frame whose delivery the writer reports back by its
/// sequence number (preview.rs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind { State, Demo(u64) }

/// The two lanes. `seq` orders lines across both (the order they were emitted); `writing`: a line is in the writer's
/// hands. `state_bytes` counts queued state lines only (`STATE_BOUND`).
/// A frame-lane line carries its frame's (call_id, rev, gen), so a drop can be reported for that call.
struct Lanes { state: VecDeque<(u64, String, Kind)>, state_bytes: usize, frames: VecDeque<(u64, String, (String, u64, u64))>, seq: u64, writing: bool }
static LANES: Mutex<Lanes> = Mutex::new(Lanes { state: VecDeque::new(), state_bytes: 0, frames: VecDeque::new(), seq: 0, writing: false });
/// Signals both ways: new lines for the writer, freed room and an idle writer for whoever waits on it.
static LANES_CV: Condvar = Condvar::new();
#[derive(Debug, Clone, PartialEq)]
pub enum Dest { Tcp(String), File(String), Gui }

/// Whether telemetry is on (a sink was named and its writer started).
static SINK: OnceLock<bool> = OnceLock::new();
static SEQ: AtomicU64 = AtomicU64::new(0);
static REV: AtomicU64 = AtomicU64::new(0);
/// Live frames pushed off the latest-only frame lane before the writer took them, since the process started.
static FRAME_LANE_DROPS: AtomicU64 = AtomicU64::new(0);
/// Events serialized to a line so far (`ser`): stays put while nobody is listening (`listening`).
static SERIALIZED: AtomicU64 = AtomicU64::new(0);
/// True while the writer holds a live connection to the sink (a file sink counts once opened). Read by the live
/// preview (preview.rs), which renders nothing while no spectator is listening.
static CONNECTED: AtomicBool = AtomicBool::new(false);
/// The writer has tried to reach the sink at least once (so `CONNECTED` means something).
static ATTEMPTED: AtomicBool = AtomicBool::new(false);
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

struct Active { call_id: String, rpc_id: Value, tool: String, target: Value, t0: Instant, snap: Snap, last_progress: Option<(Instant, f64)>, last_frame: Option<(Instant, u64)>, frames_t0: Option<(Instant, u64)>, args: Value }

/// Parse an explicit `KIMO_SPECTATOR` value. None = telemetry off (empty, `0`, `off`, unparsable or non-loopback);
/// `init` applies the default (`dest_or_default`) for unset or empty.
pub fn parse_dest(spec: &str) -> Option<Dest> {
    let s = spec.trim();
    if s.is_empty() || s == "0" || s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("false") { return None; }
    if ["1", "on", "true", "gui"].iter().any(|k| s.eq_ignore_ascii_case(k)) { return Some(Dest::Gui); }
    if let Some(p) = s.strip_prefix("file://").or_else(|| s.strip_prefix("file:")) { return (!p.is_empty()).then(|| Dest::File(p.into())); }
    let addr = s.strip_prefix("tcp://").unwrap_or(s).trim_end_matches('/');
    use std::net::ToSocketAddrs;
    let all: Vec<_> = addr.to_socket_addrs().ok()?.collect();
    (!all.is_empty() && all.iter().all(|a| a.ip().is_loopback())).then(|| Dest::Tcp(addr.into()))
}

/// The sink for a `KIMO_SPECTATOR` value with the default applied: unset or empty = the spectator GUI (`Dest::Gui`,
/// 127.0.0.1:7421..=7430 with the hello); anything else as `parse_dest` reads it (`off`, `0`, `false` = telemetry off).
pub fn dest_or_default(raw: &str) -> Option<Dest> {
    if raw.trim().is_empty() { Some(Dest::Gui) } else { parse_dest(raw) }
}

/// Read `KIMO_SPECTATOR` once and, when there is a sink (by default there is), start the writer thread and install the
/// engine's progress hook.
pub fn init() {
    let raw = std::env::var("KIMO_SPECTATOR").unwrap_or_default();
    let dest = dest_or_default(&raw);
    // a value it cannot use (not loopback, unparsable) means telemetry off, quietly: kimo-mcp's stdio belongs to the
    // MCP client, and nothing about telemetry is ever printed
    init_with(dest);
}

fn init_with(dest: Option<Dest>) {
    let sink = dest.and_then(|d| {
        // The telemetry writer thread. Purpose: drain the two event lanes to the sink so the request loop never blocks on I/O.
        // Lifecycle: started at most once per process (init runs once, SINK is a OnceLock) and only when there is a sink (by default);
        // it reconnects with backoff and lives until the process exits (SINK, which owns the sender, is never dropped). Join/cancel
        // owner: detached by design, nothing joins it; `flush` gives a bounded best-effort drain before exit. With nobody listening it
        // drops nothing itself (emitters drop before serializing) and retries on its backoff timer. KIMO_SPECTATOR=off: never started. Started through the engine's guarded spawn path; `init` runs at startup with no cancellation
        // token installed, so the writer inherits none and no tool call's time limit can cancel it.
        kimo_core::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-spectator".into()), move || writer(d)).ok()
    });
    let on = sink.is_some();
    if SINK.set(on).is_ok() && on {
        kimo_core::telemetry::set_progress_hook(Box::new(on_progress));
        since_start();
        let p0 = kimo_core::model::Project::default(); // the project Session::new() starts with
        emit(&json!({"type": "session", "reset": true, "width": p0.width, "height": p0.height, "agent": format!("kimo-mcp {}", kimo_core::buildinfo::VERSION), "read_only": true, "elapsed_s": 0, "ts_ms": ts_ms()}));
    }
}

#[inline]
pub fn enabled() -> bool { SINK.get() == Some(&true) }

/// The sink has a live listener right now.
#[inline]
pub fn connected() -> bool { CONNECTED.load(Ordering::Relaxed) }

/// Events are worth building: a spectator is connected, or the writer has not tried the sink yet (startup). False
/// while the writer has found nobody: emitters then drop their event before serializing it.
#[inline]
fn listening() -> bool { connected() || !ATTEMPTED.load(Ordering::SeqCst) }

/// `connected`, once the writer has made its first attempt to reach the sink (it does at startup, on the session
/// event): waits for that attempt for at most `within`. Spectator demo mode only, so that the first calls of a
/// session are not taken for calls with nobody listening.
pub(crate) fn connected_settled(within: Duration) -> bool {
    let t0 = Instant::now();
    while enabled() && !ATTEMPTED.load(Ordering::SeqCst) && t0.elapsed() < within { std::thread::sleep(Duration::from_millis(1)); }
    connected()
}

/// A live preview canvas frame from the preview thread: its own latest-only lane (`FRAME_DEPTH`, oldest dropped first),
/// so frames never displace, drop or hold up state telemetry, and never wait themselves. The frame carries
/// `frame_lane_drops` (counting the drop this push causes); each frame it pushes off is reported on the state lane.
pub(crate) fn emit_frame(mut v: Value) {
    if !enabled() || !listening() { return; }
    let meta = (v["call_id"].as_str().unwrap_or("").to_string(), v["rev"].as_u64().unwrap_or(0), v["gen"].as_u64().unwrap_or(0));
    let mut q = LANES.lock().unwrap_or_else(|e| e.into_inner());
    let drops = (q.frames.len() + 1).saturating_sub(FRAME_DEPTH.max(1)) as u64;
    v["frame_lane_drops"] = json!(FRAME_LANE_DROPS.fetch_add(drops, Ordering::Relaxed) + drops);
    q.seq += 1;
    let s = q.seq;
    let dropped = push_latest(&mut q.frames, (s, ser(&v), meta), FRAME_DEPTH);
    drop(q);
    LANES_CV.notify_all();
    for (_, _, (call_id, rev, gen)) in dropped { emit_skip(&crate::preview::skip_event(crate::preview::SKIP_FRAME_LANE_DROPPED, &call_id, rev, gen)); }
}

/// Live frames dropped from the frame lane so far (`frame_lane_drops`).
pub(crate) fn frame_lane_drops() -> u64 { FRAME_LANE_DROPS.load(Ordering::Relaxed) }

/// A `frame_skipped` (preview.rs): state telemetry, lossless and ordered like every other state event.
pub(crate) fn emit_skip(v: &Value) { emit(v); }

/// A spectator demo frame (preview.rs): on the lossless state lane, in order; the writer reports its delivery by `seq`
/// (`preview::demo_delivered`). False: telemetry is off, and nobody will report it.
pub(crate) fn emit_demo_frame(v: &Value, seq: u64) -> bool { listening() && enqueue(ser(v), Kind::Demo(seq)) }

pub(crate) fn now_ms() -> u64 { ts_ms() }

/// State telemetry: queued in order, never coalesced, never dropped, never displaced by frames, while a spectator is
/// listening; with nobody listening it is dropped here, before it is serialized.
fn emit(v: &Value) { if listening() { enqueue(ser(v), Kind::State); } }

/// An event serialized to its line (counted in `SERIALIZED`).
fn ser(v: &Value) -> String { SERIALIZED.fetch_add(1, Ordering::Relaxed); v.to_string() }

/// The frame lane's rule: append, then drop the oldest past `depth`. Returns what it dropped, oldest first.
pub(crate) fn push_latest<T>(q: &mut VecDeque<T>, item: T, depth: usize) -> Vec<T> {
    q.push_back(item);
    let mut dropped = Vec::new();
    while q.len() > depth.max(1) { dropped.extend(q.pop_front()); }
    dropped
}

/// The state lane's bound: a line of `len` bytes waits while `queued` bytes are already waiting and both together
/// pass `STATE_BOUND`, and only while a spectator is connected (with nobody listening the writer drops lines at once,
/// so waiting would only slow the call down). One line always fits an empty lane.
pub(crate) fn must_wait(queued: usize, len: usize, connected: bool) -> bool {
    connected && queued > 0 && queued.saturating_add(len) > STATE_BOUND
}

/// The lane the writer serves next: whichever head was emitted first (state wins a tie). `true` = the frame lane.
pub(crate) fn frame_first(state_head: Option<u64>, frame_head: Option<u64>) -> Option<bool> {
    match (state_head, frame_head) {
        (Some(a), Some(b)) => Some(b < a),
        (Some(_), None) => Some(false),
        (None, Some(_)) => Some(true),
        (None, None) => None,
    }
}

fn enqueue(line: String, kind: Kind) -> bool {
    if !enabled() { return false; }
    let n = line.len();
    let mut q = LANES.lock().unwrap_or_else(|e| e.into_inner());
    while must_wait(q.state_bytes, n, connected()) {
        q = LANES_CV.wait_timeout(q, Duration::from_millis(50)).unwrap_or_else(|e| e.into_inner()).0;
    }
    q.seq += 1;
    let s = q.seq;
    q.state_bytes += n;
    q.state.push_back((s, line, kind));
    drop(q);
    LANES_CV.notify_all();
    true
}

/// Drain before the process exits: returns once both lanes are empty and the writer is idle. Bounded by `within`,
/// extended (up to `FLUSH_CEIL`) only while the spectator is still connected, so a slow spectator still gets the end
/// of a burst and a gone one costs at most `within`.
pub fn flush(within: Duration) {
    if !enabled() { return; }
    let t0 = Instant::now();
    let mut q = LANES.lock().unwrap_or_else(|e| e.into_inner());
    while !(q.state.is_empty() && q.frames.is_empty() && !q.writing) {
        let el = t0.elapsed();
        if el >= within && (el >= FLUSH_CEIL || !connected()) { return; }
        q = LANES_CV.wait_timeout(q, Duration::from_millis(20)).unwrap_or_else(|e| e.into_inner()).0;
    }
}

/// A live connection to the sink.
enum Conn { File(std::fs::File), Tcp(std::net::TcpStream) }

impl Conn {
    /// False once the peer has closed its end (the spectator window went away): a non-blocking peek reads EOF.
    /// Checked before each write so a closed GUI is noticed at once instead of one event later.
    fn alive(&mut self) -> bool {
        match self { Conn::File(_) => true, Conn::Tcp(s) => peer_open(s) }
    }
    /// One whole line. A spectator that is open but not reading (its socket buffer is full) is waited for: a write
    /// that times out is retried for as long as the peer is open, so a slow GUI loses nothing and a live connection
    /// never carries a line cut in half. A line is only ever cut when the peer itself has closed (nobody reads it).
    fn write_line(&mut self, l: &[u8]) -> std::io::Result<()> {
        match self {
            Conn::File(f) => f.write_all(l),
            Conn::Tcp(s) => {
                let mut off = 0;
                while off < l.len() {
                    match s.write(&l[off..]) {
                        Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
                        Ok(n) => off += n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                            if !peer_open(s) { return Err(e); }
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
        }
    }
}

/// A connection to the spectator shell on 127.0.0.1:`port`: kept only when the peer's first line, within `HELLO_WAIT`, is
/// `HELLO`. Anything else (a refused port, a foreign app that stays silent or says something else) is closed unwritten.
/// Reads byte by byte so nothing past the hello is consumed.
fn greeted(port: u16) -> Option<std::net::TcpStream> {
    use std::io::Read;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(250)).ok()?;
    s.set_read_timeout(Some(HELLO_WAIT)).ok()?;
    let (t0, mut line, mut b) = (Instant::now(), Vec::new(), [0u8; 1]);
    while line.len() <= HELLO.len() && t0.elapsed() < HELLO_WAIT {
        match s.read(&mut b) { Ok(1) if b[0] == b'\n' => break, Ok(1) => line.push(b[0]), _ => return None }
    }
    if line.strip_suffix(b"\r").unwrap_or(&line) != HELLO.as_bytes() { return None; }
    s.set_read_timeout(None).ok()?;
    let _ = s.set_nodelay(true);
    let _ = s.set_write_timeout(Some(Duration::from_millis(250)));
    Some(s)
}

/// False once the peer has closed its end: a non-blocking peek reads EOF (or fails).
fn peer_open(s: &std::net::TcpStream) -> bool {
    if s.set_nonblocking(true).is_err() { return false; }
    let mut b = [0u8; 1];
    let ok = match s.peek(&mut b) {
        Ok(0) => false,
        Ok(_) => true,
        Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
    };
    s.set_nonblocking(false).is_ok() && ok
}

fn open(dest: &Dest) -> Option<Conn> {
    match dest {
        Dest::Gui => GUI_PORTS.clone().find_map(|p| greeted(p).map(Conn::Tcp)),
        Dest::File(p) => std::fs::OpenOptions::new().create(true).append(true).open(p).ok().map(Conn::File),
        Dest::Tcp(a) => {
            use std::net::ToSocketAddrs;
            let addr = a.to_socket_addrs().ok()?.find(|x| x.ip().is_loopback())?;
            let s = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(250)).ok()?;
            let _ = s.set_nodelay(true);
            let _ = s.set_write_timeout(Some(Duration::from_millis(250)));
            Some(Conn::Tcp(s))
        }
    }
}

/// Reconnect pacing: 50 ms after the first failure, doubling to a 1000 ms ceiling, back to 50 ms once connected.
/// The wait is a timestamp the writer compares, never a sleep: events arriving inside it are dropped at once.
#[derive(Debug)]
pub struct Backoff { next: Duration }
impl Backoff {
    pub const FIRST: Duration = Duration::from_millis(50);
    pub const CEIL: Duration = Duration::from_millis(1000);
    pub fn new() -> Self { Backoff { next: Self::FIRST } }
    /// A connect or write failed: how long to wait before the next attempt.
    pub fn fail(&mut self) -> Duration { let d = self.next; self.next = (self.next * 2).min(Self::CEIL); d }
    /// Connected: start over.
    pub fn reset(&mut self) { self.next = Self::FIRST; }
}
impl Default for Backoff { fn default() -> Self { Self::new() } }

/// Background writer, the only place that touches the sink. It connects lazily when an event arrives, with
/// `Backoff` between failed attempts; a write error or a peer that closed drops the connection and the event.
/// Events that arrive while disconnected (or inside a backoff wait) are dropped, never queued for later.
/// Nothing here can panic or reach the dispatch thread, which only ever appends to a lane (`enqueue`, `emit_frame`).
fn writer(dest: Dest) {
    let mut conn: Option<Conn> = None;
    let mut backoff = Backoff::new();
    let mut retry_at = Instant::now();
    loop {
        // the next line in emission order across both lanes; the writer is marked busy until it is handled. While nobody
        // is connected it also wakes on the reconnect timer (None), since emitters queue nothing then.
        let next = {
            let mut q = LANES.lock().unwrap_or_else(|e| e.into_inner());
            q.writing = false;
            loop {
                match frame_first(q.state.front().map(|x| x.0), q.frames.front().map(|x| x.0)) {
                    Some(true) => { let (_, l, _) = q.frames.pop_front().unwrap(); q.writing = true; break Some((l, None)); }
                    Some(false) => { let (_, l, k) = q.state.pop_front().unwrap(); q.state_bytes -= l.len(); q.writing = true; break Some((l, Some(k))); }
                    None => {
                        LANES_CV.notify_all();
                        if conn.is_some() { q = LANES_CV.wait(q).unwrap_or_else(|e| e.into_inner()); continue; }
                        let now = Instant::now();
                        if now >= retry_at { break None; }
                        q = LANES_CV.wait_timeout(q, retry_at - now).unwrap_or_else(|e| e.into_inner()).0;
                    }
                }
            }
        };
        LANES_CV.notify_all(); // room on the state lane for an emitter that waits on the bound
        if conn.as_mut().map_or(false, |c| !c.alive()) {
            // reconnect at once; `CONNECTED` stays set until that attempt fails, so the emitters do not drop the
            // events of a call that straddles the move to a newly opened spectator
            conn = None;
            retry_at = Instant::now();
        }
        if conn.is_none() && Instant::now() >= retry_at {
            let first = !ATTEMPTED.load(Ordering::SeqCst);
            conn = open(&dest);
            if conn.is_some() {
                backoff.reset();
                CONNECTED.store(true, Ordering::Relaxed);
                crate::preview::reconnected(first); // a spectator that just (re)opened gets the current frame
            } else { CONNECTED.store(false, Ordering::Relaxed); retry_at = Instant::now() + backoff.fail(); }
            if first { ATTEMPTED.store(true, Ordering::SeqCst); }
        }
        let Some((mut l, kind)) = next else { continue };
        if let Some(c) = conn.as_mut() {
            l.push('\n');
            if c.write_line(l.as_bytes()).is_err() { conn = None; CONNECTED.store(false, Ordering::Relaxed); retry_at = Instant::now() + backoff.fail(); }
        }
        if let Some(Kind::Demo(seq)) = kind { crate::preview::demo_delivered(seq); }
    }
}

fn ts_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

fn new_call_id() -> String {
    static BOOT: OnceLock<u64> = OnceLock::new();
    let b = *BOOT.get_or_init(|| (ts_ms() ^ (std::process::id() as u64) << 20) & 0xff_ffff);
    format!("k{b:06x}-{}", SEQ.fetch_add(1, Ordering::Relaxed) + 1)
}

/// Bounded copy of a value for display: long strings cut, long arrays cut with a count, deep nesting elided,
/// base64-looking blobs replaced by their size. Never used for anything but telemetry.
pub fn summarize(v: &Value) -> Value { sum(v, 0) }
fn sum(v: &Value, d: usize) -> Value {
    match v {
        Value::String(s) if s.len() > 512 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=\n".contains(&b)) => json!(format!("<{} bytes>", s.len())),
        Value::String(s) if s.chars().count() > STR_CAP => json!(format!("{}…", s.chars().take(STR_CAP).collect::<String>())),
        Value::Array(a) if d >= DEPTH_CAP => json!(format!("[{} items]", a.len())),
        Value::Object(o) if d >= DEPTH_CAP => json!(format!("{{{} keys}}", o.len())),
        Value::Array(a) => {
            let mut out: Vec<Value> = a.iter().take(ARR_CAP).map(|x| sum(x, d + 1)).collect();
            if a.len() > ARR_CAP { out.push(json!(format!("…+{}", a.len() - ARR_CAP))); }
            Value::Array(out)
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), sum(x, d + 1))).collect()),
        _ => v.clone(),
    }
}

fn str_of<'a>(a: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| match a.get(*k) { Some(Value::String(s)) if !s.is_empty() => Some(s.as_str()), Some(Value::Array(v)) => v.first().and_then(|x| x.as_str()), _ => None })
}

/// Best-effort target from tool arguments: track_id / clip_id / param (null when the call names none).
pub fn target_of(args: &Value) -> Value {
    let track = str_of(args, &["track_id", "track", "track_ids"]);
    let clip = str_of(args, &["clip_id", "clip", "clip_ids", "clip_a", "target_clip"]);
    let param = str_of(args, &["param", "property", "prop", "key", "parameter", "effect", "transition"]);
    json!({"track_id": track, "clip_id": clip, "param": param})
}

type ClipPos = (String, f64, f64); // clip id, start, end (seconds)

#[derive(Default)]
struct Snap { ids: Vec<(String, Vec<ClipPos>)>, clip: Option<Value>, duration: f64, undo: usize, w: u32, h: u32 }

fn snapshot(ss: &Arc<Mutex<Session>>, clip_id: Option<&str>) -> Snap {
    let g = ss.lock().unwrap_or_else(|e| e.into_inner());
    let p = &g.project;
    let ids = p.tracks.iter().map(|t| (t.id.clone(), t.clips.iter().map(|c| (c.id.clone(), c.start, c.start + c.duration)).collect())).collect();
    let clip = clip_id.and_then(|id| p.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == id)).and_then(|c| serde_json::to_value(&**c).ok());
    Snap { ids, clip, duration: p.duration(), undo: g.undo.len(), w: p.width, h: p.height }
}

impl Snap {
    fn find(&self, clip: &str) -> Option<(&str, f64, f64)> {
        self.ids.iter().find_map(|(t, cs)| cs.iter().find(|c| c.0 == clip).map(|c| (t.as_str(), c.1, c.2)))
    }
}

/// A short display label for a clip: its text, else its source file name, else its id.
fn clip_label(clip: &Value) -> Option<String> {
    let src = clip.get("source")?;
    let pick = |v: &Value| -> Option<String> {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) { return Some(t.chars().take(48).collect()); }
        v.get("path").or_else(|| v.get("asset_id")).and_then(|x| x.as_str()).map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
    };
    pick(src).or_else(|| src.as_object().and_then(|o| o.values().find_map(pick)))
}

/// Dotted paths to leaf values; arrays of objects carrying an `id` (effects, masks, keys with ids) are keyed by
/// that id, so `effects[fx5].params.amount.base` names one effect parameter however the list is ordered.
fn flatten(prefix: &str, v: &Value, d: usize, out: &mut Map<String, Value>) {
    let join = |k: &str| if prefix.is_empty() { k.to_string() } else { format!("{prefix}.{k}") };
    match v {
        Value::Object(o) if d < 6 => for (k, x) in o { flatten(&join(k), x, d + 1, out) },
        Value::Array(a) if d < 6 && !a.is_empty() && a.iter().all(|x| x.get("id").and_then(|i| i.as_str()).is_some()) =>
            for x in a { flatten(&format!("{prefix}[{}]", x["id"].as_str().unwrap_or("")), x, d + 1, out) },
        _ => { out.insert(prefix.to_string(), v.clone()); }
    }
}

/// What a call changed: clips/tracks added, removed or moved, duration, and per-field before/after of the target clip.
fn diff(a: &Snap, b: &Snap) -> Value {
    let mut out = Map::new();
    let all = |s: &Snap| s.ids.iter().flat_map(|(t, cs)| cs.iter().map(move |c| (t.clone(), c.0.clone()))).collect::<Vec<_>>();
    let (ca, cb) = (all(a), all(b));
    let added: Vec<Value> = cb.iter().filter(|x| !ca.iter().any(|y| y.1 == x.1)).take(ARR_CAP).map(|(t, c)| json!({"clip_id": c, "track_id": t})).collect();
    let removed: Vec<Value> = ca.iter().filter(|x| !cb.iter().any(|y| y.1 == x.1)).take(ARR_CAP).map(|(t, c)| json!({"clip_id": c, "track_id": t})).collect();
    let moved: Vec<Value> = cb.iter().filter_map(|(t, c)| ca.iter().find(|y| &y.1 == c && &y.0 != t).map(|y| json!({"clip_id": c, "from": y.0, "to": t}))).take(ARR_CAP).collect();
    let ta: Vec<&String> = a.ids.iter().map(|x| &x.0).collect(); let tb: Vec<&String> = b.ids.iter().map(|x| &x.0).collect();
    let tadd: Vec<&String> = tb.iter().filter(|x| !ta.contains(x)).copied().collect(); let trem: Vec<&String> = ta.iter().filter(|x| !tb.contains(x)).copied().collect();
    if !added.is_empty() { out.insert("clips_added".into(), json!(added)); }
    if !removed.is_empty() { out.insert("clips_removed".into(), json!(removed)); }
    if !moved.is_empty() { out.insert("clips_moved".into(), json!(moved)); }
    if !tadd.is_empty() { out.insert("tracks_added".into(), json!(tadd)); }
    if !trem.is_empty() { out.insert("tracks_removed".into(), json!(trem)); }
    if (a.duration - b.duration).abs() > 1e-9 { out.insert("duration".into(), json!([a.duration, b.duration])); }
    if let (Some(x), Some(y)) = (&a.clip, &b.clip) {
        let (mut fa, mut fb) = (Map::new(), Map::new()); flatten("", x, 0, &mut fa); flatten("", y, 0, &mut fb);
        let mut fields = Map::new();
        for (k, vb) in &fb { if fields.len() >= DIFF_CAP { break; } let va = fa.get(k).unwrap_or(&Value::Null); if va != vb { fields.insert(k.clone(), json!([summarize(va), summarize(vb)])); } }
        for (k, va) in &fa { if fields.len() >= DIFF_CAP { break; } if !fb.contains_key(k) { fields.insert(k.clone(), json!([summarize(va), Value::Null])); } }
        if !fields.is_empty() { out.insert("clip".into(), Value::Object(fields)); }
    }
    Value::Object(out)
}

/// The spectator router's vocabulary (from/to, clip/track/s/e/label, at, effect, on, text), filled from what the
/// engine state and the arguments actually show. Nothing is invented: a key is set only when its source exists.
fn vocabulary(p: &mut Value, tool: &str, args: &Value, target: &Value, changed: &Value, after: &Snap, ss: &Arc<Mutex<Session>>) {
    let num = |k: &str| args.get(k).and_then(|v| v.as_f64());
    // a field change on the target clip -> from/to
    if let Some(fields) = changed.get("clip").and_then(|c| c.as_object()) {
        let param = target["param"].as_str();
        let hit = param.and_then(|pa| fields.iter().find(|(k, _)| k.as_str() == pa || k.ends_with(&format!(".{pa}")) || k.starts_with(&format!("props.{pa}.")) || k.contains(&format!(".{pa}."))))
            .or_else(|| (fields.len() == 1).then(|| fields.iter().next().unwrap()));
        if let Some((k, ft)) = hit {
            p["from"] = ft[0].clone(); p["to"] = ft[1].clone();
            if param.is_none() { p["param"] = json!(k.rsplit_once(".base").map(|x| x.0).unwrap_or(k).trim_start_matches("props.")); }
        }
    }
    if p.get("to").is_none() { if let Some(v) = args.get("value").filter(|v| v.is_number() || v.is_string() || v.is_boolean()) { p["value"] = v.clone(); } }
    // an insert -> clip/track/s/e/label
    if let Some(first) = changed.get("clips_added").and_then(|a| a.get(0)) {
        let cid = first["clip_id"].as_str().unwrap_or("");
        if let Some((t, s0, e0)) = after.find(cid) {
            p["clip"] = json!(cid); p["track"] = json!(t); p["s"] = json!(s0); p["e"] = json!(e0);
            let g = ss.lock().unwrap_or_else(|e| e.into_inner());
            let label = g.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == cid).and_then(|c| serde_json::to_value(&**c).ok()).and_then(|v| clip_label(&v));
            p["label"] = json!(label.unwrap_or_else(|| cid.to_string()));
        }
    } else if let Some((_, s0, e0)) = target["clip_id"].as_str().and_then(|c| after.find(c)) {
        // the target clip's bounds after the call (trim / move / split read these)
        p["s"] = json!(s0); p["e"] = json!(e0);
    }
    if let Some(at) = num("time").or_else(|| num("at")).or_else(|| num("t")) { if tool.contains("split") || tool.contains("cut") || tool.contains("blade") || tool.contains("razor") { p["at"] = json!(at); } }
    if let Some(fx) = str_of(args, &["effect", "kind", "transition"]) { p["effect"] = json!(fx); }
    if tool.starts_with("mute") || tool.starts_with("solo") || tool.contains("_mute") || tool.contains("_solo") {
        if let Some(on) = ["on", "muted", "mute", "solo", "value", "enabled"].iter().find_map(|k| args.get(*k).and_then(|v| v.as_bool())) { p["on"] = json!(on); }
    }
    if let Some(t) = args.get("text").and_then(|v| v.as_str()) {
        p["text"] = json!(t.chars().take(STR_CAP).collect::<String>());
        if p.get("s").is_none() { if let Some(s0) = num("start") { p["s"] = json!(s0); if let Some(d) = num("duration") { p["e"] = json!(s0 + d); } } }
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 { o.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' }); }
    }
    o
}

/// A frame image a call wrote (render_frame, previews): forwarded to the viewer when it is small enough
/// (`AGENT_FRAME_MAX`); `Err(bytes)`: an image over it, which is not forwarded.
fn frame_event(call_id: &str, args: &Value, res: &Value) -> Option<Result<Value, u64>> {
    let path = res.get("path").and_then(|v| v.as_str())?;
    let lower = path.to_ascii_lowercase();
    let key = if lower.ends_with(".png") { "png_b64" } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "jpg_b64" } else { return None };
    let size = std::fs::metadata(path).ok()?.len();
    if size > AGENT_FRAME_MAX { return Some(Err(size)); }
    let bytes = std::fs::read(path).ok()?;
    let mut ev = json!({"type": "frame_rendered", "call_id": call_id, "ts_ms": ts_ms()});
    ev[key] = json!(base64(&bytes));
    if let Some(t) = args.get("time").and_then(|v| v.as_f64()) { ev["tc"] = json!(format!("{t:.2}s")); }
    Some(Ok(ev))
}

fn since_start() -> f64 {
    static T0: OnceLock<Instant> = OnceLock::new();
    (T0.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0).round() / 1000.0
}

fn event(a: &Active, phase: &str, payload: Value) -> Value {
    json!({"tool_name": a.tool, "call_id": a.call_id, "rpc_id": a.rpc_id, "phase": phase, "target": a.target, "payload": payload, "ts": since_start(), "ts_ms": ts_ms()})
}

/// A tools/call is about to run. Off: returns at once.
pub fn call_start(name: &str, args: &Value, rpc_id: Option<&Value>, ss: &Arc<Mutex<Session>>) {
    if !enabled() { return; }
    let (tool, eff, group) = if toolgroups::is_group(name) {
        match toolgroups::resolve_group_call(name, args) { Ok((a, ia)) => (a, ia, Some(name)), Err(_) => (name.to_string(), args.clone(), None) }
    } else { (name.to_string(), args.clone(), None) };
    let mut target = target_of(&eff);
    let snap = snapshot(ss, target["clip_id"].as_str());
    if target["track_id"].is_null() {
        if let Some((t, _, _)) = target["clip_id"].as_str().and_then(|c| snap.find(c)) { target["track_id"] = json!(t); }
    }
    let a = Active { call_id: new_call_id(), rpc_id: rpc_id.cloned().unwrap_or(Value::Null), tool, target, t0: Instant::now(), snap, last_progress: None, last_frame: None, frames_t0: None, args: eff };
    if listening() {
        let mut ev = event(&a, "start", json!({"args": summarize(&a.args)}));
        if let Some(g) = group { ev["group"] = json!(g); }
        emit(&ev);
    }
    *ACTIVE.lock().unwrap_or_else(|e| e.into_inner()) = Some(a);
    crate::preview::call_started();
}

/// Progress from the engine (kimo_core::telemetry) for the call in flight; throttled to ~25/s (the first and last
/// report always go out). Frame-counting tools (export) add `current_frame` (frames finished = the next frame's
/// index), `total_frames`, `fps` (instantaneous: frames finished since the previous report over the wall-clock
/// between them; 0 on the opening report) and `eta_ms` (remaining frames at the average rate since the first frame
/// report; null until a frame has finished, 0 at the end).
fn on_progress(p: &kimo_core::telemetry::Progress) {
    let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(a) = g.as_mut() else { return };
    let now = Instant::now();
    let frac = p.frac;
    // rate bookkeeping on every report (not only the emitted ones), so throttling never skews it
    let mut fps = 0.0;
    let mut eta_ms: Option<u64> = None;
    if let (Some(f), Some(n)) = (p.frame, p.total_frames) {
        if let Some((t, f0)) = a.last_frame { let dt = now.duration_since(t).as_secs_f64(); if f > f0 && dt > 0.0 { fps = (f - f0) as f64 / dt; } }
        if a.last_frame.map_or(true, |(_, f0)| f > f0) { a.last_frame = Some((now, f)); }
        let (t0, f0) = *a.frames_t0.get_or_insert((now, f));
        if f >= n { eta_ms = Some(0); }
        else if f > f0 { let per = now.duration_since(t0).as_secs_f64() / (f - f0) as f64; eta_ms = Some(((n - f) as f64 * per * 1000.0).round() as u64); }
    }
    if let Some((t, f)) = a.last_progress { if frac < 1.0 && (now.duration_since(t) < Duration::from_millis(40) || frac - f < 0.002) { return; } }
    a.last_progress = Some((now, frac));
    if !listening() { return; }
    let el = now.duration_since(a.t0).as_millis() as u64;
    let mut payload = json!({"pct": (frac * 1000.0).round() / 10.0, "progress": frac, "stage": p.stage, "elapsed_ms": el});
    if let (Some(f), Some(n)) = (p.frame, p.total_frames) {
        payload["current_frame"] = json!(f);
        payload["total_frames"] = json!(n);
        payload["fps"] = json!((fps * 10.0).round() / 10.0);
        payload["eta_ms"] = json!(eta_ms);
    }
    emit(&event(a, "progress", payload));
}

fn round1(ms: f64) -> f64 { (ms * 10.0).round() / 10.0 }

/// The call finished with this MCP `result` object (read only).
pub fn call_end(result: &Value, ss: &Arc<Mutex<Session>>) {
    if !enabled() { return; }
    let Some(a) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    let dur_ms = round1(a.t0.elapsed().as_secs_f64() * 1000.0);
    let is_err = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
    let text = result.get("content").and_then(|c| c.get(0)).and_then(|c| c.get("text")).and_then(|t| t.as_str()).unwrap_or("");
    if is_err {
        let sc = result.get("structuredContent");
        let msg = summarize(&json!(text));
        let mut p = json!({"dur_ms": dur_ms, "error": msg});
        for k in ["code", "param", "hint"] { if let Some(v) = sc.and_then(|s| s.get(k)).filter(|v| !v.is_null()) { p[k] = v.clone(); } }
        let mut ev = event(&a, "error", p); ev["duration_ms"] = json!(dur_ms);
        emit(&ev);
        crate::preview::call_finished(ss, &a.call_id, REV.load(Ordering::Relaxed), false, crate::preview::Hint::None, a.snap.duration, connected());
        crate::preview::demo_frame(&a.call_id, &a.rpc_id, &a.tool);
        return;
    }
    let after = snapshot(ss, a.target["clip_id"].as_str());
    let changed = diff(&a.snap, &after);
    let mutated = after.undo != a.snap.undo || (after.w, after.h) != (a.snap.w, a.snap.h) || changed.as_object().map_or(false, |o| !o.is_empty());
    if !listening() {
        // nobody listening: no result parsing, no event, no agent frame; keep what a spectator that connects later needs
        if mutated { REV.fetch_add(1, Ordering::Relaxed); }
        let hint = playhead_hint(&a.args, &a.target, &changed, &after);
        crate::preview::call_finished(ss, &a.call_id, REV.load(Ordering::Relaxed), mutated || declared_mutating(&a.tool), hint, after.duration, false);
        crate::preview::demo_frame(&a.call_id, &a.rpc_id, &a.tool);
        return;
    }
    let res = serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text));
    let images = result.get("content").and_then(|c| c.as_array()).map_or(0, |c| c.iter().filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("image")).count());
    let mut p = json!({"dur_ms": dur_ms, "result": summarize(&res), "changed": changed, "mutated": mutated});
    if images > 0 { p["images"] = json!(images); }
    if let Some(n) = output_bytes(&a.tool, &res) { p["bytes"] = json!(n); if p["result"].is_object() { p["result"]["bytes"] = json!(n); } }
    flash_stamps(&mut p, &a.tool, &res);
    let changed = p["changed"].clone();
    vocabulary(&mut p, &a.tool, &a.args, &a.target, &changed, &after, ss);
    let mut ev = event(&a, "complete", p); ev["duration_ms"] = json!(dur_ms);
    emit(&ev);
    let hint = playhead_hint(&a.args, &a.target, &changed, &after);
    match frame_event(&a.call_id, &a.args, &res) {
        Some(Ok(f)) => { emit(&f); crate::preview::shown_by_agent(&a.call_id); }
        // not forwarded: say so (the live preview, if one is pending, still renders)
        Some(Err(bytes)) => emit(&json!({"type": "frame_skipped", "agent": true, "skip_reason": crate::preview::SKIP_OVERSIZE_AGENT_FRAME,
            "call_id": a.call_id, "rev": REV.load(Ordering::Relaxed), "bytes": bytes, "max_bytes": AGENT_FRAME_MAX,
            "frame_lane_drops": frame_lane_drops(), "ts_ms": ts_ms()})),
        None => {}
    }
    if mutated { emit(&json!({"type": "project_changed", "rev": REV.fetch_add(1, Ordering::Relaxed) + 1, "call_id": a.call_id, "width": after.w, "height": after.h, "ts_ms": ts_ms()})); }
    // live preview: any call that changed the project (or is declared mutating) queues one frame at the playhead
    let live = mutated || declared_mutating(&a.tool);
    crate::preview::call_finished(ss, &a.call_id, REV.load(Ordering::Relaxed), live, hint, after.duration, connected());
    crate::preview::demo_frame(&a.call_id, &a.rpc_id, &a.tool);
}

/// Spectator demo mode only (preview.rs): after the response to a tools/call went out, hold the next request until this
/// call's own frame reached the spectator and the pace has passed. Headless (demo off) or nobody listening: returns at once.
pub fn pace() { if enabled() { crate::preview::demo_wait(); } }

/// Most stamps per series a flash_series complete event carries; past it `*_stamps_total` still counts them all.
const STAMP_CAP: usize = 64;

/// flash_series: its per-frame arrays are cut to `ARR_CAP` by `summarize`, so the GUI's Stamps row could never see a
/// transition past frame 8. Read the FULL result (before any capping) and add a compact `lum_stamps` / `red_stamps`
/// list, `[{t, dir}]` (dir 1 = up, -1 = down), plus `*_stamps_total`, beside `result` in the complete payload.
/// Telemetry only: the tool result on stdout and `summarize` are not touched.
fn flash_stamps(p: &mut Value, tool: &str, res: &Value) {
    if tool != "flash_series" { return; }
    let Some(f) = res.get("frames") else { return };
    let Some(t) = f.get("time").and_then(|v| v.as_array()) else { return };
    for (key, out) in [("lum_transition", "lum_stamps"), ("red_transition", "red_stamps")] {
        let Some(tr) = f.get(key).and_then(|v| v.as_array()) else { continue };
        let all: Vec<Value> = tr.iter().zip(t).filter_map(|(d, tt)| {
            let (d, tt) = (d.as_f64()?, tt.as_f64()?);
            (d != 0.0 && tt.is_finite()).then(|| json!({"t": tt, "dir": if d > 0.0 { 1 } else { -1 }}))
        }).collect();
        p[format!("{out}_total")] = json!(all.len());
        p[out] = Value::Array(all.into_iter().take(STAMP_CAP).collect());
    }
}

/// Size on disk of a finished export's output file, for the GUI's Renders history ("done · 18.4 MB"). Telemetry
/// only: the tool result on stdout is not touched. None for a path pattern (PNG sequences) or a missing file.
fn output_bytes(tool: &str, res: &Value) -> Option<u64> {
    if !(tool.starts_with("export") || tool == "join_export") { return None; }
    std::fs::metadata(res.get("path")?.as_str()?).ok().filter(|m| m.is_file()).map(|m| m.len())
}

/// The registry's own `mutates` flag for a tool (group calls arrive here already resolved to the tool).
fn declared_mutating(tool: &str) -> bool {
    kimo_core::commands::registry_ref().iter().find(|c| c.name == tool).map_or(false, |c| c.mutates)
}

/// Where the playhead goes after a call: an explicit `time`/`at`/`t` inside the touched clip (or with no clip
/// involved), else the touched clip (the one added, else the target), else it stays.
fn playhead_hint(args: &Value, target: &Value, changed: &Value, after: &Snap) -> crate::preview::Hint {
    use crate::preview::Hint;
    let explicit = ["time", "at", "t"].iter().find_map(|k| args.get(*k).and_then(|v| v.as_f64()));
    let clip = changed["clips_added"].get(0).and_then(|c| c["clip_id"].as_str()).or_else(|| target["clip_id"].as_str());
    match (clip.and_then(|c| after.find(c)), explicit) {
        (Some((_, s, e)), Some(t)) if t >= s - 1e-9 && t <= e + 1e-9 => Hint::At(t),
        (Some((_, s, e)), _) => Hint::Clip(s, e),
        (None, Some(t)) => Hint::At(t),
        (None, None) => Hint::None,
    }
}

/// The call in flight is being abandoned (hung worker; the process is about to exit). Emits its error and drains briefly.
pub fn call_abort(message: &str) {
    if !enabled() { return; }
    if let Some(a) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let dur_ms = round1(a.t0.elapsed().as_secs_f64() * 1000.0);
        let mut ev = event(&a, "error", json!({"dur_ms": dur_ms, "error": message, "code": "hung"})); ev["duration_ms"] = json!(dur_ms);
        emit(&ev);
    }
    flush(Duration::from_millis(300));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nobody_listening_serializes_nothing() {
        // telemetry on (a loopback sink nobody listens on): once the writer has found nobody, events are dropped
        // before they are serialized
        let dead = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
        init_with(Some(Dest::Tcp(format!("127.0.0.1:{dead}"))));
        let t0 = Instant::now();
        while !ATTEMPTED.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(5) { std::thread::sleep(Duration::from_millis(2)); }
        assert!(enabled() && !connected() && !listening());
        let before = SERIALIZED.load(Ordering::Relaxed);
        let ss = Arc::new(Mutex::new(Session::new()));
        for (i, (name, args)) in [("project_set", json!({"width": 320, "height": 180})), ("add_track", json!({})), ("get_project", json!({}))].into_iter().enumerate() {
            call_start(name, &args, Some(&json!(i)), &ss);
            let r = kimo_core::commands::dispatch(&mut ss.lock().unwrap(), name, &args).expect("dispatch");
            call_end(&json!({"content": [{"type": "text", "text": r.to_string()}], "isError": false}), &ss);
        }
        assert_eq!(SERIALIZED.load(Ordering::Relaxed), before, "no app connected: no event was serialized");
    }
    #[test]
    fn frames_are_latest_only_and_never_hold_state() {
        // the frame lane keeps the newest FRAME_DEPTH, dropping the oldest
        let mut f = VecDeque::new();
        let mut dropped = vec![];
        for k in 1..=5u64 { dropped.extend(push_latest(&mut f, k, FRAME_DEPTH)); }
        assert_eq!(f.iter().copied().collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(dropped, vec![1, 2, 3], "every drop is handed back, oldest first, so it can be counted and reported");
        // state waits only past STATE_BOUND, only while a spectator is connected, and one line always fits
        assert!(!must_wait(0, STATE_BOUND * 2, true), "one line always fits an empty lane");
        assert!(!must_wait(STATE_BOUND - 10, 10, true));
        assert!(must_wait(STATE_BOUND - 10, 11, true), "past the bound: backpressure, not a drop");
        assert!(!must_wait(STATE_BOUND * 4, 11, false), "nobody listening: never waits");
        // the writer serves lines in emission order across the lanes
        assert_eq!(frame_first(Some(3), Some(2)), Some(true));
        assert_eq!(frame_first(Some(2), Some(3)), Some(false));
        assert_eq!(frame_first(None, Some(9)), Some(true));
        assert_eq!(frame_first(Some(9), None), Some(false));
        assert_eq!(frame_first(None, None), None);
    }

    #[test]
    fn reconnect_backoff_doubles_to_a_ceiling_and_resets() {
        let mut b = Backoff::new();
        let seq: Vec<u64> = (0..7).map(|_| b.fail().as_millis() as u64).collect();
        assert_eq!(seq, [50, 100, 200, 400, 800, 1000, 1000]);
        b.reset();
        assert_eq!(b.fail(), Backoff::FIRST);
    }

    #[test]
    fn dest_parsing() {
        assert_eq!(parse_dest(""), None);
        assert_eq!(parse_dest("off"), None);
        assert_eq!(parse_dest("0"), None);
        assert_eq!(parse_dest("on"), Some(Dest::Gui));
        // on by default: unset or empty = the spectator GUI; off/0/false = off; explicit sinks pin exactly what they name
        for raw in ["", "  "] { assert_eq!(dest_or_default(raw), Some(Dest::Gui)); }
        for raw in ["off", "OFF", "0", "false"] { assert_eq!(dest_or_default(raw), None); }
        assert_eq!(dest_or_default("file:/tmp/x.jsonl"), Some(Dest::File("/tmp/x.jsonl".into())));
        assert_eq!(dest_or_default("tcp://127.0.0.1:7421"), Some(Dest::Tcp("127.0.0.1:7421".into())));
        assert_eq!(dest_or_default("tcp://8.8.8.8:7421"), None);
        assert_eq!(parse_dest("tcp://127.0.0.1:7421"), Some(Dest::Tcp("127.0.0.1:7421".into())));
        assert_eq!(parse_dest("127.0.0.1:9000"), Some(Dest::Tcp("127.0.0.1:9000".into())));
        assert_eq!(parse_dest("file:/tmp/x.jsonl"), Some(Dest::File("/tmp/x.jsonl".into())));
        assert_eq!(parse_dest("tcp://8.8.8.8:53"), None, "non-loopback sinks are refused");
        assert_eq!(parse_dest("nonsense"), None);
    }
    #[test]
    fn target_and_summary() {
        let t = target_of(&json!({"clip_id": "clip_4", "property": "x", "value": 3}));
        assert_eq!(t, json!({"track_id": null, "clip_id": "clip_4", "param": "x"}));
        let s = summarize(&json!({"text": "a".repeat(1000), "list": (0..20).collect::<Vec<_>>()}));
        assert_eq!(s["list"].as_array().unwrap().len(), ARR_CAP + 1);
        assert!(s["text"].as_str().unwrap().starts_with("<1000 bytes>") || s["text"].as_str().unwrap().chars().count() <= STR_CAP + 1);
    }
    #[test]
    fn diff_reports_clip_fields() {
        let a = Snap { ids: vec![("t1".into(), vec![("c1".into(), 0.0, 2.0)])], clip: Some(json!({"x": 1, "fx": {"temp": 5600}})), duration: 2.0, undo: 0, w: 1280, h: 720 };
        let b = Snap { ids: vec![("t1".into(), vec![("c1".into(), 0.0, 2.0), ("c2".into(), 2.0, 3.0)])], clip: Some(json!({"x": 1, "fx": {"temp": 6050}})), duration: 3.0, undo: 1, w: 1280, h: 720 };
        let d = diff(&a, &b);
        assert_eq!(d["clip"]["fx.temp"], json!([5600, 6050]));
        assert_eq!(d["clips_added"][0]["clip_id"], "c2");
        assert_eq!(d["duration"], json!([2.0, 3.0]));
        let fx = |v: f64| Snap { clip: Some(json!({"effects": [{"id": "fx5", "kind": "temperature", "params": {"amount": {"base": v}}}]})), ..Snap::default() };
        assert_eq!(diff(&fx(0.0), &fx(0.35))["clip"], json!({"effects[fx5].params.amount.base": [0.0, 0.35]}));
    }
    #[test]
    fn flash_stamps_come_from_the_full_result() {
        // 864 frames, 6 luminance transitions all past frame 8 (where summarize cuts the arrays) and one red one
        let n = 864usize; let at = [114usize, 232, 354, 594, 713, 733];
        let time: Vec<f64> = (0..n).map(|i| ((i as f64) / 24.0 * 1e6).round() / 1e6).collect();
        let lum: Vec<i64> = (0..n).map(|i| match at.iter().position(|&k| k == i) { Some(j) => if j % 2 == 0 { 1 } else { -1 }, None => 0 }).collect();
        let red: Vec<i64> = (0..n).map(|i| if i == 500 { -1 } else { 0 }).collect();
        let res = json!({"samples": n, "frames": {"time": time, "lum_transition": lum, "red_transition": red}, "totals": {"lum_transitions": 6}});
        let before = res.clone();
        let mut p = json!({"result": summarize(&res)});
        flash_stamps(&mut p, "flash_series", &res);
        assert_eq!(res, before, "the full result is unchanged");
        assert_eq!(p["result"]["frames"]["lum_transition"].as_array().unwrap().len(), ARR_CAP + 1, "summarize still caps");
        let st = p["lum_stamps"].as_array().unwrap();
        assert_eq!(p["lum_stamps_total"], json!(6));
        assert_eq!(st.len(), 6);
        for (s, &k) in st.iter().zip(&at) { assert_eq!(s["t"].as_f64().unwrap(), time[k]); }
        assert_eq!(st.iter().map(|s| s["dir"].as_i64().unwrap()).collect::<Vec<_>>(), vec![1, -1, 1, -1, 1, -1]);
        assert_eq!(p["red_stamps"], json!([{"t": time[500], "dir": -1}]));
        // other tools get nothing added
        let mut q = json!({"result": summarize(&res)}); flash_stamps(&mut q, "diff_frames", &res); assert!(q.get("lum_stamps").is_none());
    }
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), ""); assert_eq!(base64(b"f"), "Zg=="); assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v"); assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
