//! Spectator telemetry: opt-in, out-of-band JSON-lines events for the Kimo spectator GUI.
//!
//! Off unless `KIMO_SPECTATOR` is set. Accepted values:
//!   `tcp://127.0.0.1:7421` (loopback only; `on`, `1` and `gui` mean this, the GUI shell's listener),
//!   `file:/path/events.jsonl` (append; tests and recordings).
//! Guarantees: nothing is ever written to stdout (the MCP channel); tool arguments and results are only read,
//! never changed; the request loop never blocks on telemetry: events go into a bounded channel drained by one
//! background writer, and with no listener or a full buffer they are dropped. The writer reconnects lazily with
//! backoff when the GUI is closed and reopened mid-session; events from while it was gone are not replayed. With the variable unset every hook
//! below is a single `OnceLock` load.
//!
//! One event per line, the spectator's unified contract (crates/kimo-gui/README.md, "Unified tool events"):
//!   {tool_name, call_id, phase: start|progress|complete|error, target: {track_id, clip_id, param}, payload, ts, ts_ms}
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
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The spectator shell's loopback listener (crates/kimo-gui/src/main.rs).
pub const GUI_ADDR: &str = "127.0.0.1:7421";
const QUEUE: usize = 4096;
const STR_CAP: usize = 240;
const ARR_CAP: usize = 8;
const DEPTH_CAP: usize = 4;
const DIFF_CAP: usize = 24;

enum Msg { Line(String), Flush(SyncSender<()>) }
#[derive(Debug, Clone, PartialEq)]
pub enum Dest { Tcp(String), File(String) }

struct Sink { tx: SyncSender<Msg> }
static SINK: OnceLock<Option<Sink>> = OnceLock::new();
static SEQ: AtomicU64 = AtomicU64::new(0);
static REV: AtomicU64 = AtomicU64::new(0);
/// True while the writer holds a live connection to the sink (a file sink counts once opened). Read by the live
/// preview (preview.rs), which renders nothing while no spectator is listening.
static CONNECTED: AtomicBool = AtomicBool::new(false);
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

struct Active { call_id: String, tool: String, target: Value, t0: Instant, snap: Snap, last_progress: Option<(Instant, f64)>, last_frame: Option<(Instant, u64)>, frames_t0: Option<(Instant, u64)>, args: Value }

/// Parse a `KIMO_SPECTATOR` value. None = telemetry off (unset, empty, `0`, `off`, unparsable or non-loopback).
pub fn parse_dest(spec: &str) -> Option<Dest> {
    let s = spec.trim();
    if s.is_empty() || s == "0" || s.eq_ignore_ascii_case("off") || s.eq_ignore_ascii_case("false") { return None; }
    if ["1", "on", "true", "gui"].iter().any(|k| s.eq_ignore_ascii_case(k)) { return Some(Dest::Tcp(GUI_ADDR.into())); }
    if let Some(p) = s.strip_prefix("file://").or_else(|| s.strip_prefix("file:")) { return (!p.is_empty()).then(|| Dest::File(p.into())); }
    let addr = s.strip_prefix("tcp://").unwrap_or(s).trim_end_matches('/');
    use std::net::ToSocketAddrs;
    let all: Vec<_> = addr.to_socket_addrs().ok()?.collect();
    (!all.is_empty() && all.iter().all(|a| a.ip().is_loopback())).then(|| Dest::Tcp(addr.into()))
}

/// Read `KIMO_SPECTATOR` once and, when it names a sink, start the writer thread and install the engine's progress hook.
pub fn init() {
    let raw = std::env::var("KIMO_SPECTATOR").unwrap_or_default();
    let dest = parse_dest(&raw);
    if dest.is_none() && !raw.trim().is_empty() && !matches!(raw.trim(), "0" | "off" | "OFF" | "false") {
        eprintln!("kimo: KIMO_SPECTATOR={raw:?} ignored (use tcp://127.0.0.1:PORT or file:/path); telemetry off");
    }
    init_with(dest);
}

fn init_with(dest: Option<Dest>) {
    let sink = dest.and_then(|d| {
        let (tx, rx) = sync_channel::<Msg>(QUEUE);
        // The telemetry writer thread. Purpose: drain the bounded event queue to the sink so the request loop never blocks on I/O.
        // Lifecycle: started at most once per process (init runs once, SINK is a OnceLock) and only when KIMO_SPECTATOR names a sink;
        // it reconnects with backoff and lives until the process exits (SINK, which owns the sender, is never dropped). Join/cancel
        // owner: detached by design, nothing joins it; `flush` gives a bounded best-effort drain before exit. Headless (KIMO_SPECTATOR
        // unset or off): never started. Started through the engine's guarded spawn path; `init` runs at startup with no cancellation
        // token installed, so the writer inherits none and no tool call's time limit can cancel it.
        kimo_core::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-spectator".into()), move || writer(d, rx)).ok()?;
        Some(Sink { tx })
    });
    let on = sink.is_some();
    if SINK.set(sink).is_ok() && on {
        kimo_core::telemetry::set_progress_hook(Box::new(on_progress));
        since_start();
        let p0 = kimo_core::model::Project::default(); // the project Session::new() starts with
        emit(&json!({"type": "session", "reset": true, "width": p0.width, "height": p0.height, "agent": format!("kimo-mcp {}", kimo_core::buildinfo::VERSION), "read_only": true, "elapsed_s": 0, "ts_ms": ts_ms()}));
    }
}

#[inline]
pub fn enabled() -> bool { matches!(SINK.get(), Some(Some(_))) }

/// The sink has a live listener right now.
#[inline]
pub fn connected() -> bool { CONNECTED.load(Ordering::Relaxed) }

/// A live preview frame from the preview thread (same bounded, drop-on-full queue as every event).
pub(crate) fn emit_frame(v: &Value) { emit(v) }

pub(crate) fn now_ms() -> u64 { ts_ms() }

fn emit(v: &Value) {
    if let Some(Some(s)) = SINK.get() {
        match s.tx.try_send(Msg::Line(v.to_string())) { Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {} }
    }
}

/// Best-effort drain of queued events, bounded by `within` (used before the process exits).
pub fn flush(within: Duration) {
    let Some(Some(s)) = SINK.get() else { return };
    let deadline = Instant::now() + within;
    let (ack_tx, ack_rx) = sync_channel(1);
    let mut msg = Msg::Flush(ack_tx);
    loop {
        match s.tx.try_send(msg) {
            Ok(()) => break,
            Err(TrySendError::Full(m)) => { if Instant::now() >= deadline { return; } msg = m; std::thread::sleep(Duration::from_millis(2)); }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
    let _ = ack_rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
}

/// A live connection to the sink.
enum Conn { File(std::fs::File), Tcp(std::net::TcpStream) }

impl Conn {
    /// False once the peer has closed its end (the spectator window went away): a non-blocking peek reads EOF.
    /// Checked before each write so a closed GUI is noticed at once instead of one event later.
    fn alive(&mut self) -> bool {
        match self {
            Conn::File(_) => true,
            Conn::Tcp(s) => {
                if s.set_nonblocking(true).is_err() { return false; }
                let mut b = [0u8; 1];
                let ok = match s.peek(&mut b) {
                    Ok(0) => false,
                    Ok(_) => true,
                    Err(e) => e.kind() == std::io::ErrorKind::WouldBlock,
                };
                ok && s.set_nonblocking(false).is_ok()
            }
        }
    }
    fn write_line(&mut self, l: &[u8]) -> std::io::Result<()> {
        match self { Conn::File(f) => f.write_all(l), Conn::Tcp(s) => s.write_all(l) }
    }
    fn flush(&mut self) {
        let _ = match self { Conn::File(f) => f.flush(), Conn::Tcp(s) => s.flush() };
    }
}

fn open(dest: &Dest) -> Option<Conn> {
    match dest {
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
/// Nothing here can panic or reach the dispatch thread, which only ever `try_send`s into the bounded queue.
fn writer(dest: Dest, rx: Receiver<Msg>) {
    let mut conn: Option<Conn> = None;
    let mut backoff = Backoff::new();
    let mut retry_at = Instant::now();
    for m in rx {
        match m {
            Msg::Flush(ack) => { if let Some(c) = conn.as_mut() { c.flush(); } let _ = ack.send(()); }
            Msg::Line(mut l) => {
                if conn.as_mut().map_or(false, |c| !c.alive()) {
                    conn = None;
                    CONNECTED.store(false, Ordering::Relaxed);
                    retry_at = Instant::now();
                }
                if conn.is_none() && Instant::now() >= retry_at {
                    conn = open(&dest);
                    if conn.is_some() {
                        backoff.reset();
                        CONNECTED.store(true, Ordering::Relaxed);
                        crate::preview::reconnected(); // a spectator that just (re)opened gets the current frame
                    } else { retry_at = Instant::now() + backoff.fail(); }
                }
                if let Some(c) = conn.as_mut() {
                    l.push('\n');
                    if c.write_line(l.as_bytes()).is_err() { conn = None; CONNECTED.store(false, Ordering::Relaxed); retry_at = Instant::now() + backoff.fail(); }
                }
            }
        }
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

/// A frame image a call wrote (render_frame, previews): forwarded to the viewer when it is small enough.
fn frame_event(call_id: &str, args: &Value, res: &Value) -> Option<Value> {
    const MAX: u64 = 1_500_000;
    let path = res.get("path").and_then(|v| v.as_str())?;
    let lower = path.to_ascii_lowercase();
    let key = if lower.ends_with(".png") { "png_b64" } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "jpg_b64" } else { return None };
    if std::fs::metadata(path).ok()?.len() > MAX { return None; }
    let bytes = std::fs::read(path).ok()?;
    let mut ev = json!({"type": "frame_rendered", "call_id": call_id, "ts_ms": ts_ms()});
    ev[key] = json!(base64(&bytes));
    if let Some(t) = args.get("time").and_then(|v| v.as_f64()) { ev["tc"] = json!(format!("{t:.2}s")); }
    Some(ev)
}

fn since_start() -> f64 {
    static T0: OnceLock<Instant> = OnceLock::new();
    (T0.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0).round() / 1000.0
}

fn event(a: &Active, phase: &str, payload: Value) -> Value {
    json!({"tool_name": a.tool, "call_id": a.call_id, "phase": phase, "target": a.target, "payload": payload, "ts": since_start(), "ts_ms": ts_ms()})
}

/// A tools/call is about to run. Off: returns at once.
pub fn call_start(name: &str, args: &Value, ss: &Arc<Mutex<Session>>) {
    if !enabled() { return; }
    let (tool, eff, group) = if toolgroups::is_group(name) {
        match toolgroups::resolve_group_call(name, args) { Ok((a, ia)) => (a, ia, Some(name)), Err(_) => (name.to_string(), args.clone(), None) }
    } else { (name.to_string(), args.clone(), None) };
    let mut target = target_of(&eff);
    let snap = snapshot(ss, target["clip_id"].as_str());
    if target["track_id"].is_null() {
        if let Some((t, _, _)) = target["clip_id"].as_str().and_then(|c| snap.find(c)) { target["track_id"] = json!(t); }
    }
    let a = Active { call_id: new_call_id(), tool, target, t0: Instant::now(), snap, last_progress: None, last_frame: None, frames_t0: None, args: eff };
    let mut ev = event(&a, "start", json!({"args": summarize(&a.args)}));
    if let Some(g) = group { ev["group"] = json!(g); }
    emit(&ev);
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
        return;
    }
    let after = snapshot(ss, a.target["clip_id"].as_str());
    let changed = diff(&a.snap, &after);
    let mutated = after.undo != a.snap.undo || (after.w, after.h) != (a.snap.w, a.snap.h) || changed.as_object().map_or(false, |o| !o.is_empty());
    let res = serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text));
    let images = result.get("content").and_then(|c| c.as_array()).map_or(0, |c| c.iter().filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("image")).count());
    let mut p = json!({"dur_ms": dur_ms, "result": summarize(&res), "changed": changed, "mutated": mutated});
    if images > 0 { p["images"] = json!(images); }
    if let Some(n) = output_bytes(&a.tool, &res) { p["bytes"] = json!(n); if p["result"].is_object() { p["result"]["bytes"] = json!(n); } }
    let changed = p["changed"].clone();
    vocabulary(&mut p, &a.tool, &a.args, &a.target, &changed, &after, ss);
    let mut ev = event(&a, "complete", p); ev["duration_ms"] = json!(dur_ms);
    emit(&ev);
    let hint = playhead_hint(&a.args, &a.target, &changed, &after);
    if let Some(f) = frame_event(&a.call_id, &a.args, &res) { emit(&f); crate::preview::shown_by_agent(); }
    if mutated { emit(&json!({"type": "project_changed", "rev": REV.fetch_add(1, Ordering::Relaxed) + 1, "call_id": a.call_id, "width": after.w, "height": after.h, "ts_ms": ts_ms()})); }
    // live preview: any call that changed the project (or is declared mutating) queues one frame at the playhead
    let live = mutated || declared_mutating(&a.tool);
    crate::preview::call_finished(ss, &a.call_id, REV.load(Ordering::Relaxed), live, hint, after.duration, connected());
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
        assert_eq!(parse_dest("on"), Some(Dest::Tcp(GUI_ADDR.into())));
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
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), ""); assert_eq!(base64(b"f"), "Zg=="); assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v"); assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
