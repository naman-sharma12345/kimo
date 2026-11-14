// kimo viewer: real-time playback with sound.
// Classic script (no imports, no studio globals): defines window.KimoAV (the A/V sync maths, the same contract as the engine's
// crates/kimo-core/src/preview_stream.rs, tested in tests/playback_sync_test.mjs) and window.KimoPlayer, which plays the
// engine's preview stream (the preview_start / preview_update / preview_frame / preview_stop commands): draft frames rendered
// ahead of the playhead, drawn on a canvas, kept in sync with a Web Audio track of the mix. The audio clock is the master;
// video frames are picked from it. Without sound (fast forward, reverse, a silent timeline) a performance.now() clock stands in.
"use strict";
(function (root) {
  // ---------------------------------------------------------------------------------------------------------------
  // A/V sync maths (mirrors preview_stream.rs: AvClock, chunk_schedule, frame_due, drift_frames, Run)
  const AV = {
    clock(anchorTl, anchorAudio, rate, latency) {
      return { anchorTl, anchorAudio, rate: Number.isFinite(rate) && rate !== 0 ? rate : 1, latency: Number.isFinite(latency) ? Math.max(0, latency) : 0 };
    },
    // timeline time being heard at audio time a (during the output latency the first frame holds)
    timelineAt: (c, a) => c.anchorTl + Math.max(0, a - c.latency - c.anchorAudio) * c.rate,
    // audio time at which timeline time tl must be handed to the device
    audioAt: (c, tl) => c.anchorAudio + (tl - c.anchorTl) / c.rate,
    // start a chunk covering timeline [c0, c0 + len): {when, offset, left}, or null when it is already entirely past
    chunkSchedule(c, c0, len, now) {
      if (c.rate <= 0) return null;
      const when = AV.audioAt(c, c0), offset = Math.max(0, (now - when) * c.rate);
      if (offset >= len - 1e-9) return null;
      return { when: Math.max(when, now), offset, left: len - offset };
    },
    frameDue: (c, a, fps) => fps > 0 ? Math.floor(AV.timelineAt(c, a) * fps + 1e-6) : 0,
    driftFrames: (c, a, shown, fps) => AV.timelineAt(c, a) * fps - shown - 0.5,
    // a run's lattice: the frame at or just before `f` that the engine renders for this run (stride > 1 skips frames)
    onRun(run, f) { const d = (f - run.start) * run.dir; if (d < 0) return run.start; return run.start + run.dir * run.stride * Math.floor(d / Math.max(1, run.stride)); },
    // J/K/L: L speeds forward (1, 2, 4), J speeds backward (-1, -2, -4); pressing the other way first stops at 1 / -1
    shuttle(rate, key) {
      if (key === 'l') return rate <= 0 ? 1 : Math.min(4, rate * 2);
      if (key === 'j') return rate >= 0 ? -1 : Math.max(-4, rate * 2);
      return 0;
    },
    // loop region: where the playhead goes when it reaches `t` (null = keep going)
    wrap(t, loop, end) {
      if (!loop || !loop.on) return t >= end ? null : t;
      const a = Math.max(0, loop.in ?? 0), b = Math.min(end, loop.out ?? end);
      if (b - a < 1e-3) return t;
      return t >= b || t < a ? a : t;
    },
  };
  root.KimoAV = AV;

  // ---------------------------------------------------------------------------------------------------------------
  const BUFFER_TO_START = 8;          // frames ready before Play starts the clock
  const FETCHES = 8;                  // frame downloads in flight
  const AUDIO_WAIT_MS = 1500;         // longest wait for the first audio chunk before playing without it
  const STALL_MS = 300;               // a frame missing this long: pause the clock and re-buffer
  const POLL_MS = 120;
  const KEEP = 60;                    // decoded frames kept (ImageBitmaps, about 2 MB each at half 1080p)

  class Player {
    // opts: invoke(cmd, args) -> Promise, canvas, onTime(t), onState(state, info), onError(msg), fps(), duration(), scale()
    constructor(opts) {
      Object.assign(this, { o: opts, supported: null, started: false, gen: -1, run: { start: 0, dir: 1, stride: 1 }, frames: new Map(), loading: new Set(),
        chunks: new Map(), sources: [], pending: [], playing: false, buffering: false, rate: 1, t: 0, clock: null, now: null, raf: 0, pollT: 0, polling: false,
        shown: -1, missingSince: 0, loop: { on: false, in: null, out: null }, scrubBusy: false, scrubNext: null, drawn: 0, drawnT0: 0, fpsShown: 0, info: {} });
    }
    get fps() { return this.o.fps() || 30; }
    get end() { return this.o.duration() || 0; }

    // ---- engine session ----
    async ensure(t) {
      if (this.supported === false) return false;
      if (this.started) return true;
      try {
        const r = await this.o.invoke('preview_start', { time: t, scale: this.o.scale(), playing: false, rate: 1 });
        this.supported = true; this.started = true; this.take(r); return true;
      } catch (e) {
        const m = String(e);
        // an engine or bridge without the preview stream: the viewer keeps its draft clips
        if (/unknown (command|action|tool)|command \S+ not found|not allowed|preview_stream/i.test(m)) this.supported = false;
        else this.o.onError?.(m);
        return false;
      }
    }
    async stop() { this.pause(); if (this.started) { this.started = false; try { await this.o.invoke('preview_stop', {}); } catch (e) {} } this.dropFrames(); this.dropChunks(); }
    // the poll: our playhead in, ready frames and audio chunks out
    async poll(extra) {
      if (!this.started || this.polling && !extra) return;
      this.polling = true;
      try { const r = await this.o.invoke('preview_update', { time: this.t, playing: this.playing, rate: this.rate, ...(extra || {}) }); this.take(r); }
      catch (e) { const m = String(e); if (/no preview stream/.test(m)) { this.started = false; await this.ensure(this.t); } else this.o.onError?.(m); }
      finally { this.polling = false; }
    }
    take(r) {
      if (!r) return;
      this.info = r;
      if (r.gen !== this.gen) { this.gen = r.gen; this.run = r.run || this.run; this.dropFrames(); if (this.playing) this.rebuffer(); }
      this.pending = (r.frames || []).filter(f => !this.frames.has(f.index));
      this.pump();
      for (const c of r.audio || []) this.fetchChunk(c);
      if (r.error) this.o.onError?.(r.error);
    }

    // ---- frames ----
    dropFrames() { for (const f of this.frames.values()) f.bmp?.close?.(); this.frames.clear(); this.loading.clear(); this.pending = []; }
    // download the listed frames in play order, a few at a time, each finished download starting the next
    pump() {
      while (this.loading.size < FETCHES && this.pending.length) { const f = this.pending.shift(); if (!this.frames.has(f.index) && !this.loading.has(f.index)) this.fetchFrame(f); }
    }
    async fetchFrame(f) {
      if (this.frames.has(f.index) || this.loading.has(f.index)) return;
      const gen = this.gen; this.loading.add(f.index);
      try {
        const blob = await (await fetch(f.url, { cache: 'no-store' })).blob();
        const bmp = root.createImageBitmap ? await createImageBitmap(blob) : await new Promise((ok, no) => { const i = new Image(); i.onload = () => ok(i); i.onerror = no; i.src = URL.createObjectURL(blob); });
        if (gen !== this.gen) { bmp.close?.(); return; }
        this.frames.set(f.index, { bmp, time: f.time });
        if (this.frames.size > KEEP) this.evict();
      } catch (e) { /* replaced or deleted meanwhile: the next poll lists what is there */ }
      finally { this.loading.delete(f.index); this.pump(); }
    }
    evict() {
      const cur = Math.round(this.t * this.fps), dir = this.run.dir, score = k => Math.abs(k - cur) + ((k - cur) * dir < -8 ? 1e6 : 0);
      const far = [...this.frames.keys()].sort((a, b) => score(b) - score(a));
      while (this.frames.size > KEEP && far.length) { const k = far.shift(); this.frames.get(k)?.bmp?.close?.(); this.frames.delete(k); }
    }
    ready(from, n) { let k = 0, f = AV.onRun(this.run, from); while (k < n && this.frames.has(f)) { k++; f += this.run.dir * this.run.stride; } return k; }
    draw(f) {
      const e = this.frames.get(f); if (!e) return false;
      const c = this.o.canvas, b = e.bmp;
      if (c.width !== b.width || c.height !== b.height) { c.width = b.width; c.height = b.height; }
      c.getContext('2d').drawImage(b, 0, 0);
      if (this.shown !== f) { this.shown = f; this.drawn++; }
      return true;
    }

    // ---- audio ----
    actx() {
      if (!this.ctx) { const C = root.AudioContext || root.webkitAudioContext; if (!C) return null; this.ctx = new C({ latencyHint: 'playback', sampleRate: 48000 }); }
      return this.ctx;
    }
    dropChunks() { this.stopSources(); this.chunks.clear(); }
    async fetchChunk(c) {
      const key = c.name; if (this.chunks.has(key)) return;
      const ctx = this.actx(); if (!ctx) return;
      const e = { ...c, buf: null, scheduled: false }; this.chunks.set(key, e);
      try { const ab = await (await fetch(c.url, { cache: 'no-store' })).arrayBuffer(); e.buf = await ctx.decodeAudioData(ab); this.schedule(); }
      catch (err) { this.chunks.delete(key); }
      if (this.chunks.size > 24) { const old = [...this.chunks.values()].filter(x => x.end < this.t - 2 && !x.src); for (const x of old) this.chunks.delete(x.name); }
    }
    stopSources() { for (const s of this.sources) { try { s.stop(); } catch (e) {} } this.sources = []; for (const c of this.chunks.values()) { c.scheduled = false; c.src = null; } }
    usesAudio() { return this.rate === 1 && this.info.audio_on !== false && !!this.actx(); }
    schedule() {
      if (!this.playing || this.buffering || !this.clock || !this.usesAudio()) return;
      const ctx = this.ctx, now = ctx.currentTime + 0.01;
      for (const c of this.chunks.values()) {
        if (c.scheduled || !c.buf || !c.audible) continue;
        const s = AV.chunkSchedule(this.clock, c.start, c.buf.duration, now);
        if (!s) continue;
        const src = ctx.createBufferSource(); src.buffer = c.buf; src.connect(ctx.destination);
        src.start(s.when, s.offset); c.scheduled = true; c.src = src; this.sources.push(src);
        src.onended = () => { this.sources = this.sources.filter(x => x !== src); if (c.src === src) c.src = null; };
      }
    }
    // the clock source: the audio device when sound plays, else the page clock
    nowS() { return this.usesAudio() && this.ctx.state === 'running' ? this.ctx.currentTime : performance.now() / 1000; }

    // ---- transport ----
    async play(t, rate = 1) {
      if (!(await this.ensure(t))) return false;
      this.epoch = (this.epoch || 0) + 1; this.scrubNext = null; // scrubs still in flight must not pause what starts now
      this.t = t; this.rate = rate || 1; this.playing = true;
      if (this.usesAudio() && this.ctx.state !== 'running') { try { await this.ctx.resume(); } catch (e) {} }
      await this.poll({ seek: true, playing: true, rate: this.rate });
      this.rebuffer();
      this.tickPoll();
      return true;
    }
    pause() {
      this.playing = false; this.buffering = false; this.clock = null; cancelAnimationFrame(this.raf); this.raf = 0; clearTimeout(this.pollT);
      this.stopSources(); this.o.onState?.('paused', this.stats());
      if (this.started) this.poll({ playing: false });
    }
    setRate(rate) { if (!rate) return this.pause(); const t = this.t; if (this.playing) { this.pause(); return this.play(t, rate); } return this.play(t, rate); }
    rebuffer() {
      this.buffering = true; this.clock = null; this.stopSources(); cancelAnimationFrame(this.raf);
      this.o.onState?.('buffering', this.stats());
      const t0 = performance.now(), from = Math.round(this.t * this.fps);
      const wait = () => {
        if (!this.playing) return;
        const framesOk = this.ready(from, BUFFER_TO_START) >= Math.min(BUFFER_TO_START, Math.max(1, Math.ceil((this.end - this.t) * this.fps / this.run.stride)));
        const audioOk = !this.usesAudio() || [...this.chunks.values()].some(c => c.buf && c.start <= this.t + 1e-6 && c.end > this.t) || this.info.audio_on === false || performance.now() - t0 > AUDIO_WAIT_MS;
        // a new AudioContext says "running" a moment before its clock moves: anchor on a clock that runs
        const clockOk = !this.usesAudio() || this.ctx.state !== 'running' || this.ctx.currentTime > 0;
        if (framesOk && audioOk && clockOk) return this.go();
        this.raf = requestAnimationFrame(wait);
      };
      this.raf = requestAnimationFrame(wait);
    }
    go() {
      this.buffering = false;
      const lat = this.usesAudio() ? (this.ctx.outputLatency || this.ctx.baseLatency || 0) : 0;
      this.clock = AV.clock(this.t, this.nowS() + 0.05, this.rate, lat);
      this.missingSince = 0; this.drawn = 0; this.drawnT0 = performance.now();
      this.schedule();
      this.o.onState?.('playing', this.stats());
      const tick = () => {
        if (!this.playing || this.buffering) return;
        const now = this.nowS(), fps = this.fps;
        const t = AV.timelineAt(this.clock, now), L = this.loop;
        let jump = null;
        if (this.rate > 0) {
          const w = AV.wrap(t, L, this.end);
          if (w === null) { this.t = Math.max(0, this.end - 1 / fps); this.o.onTime(this.t); return this.pause(); }
          if (w !== t) jump = w;
        } else {
          const lo = L.on ? Math.max(0, L.in ?? 0) : 0;
          if (t <= lo) { if (!L.on) { this.t = 0; this.draw(0); this.o.onTime(0); return this.pause(); } jump = Math.min(this.end, L.out ?? this.end) - 1 / fps; }
        }
        if (jump !== null) { this.t = jump; this.o.onTime(jump); this.poll({ seek: true, playing: true, rate: this.rate }); return this.rebuffer(); }
        this.t = t;
        const due = AV.onRun(this.run, Math.floor(t * fps + 1e-6));
        if (this.draw(due)) this.missingSince = 0;
        else if (!this.missingSince) this.missingSince = performance.now();
        else if (performance.now() - this.missingSince > STALL_MS) { this.t = this.shown >= 0 ? this.shown / fps : t; return this.rebuffer(); }
        this.o.onTime(t);
        if (performance.now() - this.drawnT0 > 1000) { this.fpsShown = this.drawn * 1000 / (performance.now() - this.drawnT0); this.drawn = 0; this.drawnT0 = performance.now(); this.o.onState?.('playing', this.stats()); }
        this.schedule();
        this.raf = requestAnimationFrame(tick);
      };
      this.raf = requestAnimationFrame(tick);
    }
    tickPoll() { clearTimeout(this.pollT); if (!this.playing) return; this.pollT = setTimeout(async () => { await this.poll(); this.tickPoll(); }, POLL_MS); }

    // one frame now (scrub, frame step): latest wins, one request in flight
    async scrub(t) {
      if (this.playing || this.buffering) this.pause();
      const ep = this.epoch;
      if (!(await this.ensure(t)) || ep !== this.epoch) return false;
      this.t = t;
      const f = Math.round(t * this.fps);
      if (this.frames.has(f)) { this.draw(f); return true; }
      if (this.scrubBusy) { this.scrubNext = t; return true; }
      this.scrubBusy = true;
      try {
        const t0 = performance.now();
        const r = await this.o.invoke('preview_frame', { time: t });
        const blob = await (await fetch(r.url, { cache: 'no-store' })).blob();
        const bmp = await createImageBitmap(blob);
        if (ep !== this.epoch) { bmp.close?.(); return true; } // Play was pressed meanwhile
        const c = this.o.canvas; if (c.width !== bmp.width || c.height !== bmp.height) { c.width = bmp.width; c.height = bmp.height; }
        if (this.scrubNext === null) { c.getContext('2d').drawImage(bmp, 0, 0); this.shown = r.index; }
        bmp.close?.();
        this.o.onState?.('scrub', { ms: Math.round(performance.now() - t0), engine_ms: Math.round(r.ms), cached: r.cached });
      } catch (e) { const m = String(e); if (/no preview stream/.test(m)) this.started = false; else this.o.onError?.(m); }
      finally { this.scrubBusy = false; if (this.scrubNext !== null && ep === this.epoch) { const n = this.scrubNext; this.scrubNext = null; this.scrub(n); } }
      return true;
    }
    stats() {
      const i = this.info || {};
      return { fps: Math.round(this.fpsShown * 10) / 10, render_fps: Math.round((i.render_fps || 0) * 10) / 10, rate: this.rate, buffered: i.ready_ahead || 0,
        size: i.width ? `${i.width}×${i.height}` : '', scale: i.scale, audio: this.usesAudio() && i.audio_on !== false, proxies: (i.proxies || []).length, first_frame_ms: i.first_frame_ms, skip: i.skip || 1 };
    }
  }
  root.KimoPlayer = Player;
})(typeof window !== 'undefined' ? window : globalThis);
