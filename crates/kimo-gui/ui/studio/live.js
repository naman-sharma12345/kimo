// kimo app window: live engine-call feed.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
  $('#live-state').className = 'chip' + (running ? ' run' : '');
  const tb = $('#tb-live'); tb.classList.toggle('busy', running > 0); $('#tb-live-t').textContent = running ? `${running} running` : S.live.size ? 'Live' : 'Idle';
  $('#live-rows').innerHTML = rows.map(r => `<tr class="${r.phase === 'start' ? 'running' : ''}${r.isNew ? ' new' : ''}"><td class="mono">${clock(r.t)}</td>
    <td>${r.external ? `<span class="chip a-ext">${esc(r.actor)}</span>` : actorChip(r.actor)}</td>
    <td><div class="tool">${esc(r.tool)}${r.action ? '.' + esc(r.action) : ''}</div><div class="args mono">${esc(r.summary || '')}</div></td>
    <td>${r.phase === 'start' ? '<span class="chip run">running</span>' : r.ok ? `<span class="chip ok">${r.ms != null ? r.ms + ' ms' : 'done'}</span>` : '<span class="chip bad">error</span>'}</td>
    <td class="chg">${r.counts?.error ? `<span class="chip f-error">${r.counts.error} error${r.counts.error > 1 ? 's' : ''}</span> ` : ''}${r.counts?.warning ? `<span class="chip f-warning">${r.counts.warning} warning${r.counts.warning > 1 ? 's' : ''}</span> ` : ''}${r.change ? `<b>change #${r.change.id}</b> ` : ''}<span class="note mono">${esc((r.result || '').slice(0, 140))}</span></td></tr>`).join('');
  rows.forEach(r => r.isNew = false);
  const last = [...S.live.values()].reverse().find(r => r.change);
  $('#live-last').innerHTML = last ? `${actorChip(last.actor)} <b>${esc(last.action)}</b><div class="mono note" style="margin-top:6px">${esc(last.summary)}</div>` : '—';
}
$('#live-clear').onclick = () => { S.live.clear(); S.liveOrder = []; renderLive(); };
$('#live-graph').onclick = () => B.invoke('open_spectator').catch(e => toast(String(e), 'bad'));
