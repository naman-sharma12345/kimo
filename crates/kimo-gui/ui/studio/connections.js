// kimo app window: connections (agent CLIs, API keys, MCP config).
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
