// kimo app window: home, project files, export, first run, context menu and keyboard.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

// ================================================================================================================
// HOME
// Starters for the prompt: each fills the box (you can edit it) and says what the agent will do.
const HOME_IDEAS = [
  ['Cut the silences', 'Cut the silences and long pauses, and close the gaps.'],
  ['Add captions', 'Add word-by-word captions in a clean, readable style.'],
  ['Make a 9:16 short', 'Make a 9:16 vertical short from the best 30 seconds, with captions.'],
  ['Cut to the beat', 'Cut the footage to the beat of the music.'],
  ['Match the colour', 'Match the colour of every shot to the first one.'],
  ['Clean up the audio', 'Clean up the audio: remove noise and even out the loudness.'],
];
const projName = p => base(p).replace(/\.(kimo|json)$/, '');
const projDir = p => String(p).replace(/[\\/][^\\/]*$/, '');
const projHue = p => { let h = 0; for (const ch of String(p)) h = (h * 31 + ch.charCodeAt(0)) >>> 0; return h % 360; };
function renderRecent() {
  const r = S.app?.settings?.recent || [], cur = S.app?.project_path || null;
  const hr = new Date().getHours();
  $('#home-greet').textContent = hr < 5 ? 'Working late' : hr < 12 ? 'Good morning' : hr < 18 ? 'Good afternoon' : 'Good evening';
  if (S.app?.version) $('#home-news-t').textContent = `What’s new in kimo ${kimoRelease(S.app)}`;
  // continue: the most recent project, or (first run) what kimo does
  const c = $('#home-continue');
  if (r.length) {
    const p = r[0], open = cur && normPath(cur) === normPath(p);
    c.className = 'cont'; c.style.removeProperty('--h');
    c.innerHTML = `<div class="c-poster" style="--h:${projHue(p)}" aria-hidden="true"><b>${esc(projName(p).slice(0, 2))}</b><span class="c-ph"></span><div class="c-strip"><i></i><i></i><i></i></div></div>
      <div class="c-body"><span class="c-k">${open ? 'Open now' : 'Last opened'}</span><span class="c-n" title="${esc(p)}">${esc(projName(p))}</span><span class="c-p" title="${esc(p)}">${esc(projDir(p))}</span>
      <div class="c-act"><button class="btn acc" id="home-resume">${ICON(open ? 'film' : 'arrow')}${open ? 'Back to the Studio' : 'Continue editing'}</button></div></div>`;
    $('#home-resume').onclick = () => open ? show('edit') : openProject(p);
  } else {
    c.className = 'cont empty-first';
    c.innerHTML = `<span class="c-k">How it works</span><p class="c-lead">Edit by hand, or say what you want. Your agent works on the same timeline as you, every change shows up as it happens, and every change can be undone.</p>`;
    c.appendChild($('#home-reel').content.cloneNode(true));
  }
  // recent: everything after the one above
  const rest = r.slice(1, 9);
  $('#home-recent').innerHTML = rest.length
    ? rest.map((p, i) => { const open = cur && normPath(cur) === normPath(p); return `<a class="pcard${open ? ' cur' : ''}" data-open="${esc(p)}" title="${esc(p)}" tabindex="0"><span class="poster" style="--h:${projHue(p)}"><b>${esc(projName(p).slice(0, 2))}</b></span><span class="pn">${esc(projName(p))}</span><span class="pp">${esc(projDir(p))}</span><span class="pt">${open ? 'Open now' : ''}</span></a>`; }).join('')
    : `<div class="recent-empty">${r.length ? 'Projects you open or save show up here.' : 'No projects yet. Drop footage above or describe a video to start your first one.'}</div>`;
  icons(c); icons($('#home-recent'));
  $$('#home-recent [data-open]').forEach(a => { a.onclick = () => openProject(a.dataset.open); a.onkeydown = e => { if (e.key === 'Enter') a.click(); }; });
  renderAskWho();
}
function renderAskWho() {
  const who = $('#home-ask-who'); if (!who) return;
  const a = currentAgent();
  who.innerHTML = `with <b>${esc(actorName(a))}</b> · <a id="home-ask-change">change</a>`;
  $('#home-ask-change').onclick = () => show('agent');
}
$('#home-chips').innerHTML = HOME_IDEAS.map(([t, say]) => `<button type="button" class="chip" role="listitem" data-say="${esc(say)}">${esc(t)}</button>`).join('');
$('#home-chips').onclick = e => { const b = e.target.closest('.chip'); if (!b) return; const t = $('#home-ask-t'); t.value = b.dataset.say; t.focus(); t.setSelectionRange(t.value.length, t.value.length); };
$('#home-ask-t').addEventListener('keydown', e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); $('#home-ask').requestSubmit(); } });
$('#home-ask-t').addEventListener('input', e => { const t = e.target; t.style.height = 'auto'; t.style.height = Math.min(180, t.scrollHeight) + 'px'; });
$('#home-ask').onsubmit = async e => {
  e.preventDefault();
  const t = $('#home-ask-t'), text = t.value.trim();
  if (!text) { t.focus(); return toast('Describe the video you want, or pick an idea.'); }
  if (S.app?.engine?.state !== 'running') return toast('The engine is still starting. Try again in a moment.', 'bad');
  t.value = ''; t.style.height = '';
  show('agent');
  await sendAgent(text);
};
$('#home-import').onclick = async () => { const p = await B.invoke('pick', { kind: 'media' }).catch(e => { toast(String(e), 'bad'); return []; }); if (p?.length) { show('edit'); importPaths(p); } };
$('#home-new2').onclick = async () => { await newProject(); show('edit'); };
$('#home-cmdk').onclick = () => openPalette();
// dropping files on the home page highlights the drop tile (the window-wide drop itself is handled in main.js)
for (const ev of ['dragenter', 'dragover']) $('#tab-home').addEventListener(ev, () => $('#home-import').classList.add('hot'));
for (const ev of ['dragleave', 'drop']) $('#tab-home').addEventListener(ev, () => $('#home-import').classList.remove('hot'));

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
$('#ed-open').onclick = () => openProject(); $('#home-open').onclick = () => openProject(); $('#home-open2').onclick = () => openProject();
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
  EX.mode = r.path_used?.mode; EX.t0 = Date.now();
  $('#ex-prog').hidden = false; $('#ex-bar').style.width = '0%'; $('#ex-status').textContent = EX_MODE[EX.mode]?.[0] || 'Rendering…';
  clearInterval(EX.timer);
  EX.timer = setInterval(pollExport, 700);
};
// what the export is doing: cut-only edits skip the renderer (export_fast in kimo-core)
const EX_MODE = { stream_copy: ['Joining clips without re-encoding…', 'copied, no quality loss'], direct_encode: ['Joining and encoding clips in one pass…', 'one-pass encode'] };
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
    if (s.state === 'done') { $('#ex-bar').style.width = '100%'; $('#ex-status').textContent = `Done in ${((Date.now() - EX.t0) / 1000).toFixed(1)} s · ${base(s.result?.path || EX.path)}${EX_MODE[EX.mode] ? ' · ' + EX_MODE[EX.mode][1] : ''}`; $('#ex-reveal').hidden = false; toast('Export finished'); }
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
  $('#h-parts').innerHTML = s.ready ? (s.parts.some(p => p.satisfied === 'system') ? "Ready · using this computer's FFmpeg" : 'Ready')
    : '<span style="color:var(--bad)">FFmpeg missing</span> · <a class="h-fix" id="h-parts-fix">Set it up</a>';
  $('#h-parts-fix')?.addEventListener('click', () => $('#setup').classList.add('on'));
  $('#parts').innerHTML = s.parts.map(p => `<div class="part${p.satisfied ? ' done' : ''}" data-part="${p.id}"><span class="pic">${ICON(PART_IC[p.id] || 'film')}</span>
    <div class="top"><span class="nm">${esc(p.name)}</span><span class="note muted">${esc(p.note)}${p.optional ? ' · optional' : ''}</span><span class="sz">${p.satisfied === 'system' ? 'on this computer' : p.satisfied ? 'ready' : p.pinned ? (p.size / 1e6).toFixed(0) + ' MB' : 'not available'}</span></div>
    <div class="bar"><i style="width:${p.satisfied ? 100 : 0}%"></i></div><div class="meta"><span class="st">${p.satisfied === 'system' ? 'Using the copy already on this computer' : p.satisfied ? 'Installed' : p.pinned ? 'Will download' : 'Not published for download yet — install FFmpeg yourself'}</span><span class="spd"></span></div></div>`).join('');
  const need = s.parts.filter(p => !p.satisfied && p.pinned && !(p.optional && $('#skip-opt').checked)).reduce((n, p) => n + p.size, 0);
  $('#tot-t').textContent = need ? `${(need / 1e6).toFixed(0)} MB to download` : 'Nothing to download';
  if (!s.ready && !s.done) $('#setup').classList.add('on');
  return s;
}
function onProgress(p) {
  const el = $(`.part[data-part="${p.id}"]`); if (!el) return;
  if (p.phase === 'system') { el.classList.add('done'); el.querySelector('.st').textContent = 'Using the copy already on this computer'; el.querySelector('.bar i').style.width = '100%'; return; }
  if (p.phase === 'error' || p.phase === 'unavailable') { el.querySelector('.st').textContent = p.error || 'Not available'; return; }
  if (p.total) el.querySelector('.bar i').style.width = (100 * p.done / p.total).toFixed(1) + '%';
  el.querySelector('.spd').textContent = p.bytes_per_s ? (p.bytes_per_s / 1e6).toFixed(1) + ' MB/s' : '';
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
$('#skip-opt').onchange = () => setupStatus();
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
  const typing = e.target.closest?.('input,textarea,select,[contenteditable]');
  const mod = e.ctrlKey || e.metaKey;
  if (mod && e.key.toLowerCase() === 'k') { e.preventDefault(); if ($('#cmdk').classList.contains('on')) closePalette(); else openPalette(); return; }
  if (e.key === 'Escape') {
    // the first Escape closes an open dialog or menu; with none open it clears the selection on the Edit tab
    const layer = $$('.overlay.on').length > 0 || !$('#ctx').hidden;
    $$('.overlay.on').forEach(o => { if (!['setup'].includes(o.id)) o.classList.remove('on'); }); $('#ctx').hidden = true;
    if (!layer && !typing && S.tab === 'edit') select(null);
    return;
  }
  if (mod && /^[1-4]$/.test(e.key)) { e.preventDefault(); show(['home', 'edit', 'live', 'connections'][+e.key - 1]); return; }
  if (mod && e.key.toLowerCase() === 's') { e.preventDefault(); saveProject(e.shiftKey); return; }
  if (mod && e.key.toLowerCase() === 'o') { e.preventDefault(); openProject(); return; }
  if (mod && e.key.toLowerCase() === 'e') { e.preventDefault(); openExport(); return; }
  if (mod && e.key.toLowerCase() === 'i') { e.preventDefault(); pickMedia(); return; }
  if (typing) return;
  if ($('.overlay.on')) return; // a dialog is open: no editing shortcuts behind it
  if (e.key === '?') { e.preventDefault(); showKeys(); return; }
  if (e.key === '/' && !mod) { e.preventDefault(); show('agent'); return; }
  if (mod && e.key.toLowerCase() === 'z') { e.preventDefault(); call('project', e.shiftKey ? 'redo' : 'undo', {}); return; }
  if (mod && e.key.toLowerCase() === 'y') { e.preventDefault(); call('project', 'redo', {}); return; }
  if (S.tab !== 'edit') return;
  if (mod && e.key.toLowerCase() === 'd') { e.preventDefault(); dupSel(); return; }
  if (mod && e.shiftKey && e.key.toLowerCase() === 'x') { e.preventDefault(); setLoop('clear'); return; }
  if (mod && e.key.toLowerCase() === 'l') { e.preventDefault(); setLoop('toggle'); return; }
  if (mod) return;
  const k = e.key, kl = k.length === 1 ? k.toLowerCase() : k;
  // J / K / L shuttle (K held + J / L steps a frame), I / O set the loop's in and out
  if (kl === 'k') { kHeld = true; if (!e.repeat) shuttle('k'); return; }
  if (kl === 'j' || kl === 'l') { e.preventDefault(); if (kHeld) stepFrame(kl === 'l' ? 1 : -1); else if (!e.repeat) shuttle(kl); return; }
  if (kl === 'i' && !e.shiftKey) return setLoop('in');
  if (kl === 'o' && !e.shiftKey) return setLoop('out');
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
  else if (k === 'f' || k === 'F') fullView();
  else if (k === 'g' || k === 'G') toggleGuide('grid');
  else if (k === "'") toggleGuide('safe');
  else if (k === '\\') $('#tl-fit').click();
});

let kHeld = false;
addEventListener('keyup', e => { if (e.key === 'k' || e.key === 'K') kHeld = false; });
addEventListener('blur', () => { kHeld = false; });
