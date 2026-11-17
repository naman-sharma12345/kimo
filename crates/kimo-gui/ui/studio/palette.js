// kimo app window: the command palette (Ctrl+K) and the keyboard shortcuts sheet (?).
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

// ================================================================================================================
// COMMAND PALETTE: every action by name, and anything else you type goes to your agent
function commands() {
  const inStudio = fn => () => { show('edit'); fn(); };
  return [
    ['Import media', 'upload', 'Ctrl+I', inStudio(() => pickMedia())],
    ['Export video', 'export', 'Ctrl+E', openExport],
    ['Save project', 'save', 'Ctrl+S', () => saveProject(false)],
    ['Save project as…', 'save', 'Ctrl+Shift+S', () => saveProject(true)],
    ['Open project', 'folder', 'Ctrl+O', () => openProject()],
    ['New project', 'plus', '', newProject],
    ['Split clip at playhead', 'scissors', 'S', inStudio(splitSel)],
    ['Delete selected clip', 'trash', 'Del', inStudio(() => delSel(false))],
    ['Ripple delete selected clip', 'trash', 'Shift+Del', inStudio(() => delSel(true))],
    ['Duplicate selected clip', 'copy', 'Ctrl+D', inStudio(dupSel)],
    ['Add text at playhead', 'type', 'T', inStudio(addText)],
    ['Add track', 'layers', '', () => call('timeline', 'add_track', {})],
    ['Add marker at playhead', 'flag', 'M', () => $('#ed-marker').click()],
    ['Undo', 'undo', 'Ctrl+Z', () => call('project', 'undo', {})],
    ['Redo', 'redo', 'Ctrl+Shift+Z', () => call('project', 'redo', {})],
    ['Play or pause', 'film', 'Space', inStudio(togglePlay)],
    ['Go to start', 'stepb', 'Home', inStudio(() => seekTo(0))],
    ['Go to end', 'stepf', 'End', inStudio(() => seekTo(P.dur))],
    ['Fit the timeline to the edit', 'expand', '\\', inStudio(() => $('#tl-fit').click())],
    ['Show or hide safe areas', 'safe', "'", inStudio(() => toggleGuide('safe'))],
    ['Show or hide the thirds grid', 'grid', 'G', inStudio(() => toggleGuide('grid'))],
    ['Full-screen viewer', 'expand', 'F', inStudio(fullView)],
    ['Open the agent', 'spark', '/', () => show('agent')],
    ['Open the Inspector', 'sliders', '', inStudio(() => sidePane('insp'))],
    ['Open History', 'history', '', inStudio(() => sidePane('hist'))],
    ['New agent conversation', 'plus', '', () => { show('agent'); $('#agent-new').click(); }],
    ...[...AGENTS, 'api'].map(id => [`Use ${actorName(id)} as the agent`, 'spark', '', () => pickAgent(id)]),
    ['Go to Home', 'home', 'Ctrl+1', () => show('home')],
    ['Go to Studio', 'film', 'Ctrl+2', () => show('edit')],
    ['Go to Activity', 'live', 'Ctrl+3', () => show('live')],
    ['Go to Connections', 'plug', 'Ctrl+4', () => show('connections')],
    ['Keyboard shortcuts', 'keyboard', '?', showKeys],
    ['About kimo', 'info', '', () => $('#about').classList.add('on')],
  ];
}
// subsequence match: every typed letter in order; earlier, word-start and consecutive hits score higher
function fuzzy(q, s) {
  q = q.toLowerCase(); const t = s.toLowerCase();
  let i = 0, score = 0, last = -2;
  for (let j = 0; j < t.length && i < q.length; j++) {
    if (t[j] !== q[i]) continue;
    score += 1 + (j === last + 1 ? 3 : 0) + (j === 0 || t[j - 1] === ' ' ? 4 : 0) - j * 0.02;
    last = j; i++;
  }
  return i === q.length ? score : -1;
}
const PAL = { items: [], at: 0 };
function openPalette(prefill = '') {
  $('#cmdk').classList.add('on');
  const q = $('#cmdk-q'); q.value = prefill; renderPalette();
  q.focus(); q.select(); // at once: keys typed right after Ctrl+K must land here, not on the editor's shortcuts
}
function closePalette() { $('#cmdk').classList.remove('on'); }
function renderPalette() {
  const q = $('#cmdk-q').value.trim();
  const cmds = commands().map(c => ({ c, s: q ? fuzzy(q, c[0]) : 0 })).filter(x => x.s >= 0);
  if (q) cmds.sort((a, b) => b.s - a.s);
  const agent = currentAgent();
  const ask = q ? [{ ask: true, label: q }] : [];
  // a sentence goes to the agent first; a short word that names an action keeps the action on top
  PAL.items = q && (q.split(/\s+/).length > 2 || !cmds.length) ? [...ask, ...cmds] : [...cmds, ...ask];
  PAL.at = 0;
  $('#cmdk-list').innerHTML = PAL.items.slice(0, 40).map((x, i) => x.ask
    ? `<button class="ci ask" role="option" data-i="${i}"><span class="alogo ${esc(agent)}">${agent === 'claude-code' ? 'C' : agent === 'codex' ? '◎' : agent === 'antigravity' ? 'A' : ICON('key')}</span><span class="cl">Ask ${esc(actorName(agent))}: <b>${esc(x.label)}</b></span><kbd>Enter</kbd></button>`
    : `<button class="ci" role="option" data-i="${i}">${ICON(x.c[1])}<span class="cl">${esc(x.c[0])}</span>${x.c[2] ? `<kbd>${esc(x.c[2])}</kbd>` : ''}</button>`).join('')
    || '<div class="cmdk-none">No action matches. Press Enter to ask your agent.</div>';
  markPalette();
}
function markPalette() {
  $$('#cmdk-list .ci').forEach((b, i) => { b.classList.toggle('at', i === PAL.at); b.setAttribute('aria-selected', i === PAL.at); });
  $('#cmdk-list .ci.at')?.scrollIntoView({ block: 'nearest' });
}
function runPalette(i) {
  const x = PAL.items[i]; closePalette();
  if (!x) { const q = $('#cmdk-q').value.trim(); if (q) { show('agent'); sendAgent(q); } return; }
  if (x.ask) { show('agent'); sendAgent(x.label); return; }
  x.c[3]();
}
$('#cmdk-q').oninput = renderPalette;
$('#cmdk-q').onkeydown = e => {
  const n = Math.min(PAL.items.length, 40);
  if (e.key === 'ArrowDown') { e.preventDefault(); PAL.at = n ? (PAL.at + 1) % n : 0; markPalette(); }
  else if (e.key === 'ArrowUp') { e.preventDefault(); PAL.at = n ? (PAL.at - 1 + n) % n : 0; markPalette(); }
  else if (e.key === 'Enter') { e.preventDefault(); runPalette(PAL.at); }
  else if (e.key === 'Escape') { e.preventDefault(); closePalette(); }
};
$('#cmdk-list').onclick = e => { const b = e.target.closest('.ci'); if (b) runPalette(+b.dataset.i); };
$('#cmdk-list').onmousemove = e => { const b = e.target.closest('.ci'); if (b && +b.dataset.i !== PAL.at) { PAL.at = +b.dataset.i; markPalette(); } };
$('#cmdk-open').onclick = () => openPalette();

// ================================================================================================================
// KEYBOARD SHORTCUTS SHEET
const KEYS = [
  ['Anywhere', [['Ctrl K', 'Search actions or ask your agent'], ['Ctrl 1–4', 'Home, Studio, Activity, Connections'], ['Ctrl S', 'Save'], ['Ctrl Shift S', 'Save as'], ['Ctrl O', 'Open a project'], ['Ctrl I', 'Import media'], ['Ctrl E', 'Export'], ['Ctrl Z', 'Undo'], ['Ctrl Shift Z', 'Redo'], ['?', 'This sheet']]],
  ['Playback', [['Space', 'Play or pause'], ['L', 'Play'], ['K', 'Pause'], ['J', 'Back one second'], ['← →', 'One frame'], ['Shift ← →', 'One second'], ['Home End', 'Start, end']]],
  ['Editing', [['S', 'Split at playhead'], ['Del', 'Delete clip'], ['Shift Del', 'Ripple delete'], ['Ctrl D', 'Duplicate'], ['T', 'Add text'], ['M', 'Add marker'], ['Esc', 'Deselect']]],
  ['View', [['+ −', 'Zoom the timeline'], ['\\', 'Fit the edit'], ["'", 'Safe areas'], ['G', 'Thirds grid'], ['F', 'Full-screen viewer'], ['/', 'Talk to the agent']]],
];
function showKeys() {
  $('#keys-grid').innerHTML = KEYS.map(([g, ks]) => `<div class="kg"><h3>${esc(g)}</h3>${ks.map(([k, d]) => `<div class="kr"><span>${esc(d)}</span><span class="kk">${k.split(' ').map(x => `<kbd>${esc(x)}</kbd>`).join('')}</span></div>`).join('')}</div>`).join('');
  $('#keys').classList.add('on');
}
$('#keys-close').onclick = () => $('#keys').classList.remove('on');
$('#tb-keys').onclick = showKeys;
