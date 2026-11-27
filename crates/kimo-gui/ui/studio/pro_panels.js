// kimo app window: the pro panels in the Studio's side panel.
//   Effects  Effect Controls: every effect kimo has (searchable), each parameter live, a keyframe at the playhead per parameter
//   Graph    the value and speed graph of a keyframed property; drag keys (time and value), double-click to add, Delete to remove, ease
//   Mixer    a strip per track: fader, balance, mute, solo, insert effects; the master loudness against a delivery target
//   Colour   primary correction (exposure, contrast, saturation, temperature, tint, lift, gamma, gain), auto grade, match to a shot
//   Keys     every editing shortcut, rebindable (kept on this computer)
// Classic script sharing one global scope with the other studio/*.js files; loaded after palette.js, before main.js.
"use strict";

const PRO = { cat: null, catP: null, graph: { prop: null, effect: null, mode: 'value', sel: -1, data: null }, loud: null, target: 'youtube' };
const PANES = [['fx', 'wand', 'Effects'], ['graph', 'graph', 'Graph'], ['mix', 'audio', 'Mixer'], ['color', 'sliders', 'Colour'], ['keys', 'keyboard', 'Keys']];

// ---------- the tabs ----------
(function addTabs() {
  const tabs = $('.side-tabs'), side = $('#side');
  tabs.classList.add('many');
  for (const [id, icon, label] of PANES) {
    const b = document.createElement('button'); b.setAttribute('role', 'tab'); b.dataset.pane = id; b.setAttribute('aria-selected', 'false'); b.title = label;
    b.innerHTML = `${ICON(icon)}<span class="tl">${label}</span>`; b.onclick = () => { sidePane(id); renderPro(id); }; tabs.appendChild(b);
    const p = document.createElement('div'); p.className = 'pane pro-pane'; p.id = 'pane-' + id; p.setAttribute('role', 'tabpanel');
    p.innerHTML = `<div class="pane-h"><h3 id="${id}-title">${label}</h3></div><div class="pro-body" id="${id}-body"></div>`; side.appendChild(p);
  }
})();
// redraw the open pro pane whenever the inspector redraws (project reload, selection change)
const _renderInspector = renderInspector;
renderInspector = function () { _renderInspector(); if (PANES.some(p => p[0] === S.pane)) renderPro(S.pane); };
// the Effects and Colour panes show animated values at the playhead: follow it (one redraw per frame at most)
let proSeekRaf = 0; const _seekTo = seekTo;
seekTo = function (...a) { const r = _seekTo.apply(this, a); if ((S.pane === 'fx' || S.pane === 'color') && !proSeekRaf) proSeekRaf = setTimeout(() => { proSeekRaf = 0; renderPro(S.pane); }, 30); return r; };
function renderPro(id) { ({ fx: renderFx, graph: renderGraph, mix: renderMix, color: renderColor, keys: renderKeys })[id]?.(); }

const selClip = () => S.sel && P.clips.get(S.sel);
const localT = c => Math.max(0, Math.min(c.duration, V.t - c.start));
// a property's value at clip-local time t: its keys interpolated (an approximation of the engine's easing, for display), else its base
function pval(p, def, t = 0) {
  if (!p || typeof p !== 'object') return p ?? def;
  const k = p.keys || []; if (!k.length) return p.base ?? def; if (t <= k[0].t) return k[0].v; if (t >= k.at(-1).t) return k.at(-1).v;
  for (let i = 0; i < k.length - 1; i++) if (t <= k[i + 1].t) { if (k[i].ease === 'hold') return k[i].v; const u = (t - k[i].t) / Math.max(1e-9, k[i + 1].t - k[i].t); return k[i].v + (k[i + 1].v - k[i].v) * u; }
  return k.at(-1).v;
}
const fmt = (v, d = 3) => v == null || !isFinite(v) ? '' : String(Math.round(v * 10 ** d) / 10 ** d);
function noClip(body, what) { body.innerHTML = `<div class="insp-empty">Select a clip on the timeline to ${what}.</div>`; }
async function catalog() {
  if (PRO.cat) return PRO.cat;
  if (!PRO.catP) PRO.catP = read('effects', 'list_effects').then(r => { PRO.cat = Array.isArray(r) ? r : []; return PRO.cat; });
  return PRO.catP;
}
// a slider range from a parameter's description ("0..1", "-1..1", "degrees"), else around its default
function range(p) {
  const m = /(-?\d+(?:\.\d+)?)\s*\.\.\s*(-?\d+(?:\.\d+)?)/.exec(p.description || '');
  if (m) return [+m[1], +m[2]];
  if (/degree/i.test(p.description || '')) return [-180, 180];
  const d = p.default || 0; return d === 0 ? [-1, 1] : d > 0 ? [0, d * 4] : [d * 4, -d * 4];
}

// ================================================================================================================
// EFFECTS (Effect Controls)
async function renderFx() {
  const body = $('#fx-body'), c = selClip();
  if (!c) { $('#fx-title').textContent = 'Effects'; return noClip(body, 'see and edit its effects'); }
  $('#fx-title').innerHTML = `Effects <span class="note muted">${esc(clipLabel(c)).slice(0, 28)}</span>`;
  const cat = await catalog(); const byKind = new Map(cat.map(e => [e.kind, e]));
  const fxs = (c.effects || []).filter(e => !String(e.kind).startsWith('shape_'));
  if (document.activeElement && body.contains(document.activeElement) && document.activeElement.matches('input[type=number]')) return; // don't clobber typing
  body.innerHTML = `<div class="fx-add"><input id="fx-q" list="fx-kinds" placeholder="Add an effect… (${cat.length} to choose from)" autocomplete="off"><datalist id="fx-kinds">${cat.map(e => `<option value="${esc(e.kind)}">${esc((e.description || '').slice(0, 80))}</option>`).join('')}</datalist><button class="btn sm acc" id="fx-addb">${ICON('plus')}Add</button></div>
    ${fxs.length ? '' : '<div class="note muted" style="padding:10px 2px">No effects yet. Search above: blur, glow, colour, keys, distortions, particles, transitions…</div>'}
    ${fxs.map(e => { const d = byKind.get(e.kind) || { params: [] }; const params = d.params || [];
      return `<details class="fx-card" open data-fx="${esc(e.id)}"><summary><b>${esc(e.kind)}</b><span class="note muted">${esc((d.description || '').split(/[.:(]/)[0].slice(0, 46))}</span><button class="ib sm" data-rm="${esc(e.id)}" title="Remove effect">${ICON('trash')}</button></summary>
        ${params.length ? params.map(p => { const cur = pval(e.params?.[p.name], p.default, localT(c)); const [lo, hi] = range(p); const keyed = e.params?.[p.name]?.keys?.length || 0;
          return `<div class="fx-row" title="${esc(p.description || '')}"><span class="fx-n">${esc(p.name)}</span>
            <input type="range" min="${lo}" max="${hi}" step="${(hi - lo) / 200}" value="${cur}" data-p="${esc(p.name)}">
            <input type="number" step="any" value="${fmt(cur)}" data-p="${esc(p.name)}">
            <button class="ib sm key${keyed ? ' on' : ''}" data-key="${esc(p.name)}" title="${keyed ? keyed + ' keyframes; add one at the playhead' : 'Keyframe at the playhead'}">◆</button>
            ${keyed > 1 ? `<button class="ib sm" data-graph="${esc(p.name)}" title="Open in the graph editor">${ICON('graph')}</button>` : ''}</div>`; }).join('') : '<div class="note muted">No numeric parameters.</div>'}</details>`; }).join('')}`;
  const add = async () => { const k = $('#fx-q').value.trim(); if (!k) return; if (!byKind.has(k)) return toast(`No effect called "${k}"`, 'bad'); await call('effects', 'add_effect', { clip_id: c.id, kind: k }); loadSoon(); };
  $('#fx-addb').onclick = add; $('#fx-q').onkeydown = e => { if (e.key === 'Enter') add(); };
  body.querySelectorAll('[data-rm]').forEach(b => b.onclick = async ev => { ev.preventDefault(); await call('effects', 'remove_effect', { clip_id: c.id, effect_id: b.dataset.rm }); loadSoon(); });
  body.querySelectorAll('.fx-card').forEach(card => {
    const fid = card.dataset.fx;
    card.querySelectorAll('input[data-p]').forEach(inp => {
      const sib = () => card.querySelector(`input[data-p="${CSS.escape(inp.dataset.p)}"]:not([type=${inp.type}])`);
      inp.oninput = () => { const o = sib(); if (o) o.value = inp.type === 'range' ? fmt(+inp.value) : inp.value; };
      inp.onchange = async () => { const v = +inp.value; if (!isFinite(v)) return; const cur = (selClip()?.effects || []).find(x => x.id === fid);
        // animated: a new value is a keyframe at the playhead (as in After Effects); otherwise it sets the value
        if (cur?.params?.[inp.dataset.p]?.keys?.length) await call('animate', 'set_keyframe', { clip_id: c.id, effect_id: fid, property: inp.dataset.p, time: localT(c), value: v });
        else await call('effects', 'set_effect_param', { clip_id: c.id, effect_id: fid, param: inp.dataset.p, value: v });
        loadSoon(250); };
    });
    card.querySelectorAll('[data-key]').forEach(b => b.onclick = async () => { const v = +card.querySelector(`input[type=number][data-p="${CSS.escape(b.dataset.key)}"]`).value;
      await call('animate', 'set_keyframe', { clip_id: c.id, effect_id: fid, property: b.dataset.key, time: localT(c), value: v }); loadSoon(); });
    card.querySelectorAll('[data-graph]').forEach(b => b.onclick = () => { PRO.graph.prop = b.dataset.graph; PRO.graph.effect = fid; PRO.graph.sel = -1; sidePane('graph'); renderGraph(); });
  });
}

// ================================================================================================================
// GRAPH EDITOR
function animated(c) {
  const out = [];
  for (const [k, p] of Object.entries(c.props || {})) if (p?.keys?.length) out.push({ prop: k, effect: null, label: k, n: p.keys.length });
  for (const e of c.effects || []) for (const [k, p] of Object.entries(e.params || {})) if (p?.keys?.length) out.push({ prop: k, effect: e.id, label: `${e.kind} · ${k}`, n: p.keys.length });
  return out;
}
async function renderGraph() {
  const body = $('#graph-body'), c = selClip();
  if (!c) return noClip(body, 'edit its animation curves');
  const props = animated(c), G = PRO.graph;
  if (!props.some(p => p.prop === G.prop && p.effect === G.effect)) { const f = props.find(p => p.n > 1) || props[0]; G.prop = f?.prop || 'x'; G.effect = f?.effect || null; G.sel = -1; }
  const opts = [...props.map(p => [`${p.effect || ''}|${p.prop}`, `${p.label} (${p.n})`]), ...['x', 'y', 'scale', 'rotation', 'opacity'].filter(k => !props.some(p => !p.effect && p.prop === k)).map(k => [`|${k}`, `${k} (not animated)`])];
  body.innerHTML = `<div class="gr-bar"><select id="gr-prop">${opts.map(([v, l]) => `<option value="${esc(v)}"${v === `${G.effect || ''}|${G.prop}` ? ' selected' : ''}>${esc(l)}</option>`).join('')}</select>
      <div class="seg"><button data-mode="value" class="${G.mode === 'value' ? 'on' : ''}">Value</button><button data-mode="speed" class="${G.mode === 'speed' ? 'on' : ''}">Speed</button></div></div>
    <canvas id="gr-c" width="600" height="440"></canvas>
    <div class="gr-keys" id="gr-keys"></div>
    <div class="note muted gr-help">Drag a key to move it in time and value · double-click to add a key · select a key and press Delete to remove it · the ◆ buttons in Effects add keys at the playhead.</div>`;
  $('#gr-prop').onchange = e => { const [ef, pr] = e.target.value.split('|'); G.prop = pr; G.effect = ef || null; G.sel = -1; renderGraph(); };
  body.querySelectorAll('[data-mode]').forEach(b => b.onclick = () => { G.mode = b.dataset.mode; drawGraph(); body.querySelectorAll('[data-mode]').forEach(x => x.classList.toggle('on', x === b)); });
  const args = { clip_id: c.id, property: G.prop, samples: 120 }; if (G.effect) args.effect_id = G.effect;
  G.data = await read('animate', 'key_graph', args); G.clip = c;
  drawGraph(); wireGraph();
}
function graphBox(cv) { return { l: 38, r: cv.width - 10, t: 12, b: cv.height - 22 }; }
function graphScale() {
  const G = PRO.graph, d = G.data, cv = $('#gr-c'); if (!cv || !d || !Array.isArray(d.graph)) return null;
  const key = G.mode === 'speed' ? 'speed' : 'value'; const pts = d.graph.map(p => [p.t, p[key]]);
  const vs = pts.map(p => p[1]).concat(G.mode === 'value' ? d.keys.map(k => k.value) : []);
  let lo = Math.min(...vs), hi = Math.max(...vs); if (hi - lo < 1e-6) { lo -= 1; hi += 1; } const pad = (hi - lo) * 0.12; lo -= pad; hi += pad;
  const t0 = d.graph[0].t, t1 = d.graph.at(-1).t || t0 + 1, B = graphBox(cv);
  return { pts, lo, hi, t0, t1, B, X: t => B.l + (t - t0) / (t1 - t0 || 1) * (B.r - B.l), Y: v => B.b - (v - lo) / (hi - lo) * (B.b - B.t), T: x => t0 + (x - B.l) / (B.r - B.l) * (t1 - t0), V: y => lo + (B.b - y) / (B.b - B.t) * (hi - lo) };
}
function drawGraph() {
  const cv = $('#gr-c'); if (!cv) return; const w = Math.round((cv.clientWidth || 300) * 2); if (cv.width !== w) { cv.width = w; cv.height = 440; } // 2x for a sharp line; CSS sets the height
  const g = cv.getContext('2d'), G = PRO.graph, d = G.data; g.clearRect(0, 0, cv.width, cv.height);
  const css = getComputedStyle(document.body), ink = css.getPropertyValue('--ink3') || '#888', acc = css.getPropertyValue('--agent') || '#7c5cff', line = css.getPropertyValue('--line2') || '#333';
  g.font = '20px ' + (css.getPropertyValue('--f-mono') || 'monospace');
  if (!d || !Array.isArray(d.graph)) { g.fillStyle = ink; g.fillText(d?.note ? 'Fewer than two keys: add keys to see a curve.' : 'No curve to show.', 20, 40); $('#gr-keys').innerHTML = ''; return; }
  const s = graphScale(), B = s.B;
  g.strokeStyle = line; g.lineWidth = 1; for (let i = 0; i <= 4; i++) { const y = B.t + (B.b - B.t) * i / 4; g.beginPath(); g.moveTo(B.l, y); g.lineTo(B.r, y); g.stroke(); g.fillStyle = ink; g.fillText(fmt(s.V(y), 2), 2, y + 6); }
  g.strokeStyle = acc; g.lineWidth = 3; g.beginPath(); s.pts.forEach(([t, v], i) => { const x = s.X(t), y = s.Y(v); i ? g.lineTo(x, y) : g.moveTo(x, y); }); g.stroke();
  if (G.mode === 'value') d.keys.forEach((k, i) => { const x = s.X(k.t), y = s.Y(k.value); g.fillStyle = i === G.sel ? '#fff' : acc; g.strokeStyle = '#000'; g.lineWidth = 2; g.beginPath(); g.moveTo(x, y - 9); g.lineTo(x + 9, y); g.lineTo(x, y + 9); g.lineTo(x - 9, y); g.closePath(); g.fill(); g.stroke(); });
  g.fillStyle = ink; g.fillText(`${fmt(s.t0, 2)} s`, B.l, cv.height - 2); g.fillText(`${fmt(s.t1, 2)} s`, B.r - 80, cv.height - 2);
  const k = d.keys[G.sel];
  $('#gr-keys').innerHTML = k ? `<span>Key at <b>${fmt(k.t, 3)} s</b> = <b>${fmt(k.value, 3)}</b></span><select id="gr-ease">${['linear', 'in', 'out', 'in_out', 'hold', 'out_back', 'in_out_back', 'out_elastic', 'out_bounce', 'out_expo'].map(e => `<option${e === k.ease ? ' selected' : ''}>${e}</option>`).join('')}</select><button class="btn sm ghost" id="gr-del" style="color:var(--bad)">${ICON('trash')}Delete</button>` : `<span class="note muted">${d.keys.length} keys. Click one to edit it.</span>`;
  if (k) { $('#gr-ease').onchange = e => keyOp('set_keyframe', { time: k.t, value: k.value, ease: e.target.value }); $('#gr-del').onclick = () => keyOp('remove_key', { time: k.t }); }
}
async function keyOp(tool, extra) {
  const G = PRO.graph, c = G.clip; if (!c) return; const a = { clip_id: c.id, property: G.prop, ...extra }; if (G.effect) a.effect_id = G.effect;
  await call('animate', tool, a); if (tool === 'remove_key') G.sel = -1; loadSoon(60); setTimeout(renderGraph, 250);
}
function wireGraph() {
  const cv = $('#gr-c'); if (!cv) return; const G = PRO.graph; let drag = null;
  const at = e => { const r = cv.getBoundingClientRect(); return [(e.clientX - r.left) * cv.width / r.width, (e.clientY - r.top) * cv.height / r.height]; };
  cv.onmousedown = e => { const s = graphScale(); if (!s || G.mode !== 'value') return; const [x, y] = at(e);
    const i = G.data.keys.findIndex(k => Math.hypot(s.X(k.t) - x, s.Y(k.value) - y) < 16); G.sel = i; drawGraph(); if (i >= 0) drag = { i, t: G.data.keys[i].t, s }; };
  cv.onmousemove = e => { if (!drag) return; drag.moved = true; const [x, y] = at(e); const k = G.data.keys[drag.i]; k.t = Math.max(0, drag.s.T(x)); k.value = drag.s.V(y); drawGraph(); };
  cv.onmouseup = () => { if (!drag) return; const k = G.data.keys[drag.i], from = drag.t, moved = drag.moved; drag = null; if (!moved) return; keyOp('move_key', { time: from, new_time: Math.max(0, k.t), new_value: k.value }); };
  cv.ondblclick = e => { const s = graphScale(); if (!s) return; const [x, y] = at(e); keyOp('set_keyframe', { time: Math.max(0, s.T(x)), value: s.V(y) }); };
  cv.tabIndex = 0; cv.onkeydown = e => { const k = G.data?.keys?.[G.sel]; if (k && (e.key === 'Delete' || e.key === 'Backspace')) { e.preventDefault(); e.stopPropagation(); keyOp('remove_key', { time: k.t }); } };
}

// ================================================================================================================
// MIXER
const DSP = ['parametric_eq', 'simple_eq', 'compressor_dsp', 'limiter_dsp', 'gate', 'deesser_dsp', 'denoise_dsp', 'dehum_dsp', 'speech_enhance', 'vocal_enhancer', 'dereverb', 'reverb_dsp', 'delay_dsp', 'stereo_width', 'mastering', 'multiband_compressor', 'amplify'];
const TARGETS = [['youtube', 'YouTube −14'], ['spotify', 'Spotify −14'], ['apple_music', 'Apple Music −16'], ['podcast', 'Podcast −16'], ['tiktok', 'TikTok −14'], ['broadcast_ebu', 'EBU R128 −23'], ['broadcast_atsc', 'ATSC −24'], ['netflix', 'Netflix −27']];
function renderMix() {
  const body = $('#mix-body'), tm = P.proj?.track_mix || {};
  if (document.activeElement && body.contains(document.activeElement) && document.activeElement.type === 'range') return;
  const strips = P.tracks.map((t, i) => { const m = tm[t.id] || {}; const g = m.gain_db ?? 0, pan = m.pan ?? 0; const fx = m.effects || [];
    return `<div class="strip" data-t="${esc(t.id)}"><div class="st-h"><b>${esc(t.name || 'Track ' + (i + 1))}</b><span class="note muted">${(t.clips || []).length} clips</span>
        <button class="ib sm tog${t.muted ? ' on' : ''}" data-tog="muted" title="Mute">M</button><button class="ib sm tog${t.solo ? ' on' : ''}" data-tog="solo" title="Solo">S</button></div>
      <label class="st-r"><span>Gain</span><input type="range" min="-60" max="12" step="0.5" value="${g}" data-k="gain_db"><output>${g > 0 ? '+' : ''}${fmt(g, 1)} dB</output></label>
      <label class="st-r"><span>Pan</span><input type="range" min="-1" max="1" step="0.05" value="${pan}" data-k="pan"><output>${pan === 0 ? 'C' : (pan < 0 ? 'L' : 'R') + Math.round(Math.abs(pan) * 100)}</output></label>
      <div class="st-fx">${fx.map(f => `<span class="ichip">${esc(f.id || f.kind)}</span>`).join('')}<select data-addfx><option value="">+ insert…</option>${DSP.map(k => `<option>${k}</option>`).join('')}</select>${fx.length ? `<button class="ib sm" data-clearfx title="Remove the inserts">${ICON('x')}</button>` : ''}</div></div>`; }).join('');
  body.innerHTML = `<div class="master"><div class="st-h"><b>Master loudness</b><select id="mx-target">${TARGETS.map(([v, l]) => `<option value="${v}"${v === PRO.target ? ' selected' : ''}>${l}</option>`).join('')}</select><button class="btn sm" id="mx-meas">${ICON('refresh')}Measure</button></div><div id="mx-loud" class="loud">${PRO.loud ? loudHtml(PRO.loud) : '<span class="note muted">BS.1770 / EBU R128: integrated, range and true peak of the whole mix.</span>'}</div></div>
    ${strips || '<div class="insp-empty">No tracks yet.</div>'}`;
  $('#mx-target').onchange = e => { PRO.target = e.target.value; };
  $('#mx-meas').onclick = async () => { $('#mx-loud').innerHTML = '<span class="note muted">Measuring…</span>'; PRO.loud = await read('audio', 'loudness_report', { target: PRO.target }); $('#mx-loud').innerHTML = PRO.loud ? loudHtml(PRO.loud) : '<span class="note muted">No audio to measure.</span>'; };
  body.querySelectorAll('.strip').forEach(st => { const tid = st.dataset.t;
    st.querySelectorAll('input[data-k]').forEach(r => { r.oninput = () => { const v = +r.value; r.nextElementSibling.textContent = r.dataset.k === 'gain_db' ? `${v > 0 ? '+' : ''}${fmt(v, 1)} dB` : (v === 0 ? 'C' : (v < 0 ? 'L' : 'R') + Math.round(Math.abs(v) * 100)); };
      r.onchange = async () => { await call('audio', 'set_track_mix', { track_id: tid, [r.dataset.k]: +r.value }); loadSoon(200); }; });
    st.querySelectorAll('[data-tog]').forEach(b => b.onclick = async () => { const t = P.tracks.find(x => x.id === tid); await call('timeline', 'set_track', { track_id: tid, [b.dataset.tog]: !t?.[b.dataset.tog] }); loadSoon(); });
    st.querySelector('[data-addfx]').onchange = async e => { if (!e.target.value) return; await call('audio', 'set_track_mix', { track_id: tid, add_effect: { kind: e.target.value, params: {} } }); loadSoon(); };
    const cl = st.querySelector('[data-clearfx]'); if (cl) cl.onclick = async () => { await call('audio', 'set_track_mix', { track_id: tid, clear_effects: true }); loadSoon(); };
  });
}
function loudHtml(r) {
  const v = (x, u) => x == null ? '—' : `${x}${u}`;
  return `<div class="kv"><span>Integrated</span><b class="${r.ok ? 'ok' : 'warn'}">${v(r.integrated_lufs, ' LUFS')}</b><span>Target</span><b>${v(r.target?.lufs, ' LUFS')}</b><span>True peak</span><b>${v(r.true_peak_dbtp, ' dBTP')}</b><span>Range</span><b>${v(r.loudness_range_lu, ' LU')}</b><span>Max short-term</span><b>${v(r.max_short_term_lufs, ' LUFS')}</b></div>
    ${(r.issues || []).map(i => `<div class="note warn">${esc(i)}</div>`).join('') || '<div class="note ok">Ready for delivery.</div>'}`;
}

// ================================================================================================================
// COLOUR (primary correction on the selected clip, through the "primaries" effect)
const PRIM = [['Exposure', 'gain_m', 1, 0, 2, 'Overall brightness (gain)'], ['Contrast', 'contrast', 1, 0, 2, ''], ['Saturation', 'saturation', 1, 0, 2, ''], ['Temperature', 'temperature', 0, -1, 1, 'Cool ↔ warm'], ['Tint', 'tint', 0, -1, 1, 'Green ↔ magenta'],
  ['Lift', 'lift_m', 0, -0.5, 0.5, 'Shadows'], ['Gamma', 'gamma_m', 1, 0.3, 2.5, 'Midtones'], ['Shadows', 'shadows', 0, -1, 1, ''], ['Highlights', 'highlights', 0, -1, 1, '']];
function renderColor() {
  const body = $('#color-body'), c = selClip();
  if (!c) return noClip(body, 'grade it');
  if (document.activeElement && body.contains(document.activeElement) && document.activeElement.type === 'range') return;
  const fx = (c.effects || []).find(e => e.kind === 'primaries');
  body.innerHTML = `<div class="row" style="gap:6px;flex-wrap:wrap;margin-bottom:10px"><button class="btn sm acc" id="co-auto">${ICON('wand')}Auto grade</button><button class="btn sm" id="co-match">${ICON('image')}Match the other shots to this</button>${fx ? `<button class="btn sm ghost" id="co-reset">${ICON('undo')}Reset</button>` : ''}</div>
    <div class="isec"><div class="h">Primary correction</div>${PRIM.map(([l, k, d, lo, hi, tip]) => { const v = pval(fx?.params?.[k], d);
      return `<label class="ifield" title="${esc(tip)}"><span>${l}</span><span class="pair"><input type="range" min="${lo}" max="${hi}" step="${(hi - lo) / 200}" value="${v}" data-k="${k}" data-d="${d}"><output>${fmt(v, 2)}</output></span></label>`; }).join('')}</div>
    <div class="note muted">Edits the clip's <b>primaries</b> effect (lift/gamma/gain, contrast with pivot, saturation, white balance). Scopes are in the viewer's scope dock.</div>`;
  body.querySelectorAll('input[data-k]').forEach(r => { r.oninput = () => { r.nextElementSibling.textContent = fmt(+r.value, 2); };
    r.onchange = async () => { const cur = (selClip()?.effects || []).find(e => e.kind === 'primaries');
      if (cur) await call('effects', 'set_effect_param', { clip_id: c.id, effect_id: cur.id, param: r.dataset.k, value: +r.value });
      else await call('effects', 'add_effect', { clip_id: c.id, kind: 'primaries', params: { [r.dataset.k]: +r.value } });
      loadSoon(150); }; });
  $('#co-auto').onclick = async () => { await call('color_grade', 'auto_grade', { clip_ids: [c.id] }); loadSoon(); };
  $('#co-match').onclick = async () => { const others = [...P.clips.values()].filter(x => x.id !== c.id && clipKind(x) === 'media').map(x => x.id); if (!others.length) return toast('No other video clips to match.'); await call('color_grade', 'match_shot_look', { reference_clip_ids: [c.id], target_clip_ids: others }); loadSoon(); };
  const rs = $('#co-reset'); if (rs) rs.onclick = async () => { await call('effects', 'remove_effect', { clip_id: c.id, effect_id: fx.id }); loadSoon(); };
}

// ================================================================================================================
// KEYBOARD SHORTCUTS (rebindable; the remap sits in front of the Studio's own key handler)
const REBINDABLE = [['s', 'Split at playhead'], ['t', 'Add text'], ['m', 'Add marker'], ['i', 'Loop in'], ['o', 'Loop out'], ['j', 'Play backwards / back'], ['k', 'Pause'], ['l', 'Play forwards'], [' ', 'Play or pause'], ['f', 'Full-screen viewer'], ['g', 'Thirds grid'], ["'", 'Safe areas'], ['\\', 'Fit the edit'], ['Delete', 'Delete clip'], ['Home', 'Go to start'], ['End', 'Go to end']];
function keymap() { try { return JSON.parse(localStorage.getItem('kimo.keymap') || '{}'); } catch { return {}; } }
function saveKeymap(m) { try { localStorage.setItem('kimo.keymap', JSON.stringify(m)); } catch {} }
const keyName = k => k === ' ' ? 'Space' : k.length === 1 ? k.toUpperCase() : k;
let rebinding = null;
addEventListener('keydown', e => {
  if (rebinding) { e.preventDefault(); e.stopImmediatePropagation(); if (e.key === 'Escape') { rebinding = null; renderKeys(); return; }
    if (['Shift', 'Control', 'Alt', 'Meta'].includes(e.key)) return; const m = keymap(); const nk = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    for (const [orig, cur] of Object.entries(m)) if (cur === nk && orig !== rebinding) delete m[orig]; // a key does one thing
    if (nk === rebinding) delete m[rebinding]; else m[rebinding] = nk; saveKeymap(m); rebinding = null; renderKeys(); return; }
  if (e.kimoRemapped || e.ctrlKey || e.metaKey || e.altKey || e.target.closest?.('input,textarea,select,[contenteditable]')) return;
  const m = keymap(); if (!Object.keys(m).length) return; const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const orig = Object.entries(m).find(([, cur]) => cur === k)?.[0];
  if (orig) { e.preventDefault(); e.stopImmediatePropagation(); const ev = new KeyboardEvent('keydown', { key: orig, shiftKey: e.shiftKey, bubbles: true }); ev.kimoRemapped = true; dispatchEvent(ev); return; }
  if (m[k] && k !== m[k]) { e.preventDefault(); e.stopImmediatePropagation(); } // its action moved to another key
}, true);
function renderKeys() {
  const body = $('#keys-body'), m = keymap();
  body.innerHTML = `<div class="note muted" style="margin-bottom:8px">Click a shortcut, then press the new key (Esc cancels). Saved on this computer.</div>
    <div class="keys-tbl">${REBINDABLE.map(([k, d]) => { const cur = m[k] || k; return `<div class="kr${m[k] ? ' changed' : ''}"><span>${esc(d)}</span><button class="kk" data-rebind="${esc(k)}">${rebinding === k ? '<i>press a key…</i>' : `<kbd>${esc(keyName(cur))}</kbd>`}</button></div>`; }).join('')}</div>
    <div class="row" style="gap:6px;margin-top:10px"><button class="btn sm" id="keys-reset">${ICON('undo')}Reset to defaults</button><button class="btn sm ghost" id="keys-all">${ICON('keyboard')}All shortcuts</button></div>`;
  body.querySelectorAll('[data-rebind]').forEach(b => b.onclick = () => { rebinding = b.dataset.rebind; renderKeys(); });
  $('#keys-reset').onclick = () => { saveKeymap({}); renderKeys(); toast('Shortcuts reset'); };
  $('#keys-all').onclick = () => showKeys();
}
