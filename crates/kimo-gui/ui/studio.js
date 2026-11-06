// kimo main window. One bridge (Tauri IPC in the app; a labelled mock in a plain browser for tests and screenshots),
// one small state object, plain DOM. No network from the webview: every action goes through the app's own commands.
const $ = s => document.querySelector(s);
const $$ = s => [...document.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

let B;
if (window.__TAURI__) {
  B = { kind: 'tauri', invoke: (c, a) => window.__TAURI__.core.invoke(c, a || {}), listen: (e, f) => window.__TAURI__.event.listen(e, x => f(x.payload)) };
} else {
  B = (await import('./dev/mock-bridge.js')).bridge;
}
window.__kimoBridge = B;

const S = { app: null, tab: 'home', rows: new Map(), order: [], filter: 'all', liveTotal: 0, cards: [], cfg: 'claude_add', setup: {}, busy: false };

function toast(t) { const el = $('#toast'); el.textContent = t; el.classList.add('on'); clearTimeout(toast.t); toast.t = setTimeout(() => el.classList.remove('on'), 1800); }
async function copy(text) {
  try { await navigator.clipboard.writeText(text); }
  catch { const ta = document.createElement('textarea'); ta.value = text; document.body.appendChild(ta); ta.select(); document.execCommand('copy'); ta.remove(); }
  toast('Copied');
}
const fmtMB = b => b >= 1e9 ? (b / 1e9).toFixed(2) + ' GB' : (b / 1e6).toFixed(b >= 1e7 ? 0 : 1) + ' MB';
const fmtTime = s => s == null || !isFinite(s) ? '—' : s < 60 ? Math.ceil(s) + ' s' : Math.floor(s / 60) + ' min ' + Math.ceil(s % 60) + ' s';
const clock = t => new Date(t).toLocaleTimeString([], { hour12: false });

// ---------- tabs ----------
function show(tab) {
  S.tab = tab;
  $$('#nav a').forEach(a => a.classList.toggle('on', a.dataset.tab === tab));
  $$('.tab').forEach(s => s.classList.toggle('on', s.id === 'tab-' + tab));
  if (tab === 'live') $('#nav-live').hidden = true;
  if (tab === 'connections') detectAgents();
}
$$('#nav a').forEach(a => a.onclick = () => show(a.dataset.tab));
$$('[data-go]').forEach(el => el.onclick = () => { show(el.dataset.go); if (el.dataset.focus) setTimeout(() => document.getElementById('agent-' + el.dataset.focus)?.scrollIntoView({ behavior: 'smooth', block: 'center' }), 50); });

// ---------- app state ----------
async function refresh() {
  const a = S.app = await B.invoke('app_state');
  $('#ver').textContent = 'v' + a.version;
  const e = a.engine || {};
  const pe = $('#pill-engine'), he = $('#h-engine'), hm = $('#h-engine-m');
  pe.className = 'pill ' + (e.state === 'running' ? 'ok' : e.state === 'stopped' ? '' : 'bad');
  if (e.state === 'running') { $('#pill-engine-t').textContent = 'v' + e.version; he.textContent = 'Running · v' + e.version; hm.textContent = `${e.tools} tools (${e.actions} actions) · pid ${e.pid}`; }
  else if (e.state === 'missing') { $('#pill-engine-t').textContent = 'not found'; he.textContent = 'Not found'; hm.textContent = 'kimo-mcp was not found next to the app'; }
  else if (e.state === 'crashed') { $('#pill-engine-t').textContent = 'stopped'; he.textContent = 'Stopped'; hm.innerHTML = esc(e.error) + ' · <a href="#" id="eng-restart">restart</a>'; $('#eng-restart').onclick = ev => { ev.preventDefault(); B.invoke('engine_restart').then(refresh); }; }
  else { $('#pill-engine-t').textContent = 'starting…'; he.textContent = 'Starting…'; hm.textContent = ''; }
  renderMcp(a.mcp_enabled, null);
  renderKeys();
}

// ---------- MCP ----------
function renderMcp(on, probe) {
  const a = S.app;
  $('#mcp-switch').classList.toggle('on', !!on);
  $('#pill-mcp').className = 'pill ' + (on ? 'on' : '');
  $('#pill-mcp-t').textContent = on ? 'on · local' : 'off';
  $('#h-mcp').textContent = on ? 'On' : 'Off';
  $('#h-mcp-m').textContent = on ? `Listening on 127.0.0.1:${a.external_port ?? '7421'} for agent activity` : 'Off by default, local only';
  $('#mcp-on').hidden = !on;
  if (probe) { S.probe = probe; }
  if (on) {
    const p = S.probe;
    $('#mcp-verified').textContent = p ? `Verified: kimo ${p.version} answered initialize + tools/list · ${p.tools} tools (${p.actions} actions) · ${p.ms} ms` : 'On';
    $('#mcp-port').textContent = a.external_port ? `activity from external engines → 127.0.0.1:${a.external_port}` : '';
    renderCfg();
  }
  $$('.agent [data-add]').forEach(b => b.disabled = !on);
}
function renderCfg() {
  const sn = S.app?.snippets; if (!sn) { $('#cfg-box').textContent = 'engine not found'; return; }
  $$('#cfg-seg button').forEach(b => b.classList.toggle('on', b.dataset.c === S.cfg));
  $('#cfg-box').innerHTML = `<button class="btn sm copy">Copy</button>${esc(sn[S.cfg])}`;
  $('#cfg-box .copy').onclick = () => copy(sn[S.cfg]);
}
$$('#cfg-seg button').forEach(b => b.onclick = () => { S.cfg = b.dataset.c; renderCfg(); });
$('#mcp-switch').onclick = async () => {
  const sw = $('#mcp-switch'); if (sw.classList.contains('busy')) return;
  const want = !sw.classList.contains('on');
  sw.classList.add('busy'); $('#mcp-err').textContent = want ? 'Starting a fresh engine and checking it answers…' : '';
  try {
    const r = await B.invoke('mcp_set', { enabled: want });
    S.app.mcp_enabled = r.enabled; if (r.external_port) S.app.external_port = r.external_port; if (r.snippets) S.app.snippets = r.snippets;
    $('#mcp-err').textContent = (want && !r.enabled) ? `The engine did not answer, so MCP stays off: ${r.probe?.error || 'unknown error'}` : '';
    renderMcp(r.enabled, r.probe);
  } catch (e) { $('#mcp-err').textContent = String(e); }
  sw.classList.remove('busy');
};

// ---------- coding agents ----------
const AG = {
  claude: { color: '#D97757', letter: 'C', blurb: 'Adds kimo to Claude Code for your user (all projects).', snip: 'claude_add', doc: 'claude_doc' },
  codex: { color: '#141210', letter: 'X', blurb: 'Adds a [mcp_servers.kimo] table to ~/.codex/config.toml.', snip: 'codex_toml', doc: 'codex_doc' },
};
async function detectAgents() {
  let list = [];
  try { list = await B.invoke('agents_detect'); } catch (e) { }
  const on = !!S.app?.mcp_enabled, sn = S.app?.snippets || {};
  $('#agent-cards').innerHTML = list.map(a => {
    const g = AG[a.id];
    const status = a.linked ? `<span class="chip ok">kimo linked</span>` : a.cli ? `<span class="chip">found, not linked</span>` : `<span class="chip">not found on PATH</span>`;
    const cmd = a.id === 'claude' ? sn.claude_add : sn.codex_add;
    return `<div class="card agent" id="agent-${a.id}">
      <div class="hd"><div class="logo" style="background:${g.color}">${g.letter}</div><div><h2 style="margin:0">${esc(a.name)}</h2><div class="note mono muted">${esc(a.cli || 'command not on PATH')}</div></div><div style="margin-left:auto">${status}</div></div>
      <div class="note">${g.blurb}${a.linked && a.linked_command ? `<br>Existing entry runs <span class="mono">${esc(a.linked_command)}</span>` : ''}</div>
      <pre class="codebox mono" style="font-size:12px">${esc(cmd || '—')}</pre>
      <div class="row"><button class="btn sm" data-copy="${a.id}">Copy command</button><button class="btn sm pri" data-add="${a.id}" ${on && (a.id === 'codex' || a.cli) && !a.linked ? '' : 'disabled'}>Add for me</button>
      <a class="note" style="margin-left:auto" target="_blank" href="${esc(sn[g.doc] || '#')}">syntax checked against the official docs ↗</a></div></div>`;
  }).join('');
  $$('[data-copy]').forEach(b => b.onclick = () => copy(b.dataset.copy === 'claude' ? sn.claude_add : sn.codex_add));
  $$('[data-add]').forEach(b => b.onclick = () => confirmAdd(b.dataset.add));
}
function confirmAdd(id) {
  const sn = S.app.snippets;
  $('#cf-title').textContent = id === 'claude' ? 'Add kimo to Claude Code?' : 'Add kimo to Codex?';
  $('#cf-body').textContent = id === 'claude' ? 'kimo will run this for you (as one command, no shell):' : 'kimo will append this table to ~/.codex/config.toml (a .bak copy of the old file is kept):';
  $('#cf-code').textContent = id === 'claude' ? sn.claude_add_json : sn.codex_toml;
  $('#confirm').classList.add('on');
  $('#cf-no').onclick = () => $('#confirm').classList.remove('on');
  $('#cf-yes').onclick = async () => {
    $('#confirm').classList.remove('on');
    try { toast(await B.invoke('agent_add', { id })); } catch (e) { toast(String(e)); }
    detectAgents();
  };
}

// ---------- keys ----------
function renderKeys() {
  const a = S.app, sel = $('#key-provider');
  if (!sel.options.length) {
    sel.innerHTML = a.providers.map(p => `<option value="${p.id}">${esc(p.name)}</option>`).join('');
    sel.onchange = () => { fillProvider(); B.invoke('settings_set', { provider: sel.value }); };
  }
  if (!S.keysInit) { sel.value = a.settings.provider; S.keysInit = true; }
  fillProvider();
  $('#key-store').textContent = a.key_store;
}
function fillProvider() {
  const a = S.app, id = $('#key-provider').value, p = a.providers.find(x => x.id === id), k = a.keys.find(x => x.provider === id);
  $('#key-model').value = a.settings.models[id] || p.default_model;
  $('#key-model-hint').textContent = p.default_model ? `(default ${p.default_model}, reference only)` : '(type the model your server runs)';
  $('#key-base').value = a.settings.base_urls[id] || p.base_url;
  $('#key-page').hidden = !p.key_page; $('#key-page').href = p.key_page || '#';
  $('#key-input').placeholder = k?.set ? `Key set (${k.hint}). Paste a new one to replace it.` : 'Paste your key';
  $('#key-remember').checked = !!k?.remembered;
  const st = $('#key-state'); st.className = 'chip ' + (k?.set ? 'ok' : ''); st.textContent = k?.set ? (k.remembered ? 'saved in keychain' : 'set for this session') : 'no key';
  const cur = a.keys.find(x => x.provider === a.settings.provider);
  const curP = a.providers.find(x => x.id === a.settings.provider);
  $('#pill-ai').className = 'pill ' + (cur?.set ? 'ok' : ''); $('#pill-ai-t').textContent = cur?.set ? curP.name.split(' (')[0] : 'no key';
  $('#h-ai').textContent = cur?.set ? curP.name : 'No key yet'; $('#h-ai-m').textContent = cur?.set ? (a.settings.models[curP.id] || curP.default_model) : 'Add one in Connections';
  $('#chat-model').textContent = cur?.set ? `${curP.name} · ${a.settings.models[curP.id] || curP.default_model}` : 'no key: add one in Connections'; $('#chat-model').className = 'chip ' + (cur?.set ? 'ok' : '');
}
async function saveProvider() {
  const id = $('#key-provider').value;
  await B.invoke('settings_set', { provider: id, model: $('#key-model').value, baseUrl: $('#key-base').value });
}
$('#key-save').onclick = async () => {
  const id = $('#key-provider').value, key = $('#key-input').value;
  try {
    await saveProvider();
    if (key.trim()) { const st = await B.invoke('key_set', { provider: id, key, remember: $('#key-remember').checked }); $('#key-msg').textContent = st.note || (st.remembered ? `Saved in the ${st.store}.` : 'Set for this session.'); }
    else $('#key-msg').textContent = 'Saved.';
    $('#key-input').value = '';
    await refresh();
  } catch (e) { $('#key-msg').textContent = String(e); }
};
$('#key-test').onclick = async () => {
  const id = $('#key-provider').value; $('#key-msg').textContent = 'Testing…';
  try { await saveProvider(); $('#key-msg').textContent = await B.invoke('key_test', { provider: id, key: $('#key-input').value || null }); $('#key-msg').style.color = 'var(--ok)'; }
  catch (e) { $('#key-msg').textContent = String(e); $('#key-msg').style.color = 'var(--bad)'; }
};
$('#key-clear').onclick = async () => { await B.invoke('key_clear', { provider: $('#key-provider').value }); $('#key-msg').textContent = 'Forgotten.'; refresh(); };

// ---------- chat ----------
function addMsg(cls, html) { const d = document.createElement('div'); d.className = cls; d.innerHTML = html; $('#msgs').appendChild(d); $('#msgs').scrollTop = 1e9; return d; }
const steps = new Map();
function onChat(ev) {
  if (ev.kind === 'thinking') { if (!S.thinking) S.thinking = addMsg('step', '<span class="s"><span class="spin"></span></span>thinking…'); }
  else { S.thinking?.remove(); S.thinking = null; }
  if (ev.kind === 'tool_start') steps.set(ev.call, addMsg('step', `<span class="s"><span class="spin"></span></span><span class="mono">${esc(ev.summary)}</span>`));
  if (ev.kind === 'tool_end') {
    const el = steps.get(ev.call); if (el) el.querySelector('.s').innerHTML = ev.ok ? '<span class="st-ok">✓</span>' : '<span class="st-err">✕</span>';
    if (!ev.ok && el) el.title = ev.result;
    if (ev.card) addCard(ev.card);
  }
  if (ev.kind === 'reply') addMsg('msg agent', esc(ev.text));
  if (ev.kind === 'error') addMsg('msg err', esc(ev.message));
}
function addCard(c) {
  c.t = Date.now(); S.cards.push(c);
  renderCards(c.id);
}
function renderCards(flashId) {
  const live = S.cards.filter(c => !c.undone).length;
  $('#chg-count').textContent = live; $('#nav-changes').hidden = !live; $('#nav-changes').textContent = live;
  $('#chg-empty').hidden = S.cards.length > 0;
  $('#changes').querySelectorAll('.ccard').forEach(e => e.remove());
  for (const c of [...S.cards].reverse()) {
    const d = document.createElement('div');
    d.className = 'ccard' + (c.undone ? ' undone' : '') + (c.id === flashId ? ' flash' : '');
    const later = S.cards.filter(x => x.depth > c.depth && !x.undone).length;
    d.innerHTML = `<div class="row"><span class="lbl">${esc(c.label)}</span><span class="t" style="margin-left:auto">${clock(c.t)}</span></div>
      <div class="note mono" style="margin:4px 0 8px">${esc(c.summary)}</div>
      <div class="row">${c.undone ? '<span class="chip">undone</span>' : `<button class="btn sm" data-undo="${c.id}">Undo</button>${later ? `<span class="note muted">also undoes ${later} later change${later > 1 ? 's' : ''}</span>` : ''}`}</div>`;
    $('#changes').appendChild(d);
  }
  $$('[data-undo]').forEach(b => b.onclick = async () => {
    b.disabled = true;
    try { const r = await B.invoke('chat_undo', { card: +b.dataset.undo }); for (const c of S.cards) if (r.undone.includes(c.id)) c.undone = true; renderCards(); toast(`Undid ${r.steps} step${r.steps > 1 ? 's' : ''}`); }
    catch (e) { toast(String(e)); b.disabled = false; }
  });
}
async function sendChat(text) {
  text = (text ?? $('#chat-in').value).trim(); if (!text || S.busy) return;
  $('#chat-in').value = ''; S.busy = true; $('#chat-send').disabled = true;
  addMsg('msg user', esc(text));
  try { await B.invoke('chat_send', { text }); }
  catch (e) { if (!$('#msgs').lastElementChild?.classList.contains('err')) addMsg('msg err', esc(e)); }
  S.thinking?.remove(); S.thinking = null; S.busy = false; $('#chat-send').disabled = false;
}
$('#chat-send').onclick = () => sendChat();
$('#chat-in').onkeydown = e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendChat(); } };
$$('.composer .sugg button').forEach(b => b.onclick = () => sendChat(b.textContent));
$('#chat-reset').onclick = async () => { await B.invoke('chat_reset'); $('#msgs').querySelectorAll(':not(#chat-hello)').forEach(e => e.remove()); S.cards = []; renderCards(); };

// ---------- live ----------
function changeText(p = {}, target = {}) {
  const prm = target.param || p.param;
  if (p.from !== undefined || p.to !== undefined) return `${prm ? esc(prm) + ': ' : ''}${esc(fmt(p.from))} → <b>${esc(fmt(p.to))}</b>`;
  if (p.value !== undefined) return `${prm ? esc(prm) + ' = ' : ''}<b>${esc(fmt(p.value))}</b>`;
  if (p.at !== undefined) return `split at <b>${esc(fmt(p.at))}s</b>`;
  if (p.text !== undefined) return `text “<b>${esc(String(p.text).slice(0, 40))}</b>”`;
  if (p.effect !== undefined) return `+ <b>${esc(p.effect)}</b>`;
  if (p.s !== undefined && p.e !== undefined) return `<b>${esc(fmt(p.s))}–${esc(fmt(p.e))}s</b>`;
  if (p.on !== undefined) return p.on ? 'on' : 'off';
  return '';
}
const fmt = v => typeof v === 'number' ? (Math.round(v * 1000) / 1000).toString() : typeof v === 'object' ? JSON.stringify(v) : String(v ?? '');
function argSummary(args) {
  if (!args || typeof args !== 'object') return '';
  const inner = args.args && typeof args.args === 'object' ? args.args : args;
  return Object.entries(inner).filter(([k]) => k !== 'action').slice(0, 4).map(([k, v]) => `${k}=${fmt(v).slice(0, 24)}`).join(', ');
}
function who(src) { return src.kind === 'gui' ? '<span class="chip acc">kimo chat</span>' : `<span class="chip ext">External #${src.conn}</span>`; }
function onLive(m) {
  const ev = m.ev || {}, src = m.source || { kind: 'gui' };
  if (ev.type === 'frame_rendered' && (ev.png_b64 || ev.jpg_b64)) { $('#live-preview').innerHTML = `<img alt="engine preview" src="data:image/${ev.png_b64 ? 'png' : 'jpeg'};base64,${ev.png_b64 || ev.jpg_b64}">`; return; }
  let name = ev.tool_name || ev.name, id = ev.call_id || ev.id, phase = ev.phase || (ev.type === 'tool_start' ? 'start' : ev.type === 'tool_end' ? (ev.status === 'error' ? 'error' : 'complete') : null);
  if (!name && !phase) return;
  // the chat agent reads the engine's undo history around every call to know what changed; that bookkeeping is not an edit
  if (name === 'history_list' && src.kind === 'gui') return;
  const key = `${src.kind}:${src.conn || 0}:${id ?? name}`;
  let r = S.rows.get(key);
  if (!r) { r = { key, src, name, t: m.t || Date.now(), args: ev.payload?.args || ev.args, status: 'running' }; S.rows.set(key, r); S.order.unshift(key); S.liveTotal++; r.isNew = true; }
  if (name) r.name = name;
  if (phase === 'progress') r.pct = ev.payload?.pct ?? (ev.payload?.done && ev.payload?.total ? Math.round(100 * ev.payload.done / ev.payload.total) : null);
  if (phase === 'complete') { r.status = 'ok'; r.dur = ev.payload?.dur_ms ?? ev.dur_ms; r.change = changeText(ev.payload, ev.target); r.target = ev.target; r.isNew = true; }
  if (phase === 'error') { r.status = 'error'; r.dur = ev.payload?.dur_ms; r.change = esc(ev.payload?.error || ev.error || 'error'); r.isNew = true; }
  if (S.order.length > 400) S.rows.delete(S.order.pop());
  if (S.tab !== 'live') $('#nav-live').hidden = false;
  renderLive();
}
function renderLive() {
  const rows = S.order.map(k => S.rows.get(k)).filter(r => S.filter === 'all' || r.src.kind === S.filter);
  $('#live-empty').hidden = rows.length > 0;
  $('#live-count').textContent = S.liveTotal ? `${S.liveTotal} action${S.liveTotal > 1 ? 's' : ''} this session` : '';
  const running = rows.some(r => r.status === 'running');
  $('#live-state').textContent = running ? 'agent working' : S.liveTotal ? 'idle' : 'waiting'; $('#live-state').className = 'chip ' + (running ? 'acc' : '');
  $('#live-rows').innerHTML = rows.slice(0, 200).map(r => {
    const st = r.status === 'running' ? `<span class="st-run"><span class="spin"></span> ${r.pct != null ? r.pct + '%' : ''}</span>` : r.status === 'ok' ? `<span class="st-ok">✓ ${r.dur != null ? r.dur + ' ms' : ''}</span>` : `<span class="st-err">✕</span>`;
    const tgt = r.target && (r.target.clip_id || r.target.track_id) ? `<div class="note muted mono">${esc([r.target.track_id, r.target.clip_id].filter(Boolean).join(' / '))}</div>` : '';
    return `<tr class="${r.isNew ? 'new ' : ''}${r.status === 'running' ? 'running' : ''}"><td class="mono">${clock(r.t)}</td><td>${who(r.src)}</td><td><div class="tool mono">${esc(r.name)}</div><div class="args mono note">${esc(argSummary(r.args))}</div></td><td>${st}</td><td class="chg">${r.change || ''}${tgt}</td></tr>`;
  }).join('');
  rows.forEach(r => r.isNew = false);
  const last = rows.find(r => r.status === 'ok' && r.change);
  if (last) $('#live-last').innerHTML = `${who(last.src)} <span class="mono">${esc(last.name)}</span><div style="margin-top:6px;font-size:14px">${last.change}</div>`;
  $('#home-feed').innerHTML = S.order.slice(0, 5).map(k => S.rows.get(k)).map(r => `<div class="ev"><span class="mono muted">${clock(r.t)}</span>${who(r.src)}<span class="mono">${esc(r.name)}</span><span style="margin-left:auto">${r.status === 'ok' ? '<span class="st-ok">✓</span>' : r.status === 'error' ? '<span class="st-err">✕</span>' : '<span class="spin"></span>'}</span></div>`).join('') || $('#home-feed').innerHTML;
}
$$('#live-filter button').forEach(b => b.onclick = () => { S.filter = b.dataset.f; $$('#live-filter button').forEach(x => x.classList.toggle('on', x === b)); renderLive(); });
$('#live-graph').onclick = () => B.invoke('open_spectator').catch(e => toast(String(e)));

// ---------- project ----------
$('#proj-open').onclick = async () => {
  const p = $('#proj-path').value.trim(); if (!p) return;
  $('#proj-msg').textContent = 'Opening…';
  try { await B.invoke('project_open', { path: p }); $('#proj-msg').textContent = 'Opened. Chat edits now apply to this project.'; } catch (e) { $('#proj-msg').textContent = String(e); }
};

// ---------- first-run setup ----------
async function setupStatus() {
  const s = await B.invoke('setup_status');
  S.parts = s.parts;
  const need = s.parts.filter(p => !p.installed && !p.optional), ready = need.length === 0;
  $('#h-parts').textContent = ready ? 'Ready' : `${need.length} to download`;
  $('#h-parts-m').textContent = s.parts.map(p => `${p.name.split(' (')[0]} ${p.installed ? '✓' : '·'}`).join('  ');
  $('#parts').innerHTML = s.parts.map(p => `<div class="part${p.installed ? ' done' : ''}" id="part-${p.id}">
    <div class="top"><span class="nm">${esc(p.name)}</span><span class="note muted">${esc(p.note)}${p.optional ? ' · optional' : ''}</span><span class="sz">${p.size ? fmtMB(p.size) : 'size set at release'}</span></div>
    <div class="bar"><i style="width:${p.installed ? 100 : 0}%"></i></div>
    <div class="meta"><span class="st">${p.installed ? 'installed, SHA-512 verified' : p.pinned ? 'waiting' : 'not published yet'}</span><span class="spd"></span><span class="eta"></span></div></div>`).join('');
  updateTotal();
  if (!s.done && !ready) $('#setup').classList.add('on');
}
function updateTotal() {
  const skip = $('#skip-opt').checked;
  const ps = (S.parts || []).filter(p => !(skip && p.optional));
  const tot = ps.reduce((a, p) => a + (p.size || 0), 0);
  const done = ps.reduce((a, p) => a + (p.installed ? p.size : (S.setup[p.id]?.done || 0)), 0);
  $('#tot-bar').style.width = tot ? (100 * done / tot).toFixed(1) + '%' : '0';
  const bps = Object.values(S.setup).reduce((a, x) => a + (x.phase === 'downloading' ? x.bytes_per_s : 0), 0);
  $('#tot-t').textContent = `${fmtMB(done)} of ${fmtMB(tot)}` + (bps ? ` · ${fmtMB(bps)}/s · ${fmtTime((tot - done) / bps)} left` : '');
}
function onProgress(p) {
  S.setup[p.id] = p;
  const el = document.getElementById('part-' + p.id); if (!el) return;
  if (p.phase === 'error') { el.querySelector('.st').textContent = p.error; el.querySelector('.st').style.color = 'var(--bad)'; $('#setup-err').textContent = p.error; return; }
  el.querySelector('.bar i').style.width = (p.total ? 100 * p.done / p.total : 0).toFixed(1) + '%';
  el.querySelector('.st').textContent = p.phase === 'done' ? 'installed, SHA-512 verified' : p.phase === 'verifying' ? 'checking SHA-512…' : `${fmtMB(p.done)} of ${fmtMB(p.total)}`;
  el.querySelector('.spd').textContent = p.phase === 'downloading' ? `${fmtMB(p.bytes_per_s)}/s` : '';
  el.querySelector('.eta').textContent = p.phase === 'downloading' ? `${fmtTime(p.eta_s)} left` : '';
  el.classList.toggle('done', p.phase === 'done');
  if (p.phase === 'done') { const sp = S.parts.find(x => x.id === p.id); if (sp) sp.installed = true; }
  updateTotal();
}
$('#skip-opt').onchange = updateTotal;
$('#setup-go').onclick = async () => {
  $('#setup-err').textContent = ''; $('#setup-go').disabled = true; $('#setup-pause').hidden = false; $('#setup-go').textContent = 'Downloading…';
  try {
    const r = await B.invoke('setup_start', { skipOptional: $('#skip-opt').checked });
    if (r.ready) { $('#setup').classList.remove('on'); toast('kimo is ready'); refresh(); setupStatus(); }
    else { $('#setup-go').textContent = 'Retry'; }
  } catch (e) { $('#setup-err').textContent = String(e); $('#setup-go').textContent = 'Retry'; }
  $('#setup-go').disabled = false; $('#setup-pause').hidden = true;
};
$('#setup-pause').onclick = () => { B.invoke('setup_pause'); $('#setup-go').textContent = 'Resume'; };
$('#setup-later').onclick = () => { B.invoke('setup_skip'); $('#setup').classList.remove('on'); };

// ---------- wiring ----------
B.listen('chat-event', onChat);
B.listen('live-event', onLive);
B.listen('setup-progress', onProgress);
B.listen('engine-status', () => refresh());
await refresh();
await setupStatus();
setInterval(() => { if (S.app?.engine?.state !== 'running') refresh(); }, 2000);
window.__kimo = { S, show, onLive, refresh, setupStatus };
document.body.dataset.ready = '1';
