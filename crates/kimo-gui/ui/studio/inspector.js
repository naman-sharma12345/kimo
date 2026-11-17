// kimo app window: inspector.
// Classic script sharing one global scope with the other studio/*.js files (loaded in order by studio.html);
// studio/main.js runs last and wires the bridge events and the start-up sequence.
"use strict";

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
    enhanceInspector(body);
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
  enhanceInspector(body);
  body.querySelectorAll('[data-rmfx]').forEach(b => b.onclick = () => call('effects', 'remove_effect', { clip_id: id, effect_id: b.dataset.rmfx }));
  $('#i-split').onclick = splitSel; $('#i-dup').onclick = dupSel; $('#i-del').onclick = () => delSel(false);
  $('#insp-ask').onsubmit = e => { e.preventDefault(); const v = e.target.querySelector('input').value.trim(); if (v) { sendAgent(`For clip ${id} (“${clipLabel(c)}”): ${v}`); e.target.querySelector('input').value = ''; } };
}

// sections fold (remembered while the app is open); number fields change by dragging their label, like other editors:
// drag left or right, Shift for 10x, Alt for fine steps; a click without dragging just focuses the field
S.closedSecs = new Set();
function enhanceInspector(body) {
  body.querySelectorAll('.isec').forEach(sec => {
    const h = sec.querySelector(':scope > .h'); if (!h) return;
    const key = h.textContent.trim();
    sec.classList.toggle('closed', S.closedSecs.has(key));
    h.tabIndex = 0; h.setAttribute('role', 'button'); h.setAttribute('aria-expanded', !S.closedSecs.has(key));
    const flip = () => { const c = !sec.classList.contains('closed'); sec.classList.toggle('closed', c); h.setAttribute('aria-expanded', !c); if (c) S.closedSecs.add(key); else S.closedSecs.delete(key); };
    h.onclick = flip; h.onkeydown = e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); flip(); } };
  });
  body.querySelectorAll('.ifield').forEach(l => {
    const inp = l.querySelector('input[type=number]'), sp = l.firstElementChild;
    if (!inp || !sp || sp === inp) return;
    sp.classList.add('scrub'); sp.title = 'Drag left or right to change (Shift: faster, Alt: finer)';
    sp.onpointerdown = e => {
      if (e.button !== 0) return;
      e.preventDefault();
      const x0 = e.clientX, v0 = +inp.value || 0, st = +inp.step || 1;
      let moved = false;
      sp.setPointerCapture(e.pointerId); l.classList.add('scrubbing');
      sp.onpointermove = ev => {
        const dx = ev.clientX - x0; if (!moved && Math.abs(dx) < 3) return;
        moved = true;
        let v = v0 + Math.round(dx / 3) * st * (ev.shiftKey ? 10 : ev.altKey ? 0.1 : 1);
        if (inp.min !== '') v = Math.max(+inp.min, v);
        if (inp.max !== '') v = Math.min(+inp.max, v);
        inp.value = num(v);
      };
      sp.onpointerup = () => {
        sp.onpointermove = sp.onpointerup = null; l.classList.remove('scrubbing');
        if (moved) inp.dispatchEvent(new Event('change')); else { inp.focus(); inp.select(); }
      };
    };
  });
}
