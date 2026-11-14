// kimo app window. One bridge (Tauri IPC in the app; a labelled mock in a plain browser for tests), one state object,
// plain DOM. No network from the webview: every action goes through the app's own commands, and every engine edit
// goes through the app's ledger (so Live, Changes and the timeline agree, whoever made the edit).
const $ = s => document.querySelector(s);
const $$ = s => [...document.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));

let B;
if (window.__TAURI__) {
  B = { kind: 'tauri', invoke: (c, a) => window.__TAURI__.core.invoke(c, a || {}), listen: (e, f) => window.__TAURI__.event.listen(e, x => f(x.payload)) };
} else {
  B = (await import('./dev/mock-bridge.js')).bridge;
}
window.__kimoBridge = B;

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
const TITLES = { home: 'Home', edit: 'Edit', agent: 'Agent', live: 'Live', connections: 'Connections' };
function show(tab) {
  if (!TITLES[tab]) return;
  S.tab = tab;
  $('#tb-title').textContent = TITLES[tab];
  $$('#nav a').forEach(a => a.classList.toggle('on', a.dataset.tab === tab));
  $$('.tab').forEach(s => s.classList.toggle('on', s.id === 'tab-' + tab));
  if (tab === 'live') $('#nav-live').hidden = true;
  if (tab === 'agent') { $('#nav-agent').hidden = true; $('#slot-agent').appendChild($('#viewer')); setTimeout(() => $('#chat-in').focus(), 30); }
  if (tab === 'edit') { $('#slot-edit').prepend($('#viewer')); requestAnimationFrame(renderTimeline); }
  if (tab === 'connections') refreshCli();
}
$$('#nav a').forEach(a => a.onclick = () => show(a.dataset.tab));
document.addEventListener('click', e => { const g = e.target.closest('[data-go]'); if (g) { e.preventDefault(); show(g.dataset.go); } });

// ---------- app state ----------
async function refresh() {
  const a = S.app = await B.invoke('app_state');
  $('#ab-ver').textContent = 'v' + a.version;
  const e = a.engine || {};
  const pe = $('#pill-engine');
  pe.className = 'rail-pill ' + (e.state === 'running' ? 'ok' : e.state === 'stopped' ? '' : 'bad');
  pe.title = e.state === 'running' ? `Engine v${e.version} · ${e.tools} tools, ${e.actions} actions` : e.state === 'missing' ? 'Engine not found' : e.state === 'crashed' ? 'Engine stopped: ' + e.error : 'Engine starting…';
  $('#h-engine').innerHTML = e.state === 'running' ? `Running · v${esc(e.version)} · ${e.actions} actions`
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
const P = { proj: null, tracks: [], assets: [], markers: [], dur: 0, fps: 30, w: 1280, h: 720, clips: new Map(), prevIds: new Set(), thumbs: new Map(), loaded: false };
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
}
let loadTimer = 0;
function loadSoon(ms = 120) { clearTimeout(loadTimer); loadTimer = setTimeout(loadProject, ms); }

// ================================================================================================================
// MEDIA BIN
async function importPaths(paths) {
  if (!paths?.length) return;
  if (S.app?.engine?.state !== 'running') return toast('The engine is not running yet.', 'bad');
  let ok = 0, last = null;
  for (const path of paths) {
    toast(`Importing ${base(path)}…`);
    const r = await call('project', 'import_media', { path });
    if (r?.asset_id) { ok++; last = r.asset_id; }
  }
  if (ok) { toast(`Imported ${ok} file${ok > 1 ? 's' : ''}`); show(S.tab === 'home' ? 'edit' : S.tab); }
  await loadProject();
  if (last) $(`.asset[data-id="${last}"]`)?.scrollIntoView({ block: 'nearest' });
}
async function pickMedia() { try { importPaths(await B.invoke('pick', { kind: 'media' })); } catch (e) { toast(String(e), 'bad'); } }
$('#bin-import').onclick = pickMedia; $('#bin-drop-pick').onclick = pickMedia; $('#home-import').onclick = pickMedia;
$('#bin-q').oninput = renderBin;
function assetIcon(a) { return AUDIO_EXT.test(a.path) ? 'audio' : IMAGE_EXT.test(a.path) ? 'image' : 'video'; }
function renderBin() {
  const q = $('#bin-q').value.trim().toLowerCase();
  const list = P.assets.filter(a => !q || base(a.path).toLowerCase().includes(q));
  $('#bin-count').textContent = P.assets.length;
  const el = $('#bin-list');
  if (!P.assets.length) { el.innerHTML = '<div class="bin-empty">No media yet.</div>'; return; }
  el.innerHTML = list.map(a => `<div class="asset" data-id="${esc(a.id)}" title="${esc(a.path)}">
    <div class="th">${P.thumbs.get(a.path) ? `<img src="${esc(P.thumbs.get(a.path))}" alt="">` : ICON(assetIcon(a))}<span class="dur">${tc(a.duration)}</span>${a.used ? `<span class="used">${a.used}×</span>` : ''}
      <button class="add" title="Add to timeline at the playhead" aria-label="Add to timeline">${ICON('plus')}</button></div>
    <div class="nm">${esc(base(a.path))}</div><div class="mt">${a.width ? `${a.width}×${a.height}` : ''}${a.fps ? ` · ${Math.round(a.fps * 100) / 100}fps` : ''}</div></div>`).join('') || '<div class="bin-empty">No match.</div>';
  for (const a of list) if (!P.thumbs.has(a.path)) {
    P.thumbs.set(a.path, null);
    B.invoke('media_thumb', { path: a.path }).then(u => { if (u) { P.thumbs.set(a.path, u); const img = $(`.asset[data-id="${a.id}"] .th`); if (img && !img.querySelector('img')) { img.querySelector('svg')?.remove(); img.insertAdjacentHTML('afterbegin', `<img src="${esc(u)}" alt="">`); } } }).catch(() => {});
  }
}
$('#bin-list').addEventListener('dblclick', e => { const a = e.target.closest('.asset'); if (a) addAsset(a.dataset.id); });
$('#bin-list').addEventListener('click', e => { const b = e.target.closest('.add'); if (b) { e.stopPropagation(); addAsset(b.closest('.asset').dataset.id); } });
$('#bin-list').addEventListener('contextmenu', e => {
  const a = e.target.closest('.asset'); if (!a) return; e.preventDefault();
  const asset = P.assets.find(x => x.id === a.dataset.id);
  menu(e, [
    ['plus', 'Add at playhead', () => addAsset(asset.id)],
    ['arrow', 'Add at the end', () => addAsset(asset.id, null, P.dur)],
    ['folder', 'Show in folder', () => B.invoke('reveal', { path: asset.path })],
    ['spark', 'Ask the agent to use it', () => { show('agent'); $('#chat-in').value = `Use ${base(asset.path)} (asset ${asset.id}) `; $('#chat-in').focus(); }],
  ]);
});
// a track for new media: the selected clip's track, else the lowest unlocked track with room at `at`, else a new track
async function trackFor(at, dur, prefer) {
  if (prefer) return prefer;
  const sel = S.sel && P.clips.get(S.sel);
  const free = t => !t.locked && !(t.clips || []).some(c => c.start < at + dur - 1e-6 && clipEnd(c) > at + 1e-6);
  if (sel) { const t = P.tracks.find(t => t.id === sel._track); if (t && free(t)) return t.id; }
  const t = P.tracks.find(free);
  if (t) return t.id;
  const r = await call('timeline', 'add_track', {});
  return r?.track_id;
}
async function addAsset(id, track, at) {
  const a = P.assets.find(x => x.id === id); if (!a) return;
  at = r3(at ?? V.t);
  const dur = r3(a.duration || 5);
  const tid = await trackFor(at, dur, track);
  if (!tid) return;
  const r = await call('timeline', 'add_clip', { track_id: tid, source_type: 'media', asset_id: id, start: at, duration: dur });
  if (r?.clip_id) { S.sel = r.clip_id; await loadProject(); viewerSoon(); }
}

// ================================================================================================================
// TIMELINE
const T = { z: 80, trh: 52 };
const tlScroll = $('#tl-scroll'), tlCanvas = $('#tl-canvas');
const x2t = x => Math.max(0, x / T.z), t2x = t => t * T.z;
function displayTracks() { return [...P.tracks].reverse(); } // top of the stack first
function renderTimeline() {
  if (!P.loaded) return;
  const width = Math.max(tlScroll.clientWidth, t2x(Math.max(P.dur + 10, 30)));
  tlCanvas.style.width = width + 'px';
  // ruler
  const step = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300].find(s => s * T.z >= 64) || 600;
  const minor = step / (step >= 1 ? 5 : 2);
  const end = width / T.z;
  let r = '';
  for (let t = 0; t <= end + 1e-9; t += minor) {
    const big = Math.abs(t / step - Math.round(t / step)) < 1e-6;
    r += `<span class="tk${big ? ' big' : ''}" style="left:${t2x(t)}px"></span>`;
    if (big) r += `<span class="tl" style="left:${t2x(t)}px">${step < 1 ? tc(t) : tc(t).replace(/\.0$/, '')}</span>`;
  }
  for (const m of P.markers) r += `<span class="mk" style="left:${t2x(m.time)}px;background:${esc(m.color || '')}" title="${esc(m.name || 'Marker')} · ${tc(m.time)}" data-mk="${esc(m.id)}"></span>`;
  $('#tl-ruler').innerHTML = r;
  // tracks
  const dts = displayTracks();
  $('#tl-heads').innerHTML = dts.map((t, i) => `<div class="thead${t.locked ? ' locked' : ''}" data-track="${esc(t.id)}">
      <span class="tn" title="Double-click to rename">${esc(t.name || 'Track ' + (P.tracks.length - i))}</span>
      <button class="ib sm${t.muted ? ' on' : ''}" data-tk="muted" title="Mute">M</button>
      <button class="ib sm${t.solo ? ' on' : ''}" data-tk="solo" title="Solo">S</button>
      <button class="ib sm${t.locked ? ' on' : ''}" data-tk="locked" title="Lock">L</button></div>`).join('');
  $('#tl-tracks').innerHTML = dts.map(t => `<div class="track${t.locked ? ' locked' : ''}${t.muted ? ' muted' : ''}" data-track="${esc(t.id)}">${(t.clips || []).map(c => {
    const w = Math.max(4, t2x(clipEnd(c) - c.start));
    const kind = clipKind(c);
    const fx = (c.effects || []).length;
    return `<div class="clip${c.id === S.sel ? ' sel' : ''}${P.fresh?.has(c.id) ? ' fresh' : ''}" data-clip="${esc(c.id)}" data-kind="${esc(kind)}" style="left:${t2x(c.start)}px;width:${w}px" title="${esc(clipLabel(c))} · ${tc(c.start)}–${tc(clipEnd(c))}">
      ${c.transition_in ? `<span class="tr" style="width:${Math.min(w, t2x(c.transition_in.duration || 0.5))}px"></span>` : ''}
      <span class="h l"></span><span class="cl">${esc(clipLabel(c))}</span>${w > 70 ? `<span class="cs">${tc(clipEnd(c) - c.start)}${c.speed && c.speed !== 1 ? ' · ' + c.speed + '×' : ''}</span>` : ''}${fx ? `<span class="fx">fx${fx > 1 ? fx : ''}</span>` : ''}<span class="h r"></span></div>`;
  }).join('')}</div>`).join('');
  P.fresh = new Set();
  placePlayhead();
}
function placePlayhead() {
  $('#tl-playhead').style.left = t2x(V.t) + 'px';
  $('#tl-tc').textContent = tc2(V.t);
}
tlScroll.addEventListener('scroll', () => { $('#tl-heads').scrollTop = tlScroll.scrollTop; });
function setZoom(z, anchorT) {
  const old = T.z; T.z = clamp(z, 6, 600); $('#tl-zoom').value = T.z;
  const at = anchorT ?? V.t, sx = t2x(at) * (old ? 1 : 1);
  const px = at * old - tlScroll.scrollLeft;
  renderTimeline();
  tlScroll.scrollLeft = at * T.z - px;
  void sx;
}
$('#tl-zoom').oninput = e => setZoom(+e.target.value);
$('#tl-zin').onclick = () => setZoom(T.z * 1.4); $('#tl-zout').onclick = () => setZoom(T.z / 1.4);
$('#tl-fit').onclick = () => setZoom((tlScroll.clientWidth - 40) / Math.max(5, P.dur || 5), 0);
tlScroll.addEventListener('wheel', e => {
  if (e.ctrlKey || e.metaKey) { e.preventDefault(); const r = tlScroll.getBoundingClientRect(); setZoom(T.z * (e.deltaY < 0 ? 1.15 : 1 / 1.15), x2t(e.clientX - r.left + tlScroll.scrollLeft)); }
}, { passive: false });
// seek by ruler / empty lane
function seekTo(t, render = true) {
  V.t = clamp(r3(t), 0, Math.max(P.dur || 0, V.dur || 0, 0)); $('#vw-scrub').value = V.t; vTime(); placePlayhead();
  if (render) { if (inDraft(V.t)) $('#vw-scrub').oninput(); else viewerSeek(); }
}
let seekTimer = 0;
function viewerSeek() { const hit = V.frames.get(`${V.rev}:${fidx(V.t)}`); if (hit) { $('#vw-img').src = hit.url; vMedia('image'); } else PB.scrub(V.t); clearTimeout(seekTimer); seekTimer = setTimeout(() => { if (!V.frames.has(`${V.rev}:${fidx(V.t)}`)) viewerRefresh(); }, 140); }
$('#tl-ruler').addEventListener('pointerdown', e => {
  if (e.target.dataset.mk) { const m = P.markers.find(x => String(x.id) === e.target.dataset.mk); if (m) seekTo(m.time); return; }
  const r = tlCanvas.getBoundingClientRect();
  const mv = ev => seekTo(x2t(ev.clientX - r.left));
  mv(e);
  const up = () => { removeEventListener('pointermove', mv); removeEventListener('pointerup', up); };
  addEventListener('pointermove', mv); addEventListener('pointerup', up);
});
// track header buttons + rename
$('#tl-heads').addEventListener('click', async e => {
  const b = e.target.closest('[data-tk]'); if (!b) return;
  const tid = b.closest('.thead').dataset.track, t = P.tracks.find(x => x.id === tid);
  await call('timeline', 'set_track', { track_id: tid, [b.dataset.tk]: !t[b.dataset.tk] });
});
$('#tl-heads').addEventListener('dblclick', e => {
  const n = e.target.closest('.tn'); if (!n) return;
  const tid = n.closest('.thead').dataset.track;
  const old = n.textContent;
  n.innerHTML = `<input type="text" value="${esc(old)}">`;
  const i = n.querySelector('input'); i.focus(); i.select();
  const done = async ok => { i.onblur = null; if (ok && i.value.trim() && i.value !== old) await call('timeline', 'set_track', { track_id: tid, name: i.value.trim() }); else n.textContent = old; };
  i.onkeydown = ev => { if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); };
  i.onblur = () => done(true);
});
$('#tl-heads').addEventListener('contextmenu', e => {
  const h = e.target.closest('.thead'); if (!h) return; e.preventDefault();
  const tid = h.dataset.track, t = P.tracks.find(x => x.id === tid);
  menu(e, [
    ['layers', 'Add track', () => call('timeline', 'add_track', {})],
    ['minus', 'Close gaps on this track', () => call('timeline', 'close_gaps', { track_ids: [tid] })],
    null,
    ['trash', `Delete track${t.clips?.length ? ` and its ${t.clips.length} clip${t.clips.length > 1 ? 's' : ''}` : ''}`, async () => { if (!t.clips?.length || await confirmBox('Delete this track?', `It has ${t.clips.length} clip(s). You can undo this.`, 'Delete')) call('timeline', 'delete_track', { track_id: tid }); }, 'danger'],
  ]);
});
// snapping
function snapPoints(exceptId) {
  const pts = [0, V.t];
  P.clips.forEach(c => { if (c.id !== exceptId) pts.push(c.start, clipEnd(c)); });
  P.markers.forEach(m => pts.push(m.time));
  return pts;
}
function snap(t, pts) {
  if (!$('#tl-snap').checked) return { t, hit: null };
  let best = null, bd = 9 / T.z;
  for (const p of pts) { const d = Math.abs(p - t); if (d < bd) { bd = d; best = p; } }
  return best == null ? { t, hit: null } : { t: best, hit: best };
}
function showSnap(t) { const s = $('#tl-snapline'); if (t == null) { s.hidden = true; return; } s.hidden = false; s.style.left = t2x(t) + 'px'; }
// clip pointer interactions: select / move / trim
$('#tl-tracks').addEventListener('pointerdown', e => {
  if (e.button !== 0) return;
  const el = e.target.closest('.clip');
  if (!el) { // empty lane: deselect and seek
    select(null);
    const r = tlCanvas.getBoundingClientRect(); seekTo(x2t(e.clientX - r.left)); return;
  }
  const c = P.clips.get(el.dataset.clip); if (!c) return;
  select(c.id);
  const track = P.tracks.find(t => t.id === c._track);
  if (track?.locked) return toast('That track is locked.');
  const mode = e.target.classList.contains('l') ? 'l' : e.target.classList.contains('r') ? 'r' : 'move';
  const x0 = e.clientX, start0 = c.start, end0 = clipEnd(c), in0 = c.in_point || 0, speed = c.speed || 1;
  const pts = snapPoints(c.id);
  const isMedia = clipKind(c) === 'media';
  const srcDur = isMedia ? (P.proj.assets?.[assetOf(c)]?.duration ?? Infinity) : Infinity;
  let moved = false, ns = start0, ne = end0;
  el.setPointerCapture(e.pointerId);
  const mv = ev => {
    const dt = (ev.clientX - x0) / T.z;
    if (!moved && Math.abs(ev.clientX - x0) < 3) return;
    moved = true; el.classList.add('drag');
    if (mode === 'move') {
      let s = Math.max(0, start0 + dt);
      const a = snap(s, pts), b = snap(s + (end0 - start0), pts);
      if (a.hit != null && (b.hit == null || Math.abs(a.t - s) <= Math.abs(b.t - (s + end0 - start0)))) { s = a.t; showSnap(a.hit); }
      else if (b.hit != null) { s = b.t - (end0 - start0); showSnap(b.hit); } else showSnap(null);
      ns = Math.max(0, s); ne = ns + (end0 - start0);
    } else if (mode === 'r') {
      let en = Math.max(start0 + 1 / P.fps, end0 + dt);
      if (isMedia) en = Math.min(en, start0 + (srcDur - in0) / speed);
      const a = snap(en, pts); en = a.t; showSnap(a.hit);
      ns = start0; ne = Math.max(start0 + 1 / P.fps, en);
    } else {
      let st = Math.min(end0 - 1 / P.fps, start0 + dt);
      st = Math.max(st, isMedia ? start0 - in0 / speed : 0, 0);
      const a = snap(st, pts); st = a.t; showSnap(a.hit);
      ns = Math.min(st, end0 - 1 / P.fps); ne = end0;
    }
    el.style.left = t2x(ns) + 'px'; el.style.width = Math.max(4, t2x(ne - ns)) + 'px';
    $('#tl-tc').textContent = mode === 'move' ? tc2(ns) : `${tc2(ns)} → ${tc2(ne)}`;
  };
  const up = async () => {
    el.removeEventListener('pointermove', mv); el.removeEventListener('pointerup', up); el.classList.remove('drag'); showSnap(null);
    if (!moved) return;
    ns = r3(ns); ne = r3(ne);
    if (mode === 'move' && Math.abs(ns - start0) > 1e-4) await call('timeline', 'move_clip', { clip_id: c.id, start: ns });
    else if (mode === 'r') await call('timeline', 'trim_clip', { clip_id: c.id, duration: r3((ne - ns) * speed) });
    else if (mode === 'l' && Math.abs(ns - start0) > 1e-4) {
      const d = ns - start0;
      const args = { clip_id: c.id, duration: r3((ne - ns) * speed) };
      if (isMedia) args.in_point = r3(Math.max(0, in0 + d * speed));
      if (await call('timeline', 'trim_clip', args) != null) await call('timeline', 'move_clip', { clip_id: c.id, start: ns });
    }
    loadProject();
  };
  el.addEventListener('pointermove', mv); el.addEventListener('pointerup', up);
});
$('#tl-tracks').addEventListener('dblclick', e => { const el = e.target.closest('.clip'); if (el) { const c = P.clips.get(el.dataset.clip); if (c) seekTo(c.start + 0.001); } });
$('#tl-tracks').addEventListener('contextmenu', e => {
  const el = e.target.closest('.clip'); if (!el) return; e.preventDefault();
  const c = P.clips.get(el.dataset.clip); select(c.id);
  menu(e, [
    ['scissors', 'Split at playhead', () => splitSel(), 'S'],
    ['copy', 'Duplicate', () => dupSel(), 'Ctrl+D'],
    ['spark', 'Ask the agent about this clip', () => { show('agent'); $('#chat-in').value = `For clip ${c.id} (“${clipLabel(c)}”): `; $('#chat-in').focus(); }],
    null,
    ['trash', 'Delete', () => delSel(false), 'Del'],
    ['trash', 'Ripple delete', () => delSel(true), 'Shift+Del', 'danger'],
  ]);
});
// timeline height drag
$('#tl').addEventListener('pointerdown', e => {
  const r = $('#tl').getBoundingClientRect(); if (e.clientY - r.top > 5) return;
  const h0 = r.height, y0 = e.clientY;
  const mv = ev => $('#editor').style.setProperty('--tlh', clamp(h0 - (ev.clientY - y0), 150, innerHeight - 260) + 'px');
  const up = () => { removeEventListener('pointermove', mv); removeEventListener('pointerup', up); renderTimeline(); };
  addEventListener('pointermove', mv); addEventListener('pointerup', up);
});
// drag media from the bin onto a track (pointer based: works in every webview)
$('#bin-list').addEventListener('pointerdown', e => {
  const a = e.target.closest('.asset'); if (!a || e.target.closest('.add') || e.button !== 0) return;
  const asset = P.assets.find(x => x.id === a.dataset.id); if (!asset) return;
  const x0 = e.clientX, y0 = e.clientY; let ghost = null, over = null, at = 0;
  const mv = ev => {
    if (!ghost) { if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return; ghost = document.createElement('div'); ghost.className = 'clip drag'; ghost.dataset.kind = 'media'; ghost.style.cssText = `position:fixed;height:38px;pointer-events:none;z-index:50;width:${Math.max(60, t2x(asset.duration))}px`; ghost.innerHTML = `<span class="cl">${esc(base(asset.path))}</span>`; document.body.appendChild(ghost); }
    ghost.style.left = ev.clientX - 10 + 'px'; ghost.style.top = ev.clientY - 19 + 'px';
    $$('.track.drop').forEach(t => t.classList.remove('drop'));
    const tr = document.elementFromPoint(ev.clientX, ev.clientY)?.closest('.track');
    over = tr?.dataset.track || (document.elementFromPoint(ev.clientX, ev.clientY)?.closest('#tl') ? 'new' : null);
    if (tr) tr.classList.add('drop');
    if (over) { const r = tlCanvas.getBoundingClientRect(); const s = snap(x2t(ev.clientX - 10 - r.left), snapPoints()); at = s.t; showSnap(s.hit); } else showSnap(null);
  };
  const up = async () => {
    removeEventListener('pointermove', mv); removeEventListener('pointerup', up);
    ghost?.remove(); $$('.track.drop').forEach(t => t.classList.remove('drop')); showSnap(null);
    if (!ghost || !over) return;
    if (over === 'new') { const r = await call('timeline', 'add_track', {}); if (r?.track_id) addAsset(asset.id, r.track_id, at); }
    else addAsset(asset.id, over, at);
  };
  addEventListener('pointermove', mv); addEventListener('pointerup', up);
});

function select(id) {
  if (S.sel === id) return;
  S.sel = id;
  $$('.clip.sel').forEach(c => c.classList.remove('sel'));
  if (id) $(`.clip[data-clip="${CSS.escape(id)}"]`)?.classList.add('sel');
  renderInspector();
}
function clipAtPlayhead() {
  for (const t of displayTracks()) { if (t.locked) continue; const c = (t.clips || []).find(c => c.start <= V.t + 1e-6 && clipEnd(c) > V.t + 1e-6); if (c) return c; }
  return null;
}
async function splitSel() {
  let c = S.sel && P.clips.get(S.sel);
  if (!c || !(c.start < V.t && clipEnd(c) > V.t)) c = clipAtPlayhead();
  if (!c || !(c.start < V.t - 1e-4 && clipEnd(c) > V.t + 1e-4)) return toast('Put the playhead over a clip to split it.');
  await call('timeline', 'split_clip', { clip_id: c.id, at: r3(V.t) });
}
async function delSel(ripple) {
  const c = S.sel && P.clips.get(S.sel); if (!c) return toast('Select a clip first.');
  await call('timeline', ripple ? 'ripple_delete' : 'delete_clip', { clip_id: c.id });
  S.sel = null;
}
async function dupSel() {
  const c = S.sel && P.clips.get(S.sel); if (!c) return toast('Select a clip first.');
  const r = await call('timeline', 'duplicate_clip', { clip_id: c.id, start: r3(clipEnd(c)) });
  if (r?.clip_id) S.sel = r.clip_id;
}
async function addText() {
  const top = displayTracks()[0];
  const at = r3(V.t);
  const free = top && !top.locked && !(top.clips || []).some(c => c.start < at + 3 && clipEnd(c) > at);
  let tid = free ? top.id : (await call('timeline', 'add_track', {}))?.track_id;
  if (!tid) return;
  const r = await call('text', 'add_text', { track_id: tid, text: 'Your title', start: at, duration: 3, size: 96 });
  if (r?.clip_id) { S.sel = r.clip_id; await loadProject(); viewerSoon(); setTimeout(() => $('#insp-body [data-f=text]')?.select(), 60); }
}
$('#ed-split').onclick = splitSel; $('#ed-delete').onclick = () => delSel(false); $('#ed-dup').onclick = dupSel;
$('#ed-text').onclick = addText; $('#ed-track').onclick = () => call('timeline', 'add_track', {});
$('#ed-marker').onclick = () => call('timeline', 'add_marker', { time: r3(V.t), name: `Marker ${P.markers.length + 1}` });
$('#ed-undo').onclick = () => call('project', 'undo', {}); $('#ed-redo').onclick = () => call('project', 'redo', {});

// ================================================================================================================
// INSPECTOR
const EFFECTS = ['blur', 'glow', 'bloom', 'vignette', 'grain', 'sharpen', 'saturation', 'contrast', 'exposure', 'temperature', 'tint', 'sepia', 'teal_orange', 'drop_shadow', 'letterbox', 'pixelate', 'vhs', 'rgb_split', 'chromatic_aberration', 'film_damage', 'light_leak', 'invert', 'mirror', 'shake', 'zoom_blur', 'halftone', 'oil_paint', 'cartoon'];
function num(v, d = 3) { return v == null || !isFinite(v) ? '' : String(Math.round(v * 10 ** d) / 10 ** d); }
const prop = (c, k, def) => c.props?.[k]?.base ?? def;
function renderInspector() {
  const body = $('#insp-body');
  const c = S.sel && P.clips.get(S.sel);
  if (document.activeElement && body.contains(document.activeElement) && document.activeElement.dataset.keep === c?.id) return; // don't clobber typing
  if (!c) {
    $('#insp-title').textContent = 'Project';
    const n = P.clips.size;
    body.innerHTML = `<div class="isec"><div class="h">Canvas</div>
      <label class="ifield"><span>Format</span><select id="pj-preset"><option value="">${P.w}×${P.h}</option><option value="1920x1080">Landscape 1920×1080</option><option value="1080x1920">Vertical 1080×1920</option><option value="1080x1080">Square 1080×1080</option><option value="3840x2160">4K 3840×2160</option><option value="1280x720">720p 1280×720</option></select></label>
      <label class="ifield"><span>Frame rate</span><select id="pj-fps">${[23.976, 24, 25, 29.97, 30, 50, 60].map(f => `<option${Math.abs(f - P.fps) < 0.01 ? ' selected' : ''}>${f}</option>`).join('')}</select></label></div>
      <div class="isec"><div class="h">This edit</div><div class="kv"><span>Length</span><b>${tc(P.dur)}</b><span>Clips</span><b>${n}</b><span>Tracks</span><b>${P.tracks.length}</b><span>Media</span><b>${P.assets.length}</b><span>Markers</span><b>${P.markers.length}</b></div></div>
      <div class="insp-empty">Select a clip on the timeline to edit its timing, look, text, audio and effects.</div>`;
    $('#pj-preset').onchange = e => { const [w, h] = e.target.value.split('x').map(Number); if (w) call('project', 'project_set', { width: w, height: h }); };
    $('#pj-fps').onchange = e => call('project', 'project_set', { fps: +e.target.value });
    return;
  }
  const kind = clipKind(c), media = kind === 'media', text = kind === 'text', s = c.source || {};
  $('#insp-title').innerHTML = `<span class="chip">${esc(kind)}</span> ${esc(clipLabel(c)).slice(0, 40)}`;
  const color = Array.isArray(s.color) ? '#' + s.color.slice(0, 3).map(v => Math.round(clamp(v, 0, 1) * 255).toString(16).padStart(2, '0')).join('') : '#ffffff';
  const fxs = c.effects || [];
  body.innerHTML = `
    ${text ? `<div class="isec"><div class="h">Text</div>
      <label class="ifield"><span>Text</span><textarea data-f="text" data-keep="${esc(c.id)}">${esc(s.text || '')}</textarea></label>
      <label class="ifield"><span>Size</span><input type="number" data-f="size" min="4" step="1" value="${num(s.size, 0)}"></label>
      <label class="ifield"><span>Colour</span><input type="color" data-f="color" value="${color}"></label></div>` : ''}
    <div class="isec"><div class="h">Timing</div>
      <label class="ifield"><span>Start</span><input type="number" data-f="start" step="0.01" min="0" value="${num(c.start)}"></label>
      <label class="ifield"><span>Duration</span><input type="number" data-f="duration" step="0.01" min="0.04" value="${num(c.duration)}"></label>
      ${media ? `<label class="ifield"><span>In point</span><input type="number" data-f="in_point" step="0.01" min="0" value="${num(c.in_point || 0)}"></label>` : ''}
      <label class="ifield"><span>Speed</span><input type="number" data-f="speed" step="0.05" min="0.1" max="100" value="${num(c.speed || 1)}"></label></div>
    <div class="isec"><div class="h">Look</div>
      <label class="ifield"><span>Opacity</span><span class="pair"><input type="range" data-f="opacity" min="0" max="1" step="0.01" value="${prop(c, 'opacity', 1)}"><output>${Math.round(prop(c, 'opacity', 1) * 100)}%</output></span></label>
      <label class="ifield"><span>Scale</span><span class="pair"><input type="range" data-f="scale" min="0.1" max="4" step="0.01" value="${prop(c, 'scale', 1)}"><output>${Math.round(prop(c, 'scale', 1) * 100)}%</output></span></label>
      <label class="ifield"><span>Position X</span><input type="number" data-f="x" step="1" value="${num(prop(c, 'x', 0), 1)}"></label>
      <label class="ifield"><span>Position Y</span><input type="number" data-f="y" step="1" value="${num(prop(c, 'y', 0), 1)}"></label>
      <label class="ifield"><span>Rotation</span><input type="number" data-f="rotation" step="1" value="${num(prop(c, 'rotation', 0), 1)}"></label></div>
    ${media ? `<div class="isec"><div class="h">Audio</div>
      <label class="ifield"><span>Volume</span><span class="pair"><input type="range" data-f="volume" min="0" max="2" step="0.01" value="${c.audio?.volume?.base ?? 1}"><output>${Math.round((c.audio?.volume?.base ?? 1) * 100)}%</output></span></label>
      <label class="ifield"><span>Fade in</span><input type="number" data-f="fade_in" step="0.1" min="0" value="${num(c.audio?.fade_in || 0, 2)}"></label>
      <label class="ifield"><span>Fade out</span><input type="number" data-f="fade_out" step="0.1" min="0" value="${num(c.audio?.fade_out || 0, 2)}"></label>
      <label class="ifield"><span>Mute</span><span><input type="checkbox" data-f="mute"${c.audio?.mute ? ' checked' : ''}></span></label></div>` : ''}
    <div class="isec"><div class="h">Effects</div>
      <div class="ichips">${fxs.map(f => `<span class="ichip">${esc(f.kind || f.type || 'effect')}<button data-rmfx="${esc(f.id)}" title="Remove">${ICON('x')}</button></span>`).join('') || '<span class="note muted">None</span>'}</div>
      <label class="ifield" style="margin-top:8px"><span>Add effect</span><select data-f="addfx"><option value="">Choose…</option>${EFFECTS.map(k => `<option>${k}</option>`).join('')}</select></label>
      <label class="ifield"><span>Transition in</span><select data-f="trans"><option value="">${c.transition_in ? esc(c.transition_in.kind || 'set') : 'None'}</option>${S.transitions.map(k => `<option>${esc(k)}</option>`).join('')}</select></label></div>
    <div class="isec"><div class="h">Ask the agent</div>
      <form class="insp-ask" id="insp-ask"><input type="text" placeholder="e.g. make this clip black and white" data-keep="${esc(c.id)}"><button class="btn sm acc" type="submit">${ICON('send')}</button></form></div>
    <div class="row" style="gap:6px;flex-wrap:wrap"><button class="btn sm" id="i-split">${ICON('scissors')}Split</button><button class="btn sm" id="i-dup">${ICON('copy')}Duplicate</button><button class="btn sm ghost" id="i-del" style="color:var(--bad)">${ICON('trash')}Delete</button></div>`;
  const id = c.id;
  const set = async (f, v) => {
    switch (f) {
      case 'start': return call('timeline', 'move_clip', { clip_id: id, start: Math.max(0, v) });
      case 'duration': return call('timeline', 'trim_clip', { clip_id: id, duration: v });
      case 'in_point': return call('timeline', 'trim_clip', { clip_id: id, in_point: Math.max(0, v) });
      case 'speed': return call('timeline', 'set_speed', { clip_id: id, speed: clamp(v, 0.1, 100) });
      case 'opacity': case 'scale': case 'x': case 'y': case 'rotation': return call('animate', 'set_property', { clip_id: id, property: f, value: v });
      case 'volume': return call('audio', 'set_audio', { clip_id: id, volume: v });
      case 'fade_in': case 'fade_out': return call('audio', 'set_audio', { clip_id: id, [f]: v });
      case 'text': return call('text', 'set_text', { clip_id: id, text: v });
      case 'size': return call('text', 'set_text', { clip_id: id, size: v });
      case 'color': return call('text', 'set_text', { clip_id: id, color: v });
    }
  };
  body.querySelectorAll('[data-f]').forEach(inp => {
    const f = inp.dataset.f;
    if (f === 'addfx') { inp.onchange = () => inp.value && call('effects', 'add_effect', { clip_id: id, kind: inp.value }); return; }
    if (f === 'trans') { inp.onchange = () => inp.value && call('effects', 'add_transition', { clip_id: id, kind: inp.value, duration: 0.5 }); return; }
    if (f === 'mute') { inp.onchange = () => call('audio', 'set_audio', { clip_id: id, mute: inp.checked }); return; }
    if (inp.type === 'range') {
      const out = inp.parentElement.querySelector('output');
      inp.oninput = () => { if (out) out.textContent = Math.round(inp.value * 100) + '%'; };
      inp.onchange = () => set(f, +inp.value);
      return;
    }
    if (f === 'color') { inp.onchange = () => set(f, inp.value); return; }
    if (f === 'text') { inp.onkeydown = e => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) inp.blur(); }; inp.onchange = () => set(f, inp.value); return; }
    inp.onkeydown = e => { if (e.key === 'Enter') inp.blur(); };
    inp.onchange = () => { const v = +inp.value; if (isFinite(v)) set(f, v); };
  });
  body.querySelectorAll('[data-rmfx]').forEach(b => b.onclick = () => call('effects', 'remove_effect', { clip_id: id, effect_id: b.dataset.rmfx }));
  $('#i-split').onclick = splitSel; $('#i-dup').onclick = dupSel; $('#i-del').onclick = () => delSel(false);
  $('#insp-ask').onsubmit = e => { e.preventDefault(); const v = e.target.querySelector('input').value.trim(); if (v) { sendAgent(`For clip ${id} (“${clipLabel(c)}”): ${v}`); e.target.querySelector('input').value = ''; } };
}

// ================================================================================================================
// VIEWER (frames and drafts come from the engine through the app, cached per project revision)
const V = { t: 0, dur: 0, fps: 30, rev: null, before: null, draft: null, frames: new Map(), seq: 0, timer: null, raf: 0 };
const vw = $('#viewer'), scrub = $('#vw-scrub'), video = $('#vw-video');
const fidx = t => Math.round(Math.max(0, t) * V.fps);
// real-time playback (playback.js): the engine renders ahead of the playhead, Web Audio keeps the sound in sync
const PB = new KimoPlayer({
  invoke: (c, a) => B.invoke(c, a), canvas: $('#vw-canvas'),
  fps: () => V.fps, duration: () => V.dur, scale: () => ((V.w || 1920) > 2000 ? 0.25 : 0.5),
  onTime: t => { V.t = t; scrub.value = String(t); vTime(); },
  onState: (st, i) => {
    if (st === 'playing' || st === 'buffering') {
      vw.dataset.playing = '1'; vw.dataset.buffering = st === 'buffering' ? '1' : '0'; setView('after'); vMedia('stream'); vState('ready');
      const sp = PB.rate === 1 ? '' : ` · ${PB.rate > 0 ? '' : '−'}${Math.abs(PB.rate)}×`;
      $('#vw-msg').textContent = st === 'buffering' ? `Buffering${sp}…` : `Playing${sp} · ${i.fps || '…'} fps · draft ${i.size}${i.skip > 1 ? ` · every ${['', '', '2nd', '3rd', '4th'][i.skip] || i.skip + 'th'} frame` : ''}${i.audio ? ' · sound' : ''}${i.proxies ? ' · proxies' : ''}${PB.loop.on ? ' · looping' : ''}`;
    } else if (st === 'paused') { vw.dataset.playing = '0'; vw.dataset.buffering = '0'; playing = false; $('#vw-msg').textContent = `Paused · draft ${i.size}`; }
    else if (st === 'scrub' && !V.frames.has(`${V.rev}:${fidx(V.t)}`) && vw.dataset.state !== 'rendering') { vMedia('stream'); $('#vw-msg').textContent = i.cached ? 'Draft frame (buffered)' : `Draft frame in ${i.ms} ms`; }
  },
  onError: m => { $('#vw-msg').textContent = m; },
});
function vState(s, busy) { vw.dataset.state = s; if (busy) $('#vw-busy-t').textContent = busy; }
function vMedia(m) {
  vw.dataset.media = m;
  $('#vw-img').hidden = m !== 'image' || vw.dataset.view === 'before' || !$('#vw-img').getAttribute('src');
  video.hidden = m !== 'video' || vw.dataset.view === 'before';
  $('#vw-canvas').hidden = m !== 'stream' || vw.dataset.view === 'before';
}
function vTime() {
  $('#vw-time').textContent = V.dur ? `${tc(V.t)} / ${tc(V.dur)}` : tc(V.t);
  scrub.style.setProperty('--p', V.dur ? `${(100 * V.t / V.dur).toFixed(2)}%` : '0%');
  placePlayhead();
}
function inDraft(t) { const d = V.draft; return d && d.rev === V.rev && t >= d.start && t <= d.end - 1 / V.fps; }
function dropDraft() { if (!PB.playing && !PB.buffering) stopPlay(); V.draft = null; video.removeAttribute('src'); video.load(); vw.dataset.playing = '0'; vMedia('image'); }
function setView(v) {
  if (v === 'before' && !V.before) v = 'after';
  vw.dataset.view = v;
  $$('#vw-ab button').forEach(b => b.classList.toggle('on', b.dataset.v === v));
  if (v === 'before') { stopPlay(); $('#vw-before').src = V.before.url; }
  $('#vw-before').hidden = v !== 'before';
  const tag = $('#vw-tag'); tag.hidden = v !== 'before';
  if (v === 'before') tag.textContent = `Before · ${tc(V.before.time)}`;
  vMedia(vw.dataset.media);
}
function applyShown(s) {
  V.dur = s.duration; V.fps = s.fps || 30; V.rev = s.rev; V.w = s.width;
  scrub.max = String(Math.max(0, s.duration)); scrub.step = String(1 / V.fps);
  scrub.disabled = $('#vw-play').disabled = s.empty;
  V.before = s.before || null;
  $('#vw-ab button[data-v=before]').disabled = !V.before;
  if (V.draft && V.draft.rev !== s.rev) dropDraft();
  if (s.empty) {
    $('#vw-img').removeAttribute('src');
    V.t = 0; scrub.value = '0'; vTime(); dropDraft(); setView('after'); $('#vw-rev').textContent = 'empty';
    $('#vw-msg').textContent = 'The project has no clips yet.';
    return vState('empty');
  }
  const f = s.frame;
  V.frames.set(`${f.rev}:${f.index}`, f);
  if (V.frames.size > 300) V.frames.delete(V.frames.keys().next().value);
  $('#vw-img').src = f.url;
  if (PB.playing || PB.buffering) return vState('ready'); // real-time playback keeps the screen; the frame stays cached for later
  vMedia(inDraft(V.t) && !V.draft.frame_urls?.length ? 'video' : 'image');
  $('#vw-rev').textContent = `${s.width}×${s.height}`;
  $('#vw-msg').textContent = (f.cached ? 'From cache' : `Rendered in ${f.ms} ms`) + (s.changed ? ' · updated after an edit' : '');
  if (s.changed && vw.dataset.view === 'before') setView('after'); else setView(vw.dataset.view);
  vState('ready');
}
async function viewerRefresh() {
  if (S.app?.engine?.state !== 'running') return;
  const my = ++V.seq;
  vState(vw.dataset.state === 'empty' ? 'empty' : 'rendering', 'Rendering frame…');
  try { const s = await B.invoke('viewer_frame', { time: V.t }); if (my === V.seq) applyShown(s); }
  catch (e) { if (my !== V.seq || String(e) === 'superseded') return; $('#vw-err').textContent = String(e); vState('error'); }
}
function viewerSoon() { clearTimeout(V.timer); V.timer = setTimeout(viewerRefresh, 300); }
scrub.oninput = () => {
  V.t = +scrub.value; vTime();
  if (vw.dataset.view === 'before') setView('after');
  if (inDraft(V.t)) {
    stopPlay();
    if (V.draft.frame_urls?.length) { $('#vw-img').src = seqFrame(V.t); vMedia('image'); } else { video.currentTime = V.t - V.draft.start; vMedia('video'); }
    return;
  }
  const hit = V.frames.get(`${V.rev}:${fidx(V.t)}`);
  if (hit) { $('#vw-img').src = hit.url; vMedia('image'); } else PB.scrub(V.t);
};
scrub.onchange = () => { V.t = +scrub.value; vTime(); if (!inDraft(V.t) && !V.frames.has(`${V.rev}:${fidx(V.t)}`)) viewerRefresh(); };
function seqFrame(t) { const d = V.draft; return d.frame_urls[Math.min(d.frame_urls.length - 1, Math.max(0, Math.round((t - d.start) * (d.fps || V.fps))))]; }
function stopPlay() { cancelAnimationFrame(V.raf); V.raf = 0; if (!video.paused) video.pause(); if (PB.playing || PB.buffering) PB.pause(); vw.dataset.playing = '0'; }
function playSeq() {
  const d = V.draft, t0 = performance.now(), from = V.t;
  vw.dataset.playing = '1'; vMedia('image');
  const tick = now => {
    const t = from + (now - t0) / 1000;
    if (t >= d.end - 1 / V.fps || !V.draft) {
      V.t = Math.min(V.dur, d.end - 1 / V.fps); scrub.value = String(V.t); vTime(); stopPlay();
      if (V.t < V.dur - 2 / V.fps && playing) play(); // keep going: next draft chunk
      return;
    }
    V.t = t; scrub.value = String(t); vTime();
    $('#vw-img').src = seqFrame(t);
    V.raf = requestAnimationFrame(tick);
  };
  V.raf = requestAnimationFrame(tick);
}
let playing = false;
async function play(rate = 1) {
  if (rate > 0 && V.t >= V.dur - 1 / V.fps) V.t = PB.loop.on ? (PB.loop.in ?? 0) : 0;
  playing = true;
  setView('after');
  // real-time playback with sound (the engine's preview stream); the draft clips below are the fallback
  if (PB.supported !== false) {
    vw.dataset.playing = '1';
    if (await PB.play(V.t, rate)) return;
    if (!playing) return;
    vw.dataset.playing = '0';
    if (rate !== 1) { playing = false; return; }
  }
  if (!inDraft(V.t)) {
    const my = ++V.seq;
    vState('rendering', 'Rendering preview…');
    try {
      const d = await B.invoke('viewer_draft', { start: V.t, seconds: 4 });
      if (my !== V.seq || !playing) return;
      V.draft = d;
      if (d.frame_urls?.length) await Promise.all(d.frame_urls.map(u => { const i = new Image(); i.src = u; return i.decode().catch(() => {}); }));
      else video.src = d.url;
    } catch (e) { if (my === V.seq) { $('#vw-err').textContent = String(e); vState('error'); } playing = false; return; }
  }
  vState('ready');
  if (V.draft.frame_urls?.length) return playSeq();
  vMedia('video');
  try { video.currentTime = V.t - V.draft.start; await video.play(); vw.dataset.playing = '1'; }
  catch (e) { vw.dataset.playing = '0'; playing = false; $('#vw-msg').textContent = "This computer can't play the preview here. Frames still work."; vMedia('image'); }
}
function togglePlay() { if (vw.dataset.playing === '1' || playing && vw.dataset.state === 'rendering') { playing = false; stopPlay(); } else play(); }
video.ontimeupdate = () => { if (vw.dataset.playing === '1' && V.draft) { V.t = V.draft.start + video.currentTime; scrub.value = V.t; vTime(); } };
video.onended = () => { vw.dataset.playing = '0'; if (playing && V.t < V.dur - 2 / V.fps) play(); };
$('#vw-play').onclick = togglePlay;

// ---------- transport: J / K / L shuttle, frame step, loop region, full screen ----------
function shuttle(key) {
  const r = KimoAV.shuttle(PB.playing || PB.buffering ? PB.rate : 0, key);
  if (!r) { playing = false; return stopPlay(); }
  playing = true; if (PB.playing || PB.buffering) PB.setRate(r); else play(r);
}
function stepFrame(n) { if (playing || PB.playing) { playing = false; stopPlay(); } seekTo(V.t + n / V.fps); }
function loopLabel() {
  const L = PB.loop, b = $('#vw-loop');
  b.classList.toggle('on', L.on); b.setAttribute('aria-pressed', L.on ? 'true' : 'false');
  const a = L.in ?? 0, z = L.out ?? V.dur;
  b.title = `Loop ${L.in != null || L.out != null ? `${tc(a)}–${tc(z)}` : 'the whole timeline'} (Ctrl+L) · I / O set in and out, Ctrl+Shift+X clears`;
  scrub.style.setProperty('--li', V.dur ? `${(100 * a / V.dur).toFixed(2)}%` : '0%');
  scrub.style.setProperty('--lo', V.dur ? `${(100 * z / V.dur).toFixed(2)}%` : '100%');
  vw.dataset.loop = L.on ? '1' : '0';
}
function setLoop(k) {
  const L = PB.loop;
  if (k === 'toggle') L.on = !L.on;
  else if (k === 'in') { L.in = V.t; if (L.out != null && L.out <= L.in) L.out = null; L.on = true; }
  else if (k === 'out') { L.out = V.t; if (L.in != null && L.in >= L.out) L.in = null; L.on = true; }
  else if (k === 'clear') { L.in = L.out = null; L.on = false; }
  loopLabel(); toast(L.on ? `Loop on · ${tc(L.in ?? 0)}–${tc(L.out ?? V.dur)}` : 'Loop off');
}
function fullScreen() {
  const el = $('#viewer .vw-screen');
  if (document.fullscreenElement) document.exitFullscreen?.(); else el.requestFullscreen?.().catch(e => toast(String(e), 'bad'));
}
// Space is the play key everywhere in Edit; a focused viewer button must not also click itself on key-up (play, then pause at once)
$$('.vw-ctl button').forEach(b => b.addEventListener('keyup', e => { if (e.key === ' ') e.preventDefault(); }));
$('#vw-loop').onclick = () => setLoop('toggle');
$('#vw-full').onclick = fullScreen;
$('#viewer .vw-screen').addEventListener('dblclick', fullScreen);
loopLabel();
$$('#vw-ab button').forEach(b => b.onclick = () => setView(b.dataset.v));
$('#vw-retry').onclick = () => viewerRefresh();

// ================================================================================================================
// CHANGES (every undoable edit, whoever made it)
function renderChanges(flash) {
  const list = [...S.changes].reverse();
  $('#chg-count').textContent = S.changes.filter(c => !c.undone).length;
  $('#changes').innerHTML = list.length ? list.map(c => `<div class="ccard${c.undone ? ' undone' : ''}${c.id === flash ? ' flash' : ''}">
      <div class="row" style="gap:8px">${actorChip(c.actor)}<span class="lbl">${esc(c.action || c.tool)}</span><span class="t" style="margin-left:auto">${clock(c.t)}</span></div>
      <div class="note mono" style="margin-top:7px">${esc(c.summary)}</div>
      ${!c.undone && c.findings?.length ? `<div class="finds">${c.findings.map((f, i) => `<div class="find ${esc(f.severity)}"><span class="sv">${esc(f.severity)}</span><p class="msg">${esc(f.message)}</p>${f.fix?.tool ? `<button class="btn sm" data-fix="${c.id}:${i}" title="${esc(f.fix.tool)}">Fix</button>` : ''}</div>`).join('')}</div>` : ''}
      <div class="row" style="margin-top:8px">${c.undone ? '<span class="note muted">Undone</span>' : `<button class="btn sm" data-undo="${c.id}">Undo</button>`}${c.steps > 1 ? `<span class="note muted">${c.steps} steps</span>` : ''}</div></div>`).join('')
    : '<div class="empty sm card-ish"><svg class="km e" viewBox="0 0 1024 1024" aria-hidden="true"><use href="#km-sm"/></svg><div class="et">No changes yet</div><div class="ed">Every edit — yours or an agent\'s — lands here with its own Undo. Undo uses the engine\'s history: undoing one change also undoes the ones after it.</div></div>';
  $$('[data-fix]').forEach(b => b.onclick = async () => {
    const [cid, i] = b.dataset.fix.split(':'); const f = S.changes.find(x => String(x.id) === cid)?.findings?.[+i];
    if (!f?.fix?.tool) return;
    b.disabled = true;
    try { const r = await B.invoke('engine_call', { tool: f.fix.tool, args: f.fix.args || {} }); if (r.ok) toast('Fixed: ' + f.code.replace(/_/g, ' ')); else { toast(r.text || 'The fix did not apply', 'bad'); b.disabled = false; } }
    catch (e) { toast(String(e), 'bad'); b.disabled = false; }
  });
  $$('[data-undo]').forEach(b => b.onclick = async () => {
    b.disabled = true;
    try { const r = await B.invoke('change_undo', { id: +b.dataset.undo }); toast(`Undid ${r.steps} step${r.steps > 1 ? 's' : ''}`); } catch (e) { toast(String(e), 'bad'); b.disabled = false; }
  });
}

// ================================================================================================================
// AGENT
function agentInfo(id) { return S.cli.find(c => c.id === id); }
function agentReady(id) { if (id === 'api') return !!S.app?.keys?.find(k => k.provider === S.app.settings.provider && k.set); const a = agentInfo(id); return !!(a?.installed && a.signed_in !== false); }
function stClass(id) { if (id === 'api') return agentReady('api') ? 'ok' : 'bad'; const a = agentInfo(id); if (!a) return ''; if (!a.installed || a.signed_in === false) return 'bad'; return a.signed_in == null ? 'unk' : 'ok'; }
function currentAgent() { return S.app?.settings?.agent || 'claude-code'; }
// ---------- model + effort, per agent (lists come from each CLI: agy models, codex debug models, Claude's aliases) ----------
S.models = {};
async function loadModels(id) {
  if (!S.models[id]) { try { S.models[id] = await B.invoke('cli_models', { id }); } catch (e) { return { models: [], efforts: [], source: String(e) }; } }
  return S.models[id];
}
const modelOf = id => S.app?.settings?.agent_models?.[id] || '';
const effortOf = id => S.app?.settings?.agent_efforts?.[id] || '';
function modelLabel(id) {
  if (id === 'api') { const p = S.app?.settings?.provider; return S.app?.settings?.models?.[p] || S.app?.providers?.find(x => x.id === p)?.default_model || ''; }
  const m = modelOf(id); if (!m) return '';
  return S.models[id]?.models?.find(o => o.id === m)?.label?.replace(/ \(newest\)$/, '') || m;
}
async function saveModel(id, model, effort) {
  await B.invoke('agent_select', { agent: id, model, effort, select: false });
  S.app.settings.agent_models ??= {}; S.app.settings.agent_efforts ??= {};
  if (model != null) S.app.settings.agent_models[id] = model;
  if (effort != null) S.app.settings.agent_efforts[id] = effort;
  renderAgentPill();
}
let modelSeq = 0;
async function renderModelBar() {
  const id = currentAgent(), bar = $('#model-bar'), my = ++modelSeq;
  if (id === 'api') {
    bar.innerHTML = `<span class="mb"><span>Model</span><b style="color:var(--ink)">${esc(modelLabel('api') || 'not set')}</b></span><a class="lnk" style="margin-left:0" data-go="connections">Change in Connections → Use an API key</a>`;
    return;
  }
  if (!S.models[id]) bar.innerHTML = `<span class="note muted">Asking ${esc(actorName(id))} which models your plan has…</span>`;
  const m = await loadModels(id);
  if (my !== modelSeq || currentAgent() !== id) return;
  const cur = modelOf(id), ef = effortOf(id);
  const known = !cur || m.models.some(o => o.id === cur);
  const sel = m.models.find(o => o.id === cur);
  const efforts = sel?.efforts?.length ? sel.efforts : m.efforts;
  bar.innerHTML = `<label class="mb"><span>Model</span><select id="model-sel">
      <option value="">Default (${esc(actorName(id))} picks)</option>
      ${m.models.map(o => `<option value="${esc(o.id)}"${o.id === cur ? ' selected' : ''}>${esc(o.label)}</option>`).join('')}
      <option value="__other"${known ? '' : ' selected'}>Other model…</option></select></label>
    <input id="model-other" type="text" placeholder="Type a model name" value="${known ? '' : esc(cur)}"${known ? ' hidden' : ''} spellcheck="false">
    <label class="mb"><span>Effort</span><select id="effort-sel"><option value="">Default${sel?.default_effort ? ` (${esc(sel.default_effort)})` : ''}</option>
      ${efforts.map(e => `<option value="${esc(e)}"${e === ef ? ' selected' : ''}>${esc(e[0].toUpperCase() + e.slice(1))}</option>`).join('')}</select></label>
    <span class="note muted" id="model-src" title="${esc(m.source)}">${esc(m.source)}</span>`;
  const other = $('#model-other');
  $('#model-sel').onchange = async e => {
    const v = e.target.value;
    if (v === '__other') { other.hidden = false; other.focus(); return; }
    other.hidden = true;
    const keepEffort = !v || (m.models.find(o => o.id === v)?.efforts?.length ? m.models.find(o => o.id === v).efforts : m.efforts).includes(effortOf(id));
    await saveModel(id, v, keepEffort ? null : '');
    toast(v ? `${actorName(id)} will use ${modelLabel(id)}` : `${actorName(id)} will pick its own model`);
    renderModelBar();
  };
  other.onkeydown = e => { if (e.key === 'Enter') other.blur(); };
  other.onchange = async () => { const v = other.value.trim(); await saveModel(id, v, null); if (v) toast(`${actorName(id)} will use ${v}`); };
  $('#effort-sel').onchange = async e => { await saveModel(id, null, e.target.value); toast(e.target.value ? `Effort: ${e.target.value}` : 'Effort: default'); };
}

function renderAgentPick() {
  const cur = currentAgent();
  $('#agent-pick').innerHTML = [...AGENTS, 'api'].map(id => `<button class="apick${id === cur ? ' on' : ''}" data-agent="${id}" role="radio" aria-checked="${id === cur}" title="${esc(id === 'api' ? 'Your own API key' : (agentInfo(id)?.sign_in_note || ''))}">
    <span class="alogo ${id}">${id === 'claude-code' ? 'C' : id === 'codex' ? '◎' : id === 'antigravity' ? 'A' : ICON('key')}</span>${esc(actorName(id))}<span class="st ${stClass(id)}"></span></button>`).join('');
  $$('.apick').forEach(b => b.onclick = () => pickAgent(b.dataset.agent));
  renderAgentPill();
  renderModelBar();
}
async function pickAgent(id) {
  await B.invoke('agent_select', { agent: id });
  S.app.settings.agent = id;
  renderAgentPick(); renderCliCards();
  if (!agentReady(id)) { toast(id === 'api' ? 'Add an API key in Connections first.' : `${actorName(id)} needs setting up — see Connections.`); }
}
function renderAgentPill() {
  const id = currentAgent(), st = S.busy ? 'busy' : stClass(id);
  $('#pill-agent').className = 'rail-pill ' + st; $('#pill-agent-t').textContent = actorName(id); $('#pill-agent').title = `Agent: ${actorName(id)}`;
  const ml = modelLabel(id);
  const ask = $('#ask-agent'); ask.className = 'ask-agent ' + st; $('#ask-agent-t').textContent = actorName(id) + (ml ? ' · ' + ml : '');
  ask.title = `${actorName(id)}${ml ? ' using ' + ml : ''}${effortOf(id) ? ', ' + effortOf(id) + ' effort' : ''} · change in the Agent tab`;
  $('#chat-in').placeholder = `Tell ${actorName(id)} what to change…`;
}
$('#ask-agent').onclick = () => show('agent');
function addMsg(cls, html) { const d = document.createElement('div'); d.className = 'msg ' + cls; d.innerHTML = html; $('#msgs').appendChild(d); scrollMsgs(); return d; }
function scrollMsgs() { const m = $('#msgs'); if (m.scrollHeight - m.scrollTop - m.clientHeight < 160) m.scrollTop = 1e9; }
const TURN = { el: null, blocks: null, steps: new Map(), status: null, started: 0, agent: null };
function setBusy(b) {
  S.busy = b;
  $('#chat-stop').hidden = !b; $('#chat-send').disabled = b; $('#ask-send').disabled = b;
  $('#nav-agent').hidden = !b || S.tab === 'agent';
  renderAgentPill();
}
async function sendAgent(text) {
  text = String(text || '').trim();
  if (!text || S.busy) return;
  const agent = currentAgent();
  if (!agentReady(agent) && agent !== 'api') {
    const a = agentInfo(agent);
    if (a && !a.installed) { show('connections'); return toast(`${actorName(agent)} is not installed yet.`, 'bad'); }
  }
  $('#chat-hello')?.remove();
  addMsg('user', esc(text));
  const turn = ++S.turn;
  TURN.el = addMsg('agent', `<div class="who"><span class="alogo ${agent}" style="width:18px;height:18px;border-radius:5px;font-size:9px">${agent === 'claude-code' ? 'C' : agent === 'codex' ? '◎' : agent === 'antigravity' ? 'A' : '·'}</span>${esc(actorName(agent))}<span class="mono" id="turn-clock"></span></div><div class="blocks"></div>`);
  TURN.blocks = TURN.el.querySelector('.blocks'); TURN.steps = new Map(); TURN.started = performance.now(); TURN.agent = agent; TURN.status = null; TURN.text = '';
  setStatus('Starting…');
  setBusy(true);
  askStatus(`${actorName(agent)} is working…`);
  try {
    const r = await B.invoke('agent_send', { text, turn });
    if (turn !== S.turn) return;
    finishTurn(r?.reply);
  } catch (e) {
    if (turn !== S.turn) return;
    failTurn(String(e));
  } finally { if (turn === S.turn) setBusy(false); }
}
function lastBlock(cls) { const b = TURN.blocks?.lastElementChild; return b && b.classList.contains(cls) ? b : null; }
function setStatus(t) {
  if (!TURN.el) return;
  if (!t) { TURN.status?.remove(); TURN.status = null; return; }
  if (!TURN.status) { TURN.status = addMsg('status', '<span class="spin"></span><span></span>'); }
  TURN.status.lastElementChild.textContent = t;
  $('#msgs').appendChild(TURN.status); scrollMsgs();
}
function askStatus(t) { let el = $('#ask-status'); if (!el) { el = document.createElement('div'); el.id = 'ask-status'; el.className = 'ask-status'; $('#askbar').after(el); } el.textContent = t || ''; }
function onAgentEvent(m) {
  if (m.turn !== S.turn || !TURN.el) return;
  const ev = m.ev;
  switch (ev.kind) {
    case 'delta': {
      let b = lastBlock('body'); if (!b) { b = document.createElement('div'); b.className = 'body'; TURN.blocks.appendChild(b); }
      b.textContent += ev.text; setStatus(null); break;
    }
    case 'text': {
      let b = lastBlock('body'); if (!b) { b = document.createElement('div'); b.className = 'body'; TURN.blocks.appendChild(b); }
      b.textContent = ev.text; break;
    }
    case 'thinking': {
      let b = lastBlock('thinking'); if (!b) { b = document.createElement('div'); b.className = 'thinking'; TURN.blocks.appendChild(b); }
      b.textContent = (b.textContent + ev.text).slice(-600); setStatus('Thinking…'); break;
    }
    case 'tool_start': {
      if (TURN.steps.has(ev.id)) break;
      let wrap = lastBlock('steps'); if (!wrap) { wrap = document.createElement('div'); wrap.className = 'steps'; TURN.blocks.appendChild(wrap); }
      const d = document.createElement('details'); d.className = 'stp run';
      const input = ev.input || {};
      const action = input.action ? input.action : '';
      const argsTxt = JSON.stringify(input.args ?? (action ? {} : input));
      d.innerHTML = `<summary><span class="s"></span><span class="tn">${esc(ev.tool)}${action ? '.' + esc(action) : ''}</span><span class="ta">${esc(argsTxt === '{}' ? '' : argsTxt)}</span><span class="ms"></span></summary><pre>${esc(JSON.stringify(input, null, 2))}</pre>`;
      d._t = performance.now();
      wrap.appendChild(d); TURN.steps.set(ev.id, d);
      setStatus(`${ev.tool}${action ? '.' + action : ''}…`);
      askStatus(`${actorName(TURN.agent)}: ${ev.tool}${action ? '.' + action : ''}`);
      scrollMsgs(); break;
    }
    case 'tool_end': {
      const d = TURN.steps.get(ev.id); if (!d) break;
      d.className = 'stp ' + (ev.ok ? 'ok' : 'err');
      d.querySelector('.s').innerHTML = ICON(ev.ok ? 'check' : 'x');
      d.querySelector('.ms').textContent = Math.round(performance.now() - d._t) + ' ms';
      if (ev.output) d.querySelector('pre').textContent += '\n\n→ ' + ev.output;
      setStatus('Working…'); break;
    }
    case 'status': setStatus(ev.text); break;
    case 'done': {
      if (ev.text && !TURN.blocks.querySelector('.body')) { const b = document.createElement('div'); b.className = 'body'; b.textContent = ev.text; TURN.blocks.appendChild(b); }
      TURN.meta = [ev.turns ? `${ev.turns} turn${ev.turns > 1 ? 's' : ''}` : '', ev.cost_usd ? `≈$${ev.cost_usd.toFixed(3)} of your plan's usage` : ''].filter(Boolean).join(' · ');
      break;
    }
    case 'error': TURN.err = ev.message; break;
  }
}
function finishTurn(reply) {
  setStatus(null);
  if (reply && !TURN.blocks.querySelector('.body')) { const b = document.createElement('div'); b.className = 'body'; b.textContent = reply; TURN.blocks.appendChild(b); }
  TURN.blocks.querySelectorAll('.body:empty').forEach(b => b.remove());
  const secs = ((performance.now() - TURN.started) / 1000).toFixed(1);
  addMsg('done-meta', esc(`${secs}s${TURN.meta ? ' · ' + TURN.meta : ''}`));
  const last = [...TURN.blocks.querySelectorAll('.body')].pop()?.textContent || 'Done.';
  askStatus(`${actorName(TURN.agent)}: ${last.slice(0, 160)}`);
  if (S.tab !== 'agent') toast(`${actorName(TURN.agent)}: ${last.slice(0, 90)}`);
  TURN.el = null;
}
function failTurn(msg) {
  setStatus(null);
  TURN.blocks?.querySelectorAll('.body:empty').forEach(b => b.remove());
  TURN.steps.forEach(d => { if (d.classList.contains('run')) { d.className = 'stp err'; d.querySelector('.s').innerHTML = ICON('x'); } });
  const signin = /sign|log ?in|not installed|install/i.test(msg);
  const e = addMsg('err', `${esc(msg)}${signin ? '<div class="acts"><button class="btn sm" data-go="connections">Open Connections</button></div>' : ''}`);
  void e;
  askStatus(`${actorName(TURN.agent)}: ${msg.slice(0, 160)}`);
  if (S.tab !== 'agent') toast(msg.slice(0, 120), 'bad');
  TURN.el = null;
}
$('#chat-send').onclick = () => { const t = $('#chat-in').value; $('#chat-in').value = ''; autoGrow(); sendAgent(t); };
$('#chat-stop').onclick = () => { B.invoke('agent_stop'); setStatus('Stopping…'); };
const autoGrow = () => { const t = $('#chat-in'); t.style.height = 'auto'; t.style.height = Math.min(160, t.scrollHeight) + 'px'; };
$('#chat-in').addEventListener('input', autoGrow);
$('#chat-in').addEventListener('keydown', e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('#chat-send').click(); } });
$('#sugg').addEventListener('click', e => { const b = e.target.closest('button'); if (b) sendAgent(b.textContent); });
$('#askbar').onsubmit = e => { e.preventDefault(); const v = $('#ask-in').value; $('#ask-in').value = ''; sendAgent(v); };
$('#agent-new').onclick = async () => {
  if (S.busy) await B.invoke('agent_stop');
  await B.invoke('agent_reset'); S.turn++;
  $('#msgs').innerHTML = '<div class="msg agent hello" id="chat-hello"><b>New conversation.</b> Your project stays as it is; the agent starts fresh.</div>';
};

// ================================================================================================================
// LIVE (every engine call through the app, tagged with who made it)
const LIVE_FILTERS = [['all', 'All'], ['you', 'You'], ['agents', 'Agents'], ['external', 'External']];
$('#live-filter').innerHTML = LIVE_FILTERS.map(([k, l]) => `<button data-f="${k}"${k === 'all' ? ' class="on"' : ''}>${l}</button>`).join('');
$('#live-filter').onclick = e => { const b = e.target.closest('button'); if (!b) return; S.liveFilter = b.dataset.f; $$('#live-filter button').forEach(x => x.classList.toggle('on', x === b)); renderLive(); };
function liveMatch(r) { const f = S.liveFilter; return f === 'all' || (f === 'you' && r.actor === 'you') || (f === 'external' && r.external) || (f === 'agents' && r.actor !== 'you' && !r.external); }
function onLive(ev) {
  let r = S.live.get(ev.id);
  if (!r) { r = { id: ev.id, actor: ev.actor, t: ev.t, tool: ev.tool, action: ev.action, summary: ev.summary, args: ev.args, phase: 'start', ro: ev.read_only }; S.live.set(ev.id, r); S.liveOrder.push(ev.id); r.isNew = true; }
  if (ev.phase === 'end') {
    Object.assign(r, { phase: 'end', ok: ev.ok, ms: ev.ms, result: ev.result, change: ev.change, findings: ev.findings || [], counts: ev.counts });
    const bad = (ev.findings || []).find(f => f.severity === 'error');
    if (bad && ev.actor !== 'you' && ev.actor !== 'app') toast(`${actorName(ev.actor)}'s edit: ${bad.message}`.slice(0, 140), 'bad');
  }
  if (S.liveOrder.length > 600) { const old = S.liveOrder.shift(); S.live.delete(old); }
  if (S.tab !== 'live' && !r.ro) $('#nav-live').hidden = false;
  if (ev.actor !== 'you' && ev.actor !== 'app') { lastActor = ev.actor; }
  liveTick();
}
function onExternal(m) {
  const ev = m.ev || {}, ph = ev.phase;
  if (!ev.tool_name && !ev.type) return;
  const id = `x${m.conn}:${ev.call_id || Math.random()}`;
  let r = S.live.get(id);
  if (!r) { r = { id, actor: `External engine #${m.conn}`, external: true, t: m.t, tool: ev.tool_name || ev.type, action: '', summary: ev.tool_name || ev.type, phase: 'start', isNew: true }; S.live.set(id, r); S.liveOrder.push(id); }
  if (ph === 'complete' || ph === 'error' || ev.type) { r.phase = 'end'; r.ok = ph !== 'error'; r.ms = ev.payload?.dur_ms; r.result = ev.payload?.error || ''; }
  liveTick();
}
let liveRaf = 0;
function liveTick() { if (!liveRaf) liveRaf = requestAnimationFrame(() => { liveRaf = 0; renderLive(); }); }
function renderLive() {
  const rows = S.liveOrder.map(id => S.live.get(id)).filter(liveMatch).reverse().slice(0, 300);
  const running = [...S.live.values()].filter(r => r.phase === 'start').length;
  $('#live-empty').hidden = rows.length > 0;
  $('#live-count').textContent = `${S.live.size} action${S.live.size === 1 ? '' : 's'}`;
  $('#live-state').textContent = running ? `${running} running` : S.live.size ? 'idle' : 'waiting';
  const tb = $('#tb-live'); tb.classList.toggle('busy', running > 0); $('#tb-live-t').textContent = running ? `${running} running` : S.live.size ? 'Live' : 'Idle';
  $('#live-rows').innerHTML = rows.map(r => `<tr class="${r.phase === 'start' ? 'running' : ''}${r.isNew ? ' new' : ''}"><td class="mono">${clock(r.t)}</td>
    <td>${r.external ? `<span class="chip a-ext">${esc(r.actor)}</span>` : actorChip(r.actor)}</td>
    <td><div class="tool">${esc(r.tool)}${r.action ? '.' + esc(r.action) : ''}</div><div class="args mono">${esc(r.summary || '')}</div></td>
    <td>${r.phase === 'start' ? '<span class="chip acc">running</span>' : r.ok ? `<span class="chip ok">${r.ms != null ? r.ms + ' ms' : 'done'}</span>` : '<span class="chip bad">error</span>'}</td>
    <td class="chg">${r.counts?.error ? `<span class="chip f-error">${r.counts.error} error${r.counts.error > 1 ? 's' : ''}</span> ` : ''}${r.counts?.warning ? `<span class="chip f-warning">${r.counts.warning} warning${r.counts.warning > 1 ? 's' : ''}</span> ` : ''}${r.change ? `<b>change #${r.change.id}</b> ` : ''}<span class="note mono">${esc((r.result || '').slice(0, 140))}</span></td></tr>`).join('');
  rows.forEach(r => r.isNew = false);
  const last = [...S.live.values()].reverse().find(r => r.change);
  $('#live-last').innerHTML = last ? `${actorChip(last.actor)} <b>${esc(last.action)}</b><div class="mono note" style="margin-top:6px">${esc(last.summary)}</div>` : '—';
}
$('#live-clear').onclick = () => { S.live.clear(); S.liveOrder = []; renderLive(); };
$('#live-graph').onclick = () => B.invoke('open_spectator').catch(e => toast(String(e), 'bad'));

// ================================================================================================================
// CONNECTIONS
async function refreshCli() {
  $('#cli-msg').textContent = 'Checking…';
  try { S.cli = await B.invoke('cli_status'); $('#cli-msg').textContent = ''; }
  catch (e) { $('#cli-msg').textContent = String(e); }
  renderCliCards(); renderAgentPick();
  const any = S.cli.some(a => a.installed && a.signed_in !== false);
  $('#nav-conn').hidden = any || agentReady('api');
}
function cliCard(a, compact) {
  const cur = currentAgent() === a.id;
  const ln = (cls, txt, extra = '') => `<div class="ln ${cls}"><span class="d"></span><span>${txt}</span>${extra}</div>`;
  const inst = a.installed ? ln('ok', `Installed${a.version ? ` <span class="mono">${esc(a.version)}</span>` : ''}`) : ln('bad', 'Not installed');
  const sign = !a.installed ? '' : a.signed_in === true ? ln('ok', esc(a.sign_in_note)) : a.signed_in === false ? ln('bad', esc(a.sign_in_note)) : ln('unk', esc(a.sign_in_note));
  const reg = compact || !a.installed ? '' : ln(a.registered ? 'ok' : '', a.registered ? 'kimo is in its own MCP list too' : 'Not added to its own MCP list (optional)');
  let acts = '';
  if (!a.installed) acts += `<button class="btn sm acc" data-cli="install" data-id="${a.id}">${ICON('upload')}Install</button>` + (compact ? '' : `<a class="lnk" style="margin-left:0" data-url="${esc(a.install_url)}">Guide ↗</a>`);
  else if (a.signed_in === false) acts += `<button class="btn sm acc" data-cli="signin" data-id="${a.id}">${ICON('term')}Sign in</button>`;
  else if (!cur) acts += `<button class="btn sm ${compact ? '' : 'acc'}" data-cli="use" data-id="${a.id}">Use this agent</button>`;
  else acts += `<span class="chip ok">In use</span><button class="btn sm ghost" data-go="agent">Open chat</button>`;
  if (!compact && a.installed && a.signed_in !== true) acts += a.signed_in === false ? '' : `<button class="btn sm ghost" data-cli="signin" data-id="${a.id}">Sign in</button>`;
  if (!compact && a.installed && !a.registered) acts += `<button class="btn sm ghost" data-cli="register" data-id="${a.id}" title="So your own ${esc(a.name)} sessions can use kimo too">Add kimo to ${esc(a.name)}</button>`;
  const model = compact || !a.installed ? '' : `<div class="ln"><span class="d" style="background:transparent"></span><span>Model: <b>${esc(modelLabel(a.id) || 'default')}</b>${effortOf(a.id) ? ` · ${esc(effortOf(a.id))} effort` : ''} · pick it in the Agent tab</span></div>`;
  return `<div class="card acard${cur ? ' sel' : ''}" id="cli-${a.id}"><div class="top"><span class="alogo ${a.id}">${a.id === 'claude-code' ? 'C' : a.id === 'codex' ? '◎' : 'A'}</span><div><div class="nm">${esc(a.name)}</div><div class="plan">${esc(a.plan)}</div></div></div>
    <div class="lines">${inst}${sign}${reg}</div>${model}<div class="acts">${acts}</div></div>`;
}
function renderCliCards() {
  const list = S.cli.length ? S.cli : AGENTS.map(id => ({ id, name: actorName(id), plan: '', installed: false, signed_in: null, sign_in_note: 'checking…' }));
  $('#cli-cards').innerHTML = list.map(a => cliCard(a, false)).join('');
  $('#home-agents').innerHTML = list.map(a => cliCard(a, true)).join('');
}
document.addEventListener('click', async e => {
  const u = e.target.closest('[data-url]'); if (u) { e.preventDefault(); B.invoke('open_url', { url: u.dataset.url }).catch(err => toast(String(err), 'bad')); return; }
  const b = e.target.closest('[data-cli]'); if (!b) return;
  const id = b.dataset.id, what = b.dataset.cli;
  if (what === 'use') return pickAgent(id);
  b.disabled = true;
  try {
    const msg = what === 'install' ? await B.invoke('cli_install', { id }) : what === 'signin' ? await B.invoke('cli_signin', { id }) : await B.invoke('cli_register', { id });
    $('#cli-msg').textContent = msg; toast(what === 'register' ? 'Added kimo to ' + actorName(id) : 'Follow the steps in the new window');
    if (what === 'register') refreshCli();
    else { // check back while the user signs in / installs
      let n = 0; const iv = setInterval(async () => { n++; await refreshCli(); const a = agentInfo(id); if ((a?.installed && a.signed_in !== false) || n > 40) clearInterval(iv); }, 6000);
    }
  } catch (err) { toast(String(err), 'bad'); $('#cli-msg').textContent = String(err); }
  b.disabled = false;
});
$('#cli-refresh').onclick = refreshCli;

// API keys (optional)
function renderKeys() {
  const a = S.app; if (!a) return;
  const sel = $('#key-provider');
  if (!sel.options.length) {
    sel.innerHTML = a.providers.map(p => `<option value="${p.id}">${esc(p.name)}</option>`).join('');
    sel.onchange = () => { fillProvider(); B.invoke('settings_set', { provider: sel.value }); };
  }
  if (document.activeElement !== sel) sel.value = a.settings.provider;
  $('#key-store').textContent = a.key_store;
  fillProvider();
}
function fillProvider() {
  const a = S.app, id = $('#key-provider').value, p = a.providers.find(x => x.id === id), k = a.keys.find(x => x.provider === id);
  if (!p) return;
  if (document.activeElement !== $('#key-model')) $('#key-model').value = a.settings.models[id] || p.default_model;
  $('#key-model-hint').textContent = p.default_model ? `(default ${p.default_model})` : '';
  if (document.activeElement !== $('#key-base')) $('#key-base').value = a.settings.base_urls[id] || p.base_url;
  $('#key-page').hidden = !p.key_page; $('#key-page').dataset.url = p.key_page;
  $('#key-state').textContent = k?.set ? `${p.name} · ${k.hint}` : 'no key'; $('#key-state').className = 'chip' + (k?.set ? ' ok' : '');
  $('#key-remember').checked = !!k?.remembered;
}
async function saveProvider() { const id = $('#key-provider').value; await B.invoke('settings_set', { provider: id, model: $('#key-model').value, baseUrl: $('#key-base').value }); }
$('#key-save').onclick = async () => {
  const id = $('#key-provider').value, key = $('#key-input').value;
  try { await saveProvider(); if (key.trim()) { const st = await B.invoke('key_set', { provider: id, key, remember: $('#key-remember').checked }); $('#key-msg').textContent = st.note || (st.remembered ? `Saved in the ${st.store}.` : 'Set for this session.'); } $('#key-input').value = ''; await refresh(); }
  catch (e) { $('#key-msg').textContent = String(e); }
};
$('#key-test').onclick = async () => { $('#key-msg').textContent = 'Testing…'; try { await saveProvider(); $('#key-msg').textContent = await B.invoke('key_test', { provider: $('#key-provider').value, key: $('#key-input').value || null }); } catch (e) { $('#key-msg').textContent = String(e); } };
$('#key-clear').onclick = async () => { await B.invoke('key_clear', { provider: $('#key-provider').value }); $('#key-msg').textContent = 'Forgotten.'; refresh(); };
$('#key-use').onclick = () => pickAgent('api').then(() => show('agent'));
$('#cfg-copy').onclick = e => copy($('#cfg-json').textContent, e.target);
$('#mcp-switch').onclick = async () => {
  const sw = $('#mcp-switch'), want = !sw.classList.contains('on');
  sw.classList.add('busy');
  try { const r = await B.invoke('mcp_set', { enabled: want }); $('#mcp-err').textContent = r.enabled || !want ? '' : (r.probe?.error || 'The engine did not answer.'); await refresh(); }
  catch (e) { $('#mcp-err').textContent = String(e); }
  sw.classList.remove('busy');
};

// ================================================================================================================
// HOME
function renderRecent() {
  const r = S.app?.settings?.recent || [];
  $('#home-recent').innerHTML = r.length ? r.map(p => `<a data-open="${esc(p)}"><span data-icon="folder"></span><span class="n">${esc(base(p).replace(/\.(kimo|json)$/, ''))}</span><span class="p" title="${esc(p)}">${esc(p)}</span></a>`).join('')
    : '<div class="note muted">Projects you save or open appear here.</div>';
  icons($('#home-recent'));
  $$('[data-open]').forEach(a => a.onclick = () => openProject(a.dataset.open));
}

// ================================================================================================================
// PROJECT FILES
async function openProject(path) {
  if (!path) { const p = await B.invoke('pick', { kind: 'project' }).catch(() => []); path = p[0]; }
  if (!path) return;
  if (S.dirty && !await confirmBox('Open another project?', 'Unsaved changes in this project will be lost.', 'Open')) return;
  try { await B.invoke('project_open', { path }); S.dirty = false; toast('Opened ' + base(path)); await refresh(); show('edit'); loadProject(); viewerSoon(); }
  catch (e) { toast(String(e), 'bad'); }
}
async function saveProject(as) {
  let path = !as && S.app?.project_path;
  if (!path) { const p = await B.invoke('pick', { kind: 'save_project', suggest: 'project.kimo' }).catch(() => []); path = p[0]; }
  if (!path) return;
  try { const saved = await B.invoke('project_save', { path }); S.dirty = false; toast('Saved ' + base(saved)); await refresh(); }
  catch (e) { toast(String(e), 'bad'); }
}
async function newProject() {
  if (S.dirty && !await confirmBox('Start a new project?', 'Unsaved changes in this project will be lost.', 'New project')) return;
  await B.invoke('project_new'); S.dirty = false; S.sel = null; V.frames.clear(); dropDraft();
  await refresh(); loadProject(); viewerSoon();
}
$('#ed-open').onclick = () => openProject(); $('#home-open').onclick = () => openProject();
$('#ed-save').onclick = () => saveProject(false); $('#ed-new').onclick = newProject;
$('#tb-proj').onclick = e => menu(e, [['folder', 'Open…', () => openProject(), 'Ctrl+O'], ['save', 'Save', () => saveProject(false), 'Ctrl+S'], ['save', 'Save as…', () => saveProject(true)], ['plus', 'New project', newProject], null, ['export', 'Export video…', openExport, 'Ctrl+E']]);

// ================================================================================================================
// EXPORT
const EX = { job: null, timer: 0, path: null };
function openExport() {
  $('#export').classList.add('on');
  $('#ex-sum').textContent = P.dur ? `${tc(P.dur)} · ${P.w}×${P.h} · ${P.fps} fps · MP4 (H.264 + AAC)` : 'The timeline is empty — add a clip first.';
  if (!$('#ex-path').value) {
    const pp = S.app?.project_path;
    $('#ex-path').value = pp ? pp.replace(/\.(kimo|json)$/i, '') + '.mp4' : '';
  }
  $('#ex-go').disabled = !P.dur || !!EX.job;
}
$('#ed-export').onclick = openExport;
$('#ex-close').onclick = () => $('#export').classList.remove('on');
$('#ex-browse').onclick = async () => { const p = await B.invoke('pick', { kind: 'export', suggest: base($('#ex-path').value) || 'export.mp4' }).catch(() => []); if (p[0]) $('#ex-path').value = p[0].match(/\.mp4$/i) ? p[0] : p[0] + '.mp4'; };
$('#ex-q').onclick = e => { const b = e.target.closest('button'); if (b) $$('#ex-q button').forEach(x => x.classList.toggle('on', x === b)); };
$('#ex-go').onclick = async () => {
  let path = $('#ex-path').value.trim();
  if (!path) { await $('#ex-browse').onclick(); path = $('#ex-path').value.trim(); if (!path) return; }
  if (!/^([a-zA-Z]:[\\/]|\/)/.test(path)) return $('#ex-err').textContent = 'Choose where to save with Browse… (a full path).';
  $('#ex-err').textContent = ''; $('#ex-reveal').hidden = true;
  const crf = +$('#ex-q .on').dataset.crf;
  const r = await call('output', 'export_job_start', { path, crf, force: true });
  if (!r?.job_id) return $('#ex-err').textContent = 'The export did not start.';
  EX.job = r.job_id; EX.path = path; $('#ex-go').disabled = true;
  $('#ex-prog').hidden = false; $('#ex-bar').style.width = '0%'; $('#ex-status').textContent = 'Rendering…';
  clearInterval(EX.timer);
  EX.timer = setInterval(pollExport, 700);
};
async function pollExport() {
  if (!EX.job) return;
  const s = await read('output', 'job_status', { job_id: EX.job });
  if (!s) return;
  const pct = s.frames_total ? 100 * s.frames_done / s.frames_total : 0;
  $('#ex-bar').style.width = pct.toFixed(1) + '%';
  $('#ex-status').textContent = `${s.state} · ${s.frames_done ?? 0}/${s.frames_total ?? '?'} frames${s.chunks ? ` · chunk ${s.chunks.done}/${s.chunks.total}` : ''}`;
  $('#ex-eta').textContent = s.eta?.seconds != null ? `~${Math.ceil(s.eta.seconds)} s left` : '';
  if (['done', 'failed', 'cancelled'].includes(s.state)) {
    clearInterval(EX.timer); const job = EX.job; EX.job = null; $('#ex-go').disabled = false;
    if (s.state === 'done') { $('#ex-bar').style.width = '100%'; $('#ex-status').textContent = 'Done · ' + base(s.result?.path || EX.path); $('#ex-reveal').hidden = false; toast('Export finished'); }
    else { $('#ex-err').textContent = (s.error || s.warnings?.join(' ') || s.state) + ''; }
    void job;
  }
}
$('#ex-reveal').onclick = () => B.invoke('reveal', { path: EX.path });

// ================================================================================================================
// FIRST RUN: tools, then a one-time welcome
const PART_IC = { ffmpeg: 'film', ffprobe: 'search', whisper: 'audio', model: 'spark' };
async function setupStatus() {
  const s = await B.invoke('setup_status');
  S.setup = s;
  $('#h-parts').textContent = s.ready ? (s.parts.some(p => p.satisfied === 'system') ? 'Ready · using this computer\'s FFmpeg' : 'Ready') : 'Missing FFmpeg';
  $('#parts').innerHTML = s.parts.map(p => `<div class="part${p.satisfied ? ' done' : ''}" data-part="${p.id}"><span class="pic">${ICON(PART_IC[p.id] || 'film')}</span>
    <div class="top"><span class="nm">${esc(p.name)}</span><span class="note muted">${esc(p.note)}${p.optional ? ' · optional' : ''}</span><span class="sz">${p.satisfied === 'system' ? 'on this computer' : p.satisfied ? 'ready' : p.pinned ? (p.size / 1e6).toFixed(0) + ' MB' : 'not available'}</span></div>
    <div class="bar"><i style="width:${p.satisfied ? 100 : 0}%"></i></div><div class="meta"><span class="st">${p.satisfied === 'system' ? 'Using the copy already on this computer' : p.satisfied ? 'Installed' : p.pinned ? 'Will download' : 'Not published for download yet — install FFmpeg yourself'}</span><span class="spd"></span></div></div>`).join('');
  $('#tot-t').textContent = '';
  if (!s.ready && !s.done) $('#setup').classList.add('on');
  return s;
}
function onProgress(p) {
  const el = $(`.part[data-part="${p.id}"]`); if (!el) return;
  if (p.phase === 'system') { el.classList.add('done'); el.querySelector('.st').textContent = 'Using the copy already on this computer'; el.querySelector('.bar i').style.width = '100%'; return; }
  if (p.phase === 'error' || p.phase === 'unavailable') { el.querySelector('.st').textContent = p.error || 'Not available'; return; }
  if (p.total) el.querySelector('.bar i').style.width = (100 * p.done / p.total).toFixed(1) + '%';
  el.querySelector('.spd').textContent = p.bps ? (p.bps / 1e6).toFixed(1) + ' MB/s' : '';
  if (p.phase === 'done' || p.phase === 'verified') { el.classList.add('done'); el.querySelector('.st').textContent = 'Installed'; el.querySelector('.spd').textContent = ''; }
}
$('#setup-go').onclick = async () => {
  $('#setup-go').disabled = true; $('#setup-pause').hidden = false; $('#setup-err').textContent = '';
  try {
    const r = await B.invoke('setup_start', { skipOptional: $('#skip-opt').checked });
    if (r.ready) { $('#setup').classList.remove('on'); toast('kimo is ready'); await refresh(); maybeWelcome(); }
    else $('#setup-err').textContent = 'FFmpeg is still missing. Install it (for example `winget install Gyan.FFmpeg`, `brew install ffmpeg` or `sudo apt install ffmpeg`), then press Get ready again.';
  } catch (e) { $('#setup-err').textContent = String(e); }
  $('#setup-go').disabled = false; $('#setup-pause').hidden = true; setupStatus();
};
$('#setup-pause').onclick = () => B.invoke('setup_pause');
$('#setup-later').onclick = () => { B.invoke('setup_skip'); $('#setup').classList.remove('on'); maybeWelcome(); };
function maybeWelcome() { if (!S.app?.settings?.onboarded && !$('#setup').classList.contains('on')) $('#welcome').classList.add('on'); }
const endWelcome = () => { $('#welcome').classList.remove('on'); B.invoke('onboarded_set'); S.app.settings.onboarded = true; };
$('#wel-skip').onclick = endWelcome;
$('#wel-connect').onclick = () => { endWelcome(); show('connections'); };
$('#wel-import').onclick = () => { endWelcome(); show('edit'); pickMedia(); };
$('#brand').onclick = () => $('#about').classList.add('on');
$('#ab-close').onclick = () => $('#about').classList.remove('on');
$$('.overlay').forEach(o => o.addEventListener('mousedown', e => { if (e.target === o && !['setup', 'confirm'].includes(o.id)) o.classList.remove('on'); }));

// ================================================================================================================
// CONTEXT MENU
function menu(e, items) {
  e.preventDefault?.();
  const m = $('#ctx');
  m.innerHTML = items.map((it, i) => it ? `<button data-i="${i}" class="${it[4] || ''}">${ICON(it[0])}${esc(it[1])}${it[3] ? `<span class="k">${esc(it[3])}</span>` : ''}</button>` : '<hr>').join('');
  m.hidden = false;
  const x = Math.min(e.clientX ?? e.target.getBoundingClientRect().left, innerWidth - 230), y = Math.min(e.clientY ?? e.target.getBoundingClientRect().bottom, innerHeight - m.offsetHeight - 10);
  m.style.left = x + 'px'; m.style.top = y + 'px';
  m.onclick = ev => { const b = ev.target.closest('button'); if (!b) return; m.hidden = true; items[+b.dataset.i][2](); };
}
addEventListener('mousedown', e => { if (!e.target.closest('#ctx')) $('#ctx').hidden = true; });

// ================================================================================================================
// KEYBOARD
addEventListener('keydown', e => {
  const typing = e.target.closest('input,textarea,select,[contenteditable]');
  const mod = e.ctrlKey || e.metaKey;
  if (e.key === 'Escape') { $$('.overlay.on').forEach(o => { if (!['setup'].includes(o.id)) o.classList.remove('on'); }); $('#ctx').hidden = true; return; }
  if (mod && /^[1-5]$/.test(e.key)) { e.preventDefault(); show(['home', 'edit', 'agent', 'live', 'connections'][+e.key - 1]); return; }
  if (mod && e.key.toLowerCase() === 's') { e.preventDefault(); saveProject(e.shiftKey); return; }
  if (mod && e.key.toLowerCase() === 'o') { e.preventDefault(); openProject(); return; }
  if (mod && e.key.toLowerCase() === 'e') { e.preventDefault(); openExport(); return; }
  if (mod && e.key.toLowerCase() === 'i') { e.preventDefault(); pickMedia(); return; }
  if (typing) return;
  if (mod && e.key.toLowerCase() === 'z') { e.preventDefault(); call('project', e.shiftKey ? 'redo' : 'undo', {}); return; }
  if (mod && e.key.toLowerCase() === 'y') { e.preventDefault(); call('project', 'redo', {}); return; }
  if (S.tab !== 'edit') return;
  if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); dupSel(); return; }
  if (mod && e.shiftKey && e.key.toLowerCase() === 'x') { e.preventDefault(); setLoop('clear'); return; }
  if (mod && e.key.toLowerCase() === 'l') { e.preventDefault(); setLoop('toggle'); return; }
  if (mod) return;
  const k = e.key, kl = k.length === 1 ? k.toLowerCase() : k;
  if (kl === 'k') { kHeld = true; if (!e.repeat) shuttle('k'); return; }
  if (kl === 'j' || kl === 'l') { e.preventDefault(); if (kHeld) stepFrame(kl === 'l' ? 1 : -1); else if (!e.repeat) shuttle(kl); return; }
  if (kl === 'i' && !e.shiftKey) return setLoop('in');
  if (kl === 'o' && !e.shiftKey) return setLoop('out');
  if (kl === 'f') return fullScreen();
  if (k === ' ') { e.preventDefault(); togglePlay(); }
  else if (k === 's' || k === 'S') splitSel();
  else if (k === 'Delete' || k === 'Backspace') delSel(e.shiftKey);
  else if (k === 't' || k === 'T') addText();
  else if (k === 'm' || k === 'M') $('#ed-marker').click();
  else if (k === 'ArrowLeft') { e.preventDefault(); if (e.shiftKey) seekTo(V.t - 1); else stepFrame(-1); }
  else if (k === 'ArrowRight') { e.preventDefault(); if (e.shiftKey) seekTo(V.t + 1); else stepFrame(1); }
  else if (k === 'Home') seekTo(0);
  else if (k === 'End') seekTo(P.dur);
  else if (k === '+' || k === '=') setZoom(T.z * 1.4);
  else if (k === '-' || k === '_') setZoom(T.z / 1.4);
  else if (k === 'Escape') select(null);
});

let kHeld = false;
addEventListener('keyup', e => { if (e.key === 'k' || e.key === 'K') kHeld = false; });
addEventListener('blur', () => { kHeld = false; });

// ================================================================================================================
// FILE DROP (the OS drops files on the window; Tauri hands us their paths)
let dropDepth = 0;
B.listen('tauri://drag-enter', () => { dropDepth = 1; $('#dropcover').classList.add('on'); $('#bin').classList.add('hot'); });
B.listen('tauri://drag-leave', () => { dropDepth = 0; $('#dropcover').classList.remove('on'); $('#bin').classList.remove('hot'); });
B.listen('tauri://drag-drop', p => {
  $('#dropcover').classList.remove('on'); $('#bin').classList.remove('hot'); dropDepth = 0;
  const paths = p?.paths || [];
  const proj = paths.find(x => /\.kimo$/i.test(x));
  if (proj) return openProject(proj);
  if (S.tab !== 'edit') show('edit');
  importPaths(paths);
});

// ================================================================================================================
// WIRING
B.listen('live-event', onLive);
B.listen('external-event', onExternal);
B.listen('changes', list => { const flash = list.length > S.changes.length ? list[list.length - 1]?.id : null; S.changes = list; renderChanges(flash); });
B.listen('project-changed', m => {
  S.dirty = m?.action !== 'new_project' && m?.action !== 'load_project'; setProjectName(S.app?.project_path);
  loadSoon(m?.actor && m.actor !== 'you' ? 60 : 30); viewerSoon();
});
B.listen('agent-event', onAgentEvent);
B.listen('agent-connection', m => { renderAttached(m.attached || {}); if (m.up && m.client !== currentAgent()) toast(`${actorName(m.client)} connected to this project`); });
B.listen('setup-progress', onProgress);
B.listen('open-file', p => openProject(p));
B.listen('autosaved', m => { const el = $('#tb-auto'); el.hidden = false; el.textContent = 'Autosaved ' + new Date(m.at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); el.title = 'Saved for crash recovery to ' + m.path + '. Use Save to write your own file.'; });
B.listen('engine-status', s => { refresh().then(() => { if (s?.state === 'running') { loadProject(); viewerSoon(); } }); });
addEventListener('resize', () => { if (S.tab === 'edit') renderTimeline(); });

await refresh();
renderChanges(); renderLive(); renderCliCards(); renderAgentPick();
S.changes = await B.invoke('changes_list').catch(() => []); renderChanges();
const st = await setupStatus().catch(() => null);
if (S.app?.engine?.state === 'running') {
  await loadProject(); viewerRefresh();
  const tr = await read('effects', 'list_transitions');
  S.transitions = (Array.isArray(tr) ? tr : tr?.transitions || tr?.kinds || []).map(x => typeof x === 'string' ? x : x.kind || x.name).filter(Boolean).slice(0, 60);
}
refreshCli();
// a crash (PC restart, power cut) left unsaved work: offer it back before anything else
const lost = await B.invoke('recovery_status').catch(() => null);
if (lost) {
  const when = new Date(lost.autosaved_at).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
  const what = lost.project_path ? base(lost.project_path).replace(/\.(kimo|json)$/, '') : 'an unsaved project';
  if (await confirmBox('Restore your unsaved work?', `kimo closed before your last changes to ${what} were saved. Restore the copy autosaved at ${when}?`, 'Restore', 'Discard')) {
    try { await B.invoke('recovery_restore'); S.dirty = true; await refresh(); show('edit'); await loadProject(); viewerSoon(); toast('Restored. Press Save to keep it.'); }
    catch (e) { toast(String(e), 'bad'); }
  } else { B.invoke('recovery_discard'); }
} else if (st && (st.ready || st.done)) maybeWelcome();
// kimo crashed last time: ask before anything is sent. "Send report" opens a pre-filled GitHub issue in the browser,
// which the person reads and submits themselves; kimo sends nothing. The log stays on this computer either way.
const crashes = await B.invoke('crash_pending').catch(() => []);
if (crashes?.length) {
  const c = crashes[0], more = crashes.length > 1 ? ` (${crashes.length - 1} older crash log${crashes.length > 2 ? 's' : ''} will be filed away too)` : '';
  const code = $('#cf-code'); code.textContent = c.text.split('\n').slice(0, 12).join('\n'); code.hidden = false;
  const send = await confirmBox('kimo closed unexpectedly last time', `Send a crash report? It opens a pre-filled report on GitHub in your browser, with the log below (your home folder replaced by ~). You read it and submit it yourself: nothing is sent unless you do${more}.`, 'Send report', "Don't send");
  code.hidden = true; code.textContent = '';
  for (const r of crashes) await B.invoke('crash_answer', { file: r.file, send: send && r === c }).catch(e => toast(String(e), 'bad'));
}
// updates: release builds check GitHub releases after start; installing waits for a click here
function showUpdate(u) {
  const b = $('#tb-update');
  if (!u || u.state !== 'available') { b.hidden = true; return; }
  S.update = u; b.hidden = false; b.disabled = false; b.textContent = `Update to ${u.version}`;
}
$('#tb-update').onclick = async () => {
  const u = S.update; if (!u) return;
  const unsaved = S.dirty ? 'Your unsaved changes are autosaved first and offered back after the restart. ' : '';
  if (!await confirmBox(`Install kimo ${u.version}?`, `${unsaved}kimo downloads the update, checks its signature, closes and starts again.${u.notes ? '\n\n' + u.notes : ''}`, 'Install and restart', 'Later')) return;
  const b = $('#tb-update'); b.disabled = true; b.textContent = 'Downloading…';
  try { await B.invoke('update_install'); } catch (e) { showUpdate(u); toast(String(e), 'bad'); }
};
B.listen('update-available', showUpdate);
B.listen('update-progress', p => { if (p.total) $('#tb-update').textContent = `Downloading ${Math.round(100 * p.done / p.total)}%`; });
B.listen('update-installing', () => { $('#tb-update').textContent = 'Installing…'; });
showUpdate(S.app?.update);
setInterval(() => { if (S.app?.engine?.state !== 'running') refresh().then(() => { if (S.app?.engine?.state === 'running') { loadProject(); viewerSoon(); } }); }, 2000);
window.__kimo = { S, P, V, T, PB, show, refresh, loadProject, sendAgent, importPaths, viewerRefresh };
document.body.dataset.ready = '1';
