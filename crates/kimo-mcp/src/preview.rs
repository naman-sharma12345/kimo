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

/// Where the playhead should go after a call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hint { None, At(f64), Clip(f64, f64) }

/// `quiet`: how long the calls must pause after `last` before this frame starts (`quiet_for`).
struct Pending { gen: u64, call_id: String, rev: u64, last: Instant, quiet: Duration }
/// `since`: when the oldest change not yet on screen was requested (drives `MAX_WAIT` and the overdue rule).
/// `prev_end`: when the previous wanted mutating call finished (drives the cadence in `quiet_for`).
struct State { gen: u64, pending: Option<Pending>, since: Option<Instant>, playhead: f64, duration: f64, last_call: String, last_rev: u64, prev_end: Option<Instant> }

static ST: Mutex<State> = Mutex::new(State { gen: 0, pending: None, since: None, playhead: 0.0, duration: 0.0, last_call: String::new(), last_rev: 0, prev_end: None });
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
    let want = mutated && connected && wanted();
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    g.duration = duration;
    g.playhead = place(g.playhead, hint, duration);
    if mutated { g.gen += 1; g.last_call = call_id.to_string(); g.last_rev = rev; }
    if want {
        let now = Instant::now();
        g.since.get_or_insert(now);
        let quiet = quiet_for(g.prev_end.map(|p| now.duration_since(p)));
        g.prev_end = Some(now);
        g.pending = Some(Pending { gen: g.gen, call_id: call_id.to_string(), rev, last: now, quiet });
    }
    drop(g);
    if mutated { SS.get_or_init(|| ss.clone()); }
    if want { ensure_thread(); }
    if THREAD.get().is_some() { CV.notify_one(); } // the renderer waits for running calls to end
}

/// The agent's own `render_frame` just put an up-to-date frame on screen: a pending live frame for the same
/// project state is not needed any more.
pub fn shown_by_agent() {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    if g.pending.as_ref().map_or(false, |q| q.gen == g.gen) { g.pending = None; g.since = None; }
}

/// The sink (re)connected (the spectator window opened mid-session): send the current state once, if a call has
/// changed the project. Called from the writer thread.
pub fn reconnected() {
    if !wanted() { return; }
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
/// without a spectator sink (KIMO_SPECTATOR unset), with no live listener, or with KIMO_SPECTATOR_FRAMES=0 (see `call_finished`).
/// Started through the engine's guarded spawn path; its callers (the dispatch thread after a call, the spectator writer on reconnect)
/// hold no cancellation token, so the thread inherits none and only ever runs under the preview's own token.
fn ensure_thread() { THREAD.get_or_init(|| kimo_core::guard::spawn_builder_with_token(std::thread::Builder::new().name("kimo-preview".into()), run).is_ok()); }

fn run() {
    let Some(ss) = SS.get() else { return };
    let tok = CANCEL.get_or_init(kimo_core::guard::new_token).clone();
    // half the cores at most: a frame that does render alongside a call leaves the call room to run
    kimo_core::par::set_thread_cap((std::thread::available_parallelism().map_or(2, |n| n.get()) / 2).max(1));
    let mut cache = kimo_core::render::Cache::new();
    loop {
        let (job, t, overdue) = {
            let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
            loop {
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
            let job = g.pending.take().unwrap();
            // cancellable unless overdue: calls arriving every ~100 ms must not starve the viewer forever
            let overdue = g.since.map_or(false, |s| s.elapsed() >= MAX_WAIT);
            (job, g.playhead, overdue)
        };
        if !crate::spectator::connected() || over_half_budget() { done(); continue; } // dropped; a reconnect re-sends
        let t0 = Instant::now();
        let Some(proj) = copy_project(ss) else { done(); continue };
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
                if g.pending.is_none() { g.pending = Some(Pending { last: Instant::now(), ..job }); }
                continue;
            }
            Ok(Err(_)) => { done(); continue }
            Err(_) => { done(); cache = kimo_core::render::Cache::new(); continue }
        };
        let mut ev = json!({"type": "frame_rendered", "auto": true, "call_id": job.call_id, "rev": job.rev, "gen": job.gen,
            "time": (t * 1000.0).round() / 1000.0, "tc": format!("{t:.2}s"), "frame": (t * proj.fps).round() as i64,
            "w": w, "h": h, "src_w": proj.width, "src_h": proj.height, "render_ms": ms(t0.elapsed()),
            "stages_ms": {"prepare": tm.prepare, "render": tm.render, "output": tm.output, "scale": tm.scale, "encode": tm.encode},
            "ts_ms": crate::spectator::now_ms()});
        ev[key] = Value::String(crate::inline::b64(&bytes));
        crate::spectator::emit_frame(&ev);
        done();
    }
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
    if p.color_pipeline.enabled && [6, 7].contains(&p.color_pipeline.output_transfer) { return Err("HDR output".into()); }
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
