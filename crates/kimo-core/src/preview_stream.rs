//! Real-time preview with sound: a preview session that renders ahead of the playhead at half or quarter resolution
//! into a ring buffer, and mixes the audio for the same stretch of timeline, one short window at a time.
//!
//! Shape. One stream per engine process (`STREAM`). `start` snapshots the project and starts two background threads:
//!   * the video worker renders frames ahead of the playhead in small batches: decode through the draft path (preview
//!     proxies when enabled, else the short-GOP analysis proxies of `analysis_proxy.rs`, never waiting for one to be built:
//!     `render::prepare_stream_range`), compose with `render::export_frame_rgba8` (same pixels as export, colour pipeline
//!     included), box-downscale to the draft size and write each frame as an uncompressed top-down 32-bit BMP (no encode
//!     cost; every webview decodes it natively) into the caller's directory. Frames go into a `Ring` keyed by the run's
//!     sequence number, so the files on disk never exceed the ring's capacity;
//!   * the audio worker mixes `AUDIO_CHUNK`-second windows of the timeline (`mix_window`: only the clips that reach the
//!     window are decoded, and only the part of each that falls inside it) and writes each as a 48 kHz stereo WAV.
//!
//! A "run" is where playback started and how it moves: a start frame, a direction (forward / reverse) and a stride
//! (every frame at 1x, every 2nd at 2x ...). Seeking, changing speed and editing the project start a new run with a new
//! generation number; the files of the old one are deleted. The client (the desktop app) polls with its playhead: that
//! frees ring slots behind it and tells the workers how far ahead to go. While paused only a few frames are primed.
//!
//! The A/V sync maths (`AvClock`, `chunk_schedule`, `frame_due`) are the contract the app's Web Audio player follows:
//! the audio clock is the master, video frames are picked from it.
use crate::model::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Seconds of audio per chunk. Short enough that the first chunk is ready almost at once, long enough that the number of
/// ffmpeg decodes per second of playback stays small.
pub const AUDIO_CHUNK: f64 = 1.0;
/// Frames kept behind the playhead (frame-step back, small rewinds) and default ring capacity.
pub const BEHIND: usize = 8;
pub const RING_CAP: usize = 96;
/// Frames rendered ahead while paused, so pressing Play shows motion at once.
pub const PRIME: usize = 6;
/// Audio chunks kept ahead of the playhead while playing.
pub const AUDIO_AHEAD: usize = 3;
pub const AUDIO_CAP: usize = 16;

// ------------------------------------------------------------------------------------------------------------------
// Ring buffer

/// A fixed-capacity ring of slots addressed by a monotonically increasing sequence number: seq `s` lives in slot
/// `s % cap`. The consumer's read position (`read`) and the `behind` margin define the live window
/// `[read - behind, read - behind + cap)`; the producer may only write inside it, so it can never overwrite a slot the
/// consumer still needs, and writing seq `s` evicts whatever older seq shared its slot.
#[derive(Debug)]
pub struct Ring<T> { slots: Vec<Option<(u64, T)>>, read: u64, behind: u64 }
impl<T> Ring<T> {
    pub fn new(cap: usize, behind: usize) -> Ring<T> { let cap = cap.max(2); Ring { slots: (0..cap).map(|_| None).collect(), read: 0, behind: (behind as u64).min(cap as u64 - 1) } }
    pub fn cap(&self) -> usize { self.slots.len() }
    pub fn read(&self) -> u64 { self.read }
    /// First seq inside the live window.
    pub fn lo(&self) -> u64 { self.read.saturating_sub(self.behind) }
    /// One past the last seq inside the live window.
    pub fn hi(&self) -> u64 { self.lo() + self.cap() as u64 }
    pub fn in_window(&self, s: u64) -> bool { s >= self.lo() && s < self.hi() }
    pub fn get(&self, s: u64) -> Option<&T> { match &self.slots[(s % self.cap() as u64) as usize] { Some((k, v)) if *k == s => Some(v), _ => None } }
    pub fn contains(&self, s: u64) -> bool { self.get(s).is_some() }
    /// Store `v` at seq `s`. Refused (Err gives `v` back) when `s` is outside the live window; otherwise Ok with whatever
    /// the slot held before (an older seq, now evicted, or a previous value for the same seq).
    pub fn put(&mut self, s: u64, v: T) -> Result<Option<(u64, T)>, T> {
        if !self.in_window(s) { return Err(v); }
        let i = (s % self.cap() as u64) as usize;
        Ok(self.slots[i].replace((s, v)))
    }
    /// Move the read position (the playhead) and return the entries that fell out of the window behind it.
    /// Moving backwards is allowed only inside the window (`behind` frames); anything ahead of the new window is evicted too.
    pub fn advance(&mut self, read: u64) -> Vec<(u64, T)> {
        self.read = read;
        let (lo, hi) = (self.lo(), self.hi());
        let mut out = vec![];
        for slot in self.slots.iter_mut() { if matches!(slot, Some((k, _)) if *k < lo || *k >= hi) { out.push(slot.take().unwrap()); } }
        out
    }
    /// The next seq the producer should fill: the first missing seq in `[read, read + ahead)`, clamped to the window.
    /// None when everything wanted is there (or `skip` says it is already being made).
    pub fn next_missing(&self, ahead: usize, skip: impl Fn(u64) -> bool) -> Option<u64> {
        let end = (self.read + ahead as u64).min(self.hi());
        (self.read..end).find(|&s| !self.contains(s) && !skip(s))
    }
    /// How many consecutive seqs from the read position are already filled.
    pub fn ready_ahead(&self) -> usize { (self.read..self.hi()).take_while(|&s| self.contains(s)).count() }
    /// Empty the ring (a new run) and return everything it held.
    pub fn clear(&mut self) -> Vec<(u64, T)> { self.read = 0; self.slots.iter_mut().filter_map(|s| s.take()).collect() }
    pub fn len(&self) -> usize { self.slots.iter().filter(|s| s.is_some()).count() }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn iter(&self) -> impl Iterator<Item = &(u64, T)> { self.slots.iter().flatten() }
}

/// Where a run started and how it moves through the timeline. Sequence number `n` is frame `start + dir * stride * n`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run { pub start: i64, pub dir: i64, pub stride: i64 }
impl Run {
    pub fn frame(&self, seq: u64) -> i64 { self.start + self.dir * self.stride * seq as i64 }
    /// The seq at or just before frame `f` along the run (None for a frame behind the run's start).
    pub fn seq_at(&self, f: i64) -> Option<u64> { let d = (f - self.start) * self.dir; if d < 0 { None } else { Some((d / self.stride.max(1)) as u64) } }
    /// Speed (1, 2, 4, -1 ...) as a run: direction and stride.
    pub fn for_rate(start: i64, rate: f64) -> Run { let r = if rate.is_finite() && rate != 0.0 { rate } else { 1.0 }; Run { start, dir: if r < 0.0 { -1 } else { 1 }, stride: (r.abs().round() as i64).clamp(1, 16) } }
}

// ------------------------------------------------------------------------------------------------------------------
// A/V sync maths

/// Maps the audio device clock to timeline time. Started at audio time `anchor_audio` with the playhead at `anchor_tl`,
/// moving at `rate` timeline seconds per audio second. `latency` is the output latency: what is audible now was
/// scheduled `latency` seconds ago, so the picture follows what is heard, not what was handed to the device.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AvClock { pub anchor_tl: f64, pub anchor_audio: f64, pub rate: f64, pub latency: f64 }
impl AvClock {
    pub fn new(anchor_tl: f64, anchor_audio: f64, rate: f64, latency: f64) -> AvClock { AvClock { anchor_tl, anchor_audio, rate: if rate.is_finite() && rate != 0.0 { rate } else { 1.0 }, latency: if latency.is_finite() { latency.max(0.0) } else { 0.0 } } }
    /// Timeline time being heard at audio time `a` (never before the anchor: during the latency the first frame holds).
    pub fn timeline_at(&self, a: f64) -> f64 { self.anchor_tl + (a - self.latency - self.anchor_audio).max(0.0) * self.rate }
    /// Audio time at which timeline time `tl` must be handed to the device (start time for a source node).
    pub fn audio_at(&self, tl: f64) -> f64 { self.anchor_audio + (tl - self.anchor_tl) / self.rate }
}

/// When to start an audio chunk covering timeline `[c0, c0 + len)` given the device clock `now`: (start audio time,
/// offset into the chunk in seconds of chunk audio, how much of it is left to play). A chunk whose start is already in
/// the past starts now with the elapsed part skipped; one that is entirely past gives None.
pub fn chunk_schedule(clock: &AvClock, c0: f64, len: f64, now: f64) -> Option<(f64, f64, f64)> {
    if clock.rate <= 0.0 { return None; }
    let when = clock.audio_at(c0);
    let offset = ((now - when) * clock.rate).max(0.0);
    if offset >= len - 1e-9 { return None; }
    Some((when.max(now), offset, len - offset))
}

/// The frame index to show at audio time `a`: the frame whose span contains the timeline time being heard.
pub fn frame_due(clock: &AvClock, a: f64, fps: f64) -> i64 { if !(fps > 0.0) { return 0; } (clock.timeline_at(a) * fps + 1e-6).floor() as i64 }

/// A/V offset of a shown frame against the clock, in frames: positive = the picture is late. A player re-anchors (drops or
/// holds frames) when this passes one frame.
pub fn drift_frames(clock: &AvClock, a: f64, shown: i64, fps: f64) -> f64 { (clock.timeline_at(a) * fps) - shown as f64 - 0.5 }

/// Frame skip for normal-speed playback (1 = every frame, 2 = every 2nd ...) given the frames per second the stream makes:
/// the smallest skip that keeps 10% headroom over real time, stepping back down once there is 30% to spare.
pub fn choose_skip(fps: f64, made: f64, cur: i64) -> i64 {
    if !(made > 0.0) || !(fps > 0.0) { return cur; }
    let need = |d: i64| fps / d as f64;
    if made < need(cur) * 0.95 { return (1..=4).find(|&d| made >= need(d) * 1.1).unwrap_or(4).max(cur); }
    if cur > 1 && made >= need(cur - 1) * 1.3 { return cur - 1; }
    cur
}

/// The audio chunks (index k covers [k * AUDIO_CHUNK, (k+1) * AUDIO_CHUNK)) needed from timeline time `t`, `ahead` of them,
/// clamped to the timeline.
pub fn chunks_from(t: f64, ahead: usize, end: f64) -> Vec<u64> {
    if !(end > 0.0) { return vec![]; }
    let last = ((end / AUDIO_CHUNK).ceil() as u64).saturating_sub(1);
    let k0 = (t.max(0.0) / AUDIO_CHUNK).floor() as u64;
    (k0..k0 + ahead as u64).filter(|&k| k <= last).collect()
}

// ------------------------------------------------------------------------------------------------------------------
// Audio for one window

/// Audio of one clip for its local time [lt0, lt1), decoded through the same per-clip filters as the full mix.
/// Plain clips seek straight to the window (with a short pre-roll so filters settle before the window starts); reversed
/// and time-remapped clips decode whole once (memoized in `memo`) and are sliced.
fn decode_window(path: &str, c: &Clip, lt0: f64, lt1: f64, memo: &mut BTreeMap<String, Arc<Vec<f32>>>) -> Vec<f32> {
    let sr = crate::audio::SR as f64;
    let n = ((lt1 - lt0) * sr).round().max(0.0) as usize;
    let mut out = vec![0f32; n * 2];
    if c.audio.reverse || c.time_remap.is_some() {
        let key = format!("{path}|{}", serde_json::to_string(c).unwrap_or_default());
        if memo.len() > 8 && !memo.contains_key(&key) { memo.clear(); }
        let full = memo.entry(key).or_insert_with(|| Arc::new(crate::audio::decode_clip(path, c))).clone();
        let s0 = (lt0 * sr).round() as usize * 2;
        for (i, v) in out.iter_mut().enumerate() { if let Some(x) = full.get(s0 + i) { *v = *x; } }
        return out;
    }
    let pre = (lt0 * c.speed).min(0.1).max(0.0) / c.speed.max(1e-6);
    let mut w = c.clone();
    w.in_point = c.in_point + (lt0 - pre) * c.speed;
    w.duration = (lt1 - lt0) + pre;
    w.audio.fade_in = 0.0; w.audio.fade_out = 0.0;
    let pcm = crate::audio::decode_clip(path, &w);
    let skip = (pre * sr).round() as usize * 2;
    for (i, v) in out.iter_mut().enumerate() { if let Some(x) = pcm.get(skip + i) { *v = *x; } }
    out
}

/// Mix the timeline's audio for [t0, t1) only: the same gains as `audio::mix` (volume and duck keyframes, fades, pan,
/// track mute / solo, split-edit lead and tail), then the same limiter. Interleaved stereo at `audio::SR`.
/// Not applied per window: the master speech leveler (a slow whole-timeline leveler; export still applies it).
pub fn mix_window(p: &Project, t0: f64, t1: f64) -> Vec<f32> { mix_window_memo(p, t0, t1, &mut BTreeMap::new()) }
fn mix_window_memo(p: &Project, t0: f64, t1: f64, memo: &mut BTreeMap<String, Arc<Vec<f32>>>) -> Vec<f32> {
    let sr = crate::audio::SR as f64;
    let n = ((t1 - t0).max(0.0) * sr).round() as usize;
    let mut out = vec![0f32; n * 2];
    for t in &p.tracks { for c in &t.clips {
        let Source::Media { asset } = &c.source else { continue };
        if effective_mute(&p.tracks, t) || c.audio.mute { continue; }
        let Some(a) = p.assets.get(asset) else { continue };
        let (v, ko) = crate::audio::audio_view(c); let c = &*v;
        let (a0, b0) = (t0.max(c.start), t1.min(c.start + c.duration));
        if b0 <= a0 + 0.5 / sr { continue; }
        let (lt0, lt1) = (a0 - c.start, b0 - c.start);
        let s = decode_window(&p.resolve(&a.path), c, lt0, lt1, memo);
        let off = ((a0 - t0) * sr).round() as usize * 2;
        for i in (0..s.len()).step_by(2) {
            let lt = lt0 + (i / 2) as f64 / sr; let kt = if ko == 0.0 { lt } else { lt - ko };
            let mut g = (crate::keyframe::clamp_prop("volume", c.audio.volume.at(kt)) * c.audio.duck_gain.at(kt)) as f32;
            if c.audio.fade_in > 0.0 && lt < c.audio.fade_in { g *= (lt / c.audio.fade_in) as f32; }
            let rem = c.duration - lt; if c.audio.fade_out > 0.0 && rem < c.audio.fade_out { g *= (rem / c.audio.fade_out).max(0.0) as f32; }
            let pv = crate::keyframe::clamp_prop("pan", c.audio.pan.at(kt));
            if off + i + 1 < out.len() { out[off + i] += s[i] * g * (1.0 - pv.max(0.0)) as f32; out[off + i + 1] += s[i + 1] * g * (1.0 + pv.min(0.0)) as f32; }
        }
    } }
    crate::audio::limit(&mut out, -1.5);
    out
}

// ------------------------------------------------------------------------------------------------------------------
// Frames on disk

/// An uncompressed 32-bit top-down BMP of RGBA8 pixels (BGRA in the file, alpha kept). Every browser decodes it.
pub fn bmp(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let data = w * h * 4; let mut b = Vec::with_capacity(54 + data);
    b.extend(b"BM"); b.extend(((54 + data) as u32).to_le_bytes()); b.extend(0u32.to_le_bytes()); b.extend(54u32.to_le_bytes());
    b.extend(40u32.to_le_bytes()); b.extend((w as i32).to_le_bytes()); b.extend((-(h as i32)).to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(32u16.to_le_bytes());
    b.extend(0u32.to_le_bytes()); b.extend((data as u32).to_le_bytes()); b.extend(2835u32.to_le_bytes()); b.extend(2835u32.to_le_bytes()); b.extend(0u32.to_le_bytes()); b.extend(0u32.to_le_bytes());
    for px in rgba[..data.min(rgba.len())].chunks_exact(4) { b.extend([px[2], px[1], px[0], px[3]]); }
    b
}

/// Draft size for a scale (0.5 half, 0.25 quarter): even dimensions, at least 2.
pub fn draft_size(w: u32, h: u32, scale: f64) -> (usize, usize) {
    if scale >= 1.0 { return (w as usize, h as usize); }
    ((((w as f64 * scale) / 2.0).round() as usize * 2).max(2), (((h as f64 * scale) / 2.0).round() as usize * 2).max(2))
}

fn render_draft(p: &Project, t: f64, cache: &crate::render::Cache, nw: usize, nh: usize) -> Result<Vec<u8>, String> {
    let f = crate::render::export_frame_rgba8(p, t, cache)?;
    Ok(crate::ext::preview_range::box_downscale(&f, p.width as usize, p.height as usize, nw, nh))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Content fingerprint of a project (what decides the pixels and the sound).
pub fn project_rev(p: &Project) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(p).unwrap_or_default().hash(&mut h);
    h.finish()
}

// ------------------------------------------------------------------------------------------------------------------
// The stream

#[derive(Clone, Debug, serde::Serialize)]
pub struct FrameOut { pub seq: u64, pub index: i64, pub time: f64, pub name: String, pub ms: f64 }
#[derive(Clone, Debug, serde::Serialize)]
pub struct ChunkOut { pub chunk: u64, pub start: f64, pub end: f64, pub name: String, pub audible: bool, pub ms: f64 }

struct State {
    project: Arc<Project>, rev: u64, gen: u32, arev: u32, run: Run, ring: Ring<FrameOut>, busy: Vec<u64>,
    playing: bool, audio_on: bool, playhead: f64, audio: BTreeMap<u64, ChunkOut>, audio_busy: Option<u64>,
    rate: f64, skip: i64, run_rendered: u64,
    error: Option<String>, audio_error: Option<String>, rendered: u64, render_ms: f64, fps_made: f64, first_ms: Option<f64>, started: std::time::Instant, run_started: std::time::Instant,
    proxies: Vec<String>, scrub: Vec<(u64, i64, PathBuf)>,
}

pub struct Stream { dir: PathBuf, scale: f64, w: usize, h: usize, fps: f64, end: f64, st: Mutex<State>, cv: Condvar, stop: Arc<AtomicBool>, tok: crate::guard::Cancel, scrub_cache: Mutex<crate::render::Cache> }

static STREAM: Mutex<Option<Arc<Stream>>> = Mutex::new(None);

/// Decode cache budget of the playback worker: enough for full 24-frame windows of a few 1080p layers (one ffmpeg start per
/// window per clip instead of one every few frames), never more than a third of the engine's memory budget.
fn stream_decode_budget() -> usize { (crate::guard::limits().mem_bytes / 3).min(1 << 30).max(1 << 20) }
fn frame_name(gen: u32, index: i64) -> String { format!("{gen:08x}{:08x}.bmp", index as u32) }
fn chunk_name(arev: u32, k: u64) -> String { format!("{arev:08x}{:08x}.wav", k as u32) }
fn rm(dir: &Path, name: &str) { let _ = std::fs::remove_file(dir.join(name)); }

impl Stream {
    fn fps(&self) -> f64 { self.fps }
    fn last_index(&self) -> i64 { ((self.end * self.fps).ceil() as i64 - 1).max(0) }
    fn index_at(&self, t: f64) -> i64 { ((t.max(0.0) * self.fps + 1e-6).floor() as i64).min(self.last_index()) }

    /// A new run from `t` at `rate`: new generation, ring emptied (its files deleted).
    fn restart(&self, st: &mut State, t: f64, rate: f64) {
        for (_, f) in st.ring.clear() { rm(&self.dir, &f.name); }
        st.gen = st.gen.wrapping_add(1);
        st.rate = rate;
        st.run = Run::for_rate(self.index_at(t), rate);
        st.run.stride = (st.run.stride * st.skip).clamp(1, 16);
        st.audio_on = rate == 1.0;
        st.run_rendered = 0;
        st.playhead = t; st.first_ms = None; st.run_started = std::time::Instant::now(); st.error = None; st.audio_error = None;
        self.cv.notify_all();
    }
    /// Swap in a new project snapshot (after an edit): every frame and chunk is stale.
    fn reload(&self, st: &mut State, p: Project, rev: u64) {
        st.project = Arc::new(p); st.rev = rev; st.arev = st.arev.wrapping_add(1);
        for (_, c) in std::mem::take(&mut st.audio) { rm(&self.dir, &c.name); }
        let (t, rate) = (st.playhead, st.rate);
        self.restart(st, t, rate);
    }

    fn video_loop(self: Arc<Self>) {
        let mut cache = crate::render::Cache::new();
        cache.decode_budget = Some(stream_decode_budget());
        let threads = std::thread::available_parallelism().map(|x| x.get()).unwrap_or(2).saturating_sub(1).clamp(1, 8);
        // frames per batch; halved whenever a batch's decode window does not fit the decode cache budget (several 1080p+ layers)
        let mut max_batch = threads * 2;
        while !self.stop.load(Ordering::Relaxed) {
            // what to render next: a batch of consecutive missing seqs, small right after a seek so the first frame is quick
            let (p, run, gen, seqs) = {
                let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if self.stop.load(Ordering::Relaxed) { return; }
                    let ahead = if st.playing { st.ring.cap() - BEHIND } else { PRIME };
                    let last = self.last_index(); let run = st.run;
                    let busy = st.busy.clone();
                    let in_range = |s: u64| { let f = run.frame(s); f >= 0 && f <= last };
                    let first = st.ring.next_missing(ahead, |s| busy.contains(&s) || !in_range(s));
                    if let (Some(s0), None) = (first, st.error.as_ref()) {
                        let want = if st.ring.ready_ahead() == 0 { 1 } else { max_batch };
                        let mut v = vec![s0]; let mut s = s0 + 1;
                        while v.len() < want && s < st.ring.read() + ahead as u64 && st.ring.in_window(s) && in_range(s) { if !st.ring.contains(s) && !busy.contains(&s) { v.push(s); } s += 1; }
                        st.busy.extend(v.iter().copied());
                        break (st.project.clone(), run, st.gen, v);
                    }
                    st = self.cv.wait_timeout(st, std::time::Duration::from_millis(250)).unwrap_or_else(|e| e.into_inner()).0;
                }
            };
            let (w, h, batch_t) = (self.w, self.h, std::time::Instant::now());
            let times: Vec<f64> = seqs.iter().map(|&s| run.frame(s) as f64 / self.fps()).collect();
            let (t0, t1) = (times.iter().cloned().fold(f64::MAX, f64::min), times.iter().cloned().fold(f64::MIN, f64::max));
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<Vec<Result<(Vec<u8>, f64), String>>, String> {
                let _nb = crate::analysis_proxy::NonBlocking::new();
                crate::render::prepare_stream_range(&p, &mut cache, t0, t1)?;
                let cache = &cache; let slots: Vec<Mutex<Option<Result<(Vec<u8>, f64), String>>>> = times.iter().map(|_| Mutex::new(None)).collect();
                let next = std::sync::atomic::AtomicUsize::new(0);
                std::thread::scope(|sc| { for _ in 0..threads.min(times.len()) { let (p, slots, next, times) = (&p, &slots, &next, &times); crate::guard::scope_spawn(sc, move || loop {
                    let k = next.fetch_add(1, Ordering::SeqCst); if k >= times.len() { break; }
                    let t = std::time::Instant::now();
                    let r = render_draft(p, times[k], cache, w, h).map(|px| (bmp(&px, w, h), t.elapsed().as_secs_f64() * 1000.0));
                    *slots[k].lock().unwrap() = Some(r);
                }); } });
                Ok(slots.into_iter().map(|s| s.into_inner().unwrap().unwrap_or_else(|| Err("not rendered".into()))).collect())
            }));
            let proxies = cache.proxy_assets();
            let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
            st.busy.retain(|s| !seqs.contains(s));
            if self.stop.load(Ordering::Relaxed) { return; }
            if st.gen != gen { continue; } // a seek or an edit made this batch stale
            st.proxies = proxies;
            let out = match res { Ok(Ok(v)) => v,
                Ok(Err(e)) if e.contains("exceeds cache budget") && seqs.len() > 1 => { max_batch = (seqs.len() / 2).max(1); continue; }
                Ok(Err(e)) => { st.error = Some(e); continue; } Err(_) => { st.error = Some("the preview render stopped (cancelled or out of memory)".into()); continue; } };
            for ((s, r), t) in seqs.iter().zip(out).zip(&times) {
                match r {
                    Ok((bytes, ms)) => {
                        let index = run.frame(*s); let name = frame_name(gen, index);
                        if let Err(e) = write_atomic(&self.dir.join(&name), &bytes) { st.error = Some(e); break; }
                        st.rendered += 1; st.run_rendered += 1; st.render_ms = if st.rendered == 1 { ms } else { st.render_ms * 0.9 + ms * 0.1 };
                        if st.first_ms.is_none() { st.first_ms = Some(st.run_started.elapsed().as_secs_f64() * 1000.0); }
                        match st.ring.put(*s, FrameOut { seq: *s, index, time: *t, name: name.clone(), ms }) {
                            Ok(Some((_, old))) if old.name != name => rm(&self.dir, &old.name),
                            Ok(_) => {}
                            Err(_) => rm(&self.dir, &name), // the playhead moved past it meanwhile
                        }
                    }
                    Err(e) => { st.error = Some(e); break; }
                }
            }
            let secs = batch_t.elapsed().as_secs_f64();
            if secs > 0.0 && seqs.len() > 1 { let f = seqs.len() as f64 / secs; st.fps_made = if st.fps_made == 0.0 { f } else { st.fps_made * 0.7 + f * 0.3 }; }
            self.cv.notify_all();
        }
    }

    fn audio_loop(self: Arc<Self>) {
        let mut memo = BTreeMap::new();
        while !self.stop.load(Ordering::Relaxed) {
            let (p, k, arev) = {
                let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if self.stop.load(Ordering::Relaxed) { return; }
                    let ahead = if st.playing { AUDIO_AHEAD } else { 1 };
                    let want = if st.audio_on { chunks_from(st.playhead, ahead, self.end) } else { vec![] };
                    if let Some(k) = want.into_iter().find(|k| !st.audio.contains_key(k)) { st.audio_busy = Some(k); break (st.project.clone(), k, st.arev); }
                    st = self.cv.wait_timeout(st, std::time::Duration::from_millis(250)).unwrap_or_else(|e| e.into_inner()).0;
                }
            };
            let (c0, c1) = (k as f64 * AUDIO_CHUNK, ((k + 1) as f64 * AUDIO_CHUNK).min(self.end));
            let t = std::time::Instant::now();
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { let pcm = mix_window_memo(&p, c0, c1, &mut memo); let audible = pcm.iter().any(|v| v.abs() > 1e-6); let name = chunk_name(arev, k); crate::audio::write_wav(self.dir.join(&name).to_str().unwrap_or(""), &pcm).map(|_| (name, audible)) }));
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
            st.audio_busy = None;
            if self.stop.load(Ordering::Relaxed) { if let Ok(Ok((name, _))) = &res { rm(&self.dir, name); } return; }
            match res {
                Ok(Ok((name, audible))) => {
                    if st.arev != arev { rm(&self.dir, &name); continue; }
                    st.audio.insert(k, ChunkOut { chunk: k, start: c0, end: c1, name, audible, ms });
                    // keep the chunks around the playhead, drop the farthest
                    while st.audio.len() > AUDIO_CAP { let ph = (st.playhead / AUDIO_CHUNK) as i64; let far = *st.audio.keys().max_by_key(|&&x| (x as i64 - ph).abs()).unwrap(); if let Some(c) = st.audio.remove(&far) { rm(&self.dir, &c.name); } }
                }
                Ok(Err(e)) => { st.audio_on = false; st.audio_error = Some(e); }
                Err(_) => { st.audio_on = false; st.audio_error = Some("the mix stopped (cancelled or out of memory)".into()); }
            }
            self.cv.notify_all();
        }
    }

    /// What the client needs after a poll: the ready frames from the playhead on, the audio chunks around it, and stats.
    fn report(&self, st: &State) -> serde_json::Value {
        let ph = st.run.seq_at(self.index_at(st.playhead)).unwrap_or(0);
        let mut frames: Vec<&FrameOut> = st.ring.iter().map(|(_, f)| f).filter(|f| f.seq + BEHIND as u64 >= ph).collect();
        frames.sort_by_key(|f| f.seq);
        let audio: Vec<&ChunkOut> = st.audio.values().filter(|c| c.end > st.playhead - AUDIO_CHUNK).collect();
        let elapsed = st.started.elapsed().as_secs_f64();
        serde_json::json!({
            "gen": st.gen, "rev": format!("{:016x}", st.rev), "run": {"start": st.run.start, "dir": st.run.dir, "stride": st.run.stride},
            "fps": self.fps, "duration": self.end, "width": self.w, "height": self.h, "scale": self.scale, "playing": st.playing,
            "frames": frames, "audio": audio, "audio_on": st.audio_on, "ready_ahead": st.ring.ready_ahead(), "ring": st.ring.cap(),
            "render_ms": st.render_ms, "render_fps": st.fps_made, "skip": st.skip, "rate": st.rate,
            "first_frame_ms": st.first_ms, "rendered": st.rendered, "uptime_s": elapsed, "proxies": st.proxies, "error": st.error, "audio_error": st.audio_error,
        })
    }
}

fn current() -> Option<Arc<Stream>> { STREAM.lock().unwrap_or_else(|e| e.into_inner()).clone() }

/// Stop the stream (if any) and delete its files.
pub fn stop() -> bool {
    let Some(s) = STREAM.lock().unwrap_or_else(|e| e.into_inner()).take() else { return false };
    s.stop.store(true, Ordering::Relaxed); s.tok.store(true, Ordering::Relaxed); s.cv.notify_all();
    let mut st = s.st.lock().unwrap_or_else(|e| e.into_inner());
    for (_, f) in st.ring.clear() { rm(&s.dir, &f.name); }
    for (_, c) in std::mem::take(&mut st.audio) { rm(&s.dir, &c.name); }
    for (_, _, f) in std::mem::take(&mut st.scrub) { let _ = std::fs::remove_file(f); }
    true
}

/// Start (or restart) the stream for `p` into `dir`, at `scale` (0.5 or 0.25), from timeline time `t`.
pub fn start(p: &Project, dir: &Path, scale: f64, t: f64, rate: f64, playing: bool) -> Result<serde_json::Value, String> {
    let end = p.duration();
    if !(end > 0.0) { return Err("out_of_range: the timeline is empty".into()); }
    if !(scale > 0.0 && scale <= 1.0) { return Err("out_of_range: scale must be between 0 and 1 (0.5 half, 0.25 quarter)".into()); }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    stop();
    let fps = if p.fps > 0.0 { p.fps } else { 30.0 };
    let (w, h) = draft_size(p.width, p.height, scale);
    let now = std::time::Instant::now();
    let st = State { project: Arc::new(p.clone()), rev: project_rev(p), gen: 0, arev: 0, run: Run::for_rate(0, 1.0), ring: Ring::new(RING_CAP, BEHIND), busy: vec![],
        playing, audio_on: true, rate: 1.0, skip: 1, run_rendered: 0, playhead: t, audio: BTreeMap::new(), audio_busy: None, error: None, audio_error: None, rendered: 0, render_ms: 0.0, fps_made: 0.0, first_ms: None, started: now, run_started: now, proxies: vec![], scrub: vec![] };
    let tok = crate::guard::new_token();
    let s = Arc::new(Stream { dir: dir.to_path_buf(), scale, w, h, fps, end, st: Mutex::new(st), cv: Condvar::new(), stop: Arc::new(AtomicBool::new(false)), tok: tok.clone(), scrub_cache: Mutex::new(crate::render::Cache::new()) });
    { let mut st = s.st.lock().unwrap(); s.restart(&mut st, t.clamp(0.0, end), rate); st.playing = playing; }
    for (name, audio) in [("kimo-preview-stream", false), ("kimo-preview-audio", true)] {
        let (s2, tok2) = (s.clone(), tok.clone());
        // a token of the stream's own (not the starting call's): stop() cancels the workers' renders at their next guard check
        crate::guard::spawn_builder_with_token(std::thread::Builder::new().name(name.into()), move || crate::guard::with_token(&tok2, || if audio { s2.audio_loop() } else { s2.video_loop() })).map_err(|e| format!("cannot start the preview worker: {e}"))?;
    }
    *STREAM.lock().unwrap_or_else(|e| e.into_inner()) = Some(s.clone());
    let st = s.st.lock().unwrap();
    Ok(s.report(&st))
}

/// Move the playhead / change speed or play state. A jump outside the buffered stretch, or another speed, starts a new
/// run; `p` is checked against the stream's snapshot and an edit restarts everything.
pub fn update(p: &Project, t: f64, rate: Option<f64>, playing: Option<bool>, seek: bool) -> Result<serde_json::Value, String> {
    let s = current().ok_or("no preview stream: start one first")?;
    let rev = project_rev(p);
    let mut st = s.st.lock().unwrap_or_else(|e| e.into_inner());
    if (p.duration() - s.end).abs() > 1e-9 || p.width != st.project.width || p.height != st.project.height || p.fps != st.project.fps {
        drop(st); let (dir, scale) = (s.dir.clone(), s.scale);
        return start(p, &dir, scale, t, rate.unwrap_or(1.0), playing.unwrap_or(false));
    }
    let t = t.clamp(0.0, s.end);
    if let Some(pl) = playing { st.playing = pl; }
    let cur_rate = st.rate;
    let rate_changed = rate.is_some_and(|r| Run::for_rate(0, r) != Run::for_rate(0, cur_rate));
    if rev != st.rev { st.playhead = t; if let Some(r) = rate { st.rate = r; } s.reload(&mut st, p.clone(), rev); }
    else if seek || rate_changed {
        let f = s.index_at(t); let r = rate.unwrap_or(cur_rate);
        // a seek onto a frame the ring already holds (same speed) only moves the read position
        let keep = !rate_changed && st.run.seq_at(f).is_some_and(|q| st.ring.contains(q) && st.run.frame(q) == f);
        if keep { st.playhead = t; let q = st.run.seq_at(f).unwrap(); for (_, x) in st.ring.advance(q) { rm(&s.dir, &x.name); } }
        else { s.restart(&mut st, t, r); }
    } else {
        st.playhead = t;
        let f = s.index_at(t);
        match st.run.seq_at(f) {
            Some(q) if q + BEHIND as u64 >= st.ring.read() => { for (_, x) in st.ring.advance(q) { rm(&s.dir, &x.name); } }
            _ => s.restart(&mut st, t, cur_rate), // moved behind the run: start over from here
        }
        // a machine that cannot render every frame in real time skips frames instead of stalling, so the sound plays on
        if st.playing && st.rate == 1.0 && st.run_rendered >= 24 {
            let want = choose_skip(s.fps, st.fps_made, st.skip);
            if want != st.skip { st.skip = want; s.restart(&mut st, t, 1.0); }
        }
    }
    s.cv.notify_all();
    Ok(s.report(&st))
}

/// One frame for scrubbing, now: from the ring when it holds it, else rendered on the calling thread at the stream's
/// draft size (draft decode window, proxies when they exist). Remembered per (project, frame) for repeat scrubs.
pub fn frame(p: &Project, t: f64) -> Result<serde_json::Value, String> {
    let s = current().ok_or("no preview stream: start one first")?;
    let t0 = std::time::Instant::now();
    let rev = project_rev(p); let idx = s.index_at(t);
    {
        let st = s.st.lock().unwrap_or_else(|e| e.into_inner());
        if st.rev == rev { if let Some(f) = st.run.seq_at(idx).and_then(|q| st.ring.get(q)).filter(|f| f.index == idx) {
            return Ok(serde_json::json!({"index": idx, "time": f.time, "name": f.name, "cached": true, "ms": t0.elapsed().as_secs_f64() * 1000.0, "width": s.w, "height": s.h}));
        } }
        if let Some((_, _, f)) = st.scrub.iter().find(|(r, i, _)| *r == rev && *i == idx) {
            return Ok(serde_json::json!({"index": idx, "time": idx as f64 / s.fps, "name": f.file_name().map(|n| n.to_string_lossy().to_string()), "cached": true, "ms": t0.elapsed().as_secs_f64() * 1000.0, "width": s.w, "height": s.h}));
        }
    }
    let at = idx as f64 / s.fps;
    let px = {
        let mut cache = s.scrub_cache.lock().unwrap_or_else(|e| e.into_inner());
        let _nb = crate::analysis_proxy::NonBlocking::new();
        crate::render::prepare_stream_frame(p, &mut cache, at)?;
        render_draft(p, at, &cache, s.w, s.h)?
    };
    let name = format!("{rev:016x}{:08x}.bmp", idx as u32);
    let path = s.dir.join(&name);
    write_atomic(&path, &bmp(&px, s.w, s.h))?;
    let mut st = s.st.lock().unwrap_or_else(|e| e.into_inner());
    st.scrub.push((rev, idx, path));
    while st.scrub.len() > 48 { let (_, _, f) = st.scrub.remove(0); let _ = std::fs::remove_file(f); }
    Ok(serde_json::json!({"index": idx, "time": at, "name": name, "cached": false, "ms": t0.elapsed().as_secs_f64() * 1000.0, "width": s.w, "height": s.h}))
}

pub fn status() -> Option<serde_json::Value> { let s = current()?; let st = s.st.lock().unwrap_or_else(|e| e.into_inner()); Some(s.report(&st)) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_stores_by_sequence_and_evicts_behind_the_playhead() {
        let mut r: Ring<u64> = Ring::new(8, 2);
        assert_eq!((r.lo(), r.hi()), (0, 8));
        for s in 0..8 { assert!(r.put(s, s * 10).unwrap().is_none()); }
        assert!(r.put(8, 80).is_err(), "seq 8 would overwrite seq 0's slot while it is still in the window");
        assert_eq!(r.ready_ahead(), 8);
        // the playhead moves to 5: seqs 0..3 fall behind the two-frame margin
        let gone: Vec<u64> = { let mut g: Vec<u64> = r.advance(5).into_iter().map(|(s, _)| s).collect(); g.sort(); g };
        assert_eq!(gone, vec![0, 1, 2]);
        assert_eq!((r.lo(), r.hi()), (3, 11));
        assert_eq!(r.get(3), Some(&30), "inside the behind margin: kept for frame-step back");
        assert_eq!(r.ready_ahead(), 3, "5, 6, 7 ready");
        assert_eq!(r.next_missing(10, |_| false), Some(8));
        assert_eq!(r.next_missing(10, |s| s == 8), Some(9), "a seq being rendered is skipped");
        assert!(r.put(8, 80).unwrap().is_none(), "its slot (0) was freed");
        assert_eq!(r.get(0), None, "seq 0 is gone, seq 8 owns the slot");
        assert_eq!(r.get(8), Some(&80));
        assert!(r.put(11, 1).is_err() && r.put(2, 1).is_err(), "outside the window either side");
        assert_eq!(r.len(), 6);
        assert_eq!(r.clear().len(), 6);
        assert!(r.is_empty() && r.read() == 0);
    }

    #[test]
    fn ring_never_lets_the_producer_overrun_the_consumer() {
        // a producer that always fills the next missing seq, a consumer that reads one per step: every read finds its frame
        // once it was produced, and nothing it still needs is ever overwritten
        let mut r: Ring<u64> = Ring::new(16, 4);
        let mut next_read = 0u64;
        for step in 0..500u64 {
            for _ in 0..3 { if let Some(s) = r.next_missing(12, |_| false) { r.put(s, s).unwrap(); } }
            if step % 2 == 0 { assert_eq!(r.get(next_read), Some(&next_read), "seq {next_read} at step {step}"); next_read += 1; r.advance(next_read); }
            assert!(r.len() <= 16);
            for (s, v) in r.iter() { assert_eq!(s, v); assert!(r.in_window(*s)); }
        }
        // moving back inside the margin keeps the frames there
        r.advance(next_read - 3); assert!(r.contains(next_read - 3));
    }

    #[test]
    fn runs_map_sequence_numbers_to_frames_both_ways() {
        let fwd = Run::for_rate(100, 1.0); assert_eq!((fwd.frame(0), fwd.frame(5)), (100, 105)); assert_eq!(fwd.seq_at(105), Some(5)); assert_eq!(fwd.seq_at(99), None);
        let fast = Run::for_rate(100, 4.0); assert_eq!(fast.frame(3), 112); assert_eq!(fast.seq_at(113), Some(3), "between lattice frames: the one before");
        let back = Run::for_rate(100, -2.0); assert_eq!((back.dir, back.stride), (-1, 2)); assert_eq!(back.frame(10), 80); assert_eq!(back.seq_at(80), Some(10)); assert_eq!(back.seq_at(101), None);
        assert_eq!(Run::for_rate(0, f64::NAN), Run::for_rate(0, 1.0));
    }

    #[test]
    fn the_audio_clock_drives_the_picture() {
        // playback started at timeline 10 s when the audio clock read 2 s, 40 ms output latency, 25 fps
        let c = AvClock::new(10.0, 2.0, 1.0, 0.04);
        assert_eq!(c.timeline_at(2.0), 10.0, "during the latency the first frame holds");
        assert!((c.timeline_at(3.04) - 11.0).abs() < 1e-12);
        assert!((c.audio_at(11.0) - 3.0).abs() < 1e-12, "timeline 11 s is handed to the device at audio 3 s");
        assert_eq!(frame_due(&c, 2.0, 25.0), 250);
        assert_eq!(frame_due(&c, 2.04 + 0.039, 25.0), 250, "frame 250 spans [10.00, 10.04)");
        assert_eq!(frame_due(&c, 2.04 + 0.04, 25.0), 251);
        assert_eq!(frame_due(&c, 3.04, 25.0), 275);
        // double speed: a second of audio is two seconds of timeline
        let f = AvClock::new(0.0, 0.0, 2.0, 0.0);
        assert!((f.timeline_at(1.5) - 3.0).abs() < 1e-12 && (f.audio_at(3.0) - 1.5).abs() < 1e-12);
        // drift: showing frame 274 at the time of frame 275 is about one frame late
        assert!((drift_frames(&c, 3.04 + 0.02, 274, 25.0) - 1.0).abs() < 1e-9);
        assert!(drift_frames(&c, 3.04 + 0.02, 275, 25.0).abs() < 1e-9, "mid-frame: on time");
    }

    #[test]
    fn audio_chunks_are_scheduled_back_to_back_and_join_mid_chunk() {
        let c = AvClock::new(10.3, 5.0, 1.0, 0.0);
        // chunk 10 covers [10, 11): started 0.3 s in, at once
        let (when, off, left) = chunk_schedule(&c, 10.0, 1.0, 5.0).unwrap();
        assert!((when - 5.0).abs() < 1e-12 && (off - 0.3).abs() < 1e-12 && (left - 0.7).abs() < 1e-12);
        // chunk 11 starts exactly where chunk 10 ends: no gap, no overlap
        let (w11, o11, _) = chunk_schedule(&c, 11.0, 1.0, 5.0).unwrap();
        assert!((w11 - (when + left)).abs() < 1e-12 && o11 == 0.0);
        // a chunk that arrives late joins mid-way, still on the same clock
        let (w, o, l) = chunk_schedule(&c, 11.0, 1.0, 5.95).unwrap();
        assert!((w - 5.95).abs() < 1e-12 && (o - 0.25).abs() < 1e-9 && (l - 0.75).abs() < 1e-9);
        assert!(chunk_schedule(&c, 10.0, 1.0, 5.8).is_none(), "entirely past");
        assert!(chunk_schedule(&AvClock::new(0.0, 0.0, -1.0, 0.0), 0.0, 1.0, 0.0).is_none(), "no sound in reverse");
        assert_eq!(chunks_from(2.5, 3, 4.2), vec![2, 3, 4]);
        assert_eq!(chunks_from(3.9, 3, 4.2), vec![3, 4], "clamped to the timeline");
        assert!(chunks_from(0.0, 3, 0.0).is_empty());
    }

    #[test]
    fn a_slow_machine_skips_frames_instead_of_stalling() {
        assert_eq!(choose_skip(25.0, 30.0, 1), 1, "fast enough: every frame");
        assert_eq!(choose_skip(25.0, 10.5, 1), 3, "10.5 fps made: every 3rd frame (8.3 needed) keeps real time");
        assert_eq!(choose_skip(25.0, 14.0, 1), 2);
        assert_eq!(choose_skip(25.0, 3.0, 1), 4, "never more than every 4th");
        assert_eq!(choose_skip(25.0, 30.0, 2), 2, "not enough spare to go back to every frame (hysteresis)");
        assert_eq!(choose_skip(25.0, 40.0, 2), 1);
        assert_eq!(choose_skip(25.0, 0.0, 2), 2, "nothing measured yet");
        let mut r = Run::for_rate(10, 1.0); r.stride *= 3; assert_eq!((r.frame(2), r.seq_at(16)), (16, Some(2)));
    }

    #[test]
    fn bmp_is_top_down_bgra_with_the_right_header() {
        let px = [255u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 10, 20, 30, 40];
        let b = bmp(&px, 2, 2);
        assert_eq!(&b[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(b[2..6].try_into().unwrap()) as usize, b.len());
        assert_eq!(i32::from_le_bytes(b[22..26].try_into().unwrap()), -2, "negative height = top-down rows");
        assert_eq!(u16::from_le_bytes(b[28..30].try_into().unwrap()), 32);
        assert_eq!(&b[54..58], &[0, 0, 255, 255], "red pixel as BGRA");
        assert_eq!(&b[66..70], &[30, 20, 10, 40]);
        assert_eq!(draft_size(1920, 1080, 0.5), (960, 540));
        assert_eq!(draft_size(1920, 1080, 0.25), (480, 270));
        assert_eq!(draft_size(1281, 721, 0.5), (640, 360));
    }

    #[test]
    fn a_windowed_mix_matches_the_full_mix() {
        use crate::commands::{dispatch, Session}; use serde_json::json;
        let dir = std::env::temp_dir().join(format!("kimo-ps-mix-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=220:sample_rate=48000:duration=3", "-ac", "2"]).arg(&path).status().map(|s| s.success()).unwrap_or(false);
        if !ok { eprintln!("ffmpeg missing: skipped"); return; }
        let mut s = Session::new();
        let a = dispatch(&mut s, "import_media", &json!({"path": path})).unwrap()["asset_id"].clone();
        let tr = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
        dispatch(&mut s, "add_clip", &json!({"track_id": tr, "source_type": "media", "asset_id": a, "start": 0.5, "duration": 2.0})).unwrap();
        let full = crate::audio::mix(&s.project);
        let sr = crate::audio::SR;
        for (t0, t1) in [(0.0, 1.0), (1.0, 2.0), (2.0, 2.5)] {
            let w = mix_window(&s.project, t0, t1);
            assert_eq!(w.len(), ((t1 - t0) * sr as f64).round() as usize * 2);
            let base = (t0 * sr as f64) as usize * 2;
            let worst = w.iter().enumerate().map(|(i, v)| (v - full.get(base + i).copied().unwrap_or(0.0)).abs()).fold(0f32, f32::max);
            // decoders seek to within a few samples; a 220 Hz tone changes by at most ~0.03 per sample
            assert!(worst < 0.1, "window [{t0}, {t1}): worst sample difference {worst}");
            let loud = w.iter().map(|v| v.abs()).fold(0f32, f32::max);
            if t0 >= 0.5 { assert!(loud > 0.08, "window [{t0}, {t1}) is audible ({loud}): lavfi sine peaks at 1/8"); }
        }
        assert!(mix_window(&s.project, 0.0, 0.4).iter().all(|v| v.abs() < 1e-6), "before the clip: silence");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
