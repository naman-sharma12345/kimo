// kimo app window: changes and the agent chat.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
    <span class="alogo ${id}">${id === 'claude-code' ? 'C' : id === 'codex' ? '◎' : id === 'antigravity' ? 'A' : ICON('key')}</span><span class="an">${esc(actorName(id))}</span><span class="st ${stClass(id)}"></span></button>`).join('');
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
  const tabBtn = $('.side-tabs [data-pane=agent]'); if (tabBtn) tabBtn.title = `${actorName(id)}${ml ? ' using ' + ml : ''}${effortOf(id) ? ', ' + effortOf(id) + ' effort' : ''}`;
  $('#chat-in').placeholder = `Tell ${actorName(id)} what to change…`;
}
function addMsg(cls, html) { const d = document.createElement('div'); d.className = 'msg ' + cls; d.innerHTML = html; $('#msgs').appendChild(d); scrollMsgs(); return d; }
function scrollMsgs() { const m = $('#msgs'); if (m.scrollHeight - m.scrollTop - m.clientHeight < 160) m.scrollTop = 1e9; }
const TURN = { el: null, blocks: null, steps: new Map(), status: null, started: 0, agent: null };
const agentVisible = () => S.tab === 'edit' && S.pane === 'agent';
function setBusy(b) {
  S.busy = b;
  $('#chat-stop').hidden = !b; $('#chat-send').disabled = b;
  $('#nav-agent').hidden = !b || S.tab === 'edit';
  $('#side-agent-dot').hidden = !b || S.pane === 'agent';
  $('#side').classList.toggle('busy', b);
  askStatus(b ? S.agentStatus : '');
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
  if (S.tab === 'edit') sidePane('agent');
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
function askStatus(t) { S.agentStatus = t || ''; const el = $('#cmdk-open .ph'); if (el) el.textContent = S.busy && t ? t : 'Search actions or ask your agent'; }
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
  if (!agentVisible()) toast(`${actorName(TURN.agent)}: ${last.slice(0, 90)}`);
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
  if (!agentVisible()) toast(msg.slice(0, 120), 'bad');
  TURN.el = null;
}
$('#chat-send').onclick = () => { const t = $('#chat-in').value; $('#chat-in').value = ''; autoGrow(); sendAgent(t); };
$('#chat-stop').onclick = () => { B.invoke('agent_stop'); setStatus('Stopping…'); };
const autoGrow = () => { const t = $('#chat-in'); t.style.height = 'auto'; t.style.height = Math.min(160, t.scrollHeight) + 'px'; };
$('#chat-in').addEventListener('input', autoGrow);
$('#chat-in').addEventListener('keydown', e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('#chat-send').click(); } });
$('#sugg').addEventListener('click', e => { const b = e.target.closest('button'); if (b) sendAgent(b.textContent); });
$('#agent-new').onclick = async () => {
  if (S.busy) await B.invoke('agent_stop');
  await B.invoke('agent_reset'); S.turn++;
  $('#msgs').innerHTML = '<div class="msg agent hello" id="chat-hello"><b>New conversation.</b> Your project stays as it is; the agent starts fresh.</div>';
};
