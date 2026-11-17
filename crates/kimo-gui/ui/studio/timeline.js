// kimo app window: timeline.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
    if (big) r += `<span class="tl" style="left:${t2x(t)}px">${rulerLabel(t, step)}</span>`;
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
    const th = kind === 'media' && P.thumbs.get(P.proj?.assets?.[assetOf(c)]?.path);
    return `<div class="clip${c.id === S.sel ? ' sel' : ''}${P.fresh?.has(c.id) ? ' fresh' : ''}" data-clip="${esc(c.id)}" data-kind="${esc(kind)}" style="left:${t2x(c.start)}px;width:${w}px" title="${esc(clipLabel(c))} · ${tc(c.start)}–${tc(clipEnd(c))}">
      ${c.transition_in ? `<span class="tr" style="width:${Math.min(w, t2x(c.transition_in.duration || 0.5))}px"></span>` : ''}
      ${th ? `<span class="film" style="background-image:url(${th})"></span>` : ''}<span class="h l"></span><span class="cl">${esc(clipLabel(c))}</span>${w > 70 ? `<span class="cs">${tc(clipEnd(c) - c.start)}${c.speed && c.speed !== 1 ? ' · ' + c.speed + '×' : ''}</span>` : ''}${fx ? `<span class="fx">fx${fx > 1 ? fx : ''}</span>` : ''}<span class="h r"></span></div>`;
  }).join('')}</div>`).join('');
  P.fresh = new Set();
  placePlayhead();
}
// ruler labels: minutes:seconds, or seconds:frames when zoomed in past one second
function rulerLabel(t, step) {
  const p2 = n => String(n).padStart(2, '0'), fps = Math.round(P.fps || 30), f = Math.round(t * fps);
  return step >= 1 ? `${p2(Math.floor(f / (fps * 60)))}:${p2(Math.floor(f / fps) % 60)}` : `${p2(Math.floor(f / fps) % 60)}:${p2(f % fps)}`;
}
let tlRaf = 0;
function tlSoon() { if (!tlRaf) tlRaf = requestAnimationFrame(() => { tlRaf = 0; renderTimeline(); }); }
function placePlayhead() {
  $('#tl-playhead').style.left = t2x(V.t) + 'px';
  $('#tl-tc').textContent = tcf(V.t);
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
    $('#tl-tc').textContent = mode === 'move' ? tcf(ns) : `${tcf(ns)} → ${tcf(ne)}`;
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
  // picking a clip opens the Inspector, unless you are typing to the agent
  if (id && S.tab === 'edit' && document.activeElement !== $('#chat-in')) sidePane('insp');
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
