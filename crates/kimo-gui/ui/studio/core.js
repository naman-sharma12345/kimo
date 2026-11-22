// kimo app window. One bridge (Tauri IPC in the app; a labelled mock in a plain browser for tests), one state object,
// plain DOM. No network from the webview: every action goes through the app's own commands, and every engine edit
// goes through the app's ledger (so Live, Changes and the timeline agree, whoever made the edit).
"use strict";
const $ = s => document.querySelector(s);
const $$ = s => [...document.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

// One bridge: Tauri IPC in the app. In a plain browser (tests) it stays null here and studio/main.js loads the labelled mock.
let B = window.__TAURI__
  ? { kind: 'tauri', invoke: (c, a) => window.__TAURI__.core.invoke(c, a || {}), listen: (e, f) => window.__TAURI__.event.listen(e, x => f(x.payload)) }
  : null;

// ---------- icons ----------
const ICONS = {
  home: '<path d="M4 10.5 12 4l8 6.5V19a1.5 1.5 0 0 1-1.5 1.5H15v-5h-6v5H5.5A1.5 1.5 0 0 1 4 19z"/>',
  film: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M7.5 4.5v15M16.5 4.5v15M3.5 9h4M3.5 15h4M16.5 9h4M16.5 15h4"/>',
  spark: '<path d="M12 3.5 13.9 9l5.6 1.9-5.6 1.9L12 18.5l-1.9-5.7L4.5 10.9 10.1 9z"/><path d="M18.5 3.5v3M17 5h3"/>',
  live: '<circle cx="12" cy="12" r="2"/><path d="M8.2 8.2a5.4 5.4 0 0 0 0 7.6M15.8 8.2a5.4 5.4 0 0 1 0 7.6M5.4 5.4a9.3 9.3 0 0 0 0 13.2M18.6 5.4a9.3 9.3 0 0 1 0 13.2"/>',
  plug: '<path d="M9 3v4M15 3v4M7 7h10v4a5 5 0 0 1-10 0zM12 16v5"/>',
  folder: '<path d="M3.5 7.5A1.5 1.5 0 0 1 5 6h4l2 2h8a1.5 1.5 0 0 1 1.5 1.5v8A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z"/>',
  plus: '<path d="M12 5v14M5 12h14"/>', minus: '<path d="M5 12h14"/>',
  arrow: '<path d="M5 12h14M13 6l6 6-6 6"/>',
  undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
  redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
  scissors: '<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M8.1 8.1 20 20M8.1 15.9 20 4"/>',
  trash: '<path d="M4 7h16M10 11v6M14 11v6M5.5 7l1 12a1.5 1.5 0 0 0 1.5 1.4h8a1.5 1.5 0 0 0 1.5-1.4l1-12M9 7V4.5h6V7"/>',
  copy: '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>',
  type: '<path d="M5 7V5h14v2M12 5v14M9 19h6"/>',
  layers: '<path d="m12 4 8.5 4.5L12 13 3.5 8.5z"/><path d="m3.5 12.5 8.5 4.5 8.5-4.5M3.5 16.5 12 21l8.5-4.5"/>',
  flag: '<path d="M5 21V4M5 4h11l-2 4 2 4H5"/>',
  save: '<path d="M5 4h11l3 3v12.5A1.5 1.5 0 0 1 17.5 21h-11A1.5 1.5 0 0 1 5 19.5z"/><path d="M8 4v5h7V4M8 21v-6h8v6"/>',
  export: '<path d="M12 15V3M7 8l5-5 5 5"/><path d="M5 13v6.5A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V13"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>',
  upload: '<path d="M12 16V4M7 9l5-5 5 5"/><path d="M4 16v3.5A1.5 1.5 0 0 0 5.5 21h13a1.5 1.5 0 0 0 1.5-1.5V16"/>',
  send: '<path d="M12 19V5M6 11l6-6 6 6"/>',
  stop: '<rect x="7" y="7" width="10" height="10" rx="1.5"/>',
  graph: '<path d="M4 19h16M7 16V9M12 16V5M17 16v-4"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>',
  refresh: '<path d="M20 11a8 8 0 0 0-14.6-4.5L4 8M4 4v4h4M4 13a8 8 0 0 0 14.6 4.5L20 16M20 20v-4h-4"/>',
  key: '<circle cx="8" cy="15" r="4"/><path d="m11 12 8.5-8.5M16 7l2.5 2.5M14 9l2 2"/>',
  check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>', x: '<path d="M6 6l12 12M18 6 6 18"/>',
  video: '<rect x="3.5" y="6" width="12" height="12" rx="2"/><path d="m15.5 10.5 5-3v9l-5-3"/>',
  audio: '<path d="M4 12h1.5M7 8v8M10 5v14M13 9v6M16 7v10M19 11v2"/>',
  image: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.8"/><path d="m4 18 5.5-5.5 4 4 2.5-2.5 4.5 4.5"/>',
  term: '<rect x="3.5" y="5" width="17" height="14" rx="2.5"/><path d="m7.5 10 2.5 2-2.5 2M12.5 14.5h4"/>',
  wand: '<path d="m4 20 11-11M14 5l1-2 1 2 2 1-2 1-1 2-1-2-2-1zM19 12l.7-1.3L21 10l-1.3-.7L19 8l-.7 1.3L17 10l1.3.7z"/>',
  safe: '<rect x="3" y="5" width="18" height="14" rx="1.5"/><rect x="6.5" y="8" width="11" height="8" rx=".5" stroke-dasharray="2 2"/>',
  grid: '<rect x="3" y="5" width="18" height="14" rx="1.5"/><path d="M9 5v14M15 5v14M3 9.7h18M3 14.3h18"/>',
  expand: '<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>',
  stepb: '<path d="M6 5v14M18 6l-9 6 9 6z"/>', stepf: '<path d="M18 5v14M6 6l9 6-9 6z"/>',
  sliders: '<path d="M5 6h9M18 6h1M5 12h3M12 12h7M5 18h11M20 18h-1"/><circle cx="16" cy="6" r="2"/><circle cx="10" cy="12" r="2"/><circle cx="18" cy="18" r="2"/>',
  history: '<path d="M4 12a8 8 0 1 0 2.4-5.7L4 8.5M4 4v4.5h4.5"/><path d="M12 8v4.5l3 1.8"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6 10h.01M9.5 10h.01M13 10h.01M16.5 10h.01M7 14h10"/>',
};
const ICON = name => `<svg class="i" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name] || ''}</svg>`;
function icons(root = document) { root.querySelectorAll('[data-icon]:not([data-ic-done])').forEach(el => { el.insertAdjacentHTML('afterbegin', ICON(el.dataset.icon)); el.dataset.icDone = '1'; }); }
icons();

// ---------- small helpers ----------
function toast(t, kind) { const el = $('#toast'); el.textContent = t; el.classList.toggle('bad', kind === 'bad'); el.classList.add('on'); clearTimeout(toast.t); toast.t = setTimeout(() => el.classList.remove('on'), kind === 'bad' ? 4200 : 2200); }
async function copy(text, btn) {
  try { await navigator.clipboard.writeText(text); }
  catch { const ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select(); document.execCommand('copy'); ta.remove(); }
  toast('Copied');
  if (btn) { btn.dataset.label ??= btn.textContent; btn.textContent = 'Copied ✓'; clearTimeout(btn._t); btn._t = setTimeout(() => { btn.textContent = btn.dataset.label; }, 1400); }
}
const tc = s => { s = Math.max(0, s || 0); const m = Math.floor(s / 60); return `${m}:${(s - m * 60).toFixed(1).padStart(4, '0')}`; };
const tc2 = s => { s = Math.max(0, s || 0); const m = Math.floor(s / 60); return `${m}:${(s - m * 60).toFixed(2).padStart(5, '0')}`; };
// editing timecode, HH:MM:SS:FF at the project's frame rate (what editors read on every other tool)
const tcf = (s, fps = P.fps || 30) => { const f = Math.round(Math.max(0, s || 0) * fps), r = Math.round(fps), p2 = n => String(n).padStart(2, '0'); return `${p2(Math.floor(f / (r * 3600)))}:${p2(Math.floor(f / (r * 60)) % 60)}:${p2(Math.floor(f / r) % 60)}:${p2(f % r)}`; };
const clock = t => new Date(t).toLocaleTimeString([], { hour12: false });
const base = p => String(p || '').split(/[\\/]/).pop();
const r3 = v => Math.round(v * 1000) / 1000;
const AUDIO_EXT = /\.(mp3|wav|aac|m4a|flac|ogg|opus|aiff?)$/i, IMAGE_EXT = /\.(png|jpe?g|webp|gif|bmp|tiff?|exr|svg)$/i;
function confirmBox(title, body, yes = 'OK', no = 'Cancel') {
  return new Promise(res => {
    $('#cf-title').textContent = title; $('#cf-body').textContent = body; $('#cf-yes').textContent = yes; $('#cf-no').textContent = no;
    $('#confirm').classList.add('on');
    const done = v => { $('#confirm').classList.remove('on'); res(v); };
    $('#cf-yes').onclick = () => done(true); $('#cf-no').onclick = () => done(false);
  });
}

const S = {
  app: null, tab: 'home', cli: [], changes: [], live: new Map(), liveOrder: [], liveFilter: 'all', attached: {},
  turn: 0, busy: false, dirty: false, sel: null, transitions: [],
};
const ACTOR = { you: 'You', 'claude-code': 'Claude Code', codex: 'Codex', antigravity: 'Antigravity', 'kimo-chat': 'API agent', app: 'kimo', agent: 'MCP app', api: 'API key' };
const actorName = a => ACTOR[a] || a;
const actorChip = a => `<span class="chip a-${ACTOR[a] ? esc(a) : 'agent'}">${esc(actorName(a))}</span>`;
const AGENTS = ['claude-code', 'codex', 'antigravity'];

// ---------- engine helpers ----------
async function call(tool, action, args = {}, quiet = false) {
  try {
    const r = await B.invoke('engine_call', { tool, args: { action, args } });
    if (!r.ok) { if (!quiet) toast(r.text || `${action} failed`, 'bad'); return null; }
    return r.data ?? r.text;
  } catch (e) { if (!quiet) toast(String(e), 'bad'); return null; }
}
async function read(tool, action, args = {}) {
  try { const r = await B.invoke('engine_read', { tool, args: { action, args } }); return r.ok ? (r.data ?? r.text) : null; } catch { return null; }
}

// ---------- tabs ----------
const TITLES = { home: 'Home', edit: 'Studio', live: 'Activity', connections: 'Connections' };
function show(tab) {
  const toAgent = tab === 'agent'; // the agent lives in the Studio's side panel
  if (toAgent) tab = 'edit';
  if (!TITLES[tab]) return;
  S.tab = tab;
  $('#tb-title').textContent = TITLES[tab];
  $$('#nav a').forEach(a => a.classList.toggle('on', a.dataset.tab === tab));
  $$('.tab').forEach(s => s.classList.toggle('on', s.id === 'tab-' + tab));
  if (tab === 'live') $('#nav-live').hidden = true;
  if (tab === 'edit') { $('#nav-agent').hidden = true; requestAnimationFrame(renderTimeline); }
  if (toAgent) { sidePane('agent'); setTimeout(() => $('#chat-in').focus(), 30); }
  if (tab === 'connections') refreshCli();
}
// the Studio's side panel: Agent, Inspector, History
S.pane = 'agent';
function sidePane(name) {
  S.pane = name;
  $$('.side-tabs [data-pane]').forEach(b => { const on = b.dataset.pane === name; b.classList.toggle('on', on); b.setAttribute('aria-selected', on); });
  $$('.ed-side .pane').forEach(p => p.classList.toggle('on', p.id === 'pane-' + name));
  if (name === 'agent') $('#side-agent-dot').hidden = true;
}
$$('.side-tabs [data-pane]').forEach(b => b.onclick = () => sidePane(b.dataset.pane));
$$('#nav a').forEach(a => a.onclick = () => show(a.dataset.tab));
document.addEventListener('click', e => { const g = e.target.closest('[data-go]'); if (g) { e.preventDefault(); show(g.dataset.go); } });

// ---------- app state ----------
async function refresh() {
  const a = S.app = await B.invoke('app_state');
  $('#ab-ver').textContent = kimoRelease(a);
  const e = a.engine || {};
  const pe = $('#pill-engine');
  // the engine must be the one this app shipped with: an older one (left behind by a manual copy or an update that could not
  // replace a running engine) is missing tools and fixes, so say so instead of running it quietly
  const mismatch = e.state === 'running' && e.version && a.version && e.version !== a.version;
  pe.className = 'rail-pill ' + (mismatch ? 'warn' : e.state === 'running' ? 'ok' : e.state === 'stopped' ? '' : 'bad');
  pe.title = mismatch ? `Engine v${e.version} does not match this app (v${a.version}). Reinstall kimo to update the engine.`
    : e.state === 'running' ? `Engine v${e.version} · ${e.tools} tools, ${e.actions} actions` : e.state === 'missing' ? 'Engine not found' : e.state === 'crashed' ? 'Engine stopped: ' + e.error : 'Engine starting…';
  $('#h-engine').innerHTML = mismatch ? `<span style="color:var(--warn)">Engine v${esc(e.version)} ≠ app v${esc(a.version)}</span> — reinstall kimo`
    : e.state === 'running' ? `Running · v${esc(e.version)} · ${e.actions} actions`
    : e.state === 'missing' ? '<span style="color:var(--bad)">Not found</span> — reinstall kimo'
    : e.state === 'crashed' ? `<span style="color:var(--bad)">Stopped</span> · <a href="#" id="eng-restart">restart</a>` : 'Starting…';
  $('#eng-restart')?.addEventListener('click', ev => { ev.preventDefault(); B.invoke('engine_restart').then(() => { refresh(); loadProject(); }); });
  $('#ab-eng').textContent = e.version ? 'v' + e.version : e.state;
  $('#h-data').textContent = a.data_dir; $('#h-data').title = a.data_dir;
  setProjectName(a.project_path);
  renderAttached(a.attached || {});
  renderAgentPill();
  renderRecent();
  renderKeys();
  $('#mcp-switch').classList.toggle('on', !!a.mcp_enabled);
  if (a.snippets) $('#cfg-json').textContent = a.snippets.json;
}
function setProjectName(p) {
  $('#tb-proj-t').textContent = p ? base(p).replace(/\.(kimo|json)$/, '') : 'Untitled project';
  $('#tb-proj').title = p || 'Not saved yet';
  $('#tb-dirty').hidden = !S.dirty;
}
function renderAttached(att) {
  S.attached = att;
  const names = Object.keys(att);
  const el = $('#tb-attached');
  el.hidden = !names.length;
  el.textContent = names.length ? names.map(actorName).join(', ') + ' connected' : '';
  $('#h-attached').textContent = names.length ? names.map(actorName).join(', ') : 'none';
  $('#live-who').innerHTML = names.length ? names.map(n => `<div class="wrow"><span class="d"></span>${esc(actorName(n))}<span class="note muted" style="margin-left:auto">${att[n]} session${att[n] > 1 ? 's' : ''}</span></div>`).join('')
    : '<div class="note muted">No agent attached right now. Agents attach while they work.</div>';
}

// ================================================================================================================
// PROJECT MODEL
const P = { proj: null, tracks: [], assets: [], markers: [], dur: 0, fps: 30, w: 1280, h: 720, clips: new Map(), prevIds: new Set(), thumbs: new Map(), proxy: new Map(), loaded: false };
function clipEnd(c) { return c.start + c.duration / (c.speed || 1); }
function clipKind(c) { return c.source?.type || 'media'; }
const assetOf = c => c.source?.asset_id ?? c.source?.asset;
function clipLabel(c) {
  const s = c.source || {};
  if (s.type === 'text') return s.text || 'Text';
  if (s.type === 'media' || assetOf(c)) { const id = assetOf(c), a = P.proj?.assets?.[id]; return c.name || (a ? base(a.path) : id || 'Media'); }
  return c.name || s.type || 'Clip';
}
let loadSeq = 0, lastActor = 'you';
async function loadProject() {
  const my = ++loadSeq;
  const p = await read('project', 'get_project');
  if (my !== loadSeq || !p || typeof p !== 'object') return;
  P.proj = p; P.fps = p.fps || 30; P.w = p.width; P.h = p.height; P.markers = p.markers || [];
  P.tracks = p.tracks || [];
  const ids = new Set();
  P.clips.clear();
  let dur = 0;
  P.tracks.forEach((t, ti) => (t.clips || []).forEach(c => { c._track = t.id; c._ti = ti; P.clips.set(c.id, c); ids.add(c.id); dur = Math.max(dur, clipEnd(c)); }));
  P.fresh = P.loaded ? new Set([...ids].filter(i => !P.prevIds.has(i))) : new Set();
  P.prevIds = ids; P.dur = dur;
  const usage = {};
  P.clips.forEach(c => { const a = assetOf(c); if (a) usage[a] = (usage[a] || 0) + 1; });
  P.assets = Object.values(p.assets || {}).map(a => ({ ...a, used: usage[a.id] || 0 }));
  if (S.sel && !P.clips.has(S.sel)) S.sel = null;
  P.loaded = true;
  renderBin(); renderTimeline(); renderInspector();
  $('#tl-empty').hidden = P.clips.size > 0;
  loadProxyJobs(my);
}
let loadTimer = 0;
function loadSoon(ms = 120) { clearTimeout(loadTimer); loadTimer = setTimeout(loadProject, ms); }

// "26.11 Altair": the version without a .0 fix number, then the release's name
function kimoRelease(a) { const v = String(a?.version || '').replace(/\.0$/, ''); return a?.codename ? `${v} ${a.codename}` : v; }
