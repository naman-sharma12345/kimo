//! Live preview frames for the spectator: after a call that changed the project, the viewer gets the frame at the
//! playhead without the agent having to call `render_frame`.
//!
//! Only runs when telemetry is on AND the sink is connected (spectator.rs). The dispatch thread never renders and
//! never waits: a mutating call only bumps a generation counter and notes the playhead under a small lock
//! (`request`). One background thread (started on the first request) does the work, latest wins:
//!   * it waits until no call is running and the calls have been quiet for `QUIET`, so a burst renders once after the
//!     burst; a burst longer than `MAX_WAIT` still gets a frame between two calls every `MAX_WAIT`;
//!   * the quiet time adapts to the call cadence (`quiet_for`): when mutating calls arrive less than `STREAM_GAP` apart
//!     they are a stream, and a frame waits for a pause of twice that interval (at most `MAX_WAIT`), so a steady stream
//!     coalesces into one frame per `MAX_WAIT` however fast a single frame renders; an isolated call still renders
//!     after `QUIET`;
//!   * it takes a copy of the project with `try_lock` (a hung tool holding the session only skips the frame),
//!     renders at the playhead with its own decode cache, scales to at most `KIMO_SPECTATOR_FRAME_PX` wide
//!     (default 640), encodes a PNG in process (png.rs) and emits `{"type":"frame_rendered","auto":true,"png_b64",...}`;
//!   * a call that starts while a frame is rendering cancels it through the engine's cooperative cancel token
//!     (kimo_core::guard): the render stops at its next guard check and is retried once the calls are quiet, so a
//!     preview does not compete with a running call; a frame pending for `MAX_WAIT` renders to the end regardless,
//!     so a steady stream of calls still gets a frame at least every `MAX_WAIT` plus one render;
//!   * its renders use at most half the cores (kimo_core::par thread cap, this thread only);
//!   * it never starts while the memory budget is more than half used, so a preview can not push a tool call
//!     over the budget, and a panic inside the renderer only drops that frame.
//! `KIMO_SPECTATOR_FRAMES=0` turns the live frames off (the tool events stay).
//!
//! Two modes. Headless (the default) is everything above: frames coalesce by call cadence and the dispatch thread
//! never waits. Spectator demo mode (opt-in: `KIMO_SPECTATOR_DEMO=on|<ms>` or `kimo-mcp --spectator-demo` /
//! `--demo-pace-ms <ms>`; pace 50..=100 ms, default `DEMO_PACE_DEFAULT_MS`) is for watching and recording: every
//! tools/call gets its own frame, tagged with that call (`demo: true`, `seq`, `call_id`, `rpc_id`, `tool`), rendered from
//! the project exactly as that call left it. After the call's response is written the dispatch thread holds the next
//! request until that frame has reached the spectator and the pace has passed since (`demo_wait`). Demo frames ride the
//! lossless state lane (spectator.rs), not the latest-only frame lane, so they are never coalesced or dropped; the wait
//! for delivery is what keeps each one on screen for at least the pace (a spectator stalled past `DEMO_WAIT` releases the
//! next call, and the frame is still delivered, in order, once it reads again). A call that left the picture as it
//! was re-sends the previous frame tagged with the new call (`same: true`, no render). A frame that cannot be made
//! (HDR output, memory over half the budget, a render error) is reported as `{"type":"frame_skipped","demo":true,...}`
//! with its reason, so the count still matches. Demo mode does nothing (and paces nothing) while no spectator is
//! connected, and is off with `KIMO_SPECTATOR_FRAMES=0`. State telemetry is the same in both modes (spectator.rs).
//!
//! Headless skips. Every live frame a call queued ends as exactly one `frame_rendered` (auto) or one
//! `{"type":"frame_skipped","skip_reason",...,"call_id","rev","gen","frame_lane_drops","ts_ms"}` on the lossless state
//! lane (`skip_event`; no `auto` key, which stays the mark of a drawn live frame, and no `demo` key). `skip_reason` is
//! one of `SKIP_*`:
//!   * no frame for that call is coming, so the event also carries `"refused": true` and the human `reason` (the same
//!     words demo mode sends): `memory_budget`, `session_busy`, `pq_output` / `hlg_output` (HDR output transfer),
//!     `render_error` (the engine's error text), `render_panic`;
//!   * a newer frame replaces it (no `reason`, no `refused`): `coalesced` (a newer mutating call queued its own frame
//!     first; `render_cancelled: true` when this one was already rendering and the call cancelled it),
//!     `superseded_by_render_frame` (the agent's own render_frame showed that project state), `frame_lane_dropped`
//!     (rendered, then pushed off the latest-only frame lane before the writer took it, spectator.rs), `disconnected`
//!     (nobody listening when it was due; a reconnect re-sends the current frame).
//! `frame_lane_drops` (also on every auto `frame_rendered`) is the process's running count of frame-lane drops.
use kimo_core::commands::Session;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Quiet time after the last mutating call before its frame renders (coalesces bursts).
pub const QUIET: Duration = Duration::from_millis(40);
/// Longest a pending frame waits during a continuous burst (it still only starts between two calls).
pub const MAX_WAIT: Duration = Duration::from_millis(400);
/// Mutating calls finishing closer together than this are a stream (see `quiet_for`).
pub const STREAM_GAP: Duration = Duration::from_millis(200);

/// Quiet time a pending frame needs after a call that finished `interval` after the previous mutating call (None: no
/// previous call). Isolated calls: `QUIET`. A stream (interval under `STREAM_GAP`): twice the interval, so the next call
/// of the same rhythm arrives before the frame starts and the stream coalesces; never more than `MAX_WAIT`, which stays
/// the bound on how long a pending frame waits. Depends only on the call cadence, never on how fast a frame renders.
pub fn quiet_for(interval: Option<Duration>) -> Duration {
    match interval {
        Some(i) if i < STREAM_GAP => (i * 2).clamp(QUIET, MAX_WAIT),
        _ => QUIET,
    }
}

/// `frame_skipped.skip_reason` values (module doc, "Headless skips"). The first group carries `refused` + `reason`.
pub const SKIP_MEMORY_BUDGET: &str = "memory_budget";
pub const SKIP_SESSION_BUSY: &str = "session_busy";
pub const SKIP_PQ_OUTPUT: &str = "pq_output";
pub const SKIP_HLG_OUTPUT: &str = "hlg_output";
pub const SKIP_RENDER_ERROR: &str = "render_error";
pub const SKIP_RENDER_PANIC: &str = "render_panic";
pub const SKIP_COALESCED: &str = "coalesced";
pub const SKIP_SUPERSEDED_BY_RENDER_FRAME: &str = "superseded_by_render_frame";
pub const SKIP_FRAME_LANE_DROPPED: &str = "frame_lane_dropped";
pub const SKIP_DISCONNECTED: &str = "disconnected";
/// The agent's own render_frame image was over the size the spectator forwards (spectator.rs `frame_event`).
pub const SKIP_OVERSIZE_AGENT_FRAME: &str = "oversize_agent_frame";
/// `frame`'s refusal for an HDR (PQ = transfer 6, HLG = 7) output transfer; also the demo-mode `reason`.
const HDR_OUTPUT: &str = "HDR output";

/// A headless live frame that will not be drawn: the event for the state lane (module doc, "Headless skips").
pub(crate) fn skip_event(skip_reason: &str, call_id: &str, rev: u64, gen: u64) -> Value {
    json!({"type": "frame_skipped", "skip_reason": skip_reason, "call_id": call_id, "rev": rev, "gen": gen,
        "frame_lane_drops": crate::spectator::frame_lane_drops(), "ts_ms": crate::spectator::now_ms()})
}
/// No frame for this call is coming: `refused` plus the human reason the GUI's out-of-date chip shows.
fn refused(mut ev: Value, reason: &str) -> Value { ev["refused"] = json!(true); ev["reason"] = json!(reason); ev }
/// The skip reason for a frame `frame` could not make.
fn err_skip(p: &kimo_core::model::Project, e: &str) -> &'static str {
    if e == HDR_OUTPUT { if p.color_pipeline.output_transfer == 6 { SKIP_PQ_OUTPUT } else { SKIP_HLG_OUTPUT } } else { SKIP_RENDER_ERROR }
}

/// Spectator demo mode: default pace between two calls' frames, and the range a pace is clamped to.
pub const DEMO_PACE_DEFAULT_MS: u64 = 75;
pub const DEMO_PACE_MIN_MS: u64 = 50;
pub const DEMO_PACE_MAX_MS: u64 = 100;
/// Longest the dispatch thread waits for one call's demo frame to reach the spectator before it moves on.
pub const DEMO_WAIT: Duration = Duration::from_secs(10);

/// Parse a demo-mode setting (`KIMO_SPECTATOR_DEMO`, or the `--demo-pace-ms` value): off (unset, empty, `0`, `off`,
/// `false`), on at the default pace (`1`, `on`, `true`, `demo`), or a pace in ms, clamped to 50..=100. The second value
/// is a note for stderr when the setting was clamped or not understood (then demo mode stays off).
pub fn parse_demo(v: &str) -> (Option<u64>, Option<String>) {
    let s = v.trim();
    if s.is_empty() || ["0", "off", "false", "no"].iter().any(|k| s.eq_ignore_ascii_case(k)) { return (None, None); }
    if ["1", "on", "true", "yes", "demo"].iter().any(|k| s.eq_ignore_ascii_case(k)) { return (Some(DEMO_PACE_DEFAULT_MS), None); }
    match s.trim_end_matches("ms").trim().parse::<u64>() {
        Ok(ms) => {
            let c = ms.clamp(DEMO_PACE_MIN_MS, DEMO_PACE_MAX_MS);
            (Some(c), (c != ms).then(|| format!("demo pace {ms} ms is outside {DEMO_PACE_MIN_MS}..={DEMO_PACE_MAX_MS}; using {c} ms")))
        }
        Err(_) => (None, Some(format!("demo setting {s:?} not understood (use on, off or a pace in ms); demo mode off"))),
    }
}

static DEMO: OnceLock<Option<Duration>> = OnceLock::new();

/// Fix the demo mode for this process: the command-line value when one was given, else `KIMO_SPECTATOR_DEMO`.
/// Called once at startup (main.rs); without it the first use reads the environment.
pub fn configure_demo(cli: Option<&str>) {
    let _ = DEMO.set(demo_from(cli, true));
}

fn demo_from(cli: Option<&str>, speak: bool) -> Option<Duration> {
    let raw = match cli { Some(v) => v.to_string(), None => std::env::var("KIMO_SPECTATOR_DEMO").unwrap_or_default() };
    let (ms, note) = parse_demo(&raw);
    if speak { if let Some(n) = note { eprintln!("kimo: {n}"); } }
    if ms.is_some() && !wanted() {
        if speak { eprintln!("kimo: spectator demo mode needs live frames; KIMO_SPECTATOR_FRAMES=0 turns it off"); }
        return None;
    }
    ms.map(Duration::from_millis)
}

/// The demo pace, when spectator demo mode is on.
pub fn demo() -> Option<Duration> { *DEMO.get_or_init(|| demo_from(None, false)) }

/// Where the playhead should go after a call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hint { None, At(f64), Clip(f64, f64) }

/// `quiet`: how long the calls must pause after `last` before this frame starts (`quiet_for`).
struct Pending { gen: u64, call_id: String, rev: u64, last: Instant, quiet: Duration }
/// `since`: when the oldest change not yet on screen was requested (drives `MAX_WAIT` and the overdue rule).
/// `prev_end`: when the previous wanted mutating call finished (drives the cadence in `quiet_for`).
/// `demo`: the frame of the call that just finished, in spectator demo mode (rendered before anything pending).
struct State { gen: u64, pending: Option<Pending>, since: Option<Instant>, playhead: f64, duration: f64, last_call: String, last_rev: u64, prev_end: Option<Instant>, demo: Option<DemoJob> }

static ST: Mutex<State> = Mutex::new(State { gen: 0, pending: None, since: None, playhead: 0.0, duration: 0.0, last_call: String::new(), last_rev: 0, prev_end: None, demo: None });

/// One call's demo frame: `seq` counts the demo frames of this process (1, 2, ...), one per tools/call.
struct DemoJob { seq: u64, call_id: String, rpc_id: Value, tool: String, gen: u64, rev: u64 }
/// Demo delivery bookkeeping, shared by the dispatch thread (`demo_wait`) and the writer (`demo_delivered`).
/// `armed`: the call that just finished queued a frame the next request has to wait for.
struct DemoAck { queued: u64, delivered: u64, at: Option<Instant>, armed: bool }
static DACK: Mutex<DemoAck> = Mutex::new(DemoAck { queued: 0, delivered: 0, at: None, armed: false });
static DCV: Condvar = Condvar::new();
static CV: Condvar = Condvar::new();
static BUSY: AtomicBool = AtomicBool::new(false);
static SS: OnceLock<Arc<Mutex<Session>>> = OnceLock::new();
/// The renderer thread: started on the first frame wanted, then parked on `CV` whenever nothing is pending.
static THREAD: OnceLock<bool> = OnceLock::new();
/// The renderer's cooperative-cancel token (kimo_core::guard): a call that starts while a frame renders sets it,
/// the render stops at its next guard check and the frame is retried after the calls go quiet again.
static CANCEL: OnceLock<kimo_core::guard::Cancel> = OnceLock::new();

fn wanted() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| !matches!(std::env::var("KIMO_SPECTATOR_FRAMES").unwrap_or_default().trim(), "0" | "off" | "OFF" | "false"))
}

fn max_px() -> usize {
    static PX: OnceLock<usize> = OnceLock::new();
    *PX.get_or_init(|| std::env::var("KIMO_SPECTATOR_FRAME_PX").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(640usize).clamp(64, 1920))
}

/// Resolve the playhead from a hint: an explicit time wins; a touched clip moves it to the clip's middle unless it
/// is already inside the clip. Always clamped to the timeline (0 for an empty one).
pub fn place(cur: f64, hint: Hint, duration: f64) -> f64 {
    let t = match hint {
        Hint::None => cur,
        Hint::At(t) if t.is_finite() => t,
        Hint::At(_) => cur,
        Hint::Clip(s, e) => if cur >= s && cur < e { cur } else { s + (e - s).max(0.0) / 2.0 },
    };
    if duration <= 0.0 || !t.is_finite() { 0.0 } else { t.clamp(0.0, (duration - 1e-3).max(0.0)) }
}

/// A tools/call starts (telemetry on). No frame starts rendering while a call runs.
#[inline]
pub fn call_started() {
    BUSY.store(true, Ordering::SeqCst);
    if let Some(c) = CANCEL.get() { c.store(true, Ordering::SeqCst); }
}

/// A tools/call finished (telemetry on). `mutated`: the project changed; `connected`: the sink has a live listener.
/// Cost on the dispatch thread: one uncontended lock and a notify. Never renders, never blocks on the renderer.
pub fn call_finished(ss: &Arc<Mutex<Session>>, call_id: &str, rev: u64, mutated: bool, hint: Hint, duration: f64, connected: bool) {
    BUSY.store(false, Ordering::SeqCst);
    let want = mutated && connected && wanted() && demo().is_none(); // demo mode makes its own frame per call
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    g.duration = duration;
    g.playhead = place(g.playhead, hint, duration);
    if mutated { g.gen += 1; g.last_call = call_id.to_string(); g.last_rev = rev; }
    let mut coalesced = None;
    if want {
        let now = Instant::now();
        g.since.get_or_insert(now);
        let quiet = quiet_for(g.prev_end.map(|p| now.duration_since(p)));
        g.prev_end = Some(now);
        let gen = g.gen;
        coalesced = g.pending.replace(Pending { gen, call_id: call_id.to_string(), rev, last: now, quiet }).map(|old| (old, gen));
    }
    drop(g);
    // the replaced frame never started: say so (after the lock; enqueue may wait on a stalled spectator)
    if let Some((old, gen)) = coalesced {
        let mut ev = skip_event(SKIP_COALESCED, &old.call_id, old.rev, old.gen);
        ev["superseded_by"] = json!({"call_id": call_id, "gen": gen});
        crate::spectator::emit_skip(&ev);
    }
    if mutated || demo().is_some() { SS.get_or_init(|| ss.clone()); }
    if want { ensure_thread(); }
    if THREAD.get().is_some() { CV.notify_one(); } // the renderer waits for running calls to end
}

/// The agent's own `render_frame` (call `by`) just put an up-to-date frame on screen: a pending live frame for the
/// same project state is not needed any more (`superseded_by_render_frame`).
pub fn shown_by_agent(by: &str) {
    let cleared = {
        let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
        if g.pending.as_ref().map_or(false, |q| q.gen == g.gen) { g.since = None; g.pending.take() } else { None }
    };
    if let Some(q) = cleared {
        let mut ev = skip_event(SKIP_SUPERSEDED_BY_RENDER_FRAME, &q.call_id, q.rev, q.gen);
        ev["superseded_by"] = json!({"call_id": by});
        crate::spectator::emit_skip(&ev);
    }
}

/// Spectator demo mode: the call `call_id` finished (telemetry on). Queues its own frame for the renderer; the
/// dispatch thread then waits for it in `demo_wait`. Off, or nobody listening: nothing.
pub fn demo_frame(call_id: &str, rpc_id: &Value, tool: &str) {
    if demo().is_none() || SS.get().is_none() || !crate::spectator::connected_settled(Duration::from_secs(1)) { return; }
    let seq = { let mut a = DACK.lock().unwrap_or_else(|e| e.into_inner()); a.queued += 1; a.armed = true; a.queued };
    {
        let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
        let (gen, rev) = (g.gen, g.last_rev);
        g.demo = Some(DemoJob { seq, call_id: call_id.to_string(), rpc_id: rpc_id.clone(), tool: tool.to_string(), gen, rev });
    }
    ensure_thread();
    CV.notify_one();
}

/// Spectator demo mode, on the dispatch thread after a call's response went out: wait until that call's frame has
/// reached the spectator (at most `DEMO_WAIT`), then until the pace has passed since. Returns at once when the call
/// queued no frame (demo off, nobody listening).
pub fn demo_wait() {
    let Some(pace) = demo() else { return };
    let mut a = DACK.lock().unwrap_or_else(|e| e.into_inner());
    if !a.armed { return; }
    a.armed = false;
    let (target, t0) = (a.queued, Instant::now());
    while a.delivered < target {
        let left = DEMO_WAIT.saturating_sub(t0.elapsed());
        if left.is_zero() { break; }
        a = DCV.wait_timeout(a, left).unwrap_or_else(|e| e.into_inner()).0;
    }
    let at = if a.delivered >= target { a.at } else { None };
    drop(a);
    if let Some(at) = at { let left = pace.saturating_sub(at.elapsed()); if !left.is_zero() { std::thread::sleep(left); } }
}

/// The writer handled demo frame `seq` (sent it, or dropped it with nobody listening).
pub fn demo_delivered(seq: u64) {
    let mut a = DACK.lock().unwrap_or_else(|e| e.into_inner());
    if seq > a.delivered { a.delivered = seq; a.at = Some(Instant::now()); }
    drop(a);
    DCV.notify_all();
}

/// The sink (re)connected (the spectator window opened mid-session): send the current state once, if a call has
/// changed the project. Called from the writer thread; `first`: the writer's first connection of the process. In
/// demo mode that first one is the session starting, whose calls bring their own frames (a spectator opened later,
/// or reopened, still gets the current frame at once).
pub fn reconnected(first: bool) {
    if !wanted() || (first && demo().is_some()) { return; }
    let Some(_) = SS.get() else { return };
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    if g.gen == 0 || g.pending.is_some() { return; }
    let now = Instant::now();
    g.since.get_or_insert(now);
    g.pending = Some(Pending { gen: g.gen, call_id: g.last_call.clone(), rev: g.last_rev, last: now, quiet: QUIET });
    drop(g);
    ensure_thread();
    CV.notify_one();
}

/// The preview renderer thread. Purpose: render live preview frames off the dispatch thread (module doc above).
/// Lifecycle: started at most once per process (THREAD OnceLock) on the first wanted frame, then parked on CV while nothing is pending;
/// it lives until the process exits. Join/cancel owner: detached by design, nothing joins it; process exit ends it. Each frame's work is
/// cancelled through the preview's own CANCEL token (installed per frame in `run`), never by stopping the thread. Headless: never started
/// with telemetry off (KIMO_SPECTATOR=off), with no live listener, or with KIMO_SPECTATOR_FRAMES=0 (see `call_finished`).
/// Started through the engine's guarded spawn path; its callers (the dispatch thread after a call, the spectator writer on reconnect)
/// hold no cancellation token, so the thread inherits none and only ever runs under the preview's own token.
fn ensure_thread() { THREAD.get_or_init(|| kimo_core::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-preview".into()), run).is_ok()); }

fn run() {
    let Some(ss) = SS.get() else { return };
    let tok = CANCEL.get_or_init(kimo_core::guard::new_token).clone();
    // half the cores at most: a frame that does render alongside a call leaves the call room to run
    kimo_core::par::set_thread_cap((std::thread::available_parallelism().map_or(2, |n| n.get()) / 2).max(1));
    let mut cache = kimo_core::render::Cache::new();
    let mut last_demo: Option<Value> = None;
    loop {
        let (job, t, overdue) = {
            let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if g.demo.is_some() { break; } // demo mode: this call's frame, now
                let now = Instant::now();
                let wait = match &g.pending {
                    None => None,
                    Some(q) => {
                        let waited = g.since.map_or(Duration::ZERO, |s| now.duration_since(s));
                        let ready = now.duration_since(q.last) >= q.quiet || waited >= MAX_WAIT;
                        // clear the token BEFORE reading BUSY: a call starting after this read cancels the render
                        tok.store(false, Ordering::SeqCst);
                        let busy = BUSY.load(Ordering::SeqCst);
                        if ready && !busy { break; }
                        // a call is running: sleep until it ends (every call end notifies); else until ready
                        if busy { None } else { Some(q.quiet.saturating_sub(now.duration_since(q.last)).min(MAX_WAIT.saturating_sub(waited)).max(Duration::from_millis(1))) }
                    }
                };
                g = match wait {
                    None => CV.wait(g).unwrap_or_else(|e| e.into_inner()),
                    Some(d) => CV.wait_timeout(g, d).unwrap_or_else(|e| e.into_inner()).0,
                };
            }
            if let Some(d) = g.demo.take() {
                let t = g.playhead;
                drop(g);
                demo_render(ss, d, t, &mut cache, &mut last_demo);
                continue;
            }
            let job = g.pending.take().unwrap();
            // cancellable unless overdue: calls arriving every ~100 ms must not starve the viewer forever
            let overdue = g.since.map_or(false, |s| s.elapsed() >= MAX_WAIT);
            (job, g.playhead, overdue)
        };
        let skip = |why: &str| skip_event(why, &job.call_id, job.rev, job.gen);
        let emit_skip = |ev: Value| { crate::spectator::emit_skip(&ev); done(); };
        if !crate::spectator::connected() { emit_skip(skip(SKIP_DISCONNECTED)); continue; } // a reconnect re-sends
        if over_half_budget() { emit_skip(refused(skip(SKIP_MEMORY_BUDGET), "memory budget over half used")); continue; }
        let t0 = Instant::now();
        let Some(proj) = copy_project(ss) else { emit_skip(refused(skip(SKIP_SESSION_BUSY), "session busy")); continue };
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _cancel = (!overdue).then(|| kimo_core::guard::install_token(&tok));
            frame(&proj, t, &mut cache)
        }));
        let ms = |d: Duration| (d.as_secs_f64() * 10000.0).round() / 10.0;
        let (key, bytes, w, h, tm) = match out {
            Ok(Ok(v)) => v,
            _ if !overdue && tok.load(Ordering::SeqCst) => {
                // a call started mid-render: yield to it and retry once the calls are quiet again (unless newer work
                // is queued). `since` is kept, so by MAX_WAIT the retry renders to the end whatever arrives.
                let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
                match g.pending.as_ref().map(|q| json!({"call_id": q.call_id, "gen": q.gen})) {
                    None => { g.pending = Some(Pending { last: Instant::now(), ..job }); }
                    Some(by) => {
                        // newer work is queued: this frame is dropped in its favour (coalesced while rendering)
                        drop(g);
                        let mut ev = skip(SKIP_COALESCED);
                        ev["superseded_by"] = by; ev["render_cancelled"] = json!(true);
                        crate::spectator::emit_skip(&ev);
                    }
                }
                continue;
            }
            Ok(Err(e)) => { emit_skip(refused(skip(err_skip(&proj, &e)), &e)); continue }
            Err(_) => { emit_skip(refused(skip(SKIP_RENDER_PANIC), "render failed")); cache = kimo_core::render::Cache::new(); continue }
        };
        let mut ev = json!({"type": "frame_rendered", "auto": true, "call_id": job.call_id, "rev": job.rev, "gen": job.gen,
            "time": (t * 1000.0).round() / 1000.0, "tc": format!("{t:.2}s"), "frame": (t * proj.fps).round() as i64,
            "w": w, "h": h, "src_w": proj.width, "src_h": proj.height, "render_ms": ms(t0.elapsed()),
            "stages_ms": {"prepare": tm.prepare, "render": tm.render, "output": tm.output, "scale": tm.scale, "encode": tm.encode},
            "ts_ms": crate::spectator::now_ms()});
        ev[key] = Value::String(crate::inline::b64(&bytes));
        crate::spectator::emit_frame(ev);
        done();
    }
}

/// Spectator demo mode: render one call's frame (never cancelled: no call runs while the dispatch thread waits for it)
/// and queue it for the writer, which reports its delivery. Same project generation and playhead as the previous demo
/// frame: that frame again, tagged with this call (`same: true`). A frame that cannot be made: `frame_skipped`.
fn demo_render(ss: &Arc<Mutex<Session>>, d: DemoJob, t: f64, cache: &mut kimo_core::render::Cache, last: &mut Option<Value>) {
    let pace = demo().map_or(0, |p| p.as_millis() as u64);
    let tag = |ev: &mut Value, d: &DemoJob| {
        for (k, v) in [("demo", json!(true)), ("seq", json!(d.seq)), ("call_id", json!(d.call_id)), ("rpc_id", d.rpc_id.clone()), ("tool", json!(d.tool)),
            ("rev", json!(d.rev)), ("gen", json!(d.gen)), ("pace_ms", json!(pace)), ("ts_ms", json!(crate::spectator::now_ms()))] { ev[k] = v; }
    };
    let send = |ev: &Value, seq: u64| { if !crate::spectator::emit_demo_frame(ev, seq) { demo_delivered(seq); } };
    let skip = |d: &DemoJob, why: &str, reason: &str| { let mut ev = json!({"type": "frame_skipped", "skip_reason": why, "refused": true, "reason": reason}); tag(&mut ev, d); send(&ev, d.seq); };
    if let Some(prev) = last.as_ref().filter(|p| p["gen"] == json!(d.gen) && p["time"] == json!((t * 1000.0).round() / 1000.0)) {
        let mut ev = prev.clone();
        tag(&mut ev, &d);
        ev["same"] = json!(true); ev["render_ms"] = json!(0.0);
        send(&ev, d.seq);
        return;
    }
    if !crate::spectator::connected() { demo_delivered(d.seq); return; }
    if over_half_budget() { return skip(&d, SKIP_MEMORY_BUDGET, "memory budget over half used"); }
    let Some(proj) = copy_project(ss) else { return skip(&d, SKIP_SESSION_BUSY, "session busy") };
    let t0 = Instant::now();
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| frame(&proj, t, cache)));
    let ms = |d: Duration| (d.as_secs_f64() * 10000.0).round() / 10.0;
    let (key, bytes, w, h, tm) = match out {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return skip(&d, err_skip(&proj, &e), &e),
        Err(_) => { *cache = kimo_core::render::Cache::new(); return skip(&d, SKIP_RENDER_PANIC, "render failed") }
    };
    let mut ev = json!({"type": "frame_rendered", "auto": true,
        "time": (t * 1000.0).round() / 1000.0, "tc": format!("{t:.2}s"), "frame": (t * proj.fps).round() as i64,
        "w": w, "h": h, "src_w": proj.width, "src_h": proj.height, "render_ms": ms(t0.elapsed()),
        "stages_ms": {"prepare": tm.prepare, "render": tm.render, "output": tm.output, "scale": tm.scale, "encode": tm.encode}});
    ev[key] = Value::String(crate::inline::b64(&bytes));
    tag(&mut ev, &d);
    send(&ev, d.seq);
    *last = Some(ev);
}

/// A frame went out (or could not be made): the wait clock restarts for whatever was requested meanwhile.
fn done() {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    g.since = g.pending.as_ref().map(|_| Instant::now());
}
fn over_half_budget() -> bool {
    let b = kimo_core::guard::BUDGET.load(Ordering::Relaxed);
    b != usize::MAX && kimo_core::guard::LIVE.load(Ordering::Relaxed) > b / 2
}

/// Copy the project out of the session without waiting on a running tool.
fn copy_project(ss: &Arc<Mutex<Session>>) -> Option<kimo_core::model::Project> {
    for _ in 0..25 {
        match ss.try_lock() {
            Ok(g) => return Some(g.project.clone()),
            Err(std::sync::TryLockError::Poisoned(e)) => return Some(e.into_inner().project.clone()),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    None
}

/// Render the frame at `t`, box-scale it to the preview width and encode it as PNG in process.
/// Stage timings of one live frame, in ms.
#[derive(Default)]
struct Timing { prepare: f64, render: f64, output: f64, scale: f64, encode: f64 }

/// Returns (payload key, PNG bytes, width, height, stage timings).
fn frame(p: &kimo_core::model::Project, t: f64, cache: &mut kimo_core::render::Cache) -> Result<(&'static str, Vec<u8>, usize, usize, Timing), String> {
    if p.color_pipeline.enabled && [6, 7].contains(&p.color_pipeline.output_transfer) { return Err(HDR_OUTPUT.into()); }
    let ms = |t: &mut Instant| { let n = Instant::now(); let d = (n.duration_since(*t).as_secs_f64() * 10000.0).round() / 10.0; *t = n; d };
    let mut tm = Timing::default();
    let mut c = Instant::now();
    kimo_core::render::prepare_at(p, cache, Some(t))?;
    tm.prepare = ms(&mut c);
    let mut f = kimo_core::render::render_frame(p, t, cache)?;
    tm.render = ms(&mut c);
    kimo_core::color::pipeline::output(p, &mut f)?;
    tm.output = ms(&mut c);
    let (rgba, w, h) = downscale(&f.d, f.w, f.h, max_px());
    drop(f);
    tm.scale = ms(&mut c);
    let png = crate::png::encode_rgb(&rgba, w, h);
    tm.encode = ms(&mut c);
    Ok(("png_b64", png, w, h, tm))
}

/// Box-filter an RGBA f32 frame (0..1) down to at most `max_w` wide (never up), keeping the aspect; 8-bit out.
pub fn downscale(d: &[f32], w: usize, h: usize, max_w: usize) -> (Vec<u8>, usize, usize) {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    if w == 0 || h == 0 { return (vec![], 0, 0); }
    if w <= max_w { return (d.iter().map(|&v| q(v)).collect(), w, h); }
    let dw = max_w;
    let dh = ((h as f64 * dw as f64 / w as f64).round() as usize).max(1);
    let mut out = vec![0u8; dw * dh * 4];
    for y in 0..dh {
        let (y0, y1) = (y * h / dh, ((y + 1) * h / dh).max(y * h / dh + 1).min(h));
        for x in 0..dw {
            let (x0, x1) = (x * w / dw, ((x + 1) * w / dw).max(x * w / dw + 1).min(w));
            let mut acc = [0f32; 4];
            for sy in y0..y1 { for sx in x0..x1 { let i = (sy * w + sx) * 4; for k in 0..4 { acc[k] += d[i + k]; } } }
            let n = ((y1 - y0) * (x1 - x0)) as f32;
            let o = (y * dw + x) * 4;
            for k in 0..4 { out[o + k] = q(acc[k] / n); }
        }
    }
    (out, dw, dh)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn playhead_placement() {
        assert_eq!(place(0.0, Hint::None, 0.0), 0.0, "empty timeline");
        assert_eq!(place(1.0, Hint::At(3.0), 10.0), 3.0, "explicit time wins");
        assert_eq!(place(1.0, Hint::At(30.0), 10.0), 10.0 - 1e-3, "clamped inside the timeline");
        assert_eq!(place(1.0, Hint::Clip(0.0, 4.0), 10.0), 1.0, "already inside the touched clip: stays");
        assert_eq!(place(5.0, Hint::Clip(0.0, 4.0), 10.0), 2.0, "outside: middle of the clip");
        assert_eq!(place(7.0, Hint::None, 5.0), 5.0 - 1e-3, "timeline got shorter");
    }
    #[test]
    fn quiet_time_follows_the_call_cadence_not_the_render_speed() {
        let ms = Duration::from_millis;
        assert_eq!(quiet_for(None), QUIET, "first call: QUIET");
        assert_eq!(quiet_for(Some(ms(5000))), QUIET, "isolated call: QUIET");
        assert_eq!(quiet_for(Some(STREAM_GAP)), QUIET, "at STREAM_GAP the calls are no longer a stream");
        assert_eq!(quiet_for(Some(ms(2))), QUIET, "pipelined burst: never shorter than QUIET");
        assert_eq!(quiet_for(Some(ms(55))), ms(110), "a 50 ms stream waits for a pause of twice its interval");
        assert!(quiet_for(Some(ms(55))) > ms(55), "the next call of the same rhythm arrives before the frame starts");
        assert_eq!(quiet_for(Some(ms(199))), ms(398));
        for i in 0..400 { assert!(quiet_for(Some(ms(i))) <= MAX_WAIT && quiet_for(Some(ms(i))) >= QUIET); }
    }
    #[test]
    fn demo_setting_parses_and_clamps_to_the_pace_range() {
        for off in ["", " ", "0", "off", "OFF", "false", "no"] { assert_eq!(parse_demo(off), (None, None), "{off:?} is headless"); }
        for on in ["1", "on", "true", "yes", "demo"] { assert_eq!(parse_demo(on).0, Some(DEMO_PACE_DEFAULT_MS), "{on:?}"); }
        assert_eq!(parse_demo("60"), (Some(60), None));
        assert_eq!(parse_demo("90ms"), (Some(90), None));
        assert_eq!(parse_demo("50").0, Some(50));
        assert_eq!(parse_demo("100").0, Some(100));
        let (v, note) = parse_demo("20"); assert_eq!(v, Some(DEMO_PACE_MIN_MS)); assert!(note.unwrap().contains("using 50 ms"));
        let (v, note) = parse_demo("500"); assert_eq!(v, Some(DEMO_PACE_MAX_MS)); assert!(note.is_some());
        let (v, note) = parse_demo("fast"); assert_eq!(v, None); assert!(note.unwrap().contains("demo mode off"));
        assert!((DEMO_PACE_MIN_MS..=DEMO_PACE_MAX_MS).contains(&DEMO_PACE_DEFAULT_MS));
    }
    #[test]
    fn downscale_keeps_aspect_and_averages() {
        let (w, h) = (1280, 720);
        let d: Vec<f32> = (0..w * h).flat_map(|i| { let v = if (i % w) % 2 == 0 { 1.0 } else { 0.0 }; [v, 0.5, 0.0, 1.0] }).collect();
        let (o, dw, dh) = downscale(&d, w, h, 640);
        assert_eq!((dw, dh, o.len()), (640, 360, 640 * 360 * 4));
        assert!(o.chunks(4).all(|p| p[0] == 128 && p[1] == 128 && p[2] == 0 && p[3] == 255), "2x2 boxes average the stripes");
        let (o, dw, dh) = downscale(&d[..320 * 180 * 4], 320, 180, 640);
        assert_eq!((dw, dh, o.len()), (320, 180, 320 * 180 * 4), "never upscaled");
    }
}
