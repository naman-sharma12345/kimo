// kimo app window: media bin.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
    <div class="th">${P.thumbs.get(a.path) ? `<img src="${esc(P.thumbs.get(a.path))}" alt="">` : ICON(assetIcon(a))}<span class="dur">${tc(a.duration)}</span>${a.used ? `<span class="used">${a.used}×</span>` : ''}${proxyBadge(a.id)}
      <button class="add" title="Add to timeline at the playhead" aria-label="Add to timeline">${ICON('plus')}</button></div>
    <div class="nm">${esc(base(a.path))}</div><div class="mt">${a.width ? `${a.width}×${a.height}` : ''}${a.fps ? ` · ${Math.round(a.fps * 100) / 100}fps` : ''}</div></div>`).join('') || '<div class="bin-empty">No match.</div>';
  for (const a of list) if (!P.thumbs.has(a.path)) {
    P.thumbs.set(a.path, null);
    B.invoke('media_thumb', { path: a.path }).then(u => { if (u) { P.thumbs.set(a.path, u); const img = $(`.asset[data-id="${a.id}"] .th`); if (img && !img.querySelector('img')) { img.querySelector('svg')?.remove(); img.insertAdjacentHTML('afterbegin', `<img src="${esc(u)}" alt="">`); } tlSoon(); } }).catch(() => {});
  }
}
// Background proxies (engine: kimo_core::auto_proxy). Heavy imports (4K, long, long-GOP phone footage) get a smaller
// edit proxy for previews and an analysis proxy, built in the background; the engine streams `{type:'proxy'}` events
// and `project.proxy_jobs` lists them. The badge says where each asset stands; export always uses the original.
const PROXY_LABEL = { queued: 'Proxy queued', checking: 'Proxy queued', building: 'Proxy', ready: 'Proxy', unsupported: 'Original', failed: 'Proxy failed' };
function proxyTitle(j) {
  const why = (j.reasons || []).map(r => ({ '4k': '4K or larger', long: 'long file', long_gop: 'sparse keyframes', forced: 'always on' }[r] || r)).join(', ');
  if (j.state === 'ready') return `Previews use a lighter copy${why ? ` (${why})` : ''}. Export always uses the original.${j.note ? ' ' + j.note + '.' : ''}`;
  if (j.state === 'building') return `Building a lighter copy for smooth previews (${j.stage === 'analysis' ? 'analysis copy' : 'preview copy'}). You can keep editing; export always uses the original.`;
  if (j.state === 'unsupported') return `Previews use the original: ${j.note || 'no proxy for this file'}.`;
  if (j.state === 'failed') return `Previews use the original. ${j.error || 'The proxy could not be built.'}`;
  return `Waiting to build a lighter copy${why ? ` (${why})` : ''}.`;
}
function proxyBadge(id) {
  const j = P.proxy.get(id);
  if (!j || j.state === 'skipped' || ((j.state === 'queued' || j.state === 'checking') && !(j.reasons || []).length)) return '';
  const pct = Math.round((j.progress || 0) * 100);
  const text = j.state === 'building' ? `${PROXY_LABEL.building} ${pct}%` : PROXY_LABEL[j.state] || '';
  const icon = j.state === 'ready' ? ICON('check') : '';
  const bar = j.state === 'building' ? `<i class="px-bar" style="width:${pct}%"></i>` : '';
  return `<span class="px" data-state="${esc(j.state)}" title="${esc(proxyTitle(j))}" role="status" aria-label="${esc(text + (j.state === 'ready' ? ' ready' : ''))}">${icon}${esc(text)}</span>${bar}`;
}
function paintProxy(id) {
  const th = $(`.asset[data-id="${CSS.escape(id)}"] .th`); if (!th) return;
  th.querySelectorAll('.px, .px-bar').forEach(e => e.remove());
  th.insertAdjacentHTML('beforeend', proxyBadge(id));
}
// an event's source path is resolved; a project may store it relative, so compare the tail, slashes and case folded
const normPath = s => String(s || '').replace(/\\/g, '/').toLowerCase();
function proxyMatches(a, j) { const ap = normPath(a.path).replace(/^\.?\//, ''), jp = normPath(j.path); return !!ap && (jp === ap || jp.endsWith('/' + ap)); }
function onProxyJob(j) {
  for (const id of j.asset_ids || []) {
    const a = P.assets.find(x => x.id === id);
    if (!a || !proxyMatches(a, j)) continue; // another project's asset with the same id
    const was = P.proxy.get(id)?.state;
    P.proxy.set(id, j); paintProxy(id);
    if (j.state === 'ready' && was && was !== 'ready' && j.edit_proxy) toast(`Smooth previews ready for ${base(a.path)}`);
  }
}
async function loadProxyJobs(my) {
  const r = await read('project', 'proxy_jobs');
  if (my !== loadSeq || !r || !Array.isArray(r.jobs)) return;
  P.proxy.clear();
  for (const j of r.jobs) for (const id of j.asset_ids || []) P.proxy.set(id, j);
  P.assets.forEach(a => paintProxy(a.id));
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
