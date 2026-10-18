// Kimo spectator — store, reducer, layout engine, render loop.
// Read-only: the UI only listens. Every event is folded into one state
// object; a single rAF-batched render paints the current mode.
import { connect } from './bridge.js';
import { MODES, inferMode } from './modes.js';
import { KINDS, PRE, POST, tickRunning, pickedRows, TL_PARTS, TL_SMALL } from './panels.js';
import { route, freezeNow, flashing } from './router.js';

const DESIGN_W = 1376, DESIGN_H = 768;
// smallest viewer kept when a window is too small for a screen's fixed tracks (then the app is scaled down to fit)
const MIN_FLEX_W = 160, MIN_FLEX_H = 260;
// small windows: the Inspector's least height beside the viewer, and the least the band under it keeps (design px).
// 402 = the panel's own title line (47) + the tallest READ card, the flash pin one (341.5) + its bottom gap (13), so
// the card sits under the 'Inspector' title instead of over it; the timeline keeps 110 (ruler + 2 compact track rows, panels.js TL_SMALL).
const MIN_INSP_H = 402, MIN_BAND_H = 110;
let fitScale = 1;
const RAWISH = /^(c|clip|t|track|fx|effect|node|n|layer|l|asset|media|an|marker|mask|m|cam|light|obj)[_-]?\d+$/i;   /* GUI-FIX-2: an engine id inside a reason */

export const state = {
  status: { kind: 'down', label: 'connecting' },
  session: { project: '—', agent: '—', elapsed_s: 0, t0: performance.now(), still: false },
  mode: 'edit', activity: null, explicitActivity: false, explicitMode: false,
  calls: [], callCount: 0,
  views: {},
  stats: {},
  preview: { shown: -1, want: 0, revs: false },   // the engine frame's rev on the viewer, the project's latest rev (see previewMark)
};

// ---------- reducer ----------
function merge(dst, src) {
  for (const k of Object.keys(src)) {
    const v = src[k];
    if (v && typeof v === 'object' && !Array.isArray(v) && dst[k] && typeof dst[k] === 'object' && !Array.isArray(dst[k])) merge(dst[k], v);
    else dst[k] = v;
  }
  return dst;
}
function view(mode) { return state.views[mode] || (state.views[mode] = {}); }

/* "out of date · rev N" on every viewer the engine's frames own, from the moment a mutating call moves the project past
   the frame on screen until the engine's frame for that rev (or a newer one) arrives. Off until the engine has sent a
   frame with a rev: an engine without auto-preview never gets a mark it could not clear. */
function previewMark() {
  const P = state.preview, mark = P.revs && P.want > P.shown ? P.want : null;
  for (const [mode, M] of Object.entries(MODES)) for (const [name, spec] of Object.entries(M.panels)) {
    if (spec.kind !== 'viewer') continue;
    const v = (state.views[mode] || {})[name]; if (!v || !v._engine) continue;
    if (mark != null) v.stale = mark; else delete v.stale;
    if (mark != null && P.refused) { v.refused = P.refused; v.refusedSrc = P.refusedSrc || 'engine'; } else { delete v.refused; delete v.refusedSrc; }   /* GUI-FIX-2: why the frame is behind (fix2.js / frame_skipped) */
  }
}

export function apply(ev) {
  if (!ev || typeof ev !== 'object') return;
  switch (ev.type) {
    case 'session':
      if (ev.reset) { state.calls = []; state.callCount = 0; state.views = {}; state.stats = {}; state.preview = { shown: -1, want: 0, revs: false }; state.explicitActivity = false; state.explicitMode = false; state.activity = null; for (const k of ['_gg', '_refuse', '_trk', '_outl', '_b2Args']) delete state[k]; }
      for (const k of ['project', 'agent', 'read_only']) if (ev[k] !== undefined) state.session[k] = ev[k];
      if (ev.width > 0 && ev.height > 0) { state.session.width = ev.width; state.session.height = ev.height; }   // project size: the transform gizmo places x/y pixels with it
      if (ev.elapsed_s !== undefined) { state.session.elapsed_s = ev.elapsed_s; state.session.t0 = performance.now(); }
      if (ev.still !== undefined) state.session.still = !!ev.still;
      break;
    case 'mode':
      if (ev.mode && MODES[ev.mode]) { if (state.mode !== ev.mode) layoutDirty = true; state.mode = ev.mode; state.explicitMode = true; }
      if (ev.activity !== undefined) { state.activity = ev.activity; state.explicitActivity = true; }
      break;
    case 'view':
      merge(view(ev.mode || state.mode), ev.data || {});
      break;
    case 'stats':
      merge(state.stats, ev); delete state.stats.type;
      break;
    case 'tool_start': {
      const m = ev.mode || (state.explicitMode ? null : inferMode(ev.name));
      if (m && MODES[m] && m !== state.mode) { state.mode = m; layoutDirty = true; }
      // the engine runs one call at a time: a new start after a feed drop means a call left 'running' across the gap
      // finished while we could not hear it; its outcome is unknown, so it says so instead of ticking forever
      if (state._gapAt != null) { for (const x of state.calls) if (x.status === 'running' && x.t < state._gapAt) { x.status = 'lost'; x.dur_ms = null; } state._gapAt = null; }
      const c = { id: ev.id, name: ev.name, args: ev.args, t: performance.now() - (ev.elapsed_ms || 0), elapsed_ms: ev.elapsed_ms, dur_ms: null, status: 'running', progress: ev.progress };
      const i = state.calls.findIndex(x => x.id === ev.id && ev.id !== undefined);
      if (i >= 0) state.calls[i] = c; else { state.calls.push(c); state.callCount++; }
      if (state.calls.length > 400) state.calls.splice(0, state.calls.length - 400);
      if (ev.count !== undefined) state.callCount = ev.count;
      break;
    }
    case 'tool_end': {
      let c = ev.id !== undefined ? [...state.calls].reverse().find(x => x.id === ev.id) : state.calls[state.calls.length - 1];
      if (!c && ev.id !== undefined) { c = { id: ev.id, name: ev.name, args: ev.args, t: performance.now(), dur_ms: null, status: 'ok' }; state.calls.push(c); state.callCount++; if (state.calls.length > 400) state.calls.splice(0, state.calls.length - 400); }   // its start was lost in a drop: the call still shows, settled
      if (c) { c.dur_ms = ev.dur_ms !== undefined ? ev.dur_ms : (performance.now() - c.t); /* null = the harness gave no latency: hidden */ c.status = ev.status || 'ok'; if (ev.args) c.args = ev.args; if (ev.result !== undefined) c.result = ev.result; if (ev.error) c.error = ev.error; if (ev.diff) c.diff = ev.diff; }
      break;
    }
    case 'tool_progress': {
      const c = [...state.calls].reverse().find(x => x.id === ev.id);
      if (c) merge(c, { progress: ev.progress });
      break;
    }
    case 'frame_rendered': {
      /* Preview safety: an engine frame carries the project rev it shows (k5 auto-preview). A frame older than the one on
         the viewer is dropped (a slow render never overwrites a newer one); a frame with no rev (an older engine) shows
         as before. The GUI never asks for a frame: it only listens. */
      /* A scope render (render_scope) sends its image as a frame too: it goes to the scope dock of every screen that has
         one, never onto a viewer, and it is not a preview of the project (no rev, no parade from it). */
      const sk = ev.call_id != null && state._scopeCalls ? state._scopeCalls[ev.call_id] : undefined;
      if (sk !== undefined) {
        const ssrc = ev.png_b64 ? 'data:image/png;base64,' + ev.png_b64 : ev.jpg_b64 ? 'data:image/jpeg;base64,' + ev.jpg_b64 : ev.src;
        if (ssrc) for (const [mode, M] of Object.entries(MODES)) for (const [name, spec] of Object.entries(M.panels)) if (spec.kind === 'scopes') { const V = view(mode); (V[name] || (V[name] = {})).img = { src: ssrc, kind: sk, tc: ev.tc ?? null }; }
        break;
      }
      const P = state.preview, fr = Number.isFinite(ev.rev) ? ev.rev : null;
      if (fr != null) { if (fr < P.shown) { metrics.staleFrames++; return; } P.shown = fr; P.revs = true; }
      else if (P.revs) P.shown = Math.max(P.shown, P.want);
      if (P.shown >= P.want) { P.refused = null; P.refusedSrc = null; }   /* GUI-FIX-2: a frame for the newest rev: nothing is refused any more */
      const src = ev.png_b64 ? 'data:image/png;base64,' + ev.png_b64 : ev.jpg_b64 ? 'data:image/jpeg;base64,' + ev.jpg_b64 : ev.src;
      // the engine's frame shows in this mode's viewer and in every other mode's viewer the engine
      // already owns or that has no frame yet, so a live session never hops to an empty viewer
      for (const [mode, M] of Object.entries(MODES)) for (const [name, spec] of Object.entries(M.panels)) {
        if (spec.kind !== 'viewer') continue;
        const V = view(mode), v = V[name] || (V[name] = {});
        if (mode !== state.mode && v.src && !v._engine) continue;
        if (src) { v.src = src; v._engine = true; }
        for (const k of ['tc', 'res', 'tag', 'left', 'right']) if (ev[k] !== undefined) v[k] = ev[k];
        if (ev.frame !== undefined) v.frame = ev.frame;
      }
      // a render job still running takes each new engine frame as its Renders thumbnail
      if (src) for (const r of state.views.export?.renders?.rows || []) if (r._live) r._thumb = src;
      if (src && (ev.png_b64 || ev.jpg_b64)) computeParade(src);
      previewMark();
      break;
    }
    case 'frame_skipped': {
      /* GUI-FIX-2: the engine could not make this call's frame (demo mode sends its reason; headless sends nothing).
         The viewer keeps the last frame and its out-of-date chip reads the engine's reason; raw ids in it read as the
         grade node's effect name when the GUI knows the node, else drop out. */
      if (typeof ev.reason === 'string' && ev.reason.trim()) {
        const nodes = Object.values(state._gg || {}).reduce((m, g) => Object.assign(m, g.nodes || {}), {});
        state.preview.refusedSrc = 'engine';
        state.preview.refused = ev.reason.trim().replace(/\b([A-Za-z]+[_-]?\d+)\b/g, (t) => nodes[t] && nodes[t].fx ? '· ' + String(nodes[t].fx).replace(/_/g, ' ') : RAWISH.test(t) ? '' : t).replace(/\s+/g, ' ').trim().slice(0, 80);
        if (Number.isFinite(ev.rev)) state.preview.want = Math.max(state.preview.want, ev.rev);
        if (!state.preview.revs) state.preview.revs = true;
        previewMark();
      }
      break;
    }
    case 'project_changed':
      state.stats.saved_at = performance.now(); state.stats.rev = ev.rev;
      if (Number.isFinite(ev.rev)) { state.preview.want = Math.max(state.preview.want, ev.rev); previewMark(); }   // only a mutating call moves the rev
      if (ev.width > 0 && ev.height > 0) { state.session.width = ev.width; state.session.height = ev.height; }
      if (typeof ev.project === 'string' && ev.project.trim()) state.session.project = ev.project.trim();
      break;
    default: return;
  }
  dirty = true;
}

// ---------- layout engine ----------
// A mode lists its panels as rectangles measured on the 1376x768 mockup.
// The engine turns the set of edges into CSS grid tracks: tracks inside the
// viewer's span flex (fr), everything else stays fixed px. At the design size
// every box lands on the mockup's pixel; at other sizes only the viewer grows.
let layoutDirty = true;
const main = document.getElementById('main');
const nodes = {};
// panel elements by slot: every timeline-kind panel shares one slot (the timeline stays one element across screens)
const pool = {};
const slotOf = (name, kind) => kind === 'timeline' ? 'timeline' : name;

function tracks(edges, flexLo, flexHi) {
  const out = [];
  for (let i = 0; i < edges.length - 1; i++) {
    const a = edges[i], b = edges[i + 1], w = b - a;
    out.push(a >= flexLo && b <= flexHi ? `minmax(0,${w}fr)` : `${w}px`);
  }
  return out.join(' ');
}

function buildLayout() {
  const M = MODES[state.mode];
  document.body.className = M.cls || '';
  const root = document.getElementById('app');
  const top = M.top ?? 46, foot = M.foot ?? 0;
  root.style.setProperty('--top', top + 'px');
  root.style.setProperty('--foot', foot + 'px');
  for (const [k, v] of Object.entries(M.vars || {})) document.documentElement.style.setProperty(k, v);
  for (const k of (window.__lastVars || [])) if (!(M.vars || {})[k]) document.documentElement.style.removeProperty(k);
  window.__lastVars = Object.keys(M.vars || {});
  const H = DESIGN_H - top - foot;
  const xs = new Set([0, DESIGN_W]), ys = new Set([0, H]);
  const rects = {};
  for (const [name, r] of Object.entries(M.rects)) {
    const rr = [r[0], r[1] - top, r[2], r[3] - top];
    rects[name] = rr; xs.add(rr[0]); xs.add(rr[2]); ys.add(rr[1]); ys.add(rr[3]);
  }
  const X = [...xs].sort((a, b) => a - b), Y = [...ys].sort((a, b) => a - b);
  const fl = rects[M.flex || 'viewer'] || [0, 0, DESIGN_W, H];
  // spring-eased splits: a panel that exists on both screens of a mode hop starts at its old box and springs
  // to its new one (transform only, so nothing else reflows); a resize or first build snaps. The old boxes are read
  // BEFORE the new grid tracks are set (read after, they were the old grid lines indexed into the new tracks)
  const hop = builtMode != null && builtMode !== state.mode && !hidden, old = {};
  if (hop) for (const [k, el] of Object.entries(nodes)) old[slotOf(k, el._kind)] = [el.offsetLeft, el.offsetTop, el.offsetWidth, el.offsetHeight];
  // A window too small for this screen's fixed tracks plus a minimal viewer (e.g. 640 px wide): the app is laid out
  // at the smallest size that fits and scaled down to the window, so no panel runs past the window edge. Where it
  // already fits (every size from 1100 x 768 up) nothing is set and the layout is exactly the one above.
  const needW = DESIGN_W - (fl[2] - fl[0]) + MIN_FLEX_W, needH = DESIGN_H - (fl[3] - fl[1]) + MIN_FLEX_H;
  fitScale = Math.min(1, innerWidth / needW, innerHeight / needH);
  if (fitScale < 1) Object.assign(root.style, { width: innerWidth / fitScale + 'px', height: innerHeight / fitScale + 'px', transform: `scale(${fitScale})`, transformOrigin: '0 0' });
  else for (const k of ['width', 'height', 'transform', 'transform-origin']) root.style.removeProperty(k);
  main.style.gridTemplateColumns = tracks(X, fl[0], fl[2]);
  // Small windows only (fitScale < 1, i.e. under 1100 x 768): an Inspector that shares the viewer's rows gets at least
  // MIN_INSP_H design px there (a 4-row READ card, the flash pin one included, sits under the panel's title), taken from the
  // tallest row of the band under it (the timeline), which keeps at least MIN_BAND_H. Grid lines keep their order, so
  // every panel stays on its rows; at fitScale 1 the rows are exactly the ones above.
  let ry = Y, rHi = fl[3];
  TL_SMALL.on = false;
  if (fitScale < 1 && rects.insp && rects.insp[1] === fl[1] && rects.insp[3] === fl[3]) {
    let fixed = 0, bi = -1, bw = 0;
    for (let i = 0; i < Y.length - 1; i++) { const w = Y[i + 1] - Y[i]; if (!(Y[i] >= fl[1] && Y[i + 1] <= fl[3])) fixed += w; if (Y[i] >= fl[3] && w > bw) { bw = w; bi = i; } }
    const flexH = innerHeight / fitScale - top - foot - fixed, take = bi < 0 ? 0 : Math.min(MIN_INSP_H - flexH, bw - MIN_BAND_H);
    if (take > 0) { ry = Y.map(e => e >= fl[3] && e <= Y[bi] ? e + take : e); rHi = fl[3] + take; TL_SMALL.on = true; }   /* the shorter timeline draws compact track rows (panels.js) */
  }
  main.style.gridTemplateRows = tracks(ry, fl[1], rHi);
  springs.length = 0;
  // One workspace: a panel the new screen shares with the old one (the rail, the viewer, the timeline, the
  // inspector) is the SAME element, re-placed on the grid, never re-created; others are pooled off-document and
  // come back as the same node. Only contents change; a panel whose contents change kind fades in (200 ms, once).
  const keep = new Set(), entering = [], leaving = [];
  for (const k of Object.keys(nodes)) delete nodes[k];
  let before = null;
  for (const [name, spec] of Object.entries(M.panels)) {
    const r = rects[name]; if (!r) continue;
    const slot = slotOf(name, spec.kind);
    let el = pool[slot]; const fresh = !el;
    if (fresh) el = pool[slot] = document.createElement('section');
    const was = el._kind;
    if (el._leave) { el._leave.cancel(); el._leave = null; }
    if (el._enter) { el._enter.cancel(); el._enter = null; }
    el.removeAttribute('style'); delete el.dataset.anim; delete el.dataset.leaving;
    el.className = 'panel ' + (spec.cls || '') + ' k-' + spec.kind;
    el.dataset.panel = name;
    el.style.gridColumn = `${X.indexOf(r[0]) + 1} / ${X.indexOf(r[2]) + 1}`;
    el.style.gridRow = `${Y.indexOf(r[1]) + 1} / ${Y.indexOf(r[3]) + 1}`;
    if (spec.z) el.style.zIndex = spec.z;
    el._sz = null; el._p = null; if (was !== spec.kind) el._h = null;
    el._kind = spec.kind;
    // document order = the screen's panel order; an element already in place is not moved
    const at = before ? before.nextSibling : main.firstChild;
    if (at !== el) main.insertBefore(el, at);
    before = el; keep.add(el); nodes[name] = el;
    if (hop && (fresh || was !== spec.kind || !old[slot]) && el.animate) entering.push(el);
  }
  /* a panel the new screen does not have stays where it was, under the new ones, while they fade in over it (a true
     crossfade: no frame shows the canvas where either screen has a panel), then leaves the document. It gives up its
     name at once, so nothing that asks for a panel by name can find it. One compositor-driven opacity animation; no
     timer, no rAF. */
  for (const c of [...main.children]) {
    if (keep.has(c)) continue;
    const ob = hop && c.dataset.panel && old[slotOf(c.dataset.panel, c._kind)];
    if (!ob || !c.animate || c._leave) { if (c._leave) { c._leave.cancel(); c._leave = null; } c.remove(); continue; }
    c.dataset.leaving = c.dataset.panel; delete c.dataset.panel; delete c.dataset.anim;
    c.style.cssText = `position:absolute;left:${ob[0]}px;top:${ob[1]}px;width:${ob[2]}px;height:${ob[3]}px;z-index:0;pointer-events:none`;
    main.insertBefore(c, main.firstChild);
    const a = c._leave = c.animate([{ opacity: 1 }, { opacity: 1, offset: .55 }, { opacity: 0 }], { duration: 150, easing: 'linear' });
    a.pause(); leaving.push(a);
    a.onfinish = () => { if (c._leave === a) { c._leave = null; c.remove(); } };
  }
  if (hop) handover(entering, leaving);
  if (hop) for (const [name, el] of Object.entries(nodes)) {
    const f = old[slotOf(name, MODES[state.mode].panels[name].kind)]; if (!f) continue;
    if (state.mode === 'export' && name === 'viewer') continue;   /* hops into export: the viewer takes its new box at once (no spring) */
    const t = [el.offsetLeft, el.offsetTop, el.offsetWidth, el.offsetHeight];
    if (!t[2] || !t[3] || f.every((v, i) => Math.abs(v - t[i]) < .5)) continue;
    springs.push({ el, W: t[2], H: t[3], d: [f[0] - t[0], f[1] - t[1], f[2] / t[2], f[3] / t[3]], D: Math.max(Math.abs(f[0] - t[0]), Math.abs(f[1] - t[1]), Math.abs(f[2] - t[2]), Math.abs(f[3] - t[3])) });
    el.style.transformOrigin = '0 0';
  }
  /* the springs are armed, not started: this frame commits the new screen's first layout with every moving panel
     still posed in its old box, and the motion clock starts on the next frame:
     the build never eats a spring frame */
  if (springs.length) { springT0 = -1; springArm = metrics.raf; poseSprings(1); }
  builtMode = state.mode;
  layoutDirty = false;
}
/* RETAIN-UNTIL-READY. A panel new to this screen is built, populated and laid out in this same frame, held invisible
   over the outgoing screen (which stays fully painted under it); the crossfade (190 ms in, 200 ms out) starts only
   once every picture the incoming panels show has decoded, so no frame shows a panel half-built or a picture still
   blank. The outgoing panels leave the document when their fade ends. A newer hop cancels a pending one. Promise-
   driven: no timer and no rAF of its own. */
let hopGen = 0;
function handover(entering, leaving) {
  const gen = ++hopGen;
  if (!entering.length && !leaving.length) return;
  const pics = [];
  for (const el of entering) for (const im of el.getElementsByTagName('img')) if (!im.complete || !im.naturalWidth) pics.push(im.decode ? im.decode().catch(() => {}) : null);
  /* an incoming panel with a picture still decoding is held fully transparent (the outgoing screen shows through);
     once ready it comes in from 60 % (never through blank) to full in 190 ms while the outgoing fades under it */
  if (pics.length) for (const el of entering) el.style.opacity = '0';
  const go = () => {
    if (gen !== hopGen) return;
    for (const el of entering) {
      if (el._enter || !el.isConnected) continue;
      el.style.opacity = '';
      const a = el._enter = el.animate([{ opacity: .6 }, { opacity: 1 }], { duration: 190, easing: 'cubic-bezier(.3,.6,.3,1)' });
      a.onfinish = () => { if (el._enter === a) el._enter = null; };
    }
    for (const a of leaving) if (a.playState === 'paused') a.play();
  };
  if (pics.length) Promise.all(pics).then(go); else go();
}
// One spring per hop (all moving panels share it), critically damped (ratio 1.0, w = 72 rad/s, i.e. stiffness
// 5184): x(t) = (1 + wt) e^-wt runs 1 -> 0 with no overshoot by construction. A panel lands when its residual is
// under 0.5 px (~105 ms for a 100 px move, ~142 ms for 1000 px) and never later than 150 ms after the motion starts
// (two frames after the build: the incoming screen is laid out and painted before anything moves). Evaluated in closed form from that start, so a slow frame cannot make it ring. Driven by the render loop's own rAF only while a
// spring is moving; hidden -> snapped.
const springs = []; let builtMode = null, springT0 = 0, springArm = 0;
const pans = {}, panDirty = new Set();   /* timeline pan / zoom per mode:panel (see onWheel) */
const SW = 72, SMAX = .15;   /* w 72 rad/s: a 1000 px move is under 0.5 px at 142 ms; the clock starts once the built screen has painted */
function poseSprings(x) {
  for (const s of springs) {
    if (!s.el) continue;
    const [dx, dy, sx, sy] = s.d;
    /* the panel slides from its old place (translate) and its edge sweeps from the old size to the new (clip): the
       contents are laid out once at their new size and are never stretched. Whole px at 1x and 2x: text never sits
       on a fractional offset */
    s.el.style.transform = `translate(${Math.round(dx * x)}px, ${Math.round(dy * x)}px)`;
    s.el.style.clipPath = `inset(0 ${Math.max(0, Math.round(s.W * (1 - sx) * x))}px ${Math.max(0, Math.round(s.H * (1 - sy) * x))}px 0 round var(--radius))`;
  }
}
function stepSprings(now) {
  if (!springs.length) return false;
  if (springT0 === -1) {
    if (now && !hidden && metrics.raf <= springArm + 1) return true;      // the build frame and the frame that paints it: held at the old pose
    if (now && !hidden) { springT0 = now; for (const s of springs) if (s.el) s.el.dataset.anim = '1'; }
  }
  const t = now && springT0 > 0 ? Math.max(0, (now - springT0) / 1000) : 0, x = (1 + SW * t) * Math.exp(-SW * t);
  let live = false;
  for (const s of springs) {
    if (!s.el) continue;
    if (hidden || !now || t >= SMAX || x * s.D < .5) { s.el.style.transform = ''; s.el.style.clipPath = ''; delete s.el.dataset.anim; s.el = null; continue; }
    live = true;
  }
  if (live) poseSprings(x);
  if (!live) springs.length = 0;
  return live;
}

// ---------- frame loop ----------
// Transport callbacks never touch the DOM: events are queued, and ONE
// requestAnimationFrame drains the whole queue (coalescing bursts of
// timeline/progress/frame events into a single paint). Nothing is scheduled
// while the queue is empty, and nothing at all runs while the window is
// hidden or minimized: no rAF, no timers.
const queue = [];
let rafId = 0, dirty = true, hidden = document.visibilityState === 'hidden';
export const QMAX = 2048;
export const metrics = { raf: 0, renders: 0, scoped: 0, panelWrites: 0, textPatches: 0, trackWrites: 0, ticks: 0, events: 0, coalesced: 0, dropped: 0, qmax: 0, staleFrames: 0 };
function enqueue(ev) {
  metrics.events++;
  // Hidden: fold the event into state right away (the reducer never touches
  // the DOM) so the queue cannot grow without bound; the one paint happens
  // when the window comes back.
  if (hidden) { for (const e of route(ev, state, MODES)) apply(e); return; }
  // Bursts: progress for a call already waiting in this frame replaces the older
  // one (only the newest wipe position matters), and the queue is capped:
  // past QMAX the oldest progress goes first, then the oldest event.
  const pid = ev && (ev.phase === 'progress' ? ev.call_id : ev.type === 'tool_progress' ? ev.id : undefined);
  if (pid !== undefined) {
    for (let i = queue.length - 1; i >= 0; i--) { const q = queue[i]; if ((q.phase === 'progress' && q.call_id === pid) || (q.type === 'tool_progress' && q.id === pid)) { queue[i] = ev; metrics.coalesced++; requestFrame(); return; } }
  }
  if (queue.length >= QMAX) { const j = queue.findIndex(q => q.phase === 'progress' || q.type === 'tool_progress'); queue.splice(j >= 0 ? j : 0, 1); metrics.dropped++; }
  queue.push(ev); if (queue.length > metrics.qmax) metrics.qmax = queue.length; requestFrame();
}
// RGB parade of the engine's latest frame for the grading screen: computed once per new frame from the
// decoded image (column x value histogram per channel, log-scaled), handed to the wheels panel as an image.
// Runs only when a frame arrives; nothing ticks while the session is idle.
let paradeSeq = 0;
function computeParade(src) {
  const seq = ++paradeSeq, img = new Image();
  img.onload = () => {
    if (seq !== paradeSeq) return;   // a newer frame superseded this one
    const W = 96, H = Math.max(1, Math.round(W * img.naturalHeight / Math.max(1, img.naturalWidth)));
    const c = document.createElement('canvas'); c.width = W; c.height = H;
    const g = c.getContext('2d', { willReadFrequently: true }); g.drawImage(img, 0, 0, W, H);
    const px = g.getImageData(0, 0, W, H).data, BINS = 128, hist = [0, 1, 2].map(() => new Float32Array(W * BINS));
    for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) { const i = (y * W + x) * 4; for (let k = 0; k < 3; k++) hist[k][x * BINS + (px[i + k] >> 1)]++; }
    const OW = W * 3 + 8, out = document.createElement('canvas'); out.width = OW; out.height = BINS;
    const o = out.getContext('2d'), id = o.createImageData(OW, BINS), d = id.data, norm = Math.log1p(H * .35);
    const COL = [[255, 96, 72], [110, 214, 120], [92, 140, 255]];
    for (let k = 0; k < 3; k++) for (let x = 0; x < W; x++) for (let b = 0; b < BINS; b++) {
      const v = hist[k][x * BINS + b]; if (!v) continue;
      const a = Math.min(1, Math.log1p(v) / norm), j = ((BINS - 1 - b) * OW + k * (W + 4) + x) * 4;
      d[j] = COL[k][0]; d[j + 1] = COL[k][1]; d[j + 2] = COL[k][2]; d[j + 3] = Math.round(40 + 215 * a);
    }
    o.putImageData(id, 0, 0);
    for (const [mode, M] of Object.entries(MODES)) for (const [name, spec] of Object.entries(M.panels)) if (spec.kind === 'wheels') { const V = view(mode); (V[name] || (V[name] = {})).parade = { src: out.toDataURL('image/png'), live: true }; }
    schedule();
  };
  img.src = src;
}
function schedule() { dirty = true; requestFrame(); }
// A frame that only carries split_clip / move_clip events repaints the timeline and the rail (the call's row)
// and nothing else: no layout pass, no top bar, no Inspector, no viewer. Those catch up on the next frame that
// carries no clip edit (one rAF, queued only while something is owed), so a stream of edits never repaints them.
const CLIP_EDIT = /^(split_clip|move_clip)$/;
const deferred = { on: false };
function isClipEdit(ev) { const n = ev && (ev.tool_name ?? (ev.type === 'tool_start' || ev.type === 'tool_end' || ev.type === 'tool_progress' ? ev.name : null)); return n != null && CLIP_EDIT.test(n); }
function renderScoped() { for (const [name, spec] of Object.entries(MODES[state.mode].panels)) if (spec.kind === 'timeline' || spec.kind === 'agent') renderPanel(name); }
function requestFrame() { if (!rafId && !hidden) rafId = requestAnimationFrame(frame); }
function frame() {
  rafId = 0; metrics.raf++; freezeNow(performance.now());
  try { frameBody(); } finally { freezeNow(0); }
}
function frameBody() {
  if (queue.length > 1) metrics.coalesced += queue.length - 1;
  const evs = queue.splice(0), editOnly = evs.length > 0 && !dirty && evs.every(isClipEdit);
  for (const ev of evs) for (const e of route(ev, state, MODES)) apply(e);
  if (editOnly && !layoutDirty && builtMode === state.mode) { dirty = false; renderScoped(); metrics.renders++; metrics.scoped++; deferred.on = true; requestFrame(); }
  else if (dirty || deferred.on) { dirty = false; deferred.on = false; panDirty.clear(); render(); metrics.renders++; }
  else if (panDirty.size) { for (const n of panDirty) renderPanel(n); panDirty.clear(); metrics.renders++; }
  // meter ballistics: frames only while a hit/decay window is open, then nothing
  if (state.animUntil && performance.now() < state.animUntil) { dirty = true; requestFrame(); } else state.animUntil = 0;
  if (stepSprings(performance.now())) requestFrame();   // a split still springing: one more frame
  armTicker();
}
function setHidden(h) {
  hidden = h;
  if (h) { if (rafId) cancelAnimationFrame(rafId); rafId = 0; clearTimeout(tickId); tickId = 0; stepSprings(0); }
  else { dirty = true; requestFrame(); }
}
document.addEventListener('visibilitychange', () => setHidden(document.visibilityState === 'hidden'));

// ---------- ticker ----------
// The elapsed clock and a running call's duration are the only things that
// change without an event. They update text in place (no rAF, no layout
// work) from one timeout, armed only while visible and something ticks.
let tickId = 0;
function armTicker() {
  if (tickId || hidden) return;
  const running = !state.session.still && state.calls.length && state.calls[state.calls.length - 1].status === 'running';
  const clockLive = !state.session.still && state.status.kind !== 'down';
  if (!running && !clockLive) return;
  const delay = running ? 100 : 1000 - ((elapsed() * 1000) % 1000) + 5;
  tickId = setTimeout(() => {
    tickId = 0; metrics.ticks++;
    const c = document.getElementById('clock');
    if (c) c.textContent = fmtClock(elapsed());
    tickRunning(state);
    armTicker();
  }, delay);
}

function fmtClock(s) {
  s = Math.max(0, Math.floor(s));
  const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, x = s % 60;
  return [h, m, x].map(n => String(n).padStart(2, '0')).join(':');
}
function elapsed() {
  const S = state.session;
  return S.still ? S.elapsed_s : S.elapsed_s + (performance.now() - S.t0) / 1000;
}

function renderTop() {
  const M = MODES[state.mode];
  { const pj = state.session.project, has = pj != null && String(pj).trim() !== '' && pj !== '—', pe = document.getElementById('proj'); setText(pe, has ? String(pj) : ''); setDisp(pe, has ? '' : 'none'); setDisp(pe.previousElementSibling, has ? '' : 'none'); }   // no '/ —' without a project name
  const pill = document.getElementById('livepill');
  pill.classList.toggle('down', state.status.kind === 'down');
  setText(document.getElementById('pill-state'), state.status.kind === 'down' ? String(state.status.label) : 'live');
  const ag = state.session.agent, agEl = document.getElementById('pill-agent'), hasAg = ag != null && String(ag).trim() !== '' && ag !== '—';
  setText(agEl, hasAg ? String(ag) : ''); setDisp(agEl, hasAg ? '' : 'none'); setDisp(agEl.previousElementSibling, hasAg ? '' : 'none');   // no empty segment between hyphens
  let act = state.activity;
  if (!state.explicitActivity || act == null) act = M.activity || state.mode;
  if (act === '#calls') act = `${state.callCount} calls`;
  setText(document.getElementById('pill-act'), String(act));
  const right = document.getElementById('tb-right');
  const rc = 'right ' + (M.rightCls || ''); if (right.className !== rc) right.className = rc;
  setText(document.getElementById('clock'), fmtClock(elapsed()));
}

function renderFooter() {
  const M = MODES[state.mode];
  const f = document.getElementById('footer');
  if (!M.foot) { paint(f, ''); return; }
  const st = state.stats;
  const saved = st.saved_s ?? (st.saved_at ? Math.round((performance.now() - st.saved_at) / 1000) : null);
  const parts = [];
  if (st.cpu != null) parts.push(`<span>cpu <span class="num">${String(Math.round(st.cpu)).padStart(2, ' ')}%</span></span>`);
  if (st.ffmpeg) parts.push(`<span>ffmpeg ${st.ffmpeg}</span>`);
  if (saved != null) parts.push(`<span>saved <span class="num">${saved}s</span> ago</span>`);
  paint(f, parts.join(''));
}

// A picked call can carry more rows than a short Inspector holds: the tail (Result, then the last Args) gives way,
// and one dim '+N more' row says so, instead of rows running past the panel's bottom edge (only on a pick render)
function fitPicked(el) {
  const kv = el.querySelector('.kv'); if (!kv) return;
  const bottom = el.getBoundingClientRect().bottom - 6, over = () => kv.lastElementChild && kv.lastElementChild.getBoundingClientRect().bottom > bottom;
  if (!over()) return;
  let n = 0;
  while (kv.children.length > 3 && over()) { const last = kv.lastElementChild; if (last.classList.contains('row')) n++; last.remove(); }
  while (kv.lastElementChild && kv.lastElementChild.classList.contains('sec')) kv.lastElementChild.remove();   // no empty section title at the end
  const more = document.createElement('div'); more.className = 'row dim'; more.innerHTML = `<span class="k"></span><span class="v">+${n} more</span>`; kv.appendChild(more);
  while (kv.children.length > 3 && over()) { more.previousElementSibling.remove(); more.lastElementChild.textContent = `+${++n} more`; }
}
/* Write only what changed. A panel whose markup is the same as last frame keeps its DOM (and whatever its POST
   painted), so a frame that edits the timeline repaints the timeline, the rail row and the call's own text, not every
   panel. Panel boxes are read once per layout (grid tracks are fixed for a layout), so a render never forces one. */
function setText(el, v) { if (el && el.textContent !== v) el.textContent = v; }
function setDisp(el, v) { if (el && el.style.display !== v) el.style.display = v; }
function paint(el, html) {
  if (el._h === html) return false;
  const P = TL_PARTS && TL_PARTS.html === html ? TL_PARTS : null, Q = el._p;   /* a timeline: rewrite only the tracks that changed */
  el._p = P;
  if (P && Q && P.pre === Q.pre && P.tracks.length && P.tracks.length === Q.tracks.length) {
    const tl = el.querySelector(':scope > .tl'), trs = tl ? tl.querySelectorAll(':scope > .trk') : [];
    if (trs.length === P.tracks.length) {
      for (let i = 0; i < trs.length; i++) if (P.tracks[i] !== Q.tracks[i]) { trs[i].outerHTML = P.tracks[i]; metrics.trackWrites++; }
      if (P.post !== Q.post) { const last = tl.querySelectorAll(':scope > .trk'), end = last[last.length - 1]; while (end.nextSibling) end.nextSibling.remove(); if (P.post) end.insertAdjacentHTML('afterend', P.post); }
      el._h = html; return true;
    }
  }
  if (el._h != null && html.length < 4096 && !html.includes('flash') && retext(el, el._h, html)) { el._h = html; metrics.textPatches++; return false; }
  const of = el.querySelector(':scope > .frame'), oi = of && of.querySelector(':scope > img:not(.before)');
  const olds = el.getElementsByTagName('img').length ? [...el.getElementsByTagName('img')] : null;
  el.innerHTML = html; el._h = html; metrics.panelWrites++;
  if (of) keepFrame(el, of, oi);
  if (olds) adoptImgs(el, olds);
  return true;
}
/* A rewritten panel keeps its pictures: an <img> the new markup still shows (same src, same attributes) is the old,
   already-decoded element put back, so a mode switch or a re-render never blanks a
   picture while a fresh copy decodes (the viewer's frame box and image are kept by keepFrame below). Attributes are copied
   only onto an element with the same attribute list, so the panel's markup reads exactly as written. */
function adoptImgs(el, olds) {
  const fresh = el.getElementsByTagName('img'); if (!fresh.length) return;
  const sig = i => i.getAttributeNames().join(' ');
  const bySrc = new Map(); for (const o of olds) { const s = o.getAttribute('src'); if (s != null && !bySrc.has(s)) bySrc.set(s, o); }
  const used = new Set(), mine = new Set(olds);
  for (const n of [...fresh]) {
    if (mine.has(n)) continue;   /* already the kept element (the viewer's frame image) */
    const s = sig(n), o = bySrc.get(n.getAttribute('src'));
    if (!o || used.has(o) || sig(o) !== s) continue;
    used.add(o);
    for (const k of n.getAttributeNames()) { const v = n.getAttribute(k); if (o.getAttribute(k) !== v) o.setAttribute(k, v); }
    n.replaceWith(o);
  }
}
/* Same tags and attributes, only the words differ (a viewer HUD caption naming the new call): the text nodes are
   updated in place and every element stays. Markup carrying a flash is always rebuilt, so its flash replays. */
const TAG = /<[^>]*>/g, tpl = document.createElement('template');
function texts(root) { const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT), out = []; while (w.nextNode()) out.push(w.currentNode); return out; }
function retext(el, prev, html) {
  const a = prev.match(TAG) || [], b = html.match(TAG) || [];
  if (a.length !== b.length || a.some((t, i) => t !== b[i])) return false;
  tpl.innerHTML = html; const now = texts(tpl.content), old = texts(el);
  if (now.length !== old.length) return false;
  for (let i = 0; i < now.length; i++) if (old[i].nodeValue !== now[i].nodeValue) old[i].nodeValue = now[i].nodeValue;
  return true;
}
/* An engine frame (Naman 11:38): the viewer's frame box and its <img> are never re-mounted. When a viewer panel is
   rewritten (a new frame, the out-of-date mark, the call HUD), the frame box and the picture already on screen are
   kept and only what is drawn over them is new: the frame buffer swaps by setting src on the same <img>. */
function syncAttrs(from, to) {
  for (const a of [...to.attributes]) if (!from.hasAttribute(a.name)) to.removeAttribute(a.name);
  for (const a of from.attributes) if (to.getAttribute(a.name) !== a.value) to.setAttribute(a.name, a.value);
}
function keepFrame(el, of, oi) {
  const nf = el.querySelector(':scope > .frame'); if (!nf || !of) return;
  const ni = nf.querySelector(':scope > img:not(.before)');
  if (ni && oi) { syncAttrs(ni, oi); ni.replaceWith(oi); }
  syncAttrs(nf, of); of.replaceChildren(...nf.childNodes); nf.replaceWith(of);
  metrics.frameKeeps = (metrics.frameKeeps || 0) + 1;
}
function box(el) { return el._sz || (el._sz = [el.clientWidth, el.clientHeight]); }
function render() {
  if (layoutDirty) buildLayout();
  renderTop();
  renderFooter();
  const M = MODES[state.mode], V = view(state.mode);
  const pickCall = state.pick != null ? state.calls.find(c => c.id === state.pick) : null;
  const kvs = Object.entries(M.panels).filter(([, sp]) => sp.kind === 'kv'), pickPanel = pickCall ? ((kvs.find(([, sp]) => sp.title === 'Inspector') || kvs[0] || [])[0]) : null;
  for (const [name, spec] of Object.entries(M.panels)) {
    const el = nodes[name]; if (!el) continue;
    const fn = KINDS[spec.kind];
    if (!fn) continue;
    const [w, h] = box(el);
    let data = V[name] || {};
    if (name === pickPanel && pickCall) data = { sub: `${pickCall.name}${pickCall.status === 'running' ? '' : ''}`, blocks: pickedRows(pickCall) };   // a clicked rail row, in the existing inspector
    const args = { spec, data, view: V, state, w, h, mode: state.mode, pan: pans[state.mode + ':' + name] };
    if (PRE[spec.kind]) PRE[spec.kind](el, args);   /* measures on the clean layout, before this panel is written */
    const wrote = paint(el, fn(args));
    if (spec.kind === 'scopes') statsLine(el, data.stats);
    if (!wrote) continue;
    if (name === pickPanel && pickCall) fitPicked(el);
    if (POST[spec.kind]) POST[spec.kind](el, args);
  }
}

window.addEventListener('resize', () => { layoutDirty = true; schedule(); });
// Timeline pan / zoom, the minimal scroll the mockups allow (no scrollbar, no control drawn): wheel / trackpad
// sideways (or shift+wheel) pans a zoomed-in timeline, ctrl+wheel or a pinch zooms around the pointer, plain wheel
// steps the track rows so a track below the panel (V6) can be brought up. Input only queues a frame, and that
// frame redraws just the timeline panel; nothing runs between inputs.
function onWheel(e) {
  const el = e.target.closest && e.target.closest('[data-panel]'); if (!el) return;
  const name = el.dataset.panel, spec = (MODES[state.mode] || {}).panels[name]; if (!spec || spec.kind !== 'timeline') return;
  const data = view(state.mode)[name]; if (!data || !(data.tracks || []).length) return;
  const key = state.mode + ':' + name, P = pans[key] || (pans[key] = { z: 1, t: null, k: 0, acc: 0 });
  const o = spec.opts || {}, x0 = o.x0 ?? 48, wpx = Math.max(1, el.clientWidth - (o.xr ?? 12) - x0);
  const r = data.ruler || { t0: 0, t1: 20 }, full = r.t1 - r.t0, span = full / P.z, t0 = P.t ?? r.t0;
  const k = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? wpx : 1;
  let dx = e.deltaX * k, dy = e.deltaY * k;
  if (e.shiftKey && !dx) { dx = dy; dy = 0; }
  if (e.ctrlKey || e.metaKey) {
    const zmax = Math.max(1, full / 4), z = Math.min(zmax, Math.max(1, P.z * Math.exp(-dy * .004)));
    if (z === P.z) return;
    const fx = Math.min(1, Math.max(0, ((e.clientX - el.getBoundingClientRect().left) / fitScale - x0) / wpx)), at = t0 + fx * span;
    P.z = z; P.t = z > 1 ? at - fx * full / z : null;
  } else if (Math.abs(dx) >= Math.abs(dy)) {
    if (P.z <= 1 || !dx) return;
    P.t = t0 + dx * span / wpx;
  } else {
    if (P.kmax === 0 && !P.k) return;
    P.acc += dy; const rows = Math.trunc(P.acc / 40); if (!rows) { e.preventDefault(); return; }
    P.acc -= rows * 40; P.k = (P.k || 0) + rows;
  }
  e.preventDefault(); panDirty.add(name); requestFrame();
}
main.addEventListener('wheel', onWheel, { passive: false });
function renderPanel(name) {
  const spec = (MODES[state.mode] || {}).panels[name], el = nodes[name]; if (!spec || !el) return;
  const [w, h] = box(el), V = view(state.mode), args = { spec, data: V[name] || {}, view: V, state, w, h, mode: state.mode, pan: pans[state.mode + ':' + name] };
  if (PRE[spec.kind]) PRE[spec.kind](el, args);
  const wrote = paint(el, KINDS[spec.kind](args));
  if (spec.kind === 'scopes') statsLine(el, args.data.stats);
  if (wrote && POST[spec.kind]) POST[spec.kind](el, args);
}
/* COLOR-PERF: the Color scopes' stats line (a colour read's answer) is its own small element, kept out of the scopes
   panel's markup. Inside the markup, each read rewrote the whole scopes panel twice (on apply, and again when its flash
   class dropped): ~8 ms more p95 per frame on the colour screen. Same element, class, text and place (last child). */
function statsLine(el, S) {
  if (el._st && el._st.parentNode !== el) unflash(el._st);   /* the panel was rewritten under it: its timer goes too */
  let d = el._st && el._st.parentNode === el ? el._st : null;
  if (!S || !S.text) { if (d) { unflash(d); d.remove(); } el._st = null; return; }
  const fl = flashing(S._flash);
  if (!d || (fl && d._f !== S._flash)) {   /* a new read: a fresh element, so its one-shot flash plays */
    const n = document.createElement('div'); n.className = fl ? 'scopestats flash' : 'scopestats';
    if (d) { unflash(d); d.replaceWith(n); } else el.appendChild(n);
    d = el._st = n; d._f = S._flash;
    if (fl) n._ft = setTimeout(() => { n._ft = 0; n.classList.remove('flash'); }, STATS_FLASH_MS);   /* POLISH-1: drops even with no render after it */
  } else if (!fl && d.classList.contains('flash')) unflash(d);
  if (d.textContent !== S.text) d.textContent = S.text;
}
/* POLISH-1: one timer per stats node, cleared whenever the node loses its flash, is replaced or is removed */
const STATS_FLASH_MS = 750;
function unflash(d) { if (d._ft) { clearTimeout(d._ft); d._ft = 0; } d.classList.remove('flash'); }
// click a rail row: its args / latency / result in the existing inspector; click it again or Esc to go back
main.addEventListener('click', e => { const r = e.target.closest && e.target.closest('.call[data-call]'); if (!r) return; const id = r.dataset.call, c = state.calls.find(x => String(x.id) === id); state.pick = c && state.pick !== c.id ? c.id : null; schedule(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape' && state.pick != null) { state.pick = null; schedule(); } });
window.__kimo = { state, apply: enqueue, metrics, setHidden };

// Tauri: a minimized window keeps document.visibilityState on some
// platforms, so the shell's own window events gate the loop too.
async function watchTauriWindow() {
  const w = window.__TAURI__?.window?.getCurrentWindow?.();
  if (!w) return;
  const check = async () => { try { setHidden((await w.isMinimized()) || !(await w.isVisible())); } catch {} };
  w.onResized?.(check); w.onFocusChanged?.(check);
}

document.fonts.ready.then(() => {
  connect(enqueue, (kind, label) => { state.status = { kind, label }; if (label === 'reconnecting' && state._gapAt == null) state._gapAt = performance.now(); schedule(); });
  watchTauriWindow();
  schedule();
});
