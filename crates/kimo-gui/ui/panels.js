// Panel renderers. Each kind is a pure function of (spec, data, w, h) that
// returns markup; geometry comes from the measured panel box and fixed row
// heights, so a value changing never changes a box.
import { flashing, METER_MS, GIZMO_MS } from './router.js';
import { covAPost } from './cov_a.js';
const F = o => (o && flashing(o._flash) ? ' flash' : '') + (o && flashing(o._peek) ? ' peek' : '');
const E = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const px = n => Math.round(n * 10) / 10;

export function fmtArgs(a, tool = '') {
  if (a == null) return '';
  if (typeof a === 'string') return a;
  const kv = o => Object.entries(o).filter(([, v]) => v != null && v !== '' && !(typeof v === 'number' && !Number.isFinite(v)))
    .map(([k, v]) => `${k}: ${v && typeof v === 'object' ? JSON.stringify(v) : v}`).join(', ');
  if (Array.isArray(a)) return a.filter(x => x != null).map(x => (typeof x === 'object') ? kv(x) : String(x)).join(', ');
  // a live call's args object, in the mockups' rail style: what it acts on first, positional
  // ("set_material(cap, …)", "add_emitter(V2, dust, …)"), then its own values named; nested arrays /
  // objects stay out of the rail (they are in the inspector)
  const ID = ['clip_id', 'clip', 'name', 'id', 'track_id', 'asset_id', 'preset', 'kind', 'shape', 'mode', 'material', 'path', 'text', 'label', 'effect', 'property'];
  const prim = v => v != null && v !== '' && typeof v !== 'object' && !(typeof v === 'number' && !Number.isFinite(v));
  // the mask is the target of a mask call (its mask_id, or the name add_mask gives it): it goes first and the clip
  // is left out — "set_mask_path_key(mask_1, time: 4.2)", "add_mask(mask_1, bezier, feather: 2)"
  const maskId = prim(a.mask_id) ? a.mask_id : /mask/.test(tool) && prim(a.name) ? a.name : null;
  const skip = new Set(maskId != null ? ['mask_id', 'clip_id', 'clip', 'name'] : []);
  const TIME = /^(time|t|start|end|duration|at|from|to|in|out|offset)$/;   // times read to 1 decimal in the rail
  const num = (k, v) => TIME.test(k) ? Math.round(v * 10) / 10 : Math.round(v * 1000) / 1000;
  const pos = [...(maskId != null ? [String(maskId)] : []), ...ID.filter(k => !skip.has(k) && prim(a[k])).map(k => k === 'path' ? String(a[k]).split(/[\\/]/).pop() : String(a[k]))];
  const named = Object.entries(a).filter(([k, v]) => !ID.includes(k) && !skip.has(k) && prim(v)).map(([k, v]) => `${k === 'time' ? 't' : k}: ${typeof v === 'number' ? num(k, v) : v}`);   // 'time' reads as t, like the mockups' short keys
  const out = [...pos, ...named].join(', ');
  return out || kv(a);
}
const fmtDur = ms => (ms == null || !Number.isFinite(+ms) ? '' : Math.max(1, Math.round(+ms)) + 'ms');   // one rule (Naman 4:37): integer ms everywhere, rail and Inspector ('37ms', '100ms', '1240ms')

function rng(seed) { let s = (seed * 9301 + 49297) % 233280 || 1; return () => (s = (s * 9301 + 49297) % 233280) / 233280; }
function head(spec, data, extra = '') {
  if (spec.title === undefined && !data.title) return '';
  const t = data.title ?? spec.title;
  const m = data.meta ?? spec.meta ?? '';
  return `<div class="ph"${spec.phPad ? ` style="padding:${spec.phPad}"` : ''}><span class="t">${E(t)}${data.titleSmall ? `<small>${E(data.titleSmall)}</small>` : ''}</span><span class="m">${E(m)}${extra}</span></div>` +
    (data.sub ? `<div class="psub">${data.subHtml ? data.sub : E(data.sub)}</div>` : '');
}

// ---------- agent ----------
export function tickRunning(state) {
  for (const el of document.querySelectorAll('[data-run]')) {
    const c = state.calls.find(x => String(x.id) === el.dataset.run);
    if (c && c.status === 'running') { el.textContent = fmtDur(performance.now() - c.t); const p = el.closest('[data-panel]'); if (p) p._h = null; }   /* ticked in place: the panel's markup cache no longer matches its DOM */
  }
}
// one rail row. meas: the row as the fit measures it (full args, no fit state) — PRE.agent lays these out hidden
function railRow(c, i, calls, o, state, w, y, rowH, meas) {
  const cur = i === calls.length - 1;
  const ck = c.status === 'running' ? (o.runMark ?? '') : c.status === 'error' ? '✕' : c.status === 'lost' ? '?' : '✓';   // lost: its result fell in a feed drop
  const showCk = !(cur && o.noCheckCur);
  const ckEl = `<span class="ck">${showCk ? ck : ''}</span>`;
  const dur = c.status === 'running' ? fmtDur(state.session.still && c.elapsed_ms != null ? c.elapsed_ms : performance.now() - c.t) : fmtDur(c.dur_ms);
  const recent = o.boldLast && i >= calls.length - o.boldLast;
  const full = railArgs(c), fk = railKey(c, railGeo(cur, recent, state.mode), w, full), fit = !meas && c._fit && c._fit.k === fk ? c._fit : null, args = fit && fit.t != null ? fit.t : full, bare = !!full && args === '';   /* PRE.agent's whole-arg fit, once per row state */
  const faded = !cur && o.fade != null && i < calls.length - 1 - o.fade;
  const ch = cur && o.curH ? o.curH : rowH;
  return `<div class="call ${cur ? 'cur' : 'old'}${fit && fit.tight ? ' tight' : ''}${faded ? ' faded' : ''}${recent ? ' recent' : ''}${c.status === 'error' ? ' err' : ''}${!meas && state.pick != null && state.pick === c.id ? ' picked' : ''}"${c.id != null ? ` data-call="${E(c.id)}"` : ''}${meas ? ` data-fk="${E(fk)}"` : ''} style="top:${y}px;height:${ch}px${ch !== rowH ? `;align-items:flex-start;padding-top:${o.curPadT ?? 9}px` : ''}"><div class="ln">` +
    (o.checkRight ? '' : ckEl) +
    `<span class="sig${bare ? ' bare' : args ? '' : ' noargs'}"${(args || bare) && !meas ? ` title="${E(c.name + '(' + full + ')')}"` : ''}><span class="fn">${E(c.name)}</span><span class="pp">(</span><span class="args">${E(args)}</span><span class="pp">)</span></span>` +
    (o.checkRight && showCk ? ckEl : '') +
    `<span class="dur" ${c.status === 'running' ? `data-run="${E(c.id)}"` : ''}>${dur}</span></div></div>`;
}
/* the fit depends on the row's geometry, not on whether it is current: a current row differs only in colour, unless
   the mode hides its empty check column (--cur-ck: none, the captions screen) or the row is bold (boldLast) */
const CUR_CK = new Map();
const curCkHidden = mode => { let v = CUR_CK.get(mode); if (v == null) { v = getComputedStyle(document.documentElement).getPropertyValue('--cur-ck').trim() === 'none'; CUR_CK.set(mode, v); } return v; };
const railGeo = (cur, recent, mode) => (cur && curCkHidden(mode) ? 'w' : '') + (recent ? 'b' : '');
const railKey = (c, geo, w, full) => `${full}|${geo}|${c.status === 'running' ? 'r' : 's'}|${w}`;
/* fmtArgs once per args object (a call's args are replaced, never edited in place) */
const railArgs = c => (c._fa && c._fa[0] === c.args && c._fa[1] === c.name ? c._fa[2] : (c._fa = [c.args, c.name, fmtArgs(c.args, c.name)])[2]);
function railCalls(spec, state, h) {
  const o = spec.opts || {}, rowH = o.rowH || 37, top = o.top ?? 42, gap = o.gap || 0, reserve = o.reserve || 0;
  const slots = Math.max(1, Math.floor((h - top - reserve + gap) / (rowH + gap)));
  return { o, rowH, top, gap, calls: state.calls.slice(-slots) };
}
function agent({ spec, state, data, w, h }) {
  const { o, rowH, top, gap, calls } = railCalls(spec, state, h);
  const meta = o.count ? `${state.callCount} calls` : '';
  let html = head(spec, { ...data, meta: data.meta ?? meta });
  html += `<div class="calls ${o.chips ? 'chips' : ''}">`;
  calls.forEach((c, i) => {
    const cur = i === calls.length - 1, y = top + i * (rowH + gap);
    html += railRow(c, i, calls, o, state, w, y, rowH, false);
    if (cur && c.progress && o.progress) {
      const p = c.progress;
      html += `<div class="callbar" style="top:${y + rowH + 6}px;left:${o.barL ?? 15}px;right:${o.barR ?? 60}px"><i style="width:${(100 * p.done / p.total).toFixed(1)}%"></i></div>` +
        `<div class="mono dim" style="position:absolute;right:14px;top:${y + rowH - 1}px;font-size:${o.progFs ?? 11.5}px">${p.done} / ${p.total}</div>`;
    }
  });
  html += '</div>';
  if (data.footer) html += `<div class="pfoot">${E(data.footer)}</div>`;
  return html;
}

// ---------- viewer ----------
function viewer({ spec, data, w, h, state }) {
  const o = spec.opts || {};
  const [pt, pr, pb, pl] = o.pad || [12, 12, 44, 12];
  const fw = w - pl - pr, fh = h - pt - pb;
  let html = '';
  if (spec.title) html += head(spec, data);
  html += `<div class="frame" style="left:${pl}px;top:${pt}px;width:${fw}px;height:${fh}px">` +
    (data.src ? `<img src="${E(data.src)}" alt="">` : '<div class="ph-frame"></div>') +
    // a running grade/effect wipes across the frame: before (ungraded) on the right of the edge, result on the left
    (data.src && data.wipe != null && data.wipe < 1 ? `<img class="before" src="${E(data.src)}" alt="" style="clip-path:inset(0 0 0 ${px(data.wipe * 100)}%)"><i class="wipe" style="left:${px(data.wipe * fw)}px"></i>` : '');
  // false colour: the frame's right side through the exposure LUT (painted by POST.viewer), split edge in ember
  if (data.src && data.falsecolor) { const sp = data.falsecolor.split ?? .5; html += `<canvas class="fc" width="${Math.round(fw)}" height="${Math.round(fh)}" style="clip-path:inset(0 0 0 ${px(sp * 100)}%)"></canvas><i class="split" style="left:${px(sp * fw)}px"></i>`; }
  if (data.overlay) html += `<svg class="ov" viewBox="0 0 ${fw} ${fh}">${overlay(data.overlay, fw, fh)}</svg>`;
  const gz = state && state.gizmo, gage = gz ? performance.now() - gz.t : Infinity;
  if (gage < GIZMO_MS) html += gizmo(gz, fw, fh, state.session || {}, gage);
  for (const t of (data.tags || (data.tag ? [{ text: data.tag }] : []))) {
    const pos = t.pos === 'tr' ? `right:${t.x ?? 10}px` : `left:${t.x ?? 10}px`;
    html += `<span class="tag${t.cls ? ' ' + E(t.cls) : ''}" style="${pos};top:${t.y ?? 10}px">${E(t.text)}</span>`;
  }
  if (data.stale != null) html += data.refused ? `<span class="tag stale refused" data-source="${data.refusedSrc === 'inferred' ? 'inferred' : 'engine'}"><i></i>render refused: ${E(data.refused)}</span>`   /* GUI-FIX-2: the engine will not render this rev (app.js previewMark) */
    : `<span class="tag stale"><i></i>out of date · rev ${E(data.stale)}</span>`;   /* the frame shows an older rev than the project's (app.js previewMark) */
  const hd = state && state.hud;
  if (hd && performance.now() - hd.t < 1600) html += `<span class="hud${hd.phase === 'error' ? ' err' : hd.fam === 'inspect' ? ' read' : ''}" style="right:${o.hudX ?? 10}px;top:${o.hudY ?? 10}px"><i></i>${E(hd.fam)} - ${E(hd.text)}</span>`;
  html += '</div>';
  if (data.progress) {
    const p = data.progress, y = pt + fh + (o.progY ?? 14);
    const det = p.pct != null;   /* no progress event yet: an indeterminate sweep, no percentage */
    html += `<div class="pbar${det ? '' : ' indet'}${p.halt ? ' halt' : ''}" style="position:absolute;left:${pl}px;right:${pr}px;top:${y}px;height:${o.progH ?? 3}px;background:var(--raise2);border-radius:2px${det ? '' : ';overflow:hidden'}"><div style="width:${det ? p.pct : 28}%;height:100%;background:var(--ember);border-radius:2px"></div></div>` +
      `<div class="vmeta" style="top:${y + (o.progMetaDy ?? 9)}px;left:${pl}px;right:${pr}px"><span>${E(p.left)}</span><span class="r">${det ? `<span class="em">${E(p.pct)}%</span>&nbsp;&nbsp;&nbsp;` : ''}${E(p.right)}</span></div>`;
  }
  if (data.thumbs) {
    const ty = o.thumbY ?? (pt + fh + 12), th = o.thumbH ?? 50;
    html += `<div class="thumbs" style="left:${pl}px;right:${o.thumbR ?? pr}px;top:${ty}px;height:${th}px;gap:${o.thumbGap ?? 6}px;overflow:hidden">` +
      data.thumbs.map(t => `<div class="thumb ${t.sel ? 'sel' : ''} ${t.dim ? 'dimmed' : ''}"${o.thumbW ? ` style="flex:none;width:${o.thumbW}px"` : ''}>${t.src ? `<img src="${E(t.src)}" alt="">` : ''}${t.check ? '<span class="badge">✓</span>' : ''}</div>`).join('') + '</div>';
  }
  const my = o.metaY ?? (pt + fh + (pb - 20) / 2);
  if (data.tc || data.left || data.res) {
    html += `<div class="vmeta" style="top:${my}px;left:${o.metaL ?? 14}px;right:${o.metaR ?? 14}px">` +
      `<span>${E(data.left || '')}</span>` +
      (data.tc ? `<span class="tc" style="${o.tcLeft ? 'position:static;transform:none;font-weight:400;color:var(--dim);font-size:var(--tcl-fs,var(--fs-s));font-family:var(--tcl-font,var(--mono))' : ''}">${E(data.tc)}</span>` : '') +
      `<span class="r">${E(data.res || '')}</span></div>`;
  }
  return html;
}

function overlay(ov, w, h) {
  let s = '';
  for (const o of ov) {
    if (o.quad) {
      const p = o.quad.map(([x, y]) => [x * w, y * h]);
      s += `<polygon points="${p.map(q => q.join(',')).join(' ')}" fill="rgba(255,90,35,.08)" stroke="var(--ember)" stroke-width="1.5"${o.roi ? ' stroke-dasharray="6 4"' : ''}/>`;
      p.forEach(q => s += `<circle cx="${q[0]}" cy="${q[1]}" r="4" fill="var(--ember)"/>`);
    }
    if (o.lens) {   // undistort: the k1/k2 grid it straightens (Brown radial model, normalised to the half-diagonal)
      const [k1, k2] = o.lens, cx = (o.c && o.c[0] != null ? o.c[0] : .5) * w, cy = (o.c && o.c[1] != null ? o.c[1] : .5) * h, R0 = Math.hypot(w, h) / 2;
      const D = (x, y) => { const u = (x - cx) / R0, v = (y - cy) / R0, r2 = u * u + v * v, f = 1 + k1 * r2 + k2 * r2 * r2; return [cx + u * f * R0, cy + v * f * R0]; };
      const line = pts => `<polyline points="${pts.map(([x, y]) => D(x, y)).map(q => px(q[0]) + ',' + px(q[1])).join(' ')}" fill="none" stroke="var(--cream)" stroke-opacity=".55" stroke-width="1" stroke-dasharray="4 3"/>`;
      for (let i = 1; i < 8; i++) { const x = w * i / 8; s += line(Array.from({ length: 21 }, (_, j) => [x, h * j / 20])); }
      for (let i = 1; i < 6; i++) { const y = h * i / 6; s += line(Array.from({ length: 21 }, (_, j) => [w * j / 20, y])); }
    }
    if (o.rect && !(o.shape && /ellipse|circle/.test(o.shape))) { const [x, y, rw, rh] = o.rect; s += `<rect x="${px(x * w)}" y="${px(y * h)}" width="${px(rw * w)}" height="${px(rh * h)}" fill="${o.em ? 'rgba(255,90,35,.08)' : 'none'}" stroke="${o.em || o.shape ? 'var(--ember)' : 'var(--cream)'}" stroke-width="${o.em ? 1.6 : 1.25}"${o.em || o.shape ? '' : ' stroke-dasharray="5 3"'}/>` + (o.label ? `<rect x="${px(x * w)}" y="${px(y * h - 18)}" width="${px(7 * String(o.label).length + 10)}" height="17" rx="3" fill="${o.em ? 'var(--ember)' : 'rgba(20,20,18,.8)'}"/><text x="${px(x * w + 5)}" y="${px(y * h - 5.5)}" style="fill:${o.em ? '#141412' : 'var(--cream)'};font-family:var(--mono);font-size:11.5px">${E(o.label)}</text>` : ''); }
    if (o.brackets) { const m = 14, L = 26; s += `<path d="M${m} ${m + L}V${m}H${m + L}M${w - m - L} ${m}H${w - m}V${m + L}M${w - m} ${h - m - L}V${h - m}H${w - m - L}M${m + L} ${h - m}H${m}V${h - m - L}" fill="none" stroke="var(--ember)" stroke-width="2"/>` + (o.label ? `<text x="${m + 6}" y="${m + 18}" style="fill:var(--cream);font-family:var(--mono);font-size:11px;paint-order:stroke;stroke:rgba(10,10,9,.7);stroke-width:3px">${E(o.label)}</text>` : ''); }
    if (o.to) { const [a, b, c, d] = o.to; s += `<rect x="${px(a * w)}" y="${px(b * h)}" width="${px(c * w)}" height="${px(d * h)}" fill="none" stroke="var(--ember)" stroke-opacity=".6" stroke-width="1" stroke-dasharray="4 4"/>`; }
    if (o.text != null) { const [x, y] = [o.at[0] * w, o.at[1] * h], tw = Math.min(w - 20, 9 * String(o.text).length + 24); s += (o.box ? `<rect x="${px(x - tw / 2)}" y="${px(y - 17)}" width="${px(tw)}" height="34" rx="8" fill="rgba(20,20,18,.82)" stroke="var(--ember)" stroke-width="1.2"/>` : '') + `<text x="${px(x)}" y="${px(y + 5)}" text-anchor="middle" style="fill:var(--cream);font-family:var(--mono);font-size:${o.box ? 13 : 18}px;paint-order:stroke;stroke:rgba(10,10,9,.7);stroke-width:3px">${E(o.text)}</text>`; }
    if (o.shape) { const [a, b, c, d] = o.rect.map((v, i) => v * (i % 2 ? h : w)); s += /ellipse|circle/.test(o.shape) ? `<ellipse cx="${px(a + c / 2)}" cy="${px(b + d / 2)}" rx="${px(c / 2)}" ry="${px(d / 2)}" fill="rgba(255,90,35,.08)" stroke="var(--ember)" stroke-width="1.5"/>` : ''; }
    if (o.path) s += `<${o.closed ? 'polygon' : 'polyline'} points="${o.path.map(([x, y]) => px(x * w) + ',' + px(y * h)).join(' ')}" fill="none" stroke="var(--ember)" stroke-width="1.5"/>` + o.path.map(([x, y]) => `<circle cx="${px(x * w)}" cy="${px(y * h)}" r="3" fill="var(--cream)"/>`).join('');
    if (o.squares) { const R = rng(o.seed || 1); for (let i = 0; i < o.squares; i++) { const sz = 4 + R() * 10; s += `<rect x="${px(R() * (w - sz))}" y="${px(R() * (h - sz))}" width="${px(sz)}" height="${px(sz)}" fill="${['var(--cream)', 'var(--ember)', '#141412'][i % 3]}" opacity=".85"/>`; } }
    if (o.crop) { const cw = w * o.crop, ch = h * o.crop; s += `<rect x="${px((w - cw) / 2)}" y="${px((h - ch) / 2)}" width="${px(cw)}" height="${px(ch)}" fill="none" stroke="var(--ember)" stroke-width="1.5" stroke-dasharray="6 4"/>`; }
    if (o.vline != null) s += `<line x1="${o.vline * w}" x2="${o.vline * w}" y1="0" y2="${h}" stroke="var(--ember)" stroke-width="2"/>`;
    if (o.quad && o.grid) {   // mesh tracker: the nx x ny lattice inside the quad (bilinear)
      const q = o.quad.map(([x, y]) => [x * w, y * h]), L = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
      const [nx, ny] = o.grid;
      for (let i = 1; i < nx; i++) { const t = i / nx, a = L(q[0], q[1], t), b = L(q[3], q[2], t); s += `<line x1="${px(a[0])}" y1="${px(a[1])}" x2="${px(b[0])}" y2="${px(b[1])}" stroke="rgba(255,90,35,.55)" stroke-width="1"/>`; }
      for (let j = 1; j < ny; j++) { const t = j / ny, a = L(q[0], q[3], t), b = L(q[1], q[2], t); s += `<line x1="${px(a[0])}" y1="${px(a[1])}" x2="${px(b[0])}" y2="${px(b[1])}" stroke="rgba(255,90,35,.55)" stroke-width="1"/>`; }
    }
    // GUI-FIX-2 tracking: feature points (with the residual vector, already magnified by the router), per-corner
    // disagreement / jitter rings, the long-gap tracker's boxes joined by their centres
    if (o.pts) for (const q of o.pts) { const x = q.p[0] * w, y = q.p[1] * h; if (q.v) s += `<line x1="${px(x)}" y1="${px(y)}" x2="${px(x - q.v[0] * w)}" y2="${px(y - q.v[1] * h)}" stroke="var(--ember)" stroke-width="1.5"/>`; s += `<circle cx="${px(x)}" cy="${px(y)}" r="3" fill="var(--cream)"/>`; }
    if (o.rings) for (const q of o.rings) s += `<circle cx="${px(q.p[0] * w)}" cy="${px(q.p[1] * h)}" r="${px(q.r)}" fill="none" stroke="var(--cream)" stroke-opacity=".8" stroke-width="1.25"/>`;
    if (o.boxes && o.boxes.length) { const n = o.boxes.length; o.boxes.forEach((b, i) => { s += `<rect x="${px(b[0] * w)}" y="${px(b[1] * h)}" width="${px(b[2] * w)}" height="${px(b[3] * h)}" fill="none" stroke="${i === n - 1 ? 'var(--ember)' : 'var(--cream)'}" stroke-opacity="${i === n - 1 ? 1 : .35}" stroke-width="${i === n - 1 ? 1.6 : 1}"/>`; }); s += `<polyline points="${o.boxes.map(b => px((b[0] + b[2] / 2) * w) + ',' + px((b[1] + b[3] / 2) * h)).join(' ')}" fill="none" stroke="var(--ember)" stroke-width="1.25"/>`; }
    if (o.pt) { const [x, y] = [o.pt[0] * w, o.pt[1] * h], r = (o.r ?? .12) * Math.min(w, h); s += `<circle cx="${px(x)}" cy="${px(y)}" r="${px(r)}" fill="none" stroke="var(--ember)" stroke-width="1.5"/><circle cx="${px(x)}" cy="${px(y)}" r="3.5" fill="var(--ember)"/><path d="M${px(x - r - 6)} ${px(y)}h8M${px(x + r - 2)} ${px(y)}h8M${px(x)} ${px(y - r - 6)}v8M${px(x)} ${px(y + r - 2)}v8" stroke="var(--cream)" stroke-width="1"/>`; }
    if (o.poly) s += `<polygon points="${o.poly.map(([x, y]) => px(x * w) + ',' + px(y * h)).join(' ')}" fill="rgba(245,241,231,.07)" stroke="var(--cream)" stroke-width="1.25" stroke-dasharray="5 3"/>`;
    if (o.light) {
      const [x, y] = [o.light[0] * w, o.light[1] * h], c = o.color && o.color.every(v => v != null) ? `rgb(${o.color.map(v => Math.round(Math.min(1, Math.max(0, v)) * 255)).join(',')})` : 'var(--cream)';
      if (o.kind === 'subject') s += `<circle cx="${px(x)}" cy="${px(y)}" r="9" fill="none" stroke="var(--cream)" stroke-width="1" stroke-dasharray="3 3"/><path d="M${px(x - 4)} ${px(y)}h8M${px(x)} ${px(y - 4)}v8" stroke="var(--cream)"/>`;
      else { s += `<circle cx="${px(x)}" cy="${px(y)}" r="7" fill="${c}" stroke="#141412" stroke-width="1.5"/>`; for (let k = 0; k < 8; k++) { const a = k * Math.PI / 4; s += `<line x1="${px(x + 11 * Math.cos(a))}" y1="${px(y + 11 * Math.sin(a))}" x2="${px(x + 16 * Math.cos(a))}" y2="${px(y + 16 * Math.sin(a))}" stroke="var(--ember)" stroke-width="1.4"/>`; } }
      if (o.label) s += `<text x="${px(x + 20)}" y="${px(y + 4)}" style="fill:var(--cream);font-family:var(--mono);font-size:11.5px">${E(o.label)}</text>`;
    }
    if (o.cam) {   // m13: orbit ring + camera at its pan with a frustum wedge at its fov + pan keyframe diamonds + axis triad
      const c = o.cam, cx = w / 2, cy = h * .46, rx = w * .34, ry = h * .15, P = d => { const a = (d || 0) * Math.PI / 180; return [cx + rx * Math.sin(a), cy + ry * Math.cos(a)]; };
      s += `<ellipse cx="${px(cx)}" cy="${px(cy)}" rx="${px(rx)}" ry="${px(ry)}" fill="none" stroke="rgba(245,241,231,.55)" stroke-width="1.2" stroke-dasharray="2 5" stroke-linecap="round"/>`;
      for (const v of o.keys || []) { const [x, y] = P(v); s += `<path d="M${px(x)} ${px(y - 6)}l6 6-6 6-6-6z" fill="var(--cream)" stroke="#141412" stroke-width="1"/>`; }
      const [x, y] = P(c.cam_pan), ang = Math.atan2(cy - y, cx - x), fov = (c.cam_fov ?? 40) * Math.PI / 180, L = Math.min(w, h) * .22;
      const a1 = [x + L * Math.cos(ang - fov / 2), y + L * Math.sin(ang - fov / 2)], a2 = [x + L * Math.cos(ang + fov / 2), y + L * Math.sin(ang + fov / 2)];
      s += `<path d="M${px(x)} ${px(y)}L${px(a1[0])} ${px(a1[1])}L${px(a2[0])} ${px(a2[1])}Z" fill="rgba(255,90,35,.10)" stroke="var(--ember)" stroke-width="1.2"/><rect x="${px(x - 7)}" y="${px(y - 5)}" width="14" height="10" rx="2" fill="var(--ember)" transform="rotate(${px(ang * 180 / Math.PI)} ${px(x)} ${px(y)})"/>`;
      const label = [c.cam_fov != null ? `fov ${fmtNum(c.cam_fov)}°` : '', c.cam_pan != null ? `pan ${fmtNum(c.cam_pan)}°` : '', c.cam_tilt != null ? `tilt ${fmtNum(c.cam_tilt)}°` : ''].filter(Boolean).join('  ');
      if (label) s += `<text x="${px(x + 12)}" y="${px(y - 10)}" style="fill:var(--cream);font-family:var(--mono);font-size:11.5px">${E(label)}</text>`;
      const tx = w - 46, ty = 78, pa = (c.cam_pan || 0) * Math.PI / 180, ax = [[Math.cos(pa), Math.sin(pa) * .45, '#e5533b', 'x'], [0, -1, '#e8a33a', 'y'], [-Math.sin(pa) * .8, Math.cos(pa) * .4, '#7fbf6a', 'z']];
      for (const [dx, dy, col, lab] of ax) s += `<line x1="${tx}" y1="${ty}" x2="${px(tx + dx * 24)}" y2="${px(ty + dy * 24)}" stroke="${col}" stroke-width="1.6"/><text x="${px(tx + dx * 32 - 3)}" y="${px(ty + dy * 32 + 4)}" style="fill:${col};font-family:var(--mono);font-size:10.5px">${lab}</text>`;
    }
    if (o.body) { const [x, y] = [o.body[0] * w, o.body[1] * h], r = Math.max(4, (o.r ?? .01) * w); s += `<circle cx="${px(x)}" cy="${px(y)}" r="${px(r)}" fill="rgba(255,90,35,.08)" stroke="var(--ember)" stroke-width="1.25"/><path d="M${px(x)} ${px(y)}v${px(r * .9)}" stroke="var(--ember)" stroke-width="1.25"/>${o.label ? `<text x="${px(x + r + 5)}" y="${px(y + 4)}" style="fill:var(--cream);font-family:var(--mono);font-size:11px">${E(o.label)}</text>` : ''}`; }
  }
  return s;
}

const fmtNum = n => String(Math.round(n * 10) / 10);
// ---------- transform gizmo (router xfOps) ----------
// The layer's box (a layer at scale 1 fills the frame), 8 handles, anchor crosshair, the previous pose
// dashed, a move arrow, plus the family's marks (null, parent link, motion path, bezier ease inset).
// It fades in CSS; a re-render mid-fade resumes at its age (negative animation-delay), no timers.
function gizmo(g, w, h, S, age) {
  const W = S.width, H = S.height;
  const P = p => { const s = p.scale ?? 1; return { cx: w / 2 + (W ? (p.x || 0) / W * w : 0), cy: h / 2 + (H ? (p.y || 0) / H * h : 0), bw: g.null ? 44 * s : w * s, bh: g.null ? 44 * s : h * s, r: p.rotation || 0 }; };
  const rot = (q, x, y) => { const a = q.r * Math.PI / 180, dx = x - q.cx, dy = y - q.cy; return [q.cx + dx * Math.cos(a) - dy * Math.sin(a), q.cy + dx * Math.sin(a) + dy * Math.cos(a)]; };
  const rect = (q, a) => `<rect x="${px(q.cx - q.bw / 2 + .5)}" y="${px(q.cy - q.bh / 2 + .5)}" width="${px(Math.max(1, q.bw - 1))}" height="${px(Math.max(1, q.bh - 1))}" transform="rotate(${px(q.r)} ${px(q.cx)} ${px(q.cy)})" ${a}/>`;
  const a = P(g.prev), b = P(g.next);
  let s = '';
  const moved = Math.abs(a.cx - b.cx) + Math.abs(a.cy - b.cy) + Math.abs(a.bw - b.bw) + Math.abs(a.r - b.r) > 1;
  if (moved) s += rect(a, 'fill="none" stroke="rgba(245,241,231,.45)" stroke-width="1" stroke-dasharray="4 4"');
  s += rect(b, 'fill="rgba(255,90,35,.05)" stroke="var(--ember)" stroke-width="1.25"');
  if (g.null) { const [x1, y1] = rot(b, b.cx - b.bw / 2, b.cy - b.bh / 2), [x2, y2] = rot(b, b.cx + b.bw / 2, b.cy + b.bh / 2), [x3, y3] = rot(b, b.cx + b.bw / 2, b.cy - b.bh / 2), [x4, y4] = rot(b, b.cx - b.bw / 2, b.cy + b.bh / 2); s += `<path d="M${px(x1)} ${px(y1)}L${px(x2)} ${px(y2)}M${px(x3)} ${px(y3)}L${px(x4)} ${px(y4)}" stroke="var(--ember)" stroke-width="1"/>`; }
  for (const [u, v] of [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]]) { const [x, y] = rot(b, b.cx + u * b.bw / 2, b.cy + v * b.bh / 2); s += `<rect x="${px(Math.min(w - 7, Math.max(0, x - 3.5)))}" y="${px(Math.min(h - 7, Math.max(0, y - 3.5)))}" width="7" height="7" fill="var(--cream)" stroke="#141412" stroke-width="1"/>`; }
  s += `<circle cx="${px(b.cx)}" cy="${px(b.cy)}" r="5" fill="none" stroke="var(--cream)" stroke-width="1.2"/><path d="M${px(b.cx - 9)} ${px(b.cy)}h18M${px(b.cx)} ${px(b.cy - 9)}v18" stroke="var(--cream)" stroke-width="1"/>`;
  if (moved && Math.hypot(a.cx - b.cx, a.cy - b.cy) > 3) { const ang = Math.atan2(b.cy - a.cy, b.cx - a.cx), hx = b.cx - 9 * Math.cos(ang), hy = b.cy - 9 * Math.sin(ang); s += `<path d="M${px(a.cx)} ${px(a.cy)}L${px(hx)} ${px(hy)}" stroke="var(--ember)" stroke-width="1.5"/><path d="M${px(b.cx)} ${px(b.cy)}L${px(hx - 6 * Math.cos(ang - .5))} ${px(hy - 6 * Math.sin(ang - .5))}M${px(b.cx)} ${px(b.cy)}L${px(hx - 6 * Math.cos(ang + .5))} ${px(hy - 6 * Math.sin(ang + .5))}" stroke="var(--ember)" stroke-width="1.5"/>`; }
  if (g.three) {   // 3D props: orbit rings around the anchor (rot_y horizontal, rot_x vertical) with the angle as an ember arc, axis handles
    const R = Math.min(b.bw, b.bh) * .32, ry = (g.next.rot_y || 0) * Math.PI / 180, rx = (g.next.rot_x || 0) * Math.PI / 180;
    s += `<ellipse cx="${px(b.cx)}" cy="${px(b.cy)}" rx="${px(R)}" ry="${px(R * .32)}" fill="none" stroke="rgba(245,241,231,.6)" stroke-width="1.1"/><ellipse cx="${px(b.cx)}" cy="${px(b.cy)}" rx="${px(R * .32)}" ry="${px(R)}" fill="none" stroke="rgba(245,241,231,.6)" stroke-width="1.1"/>`;
    const arcH = (a0, a1) => { const p = t => [b.cx + R * Math.sin(t), b.cy + R * .32 * Math.cos(t)]; const [x0, y0] = p(a0), [x1, y1] = p(a1); return `M${px(x0)} ${px(y0)}A${px(R)} ${px(R * .32)} 0 ${Math.abs(a1 - a0) > Math.PI ? 1 : 0} ${a1 > a0 ? 0 : 1} ${px(x1)} ${px(y1)}`; };
    const arcV = (a0, a1) => { const p = t => [b.cx + R * .32 * Math.sin(t), b.cy - R * Math.cos(t)]; const [x0, y0] = p(a0), [x1, y1] = p(a1); return `M${px(x0)} ${px(y0)}A${px(R * .32)} ${px(R)} 0 ${Math.abs(a1 - a0) > Math.PI ? 1 : 0} ${a1 > a0 ? 1 : 0} ${px(x1)} ${px(y1)}`; };
    if (Math.abs(ry) > 1e-3) s += `<path d="${arcH(0, ry)}" fill="none" stroke="var(--ember)" stroke-width="2.2"/>`;
    if (Math.abs(rx) > 1e-3) s += `<path d="${arcV(0, rx)}" fill="none" stroke="var(--ember)" stroke-width="2.2"/>`;
    for (const [dx, dy, col] of [[1, 0, '#e5533b'], [0, -1, '#e8a33a'], [-.55, .45, '#7fbf6a']]) { const L = R * 1.25, ex = b.cx + dx * L, ey = b.cy + dy * L; s += `<line x1="${px(b.cx)}" y1="${px(b.cy)}" x2="${px(ex)}" y2="${px(ey)}" stroke="${col}" stroke-width="1.6"/><circle cx="${px(ex)}" cy="${px(ey)}" r="3.5" fill="${col}"/>`; }
  }
  if (g.parent) { const q = P(g.parent); s += `<path d="M${px(b.cx)} ${px(b.cy)}L${px(q.cx)} ${px(q.cy)}" stroke="var(--cream)" stroke-width="1" stroke-dasharray="3 3"/><circle cx="${px(q.cx)}" cy="${px(q.cy)}" r="4" fill="var(--cream)"/>`; }
  if (g.path && g.path.length) {
    let pts;
    if (W && H) pts = g.path.map(([x, y]) => [w / 2 + x / W * w, h / 2 + y / H * h]);
    else { const xs = g.path.map(p => p[0]), ys = g.path.map(p => p[1]), mx = Math.min(...xs), my = Math.min(...ys), sx = Math.max(...xs) - mx || 1, sy = Math.max(...ys) - my || 1, k = Math.min(w * .6 / sx, h * .6 / sy); pts = g.path.map(([x, y]) => [w / 2 + (x - mx - sx / 2) * k, h / 2 + (y - my - sy / 2) * k]); }
    s += `<path d="${catmull(pts)}" fill="none" stroke="var(--ember)" stroke-width="1.5"${W && H ? '' : ' stroke-dasharray="6 4"'}/>` + pts.map(([x, y]) => `<path d="M${px(x)} ${px(y - 5)}l5 5-5 5-5-5z" fill="var(--cream)"/>`).join('');
  }
  // a removed motion path (normalized frame coordinates, as the engine stores it): dashed, hollow points
  if (g.gonePath && g.gonePath.length > 1) { const pts = g.gonePath.map(([x, y]) => [x * w, y * h]); s += `<path class="gone" d="${catmull(pts)}" fill="none" stroke="var(--cream)" stroke-opacity=".55" stroke-width="1.5" stroke-dasharray="5 5"/>` + pts.map(([x, y]) => `<path d="M${px(x)} ${px(y - 5)}l5 5-5 5-5-5z" fill="none" stroke="var(--ember)" stroke-width="1.3"/>`).join(''); }
  // readout pill at the box's top-left, kept inside the frame
  const rx = Math.max(8, Math.min(w - 240, b.cx - b.bw / 2 + 10)), ry = Math.max(8, Math.min(h - 34, b.cy - b.bh / 2 + 10));
  let html = `<svg class="ov gz" viewBox="0 0 ${w} ${h}" style="animation-delay:-${Math.round(age)}ms">${s}</svg>`;
  let bez = '';
  if (g.bez) { const [x1, y1, x2, y2] = g.bez, S2 = 36; bez = `<svg width="${S2}" height="${S2}" viewBox="-2 -2 ${S2 + 4} ${S2 + 4}" style="margin-left:2px;overflow:visible"><path d="M0 ${S2}L${S2} 0" stroke="#3a3935" stroke-width="1"/><path d="M0 ${S2}L${px(x1 * S2)} ${px(S2 - y1 * S2)}M${S2} 0L${px(x2 * S2)} ${px(S2 - y2 * S2)}" stroke="#8a867d" stroke-width="1"/><path d="M0 ${S2}C${px(x1 * S2)} ${px(S2 - y1 * S2)} ${px(x2 * S2)} ${px(S2 - y2 * S2)} ${S2} 0" fill="none" stroke="var(--ember)" stroke-width="1.5"/><circle cx="${px(x1 * S2)}" cy="${px(S2 - y1 * S2)}" r="2.5" fill="var(--cream)"/><circle cx="${px(x2 * S2)}" cy="${px(S2 - y2 * S2)}" r="2.5" fill="var(--cream)"/></svg>`; }
  html += `<span class="gzread gz" style="left:${px(rx)}px;top:${px(ry)}px;animation-delay:-${Math.round(age)}ms">${g.read.slice(0, 2).map(E).join('<i></i>')}${bez}</span>`;
  return html;
}

// ---------- key/value inspector ----------
function kv({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  html += `<div class="kv ${o.lines ? 'lines' : ''} ${o.topLine ? 'topline' : ''} ${o.secRule ? 'secrule' : ''}" style="top:${o.top ?? (data.sub ? 69 : 46)}px${o.pad != null ? `;padding:0 ${o.pad}px` : ''}">`;
  for (const b of (data.blocks || [])) html += block(b, w - 30);
  html += '</div>';
  // GUI-FIX-1: the last read's answer (get_* / list_* / inspect_* / QA), pinned to the panel's bottom edge, quiet styling:
  // a read is not a value of the project, so it never renders as an Inspector row
  const R = data.read;
  if (R && R.rows && R.rows.length) html += `<div class="readcard${F(R)}${R.title ? ' ' + E(R.title) : ''}"><div class="rh"><span class="rt">${E(R.title || 'read')}</span>${E(R.tool)}</div>${R.rows.map(r => `<div class="rr${r[2] === 'wrap' ? ' wrap' : r[2] === 'stack' ? ' stack' : ''}"><span class="rk">${E(r[0])}</span><span class="rv">${E(r[1])}</span></div>`).join('')}</div>`;
  return html;
}
function block(b, w) {
  if (b.sec !== undefined) return `<div class="sec" ${b.h ? `style="height:${b.h}px"` : ''}>${E(b.sec)}</div>`;
  if (b.gap) return `<div style="height:${b.gap}px"></div>`;
  if (b.list) return `<div class="list">${b.list.map((x, i) => { const m = /^(\d+)\s+(.*)$/.exec(String(x)); return `<div class="${i === b.sel ? 'sel' : ''}">${m ? `<span class="n">${E(m[1])}</span>${E(m[2])}` : E(x)}</div>`; }).join('')}</div>`;
  if (b.curve) {   // a colour curve as the call gave it (PCHIP points), drawn on a unit box
    const W2 = Math.max(60, w), H2 = 64, col = { r: '#e2553f', g: '#5fbf6a', b: '#4f7fe0' }[b.ch] || 'var(--cream)';
    const P = b.curve.map(([x, y]) => [px(6 + x * (W2 - 12)), px(H2 - 6 - y * (H2 - 12))]);
    return `<div class="widget" style="height:${H2}px;margin:6px 0"><svg viewBox="0 0 ${W2} ${H2}" width="100%" height="100%"><path d="M6 ${H2 - 6}L${W2 - 6} 6" stroke="#2a2a28" stroke-dasharray="3 3"/><path d="${catmull(P)}" fill="none" stroke="${col}" stroke-width="1.5"/>${P.map(([x, y]) => `<circle cx="${x}" cy="${y}" r="2.6" fill="var(--cream)"/>`).join('')}</svg></div>`;
  }
  if (b.code) return `<div class="code">${E(b.code)}</div>`;
  if (b.cap) return `<div class="cap"><span>${E(b.cap[0])}</span><span>${E(b.cap[1] || '')}</span></div>`;
  if (b.widget) return `<div class="widget${b.src ? ' img' : ''}" style="height:${b.h || 90}px${b.style ? ';' + b.style : ''}">${b.src ? `<img src="${E(b.src)}" alt="">` : (WIDGETS[b.widget] || (() => ''))(b, w, b.h || 90)}</div>`;
  if (b.html) return b.html;
  const cls = ['row', b.from != null || b.ch ? 'ch' : '', b.chip ? 'chip' : '', b.emb ? 'emb' : '', b.bar ? 'bar' : '', b.dim ? 'dim' : '', b.tight ? 'tight' : ''].join(' ');
  const v = b.from != null ? `<span class="from">${E(b.from)}</span><span class="arr">→</span>${E(b.v)}` : E(b.v);
  return `<div class="${cls}${F(b)}"${b.h ? ` style="height:${b.h}px"` : ''}><span class="k"${b.kStyle ? ` style="${b.kStyle}"` : ''}>${E(b.k)}</span><span class="v"${b.vStyle ? ` style="${b.vStyle}"` : ''}>${v}</span></div>`;
}

// ---------- small inspector widgets (SVG from data) ----------
const WIDGETS = {
  eq(b, w, h) { // b.bands: [{f,g,q,type}] ; response on a log axis 20..20k inside a box, decade labels under it
    const bh = h - (b.labH ?? 24), fx = f => (Math.log10(f) - Math.log10(20)) / 3 * w, gy = g => bh / 2 - g * (bh / 2) / 18;
    let s = `<rect x=".5" y=".5" width="${w - 1}" height="${bh - 1}" rx="4" fill="#161615" stroke="#2c2b29"/>`;
    for (let d = 10; d <= 10000; d *= 10) for (let k = 1; k <= 9; k++) { const f = d * k; if (f < 20 || f > 20000) continue; const x = px(fx(f)); s += `<line x1="${x}" x2="${x}" y1="1" y2="${bh - 1}" stroke="${k === 1 ? '#2e2d2a' : '#222120'}"/>`; }
    for (let g = -18; g <= 18; g += 4.5) s += `<line x1="1" x2="${w - 1}" y1="${px(gy(g))}" y2="${px(gy(g))}" stroke="${g === 0 ? '#34322e' : '#222120'}"/>`;
    const resp = f => (b.bands || []).reduce((sum, B) => {
      const x = Math.log2(f / B.f);
      if (B.type === 'highpass') return sum - 10 * Math.log10(1 + Math.pow(B.f / f, 2 * (B.n ?? 2)));   // Butterworth-shaped skirts
      if (B.type === 'lowpass') return sum - 10 * Math.log10(1 + Math.pow(f / B.f, 2 * (B.n ?? 1)));
      if (B.type === 'lowshelf') return sum + B.g / (1 + Math.exp(x * 4));
      if (B.type === 'highshelf') return sum + B.g / (1 + Math.exp(-x * 4));
      return sum + B.g * Math.exp(-(x * x) * (B.q || 1) * 1.6);
    }, 0);
    const pts = []; for (let i = 0; i <= 160; i++) { const f = 20 * Math.pow(1000, i / 160); pts.push([fx(f), Math.min(bh - 1, Math.max(1, gy(resp(f))))]); }
    const d = pts.map((q, i) => (i ? 'L' : 'M') + px(q[0]) + ' ' + px(q[1])).join('');
    s += `<path d="${d} L${w} ${px(gy(0))} L0 ${px(gy(0))}Z" fill="rgba(190,178,146,.22)"/><path d="${d}" fill="none" stroke="#e9e3d6" stroke-width="1.6"/>`;
    for (const B of (b.bands || [])) if (B.g != null && !/pass/.test(B.type || '')) s += `<circle cx="${px(fx(B.f))}" cy="${px(gy(resp(B.f)))}" r="${B.sel ? 5.5 : 4.5}" fill="${B.sel ? '#efe9dc' : B.dim ? '#8d887e' : '#cfc9bd'}" stroke="#121211" stroke-width="1.5"${F(B).trim() ? ' class="flash"' : ''}/>`;
    s += [100, 1000, 10000].map((f, i) => `<text x="${px(fx(f))}" y="${bh + (b.labY ?? 15)}" font-size="${b.labFs ?? 11}" fill="var(--dim)" text-anchor="middle" font-family="var(--mono)">${['100', '1k', '10k'][i]}</text>`).join('');
    return `<svg viewBox="0 0 ${w} ${h}">${s}</svg>`;
  },
  comp(b, w, h) { // compressor transfer curve in a box + Threshold / Ratio rows beside it
    const bw = b.boxW ?? 76, bh = b.boxH ?? 67, x0 = .5, y0 = b.boxY ?? 7, th = b.threshold ?? -18, r = b.ratio ?? 3;
    const X = db => x0 + (db + 60) / 60 * bw, outDb = db => db < th ? db : th + (db - th) / r, top = outDb(0);
    const Y = db => y0 + bh - (db + 60) / (top + 60) * bh;   // output scaled so 0 dB in lands on the box's top edge
    let g = `<rect x="${x0}" y="${y0}" width="${bw}" height="${bh}" rx="3" fill="#161615" stroke="#2c2b29"/>`;
    for (let k = 1; k < 4; k++) g += `<line x1="${px(x0 + bw * k / 4)}" x2="${px(x0 + bw * k / 4)}" y1="${y0}" y2="${y0 + bh}" stroke="#222120"/><line x1="${x0}" x2="${x0 + bw}" y1="${px(y0 + bh * k / 4)}" y2="${px(y0 + bh * k / 4)}" stroke="#222120"/>`;
    const curve = []; for (let db = -60; db <= 0; db += 1) curve.push([X(db), Y(outDb(db))]);
    const kx = X(th), ky = Y(th);
    g += `<path d="M${x0} ${y0 + bh} ${curve.filter(q => q[0] <= kx).map(q => 'L' + px(q[0]) + ' ' + px(q[1])).join(' ')} L${px(kx)} ${y0 + bh}Z" fill="rgba(190,178,146,.22)"/>` +
      `<rect x="${px(kx)}" y="${px(ky)}" width="${px(Math.min(9, bw - kx))}" height="${px(y0 + bh - ky)}" fill="rgba(0,0,0,.28)"/>` +
      `<path d="${curve.map((q, i) => (i ? 'L' : 'M') + px(q[0]) + ' ' + px(q[1])).join('')}" fill="none" stroke="#e9e3d6" stroke-width="1.6"/>`;
    // live gain reduction while a compress call runs: ember dot at the input level, ember bar = reduction
    if (b.inDb != null && b._grT != null && performance.now() - b._grT < 1600) {
      const ix = X(b.inDb), iy = Y(outDb(b.inDb)), uy = Y(b.inDb);
      g += `<line x1="${px(ix)}" x2="${px(ix)}" y1="${px(Math.max(y0, uy))}" y2="${px(iy)}" stroke="var(--ember)" stroke-width="3"/><circle cx="${px(ix)}" cy="${px(iy)}" r="3.5" fill="var(--ember)"/>`;
    }
    const tx = bw + (b.gap ?? 18), fs = b.fs ?? 17, ry = [y0 + bh * .21, y0 + bh * .81];
    const row = (y, k, v, flash) => `<text x="${tx}" y="${px(y + fs * .35)}" font-size="${fs}" fill="var(--klabel,var(--text))" style="font-family:var(--label-font)">${E(k)}</text><text x="${w}" y="${px(y + fs * .35)}" font-size="${fs}" fill="${flash ? 'var(--ember-text)' : 'var(--cream)'}" text-anchor="end" font-family="var(--mono)">${E(v)}</text>`;
    g += row(ry[0], 'Threshold', `${th} dB`, flashing(b._flashT)) + `<line x1="${tx}" x2="${w}" y1="${px((ry[0] + ry[1]) / 2)}" y2="${px((ry[0] + ry[1]) / 2)}" stroke="var(--line2)"/>` + row(ry[1], 'Ratio', `${r}:1`, flashing(b._flashR));
    return `<svg viewBox="0 0 ${w} ${h}" style="overflow:visible">${g}</svg>`;
  },
  solve(b, w, h) { // camera solve: point cloud + frustum + dotted orbit
    const R = rng(b.seed || 5); let s = '';
    for (let i = 0; i < 260; i++) { const a = R() * Math.PI * 2, rr = Math.pow(R(), .6) * h * .32; s += `<circle cx="${px(w * .5 + Math.cos(a) * rr * 1.7)}" cy="${px(h * .55 + Math.sin(a) * rr * .55)}" r="${R() < .2 ? 1.2 : .7}" fill="${R() < .5 ? '#cfcac0' : '#77736b'}"/>`; }
    s += `<ellipse cx="${w * .5}" cy="${h * .55}" rx="${w * .38}" ry="${h * .22}" fill="none" stroke="#8a867d" stroke-dasharray="2 4"/>`;
    const cx = w * .22, cy = h * .32;
    s += `<path d="M${cx} ${cy} l22 -9 l0 22 z" fill="none" stroke="var(--ember)" stroke-width="1.3"/><circle cx="${cx}" cy="${cy}" r="2.5" fill="var(--ember)"/>`;
    return `<svg viewBox="0 0 ${w} ${h}">${s}</svg>`;
  },
  swatches(b, w, h) {
    const n = b.items.length, gap = b.gap2 ?? 10, d = b.d ?? Math.min(h - 6, (w - gap * (n - 1)) / n);
    let s = '<defs>' + b.items.map((it, i) => `<radialGradient id="sw${i}" cx="35%" cy="30%" r="75%"><stop offset="0" stop-color="${it.hi}"/><stop offset=".55" stop-color="${it.mid}"/><stop offset="1" stop-color="${it.lo}"/></radialGradient>`).join('') + '</defs>';
    b.items.forEach((it, i) => {
      const x = i * (d + gap) + d / 2;
      s += `<rect x="${x - d / 2 + .75}" y="${.75}" width="${d - 1.5}" height="${d - 1.5}" rx="6" fill="${it.src ? '#1c1b19' : '#141413'}" stroke="${it.sel ? 'var(--cream)' : it.src ? 'none' : '#2a2a28'}" stroke-width="${it.sel ? 1.5 : 1}"/>` + (it.src ? `<image href="${E(it.src)}" x="${x - d / 2 + 4}" y="4" width="${d - 8}" height="${d - 8}" preserveAspectRatio="xMidYMid slice"/>` : `<circle cx="${x}" cy="${d / 2}" r="${d / 2 - 7}" fill="url(#sw${i})"/>`);
    });
    return `<svg viewBox="0 0 ${w} ${h}">${s}</svg>`;
  },
  thumbs(b, w, h) {
    const n = b.items.length, gap = b.tgap ?? 8, lh = b.labH ?? 18;   // items may carry their own width (m15: the selected clip is wider)
    const W = t => t.w ? `flex:none;width:${t.w}px` : 'flex:1';
    return `<div style="display:flex;gap:${gap}px;height:100%;align-items:flex-start">` + b.items.map(t => `<div class="thumb ${t.sel ? 'sel' : ''}" style="${W(t)};height:${t.h ?? (b.items.some(x => x.label) ? h - lh : h)}px">${t.src ? `<img src="${E(t.src)}">` : ''}</div>`).join('') + '</div>' +
      (b.items.some(t => t.label) ? `<div style="display:flex;gap:${gap}px;position:absolute;left:0;right:0;bottom:0;height:${lh}px;align-items:center">${b.items.map(t => `<span style="${W(t)};text-align:center;font-family:var(--mono);font-size:var(--thl-fs,10.5px);color:var(--thl-c,var(--dim))">${E(t.label || '')}</span>`).join('')}</div>` : '');
  },
};

// ---------- timeline (edit / effects / graphics / captions / audio) ----------
function rulerX(r, x0, x1) { return t => x0 + (t - r.t0) / (r.t1 - r.t0) * (x1 - x0); }
function fmtT(t, f) {
  if (f === 'n') return String(t);
  if (f === 's') return t + 's';
  const m = Math.floor(t / 60), s = Math.round(t % 60);
  return String(m).padStart(2, '0') + ':' + String(s).padStart(2, '0');
}
function ruler(r, X, y, o = {}) {
  let s = `<div class="ruler" style="top:${y}px">`;
  if (r.minor) for (let t = r.minorFrom ?? r.t0; t <= r.t1 + 1e-6; t += r.minor) s += `<i class="mn" style="left:${px(X(t))}px"></i>`;
  const last = r.last ?? r.t1;
  for (let t = r.first ?? r.t0; t <= last + 1e-6; t += r.step) {
    const x = X(t);
    const tr = r.edge && t === r.t0 ? 'none' : r.edge && t + r.step > last + 1e-6 ? 'translateX(-100%)' : r.align === 'right' ? 'translateX(-100%) translateX(-5px)' : r.align === 'left' ? `translateX(${r.lx ?? 4}px)` : 'translateX(-50%)';
    s += `<span style="left:${px(x)}px;transform:${tr}">${fmtT(Math.round(t * 100) / 100, r.fmt)}</span>`;
    if (r.ticks !== false) s += `<i style="left:${px(x)}px"></i>`;
  }
  return s + '</div>';
}
function peaksPath(w, h, s) { // s: base-36 peak string, one char per column (0..35)
  const n = s.length, up = [], dn = [];
  for (let i = 0; i < n; i++) { const a = DIG.indexOf(s[i]) / 35 * (h / 2 - 1), x = i / (n - 1) * w; up.push(`${px(x)},${px(h / 2 - a)}`); dn.push(`${px(x)},${px(h / 2 + a)}`); }
  return `M${up.join(' L')} L${dn.reverse().join(' L')}Z`;
}
function wave(w, h, seed, amp = .8, dens = 1, sym = true) {
  const R = rng(seed); const n = Math.max(8, Math.floor(w / 2.2 * dens)); const up = [], dn = [];
  let env = .5;
  for (let i = 0; i <= n; i++) {
    env += (R() - .5) * .35; env = Math.max(.15, Math.min(1, env));
    const a = (R() * .55 + .45) * env * amp * (h / 2);
    const x = i / n * w; up.push(`${px(x)},${px(h / 2 - a)}`); dn.push(`${px(x)},${px(h / 2 + (sym ? a : a * .9))}`);
  }
  return `M${up.join(' L')} L${dn.reverse().join(' L')}Z`;
}
function smoothArea(w, h, a) {
  const R = rng(a.seed || 1); const pts = [];
  if (a.pts) a.pts.forEach((v, i) => pts.push([i / (a.pts.length - 1) * w, v * h]));  // explicit envelope (0 = top)
  else { const n = 9; for (let i = 0; i <= n; i++) pts.push([i / n * w, h * (a.base ?? .5) + (R() - .5) * h * (a.amp ?? .35)]); }
  let d = `M0 ${h} L${px(pts[0][0])} ${px(pts[0][1])}`;
  for (let i = 1; i < pts.length; i++) { const [x0, y0] = pts[i - 1], [x1, y1] = pts[i], mx = (x0 + x1) / 2; d += ` C${px(mx)} ${px(y0)} ${px(mx)} ${px(y1)} ${px(x1)} ${px(y1)}`; }
  return d + ` L${px(w)} ${h}Z`;
}
// Pan / zoom of a timeline panel (app.js wheel handler; nothing drawn for it, no scrollbar or control): pan.z > 1
// shows 1/z of the ruler's span starting at pan.t seconds, on a ruler re-stepped for the visible span; pan.k is the
// first track row drawn (row-snapped vertical scroll, so a track never draws half under the ruler). Both are
// clamped here and written back. With no pan the panel draws exactly as before.
const PAN_STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
function panRuler(r, pan, wpx) {
  const full = r.t1 - r.t0, span = full / pan.z, t0 = Math.min(Math.max(pan.t ?? r.t0, r.t0), r.t1 - span);
  pan.t = t0;
  const step = PAN_STEPS.find(s => s * wpx / span >= 72) || PAN_STEPS[PAN_STEPS.length - 1];
  return { t0, t1: t0 + span, step, first: Math.ceil(t0 / step - 1e-9) * step, last: t0 + span, fmt: r.fmt, ticks: r.ticks };
}
export function panRows(tracks, o, h, pan) {
  let kmax = 0, need = o.trackY ?? 30;
  for (let i = tracks.length - 1; i >= 0; i--) { need += (tracks[i].h ?? o.trackH ?? 38) + (o.trackGap ?? 0); if (need > h + (o.trackGap ?? 0)) { kmax = i + 1; break; } }
  pan.k = Math.min(Math.max(pan.k || 0, 0), kmax); pan.kmax = kmax;
  return pan.k;
}
// The last timeline's markup in parts (head + ruler, one string per track, playhead / razor): app.js rewrites only
// the tracks whose markup changed, so an edit on one track never re-parses the other five. RUNS caches the merged
// overview runs (a 1k-clip track is a few hundred seams in one gradient) by geometry.
export let TL_PARTS = null;
let RUNS = new Map(), RUNS_NEXT = new Map();   /* the runs the last timeline drew, and the ones this one draws: only the current set is kept */
// Small windows only: set by app.js when it gives the Inspector rows taken from the timeline band (fitScale < 1). A
// timeline with plain track rows (trackH up to 38) then draws them compact, so the shorter band still holds 2 whole
// tracks; text sizes are unchanged. At fitScale 1 it is always off and the rows are the mode's own.
export const TL_SMALL = { on: false };
const TL_SMALL_ROWS = { trackY: 34, trackH: 30, trackGap: 6 };
function timeline({ spec, data, w, h, pan }) {
  const o0 = spec.opts || {};
  const o = TL_SMALL.on && o0.trackY != null && (o0.trackH ?? 38) <= 38
    ? { ...o0, trackY: Math.min(o0.trackY, TL_SMALL_ROWS.trackY), trackH: Math.min(o0.trackH ?? 38, TL_SMALL_ROWS.trackH), trackGap: Math.min(o0.trackGap ?? 0, TL_SMALL_ROWS.trackGap) }
    : o0;
  const x0 = o.x0 ?? 48, x1 = w - (o.xr ?? 12);
  let r = data.ruler || { t0: 0, t1: 20, step: 5 };
  const r0 = r;
  if (pan && pan.z > 1) r = panRuler(r, pan, x1 - x0);
  const X = rulerX(r, x0, x1), inView = x => !pan || pan.z <= 1 || (x >= x0 - 1 && x <= x1 + 1);
  let html = head(spec, data);
  html += `<div class="tl">` + ruler(r, X, o.rulerY ?? 6);
  let y = o.trackY ?? 30;
  const pre = html;
  const TR = data.tracks || [], pv = pan || {}, k0 = panRows(TR, o, h, pv);   /* rows are measured with or without a pan */
  let below = 0;   /* tracks wholly under the panel's bottom edge */
  const tracks = [];   /* each track's markup on its own, so a frame can rewrite just the track that changed (app.js paint) */
  for (const t of (k0 ? TR.slice(k0) : TR)) {
    let tr = '';
    const th = t.h ?? o.trackH ?? 38;
    tr += `<div class="trk ${t.cls || ''}${t.mute ? ' muted' : ''}${F(t)}" style="top:${y}px;height:${th}px">`;
    if (t.area) tr += `<svg style="position:absolute;left:${px(X(t.area.s ?? r.t0))}px;top:0;width:${px((x1) - X(t.area.s ?? r.t0))}px;height:${th}px" viewBox="0 0 ${px(x1 - X(t.area.s ?? r.t0))} ${th}"><rect width="100%" height="100%" rx="7" fill="${t.area.bg || '#2a2824'}"/><path d="${smoothArea(x1 - X(t.area.s ?? r.t0), th, t.area)}" fill="${t.area.fill || '#4a4741'}"/></svg>`;
    if (t.mute || t.solo) tr += `<span class="ms" style="left:${o.msX ?? 34}px">${t.mute ? '<b class="m">M</b>' : ''}${t.solo ? '<b class="s">S</b>' : ''}</span>`;
    if (t.label) tr += t.chip ? `<span class="labchip" style="left:${o.chipX ?? 10}px;width:${o.chipW ?? 120}px">${E(t.label)}</span>` : `<span class="lab">${E(t.label)}</span>`;
    if (t.lane) { const li = t.laneInset ?? o.laneInset ?? 3; tr += `<div class="lane" style="position:absolute;left:${x0}px;right:${w - x1}px;top:${li}px;bottom:${li}px;background:${t.lane}"></div>`; }
    if (t.wave) {
      const a = X(t.wave.s ?? r.t0), b = X(t.wave.e ?? r.t1), wh = th - 6;
      tr += `<svg style="position:absolute;left:${px(a)}px;top:3px;width:${px(b - a)}px;height:${wh}px" viewBox="0 0 ${px(b - a)} ${wh}"><path d="${t.wave.peaks ? peaksPath(b - a, wh, t.wave.peaks) : wave(b - a, wh, t.wave.seed || 3, t.wave.amp ?? .8, t.wave.dens ?? 1)}" fill="${t.wave.color || '#5f5c56'}"/>` +
        (t.wave.env ? `<path d="${catmull(t.wave.env.map(p => [X(p[0]) - a, wh * (1 - p[1])]))}" fill="none" stroke="var(--ember)" stroke-width="${o.envW ?? 1.5}"/>` : '') +
        (t.wave.beats ? t.wave.beats.map(bt => `<line x1="${px(X(bt) - a)}" x2="${px(X(bt) - a)}" y1="${o.beatY0 ?? 0}" y2="${o.beatY1 ?? 5}" stroke="var(--cream)" stroke-width="${o.beatW ?? 1.2}"/>`).join('') : '') +
        `</svg>`;
    }
    // virtualized: a track below the panel draws nothing; clips outside the ruler are skipped; clips under MINW px
    // wide (a long timeline fitted into the panel) merge into one plain run per stretch, so the panel holds at most
    // about one element per few pixel columns whatever the clip count (tests/timeline_perf_test.py: 1,000 clips)
    if (y >= h) { tr += '</div>'; tracks.push(tr); html += tr; y += th + (o.trackGap ?? 0); below++; continue; }
    const MINW = 12; let run = null;   // under 12 px a clip has no room for its label; its edges stay as 1 px seams in the run
    // a flashing (just edited) narrow clip draws on its own so the edit stays visible; when a burst flashes more than
    // FLASH_SOLO of them on one track (a 1,000-clip load), they merge like the rest and their run carries the flash
    const FLASH_SOLO = 24; let nf = 0; for (const c of (t.clips || [])) if (c._flash != null && F(c).trim() && ++nf > FLASH_SOLO) break;
    const flush = () => { if (!run) return;
      let h1 = 0x811c9dc5 | 0, h2 = 7; const mix = v => { const q = Math.round(v * 1000); h1 = Math.imul(h1 ^ q, 16777619); h2 = (Math.imul(h2, 31) + q) | 0; };
      mix(run[0]); mix(run[1]); for (const x of run[2]) mix(x);
      const key = `${h1}:${h2}:${run[2].length}${run[3] || ''}`;   /* a run unchanged since the last frame reuses its markup */
      let s = RUNS.get(key) ?? RUNS_NEXT.get(key);
      if (s == null) {
        const w0 = Math.max(1, run[1] - run[0]), cuts = run[2].filter(x => x > run[0] + .5 && x < run[1] - .5).map(x => px(x - run[0]));
        const bg = cuts.length ? `;background:linear-gradient(90deg,${cuts.map((x, i) => `var(--clip) ${i ? px(cuts[i - 1] + 1) : 0}px ${x}px,transparent ${x}px ${px(x + 1)}px`).join(',')},var(--clip) ${px(cuts[cuts.length - 1] + 1)}px)` : '';
        s = `<div class="clip crun${run[3] || ''}" style="left:${px(run[0])}px;width:${px(w0)}px${bg}"></div>`;
      }
      RUNS_NEXT.set(key, s);
      tr += s; run = null; };
    const CL = t.clips || [];
    for (let ci = 0; ci < CL.length; ci++) {
      const c = CL[ci], a = X(c.s), b = X(c.e);
      if (b < x0 - 1 || a > x1 + 1) continue;
      const fl = F(c).trim();
      if (b - a < MINW && !c.keys && (!fl || nf > FLASH_SOLO) && !c.pill && !c.cls && !c.bg && !c.thumb && !c.cover && !c.fx && !c.beats) { if (run && a <= run[1] + 1) { run[2].push(a); run[1] = Math.max(run[1], b); if (fl) run[3] = ' ' + fl; } else { flush(); run = [a, b, [], fl ? ' ' + fl : '']; } continue; }
      flush();
      const nx = CL[ci + 1], xn = nx && nx.s != null ? X(nx.s) : null, cw = xn != null && xn > a + 2 && xn < b + 1 ? px(xn) - px(a) - 1 : b - a;   /* back to back (or the next clip laid over this one's tail): this clip ends 1 px before the next */
      tr += `<div class="clip ${c.cls || ''}${F(c)}" style="left:${px(a)}px;width:${px(cw)}px${c.bg ? ';background:' + c.bg : ''}">` +
        (c.thumb ? `<span class="ct"><img src="${E(c.thumb)}" alt=""></span>` : c.sw ? '<span class="ct sw"></span>' : '') + (c.cover ? `<img src="${E(c.cover)}" alt="">` : '') + (c.label ? `<span class="cl">${E(c.label)}</span>` : '') +
        (c.pill ? `<span class="cpill">${E(c.pill)}</span>` : '') +
        (c.beats ? c.beats.map(t => `<i class="cbeat" style="left:${px(X(t) - a)}px"></i>`).join('') : '') +
        (c.keys ? c.keys.map(k => `<i class="ckey${k.em ? ' em' : ''}" style="left:${px(X(k.t) - a)}px"></i>`).join('') : '') +
        (c.fx ? (o.fxRow ? `<span class="fxrow">${c.fx.map(f => `<span class="fxchip">${E(f)}</span>`).join('')}</span>` : c.fx.map(f => `<span class="fxchip">${E(f)}</span>`).join('')) : '') + '</div>';
    }
    flush();
    tr += '</div>'; tracks.push(tr); html += tr;
    y += th + (o.trackGap ?? 0);
  }
  let post = '';
  if (data.playhead != null && inView(X(data.playhead))) post += `<div class="playhead" style="left:${px(X(data.playhead))}px;top:${o.phTop ?? 4}px;bottom:${o.phBot ?? 8}px"></div>`;
  if (data.razor && flashing(data.razor.t) && inView(X(data.razor.at))) post += `<div class="razor" style="left:${px(X(data.razor.at))}px;top:${o.trackY ?? 30}px;bottom:4px"></div>`;
  // where the view sits: zoomed in, a 2 px bar on the ruler's bottom edge with the visible span as its thumb; whole rows
  // above or below the panel, a 2 px bar at the right edge with the visible rows as its thumb (no control, nothing to grab)
  if (pan && pan.z > 1) { const full = r0.t1 - r0.t0, W = x1 - x0, bw = Math.max(6, W / pan.z), a = Math.min(W - bw, (r.t0 - r0.t0) / full * W);
    post += `<div class="tlbar" style="left:${x0}px;width:${px(W)}px;top:${(o.rulerY ?? 6) + 23}px"><i style="left:${px(a)}px;width:${px(bw)}px"></i></div>`; }
  if (k0 + below > 0) { const ty = o.trackY ?? 30, H = h - ty - 8, n = TR.length, vis = n - k0 - below;
    post += `<div class="tlbar v" style="top:${ty}px;height:${px(H)}px"><i style="top:${px(k0 / n * H)}px;height:${px(vis / n * H)}px"></i></div>`; }
  post += covAPost(data, X, o, inView, flashing);   /* markers, detected cuts, silence joints */
  html += post + '</div>';
  TL_PARTS = { html, pre, tracks, post };
  if (RUNS_NEXT.size) { RUNS = RUNS_NEXT; RUNS_NEXT = new Map(); }
  return html;
}

// ---------- key lanes (mask keys, keyframes, simulation cache, residual) ----------
function keys({ spec, data, w, h }) {
  const o = spec.opts || {};
  const x0 = o.x0 ?? 190, x1 = w - (o.xr ?? 14);
  const r = data.ruler || { t0: 0, t1: 20, step: 5 }; const X = rulerX(r, x0, x1);   // a live session hops here before any key exists
  let html = head(spec, data, data.metaChip ? `<span class="chip" style="padding:3px 8px;border-radius:4px;background:var(--raise);color:var(--text)">${E(data.metaChip)}</span>` : '');
  html += '<div class="tl">';
  if (data.strip) { // mini curve strip above ruler
    const sy = o.stripY ?? 4, sh = o.stripH ?? 40;
    let s = o.stripBg ? `<rect x="${o.stripX ?? 0}" y="0" width="${w - (o.stripX ?? 0)}" height="${sh}" rx="${o.stripR ?? 0}" fill="${o.stripBg}"/>` : '';
    const sp = o.stripPad ?? 4;
    for (const c of data.strip) s += `<path d="${c.pts.map((p, i) => (i ? 'L' : 'M') + px(X(p[0])) + ' ' + px(sh - sp - p[1] * (sh - 2 * sp))).join('')}" fill="none" stroke="${c.em ? 'var(--ember)' : (o.stripC ?? '#bdb8ae')}" stroke-width="${o.stripW ?? 1.4}"${c.em && o.stripEmOp ? ` stroke-opacity="${o.stripEmOp}"` : ''} stroke-linejoin="round"/>`;
    html += `<svg style="position:absolute;left:0;top:${sy}px;width:${w}px;height:${sh}px" viewBox="0 0 ${w} ${sh}">${s}</svg>`;
  }
  if (data.graph) { // residual-style graph box
    const g = data.graph, gy = o.graphY ?? 30, gh = o.graphH ?? 76, gx = o.graphX ?? x0;
    const vy = v => gh - 6 - (v / g.max) * (gh - 14);
    const d = g.pts.map((v, i) => (i ? 'L' : 'M') + px(X(r.t0 + i * (r.t1 - r.t0) / (g.pts.length - 1)) - gx) + ' ' + px(vy(v))).join('');
    html += `<svg style="position:absolute;left:${gx}px;top:${gy}px;width:${x1 - gx + 8}px;height:${gh}px;background:var(--graph-bg,#121211);border-radius:4px" viewBox="0 0 ${x1 - gx + 8} ${gh}">` +
      (g.thr != null ? `<line x1="0" x2="${x1 - gx + 8}" y1="${vy(g.thr)}" y2="${vy(g.thr)}" stroke="#8a867d" stroke-dasharray="3 3"/><text x="${x1 - gx}" y="${vy(g.thr) - 4}" font-size="10" fill="var(--dim)" text-anchor="end">${E(g.thrLabel ?? '')}</text>` : '') +
      `<path d="${d}" fill="none" stroke="var(--cream)" stroke-width="1.3"/></svg>`;
  }
  const XR = o.rLabelK ? (t => x0 + (X(t) - x0) * o.rLabelK) : X;   // m13: the mockup's ruler labels run 0.6% wider than its keys
  html += ruler(r, XR, o.rulerY ?? 6);
  // m13: thin second ticks just left of each ruler label, and the playhead's second in ember
  if (o.rTickDx != null) for (let t = r.first ?? r.t0; t <= (r.last ?? r.t1) + 1e-6; t += r.step) html += `<i style="position:absolute;left:${px(XR(t) + o.rTickDx)}px;top:${(o.rulerY ?? 6) + (o.rTickY ?? 2)}px;height:${o.rTickH ?? 16}px;width:1px;background:${o.rTickC ?? '#2e2d2a'}"></i>`;
  if (o.rulerHl && data.playhead != null) { const t = Math.round(data.playhead); if (Math.abs(t - data.playhead) < 1e-6) html += `<div class="ruler" style="top:${o.rulerY ?? 6}px;pointer-events:none"><span style="left:${px(XR(t))}px;transform:translateX(${r.lx ?? 4}px);color:var(--ember);font-weight:600;background:var(--panel);padding-right:2px">${fmtT(t, r.fmt)}</span></div>`; }
  if (o.sepX != null) html += `<i style="position:absolute;left:${o.sepX}px;top:${o.sepY ?? 5}px;height:${o.sepH ?? 25}px;width:1px;background:var(--line)"></i>`;
  if (o.divX != null) html += `<i style="position:absolute;left:${o.divX}px;top:0;bottom:0;width:1px;background:var(--line)"></i>`;
  if (o.headLine != null) html += `<i style="position:absolute;left:0;width:${o.divX ?? w}px;top:${o.headLine}px;height:1px;background:var(--line)"></i>`;
  let y = o.laneY ?? 32;
  (data.lanes || []).forEach((L, li) => {
    const lh = L.h ?? o.laneH ?? 32;
    // selBg / altBg (every other lane) cover the whole row, or only the key area right of o.selX
    const bg = L.sel ? (o.selBg ?? 'rgba(255,90,35,.07)') : (o.altBg && li % 2 === 0 ? o.altBg : null);
    // m13: the selected lane warms toward the playhead and cools off after it (the mockup's glow), else flat
    const ph = L.sel && o.selGlow && data.playhead != null ? X(data.playhead) : null, G = o.selGlow;
    const bgCss = bg ? (ph != null && o.selX != null ? `;background:linear-gradient(90deg,transparent ${o.selX}px,${bg} ${o.selX}px,${bg} ${px(Math.max(o.selX, ph - G[1]))}px,${G[0]} ${px(ph)}px,${bg} ${px(ph + G[2])}px)`
      : o.selX != null ? `;background:linear-gradient(90deg,transparent ${o.selX}px,${bg} ${o.selX}px)` : `;background:${bg}`) : '';
    html += `<div class="trk" style="top:${y}px;height:${lh}px${bgCss}${o.laneSep ? `;border-top:1px solid ${o.laneSep}` : ''}">`;
    html += `<span class="labchip" style="left:${o.chipX ?? 10}px;width:${o.chipW ?? (x0 - 20)}px">${L.html ? L.label : E(L.label)}</span>`;
    const mid = lh / 2;
    let s = '';
    if (o.laneBg) s += `<rect x="${px(x0 - 6)}" y="${px(mid - (o.laneBgH ?? lh - 4) / 2)}" width="${px(x1 - x0 + 10)}" height="${o.laneBgH ?? lh - 4}" rx="${o.laneBgR ?? 4}" fill="${o.laneBg}"/>`;
    if (L.bar) for (const [a, b, c] of L.bar) s += `<rect x="${px(X(a))}" y="${px(mid - (o.barH ?? 9))}" width="${px(X(b) - X(a))}" height="${(o.barH ?? 9) * 2}" rx="3" fill="${c || '#6b665d'}"/>`;
    if (L.cache) { const [done, total] = L.cache, ch = o.cacheH ?? 18; s += `<rect x="${px(X(done))}" y="${mid - ch / 2}" width="${px(x1 - X(done))}" height="${ch}" rx="3" fill="#070707" stroke="#2a2a28"/><rect x="${x0}" y="${mid - ch / 2}" width="${px(X(done) - x0)}" height="${ch}" rx="4" fill="${o.cacheC ?? '#bdb8ae'}"/>`; for (let f = done + 2; f < total; f += 2) s += `<line x1="${px(X(f))}" x2="${px(X(f))}" y1="${mid - 4}" y2="${mid + 4}" stroke="#2a2a28"/>`; }
    if (L.area) { const pts = L.area, ah = o.areaH ?? 18, ab = mid + ah / 2; s += `<path d="M${px(X(pts[0][0]))} ${ab} ${pts.map(p => `L${px(X(p[0]))} ${px(ab - p[1] * ah)}`).join(' ')} L${px(X(pts[pts.length - 1][0]))} ${ab}Z" fill="${L.color || '#5a5650'}"${L.stroke ? ` stroke="${L.stroke}" stroke-width="1"` : ''}${o.areaR ? ` stroke-linejoin="round"` : ''}/>`; }
    if (L.dots) for (const t of L.dots) s += `<circle cx="${px(X(t))}" cy="${mid}" r="${L.r || 2.6}" fill="${L.dotColor || '#77736b'}"/>`;
    if ((L.line || o.keyLine) && L.keys && L.keys.length > 1) { const ts = L.keys.map(k => typeof k === 'number' ? k : k.t); s += `<line x1="${px(X(Math.min(...ts)))}" x2="${px(X(Math.max(...ts)))}" y1="${mid}" y2="${mid}" stroke="${o.keyLine || '#5a5853'}" stroke-width="1"/>`; }
    if (L.keys) for (const k of L.keys) { const t = typeof k === 'number' ? k : k.t; const em = typeof k === 'object' && k.em; const d = (L.size || o.keySize || 5.5) * (em && o.emKeyScale ? o.emKeyScale : 1); s += `<path d="M${px(X(t))} ${mid - d} L${px(X(t) + d)} ${mid} L${px(X(t))} ${mid + d} L${px(X(t) - d)} ${mid}Z" fill="${em ? 'var(--ember)' : (L.keyColor || 'var(--cream)')}"/>`; }
    html += `<svg style="position:absolute;left:0;top:0;width:${w}px;height:${lh}px" viewBox="0 0 ${w} ${lh}">${s}</svg></div>`;
    y += lh + (o.laneGap ?? 0);
  });
  if (data.playhead != null) html += `<div class="playhead" style="left:${px(X(data.playhead) + (o.phDx ?? 0))}px;top:${o.phTop ?? 4}px;bottom:${o.phBot ?? 8}px"></div>`;
  return html + '</div>';
}

// ---------- graph editor (curves) ----------
function catmull(P) { // smooth path through points (uniform Catmull-Rom as cubic beziers)
  let d = `M${px(P[0][0])} ${px(P[0][1])}`;
  for (let i = 0; i < P.length - 1; i++) {
    const p0 = P[i - 1] || P[i], p1 = P[i], p2 = P[i + 1], p3 = P[i + 2] || p2;
    const c1 = [p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6], c2 = [p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6];
    d += ` C${px(c1[0])} ${px(c1[1])} ${px(c2[0])} ${px(c2[1])} ${px(p2[0])} ${px(p2[1])}`;
  }
  return d;
}
function curves({ spec, data, w, h }) {
  const o = spec.opts || {};
  const x0 = o.x0 ?? 196, x1 = w - (o.xr ?? 12);
  const r = data.ruler || { t0: 0, t1: 20, step: 5 }; const X = rulerX(r, x0, x1);   // a live session hops here before any key exists
  const gy0 = o.gy0 ?? 72, gy1 = h - (o.gyb ?? 10);
  let html = head(spec, data) + '<div class="tl">';
  let s = '';
  const gl = o.gridC || '#1a1a19';
  if (o.band != null) s += `<rect x="${o.bandX ?? x0}" y="${o.band[0]}" width="${w - (o.bandX ?? x0)}" height="${o.band[1] - o.band[0]}" fill="${o.bandC || '#171715'}"/>`;
  const hl = o.hlines || 4;
  for (let i = 0; i <= hl; i++) { const y = gy0 + i * (gy1 - gy0) / hl; s += `<line x1="${o.bandX ?? x0}" x2="${w}" y1="${px(y)}" y2="${px(y)}" stroke="${gl}"/>`; }
  for (let t = r.first ?? r.t0; t <= r.t1; t += r.step) s += `<line x1="${px(X(t))}" x2="${px(X(t))}" y1="${o.vTop ?? gy0 - 6}" y2="${gy1}" stroke="${gl}"/>`;
  const bs = o.beatS ?? 4;
  if (data.beats) for (const t of data.beats.list || (function* () { for (let t = data.beats.t0; t <= r.t1; t += data.beats.step) yield t; })()) { const x = X(t), y = o.beatY ?? 14; s += `<path d="M${px(x)} ${y - bs} L${px(x + bs)} ${y} L${px(x)} ${y + bs} L${px(x - bs)} ${y}Z" fill="#d9d4c9"/>`; }
  const V = v => gy1 - v * (gy1 - gy0);
  const ks = o.keyS ?? 7;
  for (const c of data.curves || []) {
    const P = c.pts.map(p => [X(p[0]), V(p[1])]);
    const d = c.smooth ? catmull(P) : P.map((q, i) => (i ? 'L' : 'M') + px(q[0]) + ' ' + px(q[1])).join('');
    const col = c.em ? 'var(--ember)' : c.color || 'var(--cream)';
    s += `<path${c.cls ? ` class="${c.cls}"` : ''} d="${d}" fill="none" stroke="${col}" stroke-width="${c.em ? (o.emW ?? 1.8) : (c.w ?? o.lineW ?? 1.4)}"${c.dash ? ` stroke-dasharray="${c.dash}"` : ''}/>`;
    for (const hd of (c.handles || [])) { const [a, b] = hd; s += `<line x1="${px(X(a[0]))}" y1="${px(V(a[1]))}" x2="${px(X(b[0]))}" y2="${px(V(b[1]))}" stroke="var(--ember)" stroke-width="1"/>` + [a, b].map(q => `<circle cx="${px(X(q[0]))}" cy="${px(V(q[1]))}" r="${o.hR ?? 3}" fill="${o.hFill ? 'var(--ember)' : 'var(--ink)'}" stroke="var(--ember)"/>`).join(''); }
    for (const k of (c.keys || [])) s += `<rect x="${px(X(k[0]) - ks / 2)}" y="${px(V(k[1]) - ks / 2)}" width="${ks}" height="${ks}" fill="${k[2] || col}" stroke="${k[3] || o.keyStroke || 'none'}"${k[3] ? ' stroke-width="1.5"' : ''}/>`;   /* k[3]: a hollow key (one the call removed or moved away from) */
  }
  html += `<svg style="position:absolute;left:0;top:0;width:${w}px;height:${h}px" viewBox="0 0 ${w} ${h}">${s}</svg>`;
  html += ruler(r, X, o.rulerY ?? 34);
  (data.lanes || []).forEach((L, i) => {
    html += `<span class="labchip" style="position:absolute;transform:none;left:${o.chipX ?? 10}px;width:${o.chipW ?? 176}px;top:${(o.laneY ?? 76) + i * (o.laneStep ?? 34)}px">${E(L)}</span>`;
  });
  if (data.playhead != null) html += `<div class="playhead ${o.phCls ?? 'both'}" style="left:${px(X(data.playhead))}px;top:${o.phTop ?? 26}px;bottom:${o.phBot ?? 6}px"></div>`;
  return html + '</div>';
}

// ---------- transcript ----------
function transcript({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  (data.lines || []).forEach((L, i) => {
    html += `<div class="tline" style="top:${px((o.y0 ?? 47) + i * (o.step ?? 40))}px">` +
      `<span class="spk">${E(L.spk)}</span><span class="ttc">${E(L.tc)}</span><span class="tx">${L.html || E(L.text)}</span></div>`;
  });
  return html;
}

// ---------- history / checkpoints ----------
function history({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  const y0 = o.y0 ?? 48, step = o.step ?? 70;
  html += `<div style="position:absolute;left:${(o.tw ?? 70) / 2 + 15}px;top:${y0 + (o.th ?? 44) / 2}px;height:${step * ((data.items || []).length - 1)}px;width:var(--hist-line-w,1px);background:var(--hist-line,var(--line))"></div>`;
  (data.items || []).forEach((it, i) => {
    const y = y0 + i * step;
    html += `<div class="hrow${F(it)}" style="position:absolute;left:15px;right:15px;top:${y}px;height:${o.th ?? 44}px;display:flex;align-items:center;gap:12px${it.dim ? ';opacity:.45' : ''}">` +
      `<div class="thumb ${it.sel ? 'sel' : ''}" style="flex:none;width:${o.tw ?? 70}px;height:100%">${it.src ? `<img src="${E(it.src)}">` : ''}</div>` +
      `<div style="font-family:var(--mono);font-size:var(--hist-fs,12px);line-height:var(--hist-lh,17px);min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis"><div class="cream" style="overflow:hidden;text-overflow:ellipsis">${E(it.id)}</div><div class="${o.nameCream ? 'cream' : 'dim'}" style="overflow:hidden;text-overflow:ellipsis">${E(it.name)}</div></div>` +
      `<span style="margin-left:auto;font-family:var(--mono);font-size:var(--hist-ago-fs,11.5px);color:var(--ago-c,var(--dim))">${E(it.ago)}${it.live ? ' <span class="em">●</span>' : ''}</span></div>`;
  });
  return html;
}

// ---------- diff + batch table ----------
function diffbatch({ spec, data, w, h }) {
  const o = spec.opts || {};
  const split = o.split ?? Math.round(w * .56);
  let html = head(spec, data);
  const col = o.col || { '+': '#cfe0b8', '~': 'var(--ember-text)', '-': '#e28a7a' };
  (data.diff || []).forEach((L, i) => {
    html += `<div style="position:absolute;left:15px;width:${split - 30}px;top:${(o.y0 ?? 52) + i * (o.step ?? 24)}px;display:flex;gap:var(--diff-gap,12px);font-family:var(--mono);font-size:var(--diff-fs,12px);white-space:pre"><span style="color:${col[L[0]] || 'var(--cream)'};width:10px">${E(L[0])}</span><span style="color:${L[0] === '~' && !o.col ? 'var(--text)' : (o.txt || col)[L[0]]}">${E(L[1])}</span></div>`;
  });
  html += `<div style="position:absolute;left:${split}px;top:${o.splitTop ?? 14}px;bottom:14px;width:1px;background:var(--line)"></div>`;
  const b = data.batch || {};
  html += `<div style="position:absolute;left:${split + (o.bx ?? 16)}px;right:${o.br ?? 15}px;top:${o.bty ?? 12}px;display:flex;justify-content:space-between;align-items:baseline;font-family:var(--title-font,var(--sans));font-weight:var(--batch-t-w,600);font-size:var(--batch-t-fs,13px)"><span>${E(b.title || '')}</span><span class="mono dim" style="font-weight:400;font-size:var(--batch-m-fs,inherit)${o.metaInline ? ';margin-left:10px;margin-right:auto;color:var(--cream);font-family:var(--sans)' : ''}">${E(b.meta || '')}</span></div>`;
  (b.rows || []).forEach((r, i) => {
    const st = r[2];
    html += `<div style="position:absolute;left:${split + (o.bx ?? 16)}px;right:${o.br ?? 15}px;top:${(o.by0 ?? 46) + i * (o.bstep ?? 24)}px;display:grid;grid-template-columns:var(--batch-cols,1fr 1fr 70px);font-family:var(--mono);font-size:var(--diff-fs,12px);${st === 'queued' ? 'opacity:.45' : ''}"><span style="color:${st === 'grading' ? 'var(--cream)' : (o.nameC || 'inherit')}">${E(r[0])}</span><span style="color:${o.lookC || 'var(--dim)'}">${E(r[1])}</span><span style="text-align:right;color:${st === 'grading' ? (o.activeC || 'var(--ember-text)') : (o.doneC || 'var(--dim)')}">${E(st)}</span></div>`;
  });
  return html;
}

// ---------- media list ----------
function media({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  (data.items || []).forEach((it, i) => {
    html += `<div style="position:absolute;left:15px;right:15px;top:${(o.y0 ?? 46) + i * (o.step ?? 32)}px;height:${o.th ?? 26}px;display:flex;align-items:center;gap:${o.gap ?? 10}px;font-family:var(--mono);font-size:var(--media-fs,12px)">` +
      `<div class="thumb" style="flex:none;width:${o.tw ?? 38}px;height:${o.thh ?? 24}px">${it.src ? `<img src="${E(it.src)}">` : (it.wave ? `<svg viewBox="0 0 38 24" style="width:38px;height:24px"><path d="${wave(38, 24, 9, .9)}" fill="#8a867d"/></svg>` : '')}</div>` +
      `<span class="cream">${E(it.name)}</span><span style="margin-left:auto;display:flex;align-items:center;gap:6px;font-size:${o.noteFs ? o.noteFs + 'px' : 'inherit'};${o.noteC ? 'color:' + o.noteC : ''}" class="${it.em ? 'em' : (o.noteC ? '' : 'dim')}">${it.link ? '<svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M6.5 9.5l3-3M7 4.5l1-1a3 3 0 0 1 4.2 4.2l-1 1M9 11.5l-1 1a3 3 0 0 1-4.2-4.2l1-1"/></svg>' : ''}${E(it.note || '')}</span></div>`;
  });
  if (data.foot) html += `<div class="pfoot${flashing(data._footFlash) ? ' flash' : ''}">${E(data.foot)}</div>`;
  return html;
}

// ---------- scopes ----------
function lumaTrace(w, h, seed, color, alpha, shape) {
  // dense vertical scatter: for each column, a distribution of luma values
  const R = rng(seed); let s = '';
  const cols = Math.floor(w / 1.6);
  for (let i = 0; i < cols; i++) {
    const x = i / cols * w, u = i / cols;
    const base = shape ? shape(u) : .45 + .25 * Math.sin(u * 6.3 + 1);
    for (let k = 0; k < 9; k++) {
      const v = Math.max(.02, Math.min(.98, base + (R() - .5) * .38 * (k < 3 ? .4 : 1)));
      s += `<rect x="${px(x)}" y="${px(h - v * h)}" width="1.2" height="${px(1 + R() * 3)}" fill="${color}" opacity="${alpha}"/>`;
    }
  }
  return s;
}
function scopeAxis(x, y0, y1) {
  return [1023, 768, 512, 256, 0].map((v, i) => `<text x="${x}" y="${px(y0 + i * (y1 - y0) / 4 + 3)}" font-size="9.5" fill="var(--dim)" text-anchor="end">${v}</text><line x1="${x + 6}" x2="9999" y1="${px(y0 + i * (y1 - y0) / 4)}" y2="${px(y0 + i * (y1 - y0) / 4)}" stroke="#1a1a19"/>`).join('');
}
function vectorscope(cx, cy, r, seed) {
  let s = `<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="#e0613a" stroke-width="1.6"/><circle cx="${cx}" cy="${cy}" r="${r * .66}" fill="none" stroke="#2a2a28"/><circle cx="${cx}" cy="${cy}" r="${r * .33}" fill="none" stroke="#2a2a28"/>` +
    `<line x1="${cx - r}" x2="${cx + r}" y1="${cy}" y2="${cy}" stroke="#2a2a28"/><line x1="${cx}" x2="${cx}" y1="${cy - r}" y2="${cy + r}" stroke="#2a2a28"/>`;
  const targets = [[103, 'R'], [167, 'Mg'], [241, 'B'], [283, 'Cy'], [347, 'G'], [61, 'Yl']];
  for (const [a] of targets) { const t = a * Math.PI / 180; s += `<rect x="${px(cx + Math.cos(t) * r * .75 - 4)}" y="${px(cy - Math.sin(t) * r * .75 - 4)}" width="8" height="8" fill="none" stroke="#5a5650"/>`; }
  const sk = 123 * Math.PI / 180; s += `<line x1="${cx}" y1="${cy}" x2="${px(cx + Math.cos(sk) * r)}" y2="${px(cy - Math.sin(sk) * r)}" stroke="#bdb8ae" stroke-width="1"/>`;
  const R = rng(seed);
  for (let i = 0; i < 700; i++) {
    const teal = R() < .45; const a = (teal ? 300 : 120) * Math.PI / 180 + (R() - .5) * .7; const rr = Math.pow(R(), 1.8) * r * (teal ? .5 : .62);
    s += `<circle cx="${px(cx + Math.cos(a) * rr)}" cy="${px(cy - Math.sin(a) * rr)}" r=".9" fill="${teal ? '#4fb3b0' : '#f08a4b'}" opacity=".75"/>`;
  }
  return s;
}
function scopes(a) {   // GUI-FIX-1: a colour read's stats line is drawn by app.js statsLine() as its own element, not in this markup
  return scopesBody(a);
}
function scopesBody({ spec, data, w, h }) {
  const o = spec.opts || {};
  const dock = data.img ? scopeDock(data.img, o, w, h, data.wave ? scopeGeom(o, w, h).split : (o.split ?? Math.round(w * .5))) : '';
  if (!data.wave) {   // legacy synthetic look (no binned scope data from the engine)
    const split = o.split ?? Math.round(w * .5);
    let s = '';
    const y0 = o.y0 ?? 18, y1 = h - (o.yb ?? 34);
    s += `<g>${scopeAxis(44, y0, y1)}</g>`;
    s += `<svg x="52" y="${y0}" width="${split - 66}" height="${y1 - y0}">${lumaTrace(split - 66, y1 - y0, 4, '#e9e4d8', .5)}</svg>`;
    s += `<line x1="${split}" x2="${split}" y1="12" y2="${h - 12}" stroke="var(--line)"/>`;
    const cx = split + (w - split) / 2, cy = (y0 + y1) / 2, r = Math.min((w - split) / 2 - 20, (y1 - y0) / 2 + 6);
    s += vectorscope(cx, cy, r, 7);
    return head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>` + dock;
  }
  // Binned scope data (what an engine's render_scope sends): drawn on canvas after mount.
  const g = scopeGeom(o, w, h);
  let s = '';
  [1023, 768, 512, 256, 0].forEach(v => {
    const y = g.wy1 - v / 1023 * (g.wy1 - g.wy0);
    s += `<line x1="${g.wx0}" x2="${g.wx1}" y1="${px(y)}" y2="${px(y)}" stroke="#1f1f1d"/><text x="${g.wx0 - 8}" y="${px(y + 4.5)}" text-anchor="end" font-size="${o.axFs ?? 13}" fill="var(--dim)" font-family="var(--mono)">${v}</text>`;
  });
  for (let x = g.wx0; x <= g.wx1; x += (o.vgrid ?? 21.2)) s += `<line x1="${px(x)}" x2="${px(x)}" y1="${g.wy0}" y2="${g.wy1}" stroke="#1a1a18"/>`;
  if (o.splitLine) s += `<line x1="${g.split}" x2="${g.split}" y1="0" y2="${h}" stroke="var(--line)"/>`;
  s += `<text x="${px((g.wx0 + g.wx1) / 2)}" y="${h - (o.labB ?? 5)}" text-anchor="middle" font-size="${o.labFs ?? 15}" fill="var(--dim)" font-family="var(--mono)">waveform</text>`;
  s += `<text x="${g.vx}" y="${h - (o.labB ?? 5)}" text-anchor="middle" font-size="${o.labFs ?? 15}" fill="var(--dim)" font-family="var(--mono)">vectorscope</text>`;
  return head(spec, data) + `<canvas class="scopecv" width="${w}" height="${h}" style="position:absolute;left:0;top:0;width:${w}px;height:${h}px"></canvas>` +
    `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>` + dock;
}
/* the engine's own scope image (render_scope) in its half of the dock: a vectorscope over the vectorscope, anything
   else (waveform, parade, histogram) over the waveform; the main viewer keeps the frame */
function scopeDock(img, o, w, h, split) {
  const vec = /vector/i.test(img.kind), x = vec ? split + 6 : 6, bw = vec ? w - split - 12 : split - 12, top = 6, bh = h - 12;
  return `<div class="scopedock" style="left:${px(x)}px;top:${top}px;width:${px(bw)}px;height:${bh}px"><img src="${E(img.src)}" alt=""><span class="lab">${E(img.kind)}${img.tc ? ' · ' + E(img.tc) : ''}</span></div>`;
}
function scopeGeom(o, w, h) {
  return { wx0: o.wx0 ?? 44, wx1: o.wx1 ?? 680, wy0: o.wy0 ?? 20, wy1: o.wy1 ?? 222, split: o.split ?? 701, vx: o.vx ?? 894, vy: o.vy ?? 122, vr: o.vr ?? 110 };
}
const DIG = '0123456789abcdefghijklmnopqrstuvwxyz';
// ---------- false colour ----------
// Standard exposure LUT on Rec.709 luma (IRE = code value / 255 * 100):
//   < 2.5 crushed: pink-purple · 2.5-16 shadows: purple · 39.5-43 on skin-hued pixels: green
//   90-99 highlights: white/ink zebra · >= 99 clipping: red/ink zebra · else grey.
// Pure function of the decoded frame; cached per src+size so a re-render only blits.
export function falseColorLUT(Y, x, y, skin = true) {
  if (Y < 2.5) return [138, 44, 122];
  if (Y < 16) { const k = .55 + Y / 16 * .9; return [34 * k, 19 * k, 51 * k]; }
  if (Y >= 39.5 && Y < 43 && skin) { const k = Y / 41.6; return [84 * k, 120 * k, 62 * k]; }
  // highlights get diagonal zebra stripes like the mockup: white/ink over 90 IRE, red/ink at clipping
  const zebra = ((x + y) >> 2) & 1;
  if (Y >= 99) return zebra ? [36, 12, 10] : [224, 48, 42];
  if (Y >= 90) return zebra ? [24, 22, 20] : [244, 240, 232];
  return null;
}
const FC = new Map();
function paintFalseColor(cv, im) {
  const w = cv.width, h = cv.height, key = im.src.length + ':' + im.src.slice(-64) + ':' + w + 'x' + h;
  let d = FC.get(key);
  if (!d) {
    const c = document.createElement('canvas'); c.width = w; c.height = h; const g = c.getContext('2d');
    const s = Math.max(w / im.naturalWidth, h / im.naturalHeight), dw = im.naturalWidth * s, dh = im.naturalHeight * s; // object-fit: cover
    g.drawImage(im, (w - dw) / 2, (h - dh) / 2, dw, dh);
    d = g.getImageData(0, 0, w, h); const p = d.data;
    for (let i = 0, n = 0; i < p.length; i += 4, n++) {
      const Y = (.2126 * p[i] + .7152 * p[i + 1] + .0722 * p[i + 2]) / 2.55, x = n % w, y = (n / w) | 0;
      const r = p[i], g = p[i + 1], b = p[i + 2], skin = r > g && g >= b * .8; // skin-hued (warm) pixels only get the skin band
      const f = falseColorLUT(Y, x, y, skin);
      if (f) { p[i] = f[0]; p[i + 1] = f[1]; p[i + 2] = f[2]; }
      else { const v = Y * 2.55; p[i] = v * .85 + p[i] * .15; p[i + 1] = v * .85 + p[i + 1] * .15; p[i + 2] = v * .85 + p[i + 2] * .15; }
    }
    if (FC.size > 8) FC.clear();
    FC.set(key, d);
  }
  cv.getContext('2d').putImageData(d, 0, 0);
}

// After-mount painters (canvas): called by the render loop right after a panel's markup lands.
// A rail row the person clicked: its own args, latency and result, as rows for the mode's EXISTING
// inspector (no new panel). Only what the call carried; nested values are summarised, never invented.
export function pickedRows(c) {
  const short = v => v == null ? '' : typeof v === 'object' ? (Array.isArray(v) ? `[${v.length}]` : `{${Object.keys(v).slice(0, 3).join(', ')}${Object.keys(v).length > 3 ? ', …' : ''}}`) : typeof v === 'number' ? String(Math.round(v * 1000) / 1000) : String(v);
  const rows = [{ sec: 'Call' }, { k: 'Status', v: c.status === 'running' ? 'running' : c.status === 'error' ? 'error' : c.status === 'lost' ? 'no result (feed dropped)' : 'ok', ch: c.status === 'error' }];
  const d = fmtDur(c.dur_ms); if (c.status !== 'running' && d) rows.push({ k: 'Latency', v: d });
  if (c.error) rows.push({ k: 'Error', v: String(c.error).slice(0, 80), ch: true });
  if (Array.isArray(c.diff) && c.diff.length) { rows.push({ sec: 'Diff' }); for (const d of c.diff) rows.push({ k: d.k, from: d.from, v: d.v }); }   // before -> after, only as the engine reported it
  const A = c.args && typeof c.args === 'object' && !Array.isArray(c.args) ? Object.entries(c.args) : Array.isArray(c.args) ? c.args.map((v, i) => [`#${i + 1}`, v]) : c.args != null ? [['args', c.args]] : [];
  if (A.length) { rows.push({ sec: 'Args' }); for (const [k, v] of A.slice(0, 12)) if (v != null && v !== '') rows.push({ k, v: short(v) }); }
  const R = c.result && typeof c.result === 'object' ? Object.entries(c.result).filter(([, v]) => v != null && v !== '') : [];
  if (R.length) { rows.push({ sec: 'Result' }); for (const [k, v] of R.slice(0, 8)) rows.push({ k, v: short(v) }); }
  return rows;
}
// the top-level args of a rail string: split on ", " outside brackets and quotes, so "[0.3, 1.5]" or a quoted
// "a, b" stays one arg and is dropped whole
function topArgs(t) {
  const out = []; let d = 0, q = '', from = 0;
  for (let i = 0; i < t.length; i++) {
    const ch = t[i];
    if (q) { if (ch === q && t[i - 1] !== '\\') q = ''; continue; }
    if (ch === '"' || ch === "'") q = ch; else if ('[{('.includes(ch)) d++; else if (']})'.includes(ch)) d = Math.max(0, d - 1);
    else if (ch === ',' && t[i + 1] === ' ' && d === 0) { out.push(t.slice(from, i)); from = i + 2; i++; }
  }
  out.push(t.slice(from)); return out;
}
// the width of "…" in an element's font (with its letter-spacing), measured once per font and kept
const ELL = new Map();
function ellW(el) {
  const cs = getComputedStyle(el), key = cs.font + '|' + cs.letterSpacing;
  let w = ELL.get(key);
  if (w == null) {
    const m = document.createElement('span'); m.style.cssText = 'position:absolute;visibility:hidden;white-space:pre'; m.textContent = '…';
    el.appendChild(m); w = m.getBoundingClientRect().width; m.remove(); ELL.set(key, w);
  }
  return w;
}
// rail rows: a call whose args do not fit drops whole args, "export(1920x1080, h264, …)", instead of cutting one
// mid-word ("crf…"); a from→to change is the point of its row, so it is the last arg to go ("set_property(clip4,
// …, scale: 104% → 112%)"). The full call stays in the hover title. Measured BEFORE the panel is written, on hidden
// copies of only the rows whose fit is unknown (new row, row no longer current, call settled), while the page's
// layout is still clean: one small layout, no forced layout of the freshly written panel. Remembered on the call.
let SANDBOX = null;
export const PRE = {
  agent(el, { spec, state, w, h }) {
    const { o, rowH, top, gap, calls } = railCalls(spec, state, h);
    let rows = '';
    calls.forEach((c, i) => { const full = railArgs(c), recent = o.boldLast && i >= calls.length - o.boldLast; if (full && !(c._fit && c._fit.k === railKey(c, railGeo(i === calls.length - 1, recent, state.mode), w, full))) rows += railRow(c, i, calls, o, state, w, top + i * (rowH + gap), rowH, true); });
    if (!rows) return;
    /* a hidden box inside this panel, sized like it: its layout is its own (contain:strict), so laying out these rows
       touches nothing else, and the panel (rewritten next anyway) is the only part of the page it changes */
    const box = SANDBOX || (SANDBOX = document.createElement('div'));
    box.className = `calls ${o.chips ? 'chips' : ''}`;
    box.style.cssText = `position:absolute;left:0;top:0;width:${el.clientWidth}px;height:${el.clientHeight}px;visibility:hidden;pointer-events:none;contain:strict`;
    box.innerHTML = rows; el.appendChild(box);
    const rg = document.createRange(), wOf = (tn, i, j) => { rg.setStart(tn, i); rg.setEnd(tn, j); return rg.getBoundingClientRect().width; };
    for (const row of box.children) {
      const c = calls.find(x => String(x.id) === row.dataset.call), a = row.querySelector('.args'), tn = a && a.firstChild; if (!c || !tn) continue;
      let t = null, tight = false;
      /* a row with a from→to change always goes through here, fits or not: the rail shows every change in one form */
      if (a.scrollWidth > a.clientWidth + .5 || / → /.test(tn.nodeValue)) {
        const full = tn.nodeValue, parts = topArgs(full), avail = a.getBoundingClientRect().width, ell = ellW(a);
        const pw = []; let at = 0; for (const q of parts) { pw.push(wOf(tn, at, at + q.length)); at += q.length + 2; }
        const sep = parts.length > 1 ? wOf(tn, parts[0].length, parts[0].length + 2) : 0;
        const d = row.querySelector('.dur'), dt = d && d.firstChild && d.firstChild.nodeType === 3 ? d.firstChild : null;
        /* a settled row may borrow the room the duration column holds for wider values (its min-width beyond its text) */
        const spare = c.status !== 'running' && dt ? Math.max(0, d.getBoundingClientRect().width - wOf(dt, 0, dt.nodeValue.length)) : 0;
        /* candidates, most shown first: drop other args from the end (keeping the target and every from→to change),
           then the target (a change is the point of its row), then — only when the changes themselves cannot fit —
           the changes from the end, target kept, then the target */
        const delta = parts.map(q => /→/.test(q)), cands = [], keep = parts.map(() => 1);
        /* a change too long for the row tightens first: one unit, no spaces round the arrow, "scale 104→112%" for
           "scale: 104% → 112%"; only when even that cannot fit does it drop its name, "104→112%" — the old and the new
           value always both show (the name stays in the title). Both are measured from pieces of the same text node. */
        const tf = parts.map(() => null), cw = parts.map(() => null);
        parts.forEach((q, i) => {
          if (!delta[i]) return;
          const m = /^([^:]+): (.+?) → (.+)$/.exec(q); if (!m) return;
          let o = 0; for (let j = 0; j < i; j++) o += parts[j].length + 2;
          const [, k, a, b] = m, ka = o + k.length + 2, ar = ka + a.length + 1, kb = ar + 2;   /* offsets: from value, arrow, to value */
          const ua = /[^\d.]*$/.exec(a)[0], ub = /[^\d.]*$/.exec(b)[0], one = ua && ua === ub && /\d$/.test(a.slice(0, a.length - ua.length));
          const a2 = one ? a.slice(0, a.length - ua.length) : a;
          tf[i] = [`${k} ${a2}→${b}`, wOf(tn, o, o + k.length) + wOf(tn, o + k.length + 1, o + k.length + 2) + wOf(tn, ka, ka + a2.length) + wOf(tn, ar, ar + 1) + wOf(tn, kb, kb + b.length)];
          cw[i] = [`${a2}→${b}`, wOf(tn, ka, ka + a2.length) + wOf(tn, ar, ar + 1) + wOf(tn, kb, kb + b.length)];   /* values only: both stay visible (Naman 7:37) */
        });
        /* one form for every change on the rail (root, 8:4x): the tight "scale 104→112%", whether or not the full one fits */
        parts.forEach((q, i) => { if (tf[i]) keep[i] = 3; });
        const k0 = keep.slice(), anyT = k0.includes(3);
        for (let i = parts.length - 1; i >= 1; i--) if (!delta[i]) { keep[i] = 0; cands.push(keep.slice()); }
        if (delta.some(Boolean)) {
          if (!delta[0]) { const kp = keep.slice(); kp[0] = 0; cands.push(kp); }
          for (const f of [3, 2]) if (cw.some(Boolean)) { const kp = keep.map((v, i) => v && cw[i] ? f : v); cands.push(kp.slice()); if (!delta[0]) { kp[0] = 0; cands.push(kp); } }
          const kp = keep.slice(); for (let i = parts.length - 1; i >= 0; i--) if (delta[i]) { kp[i] = 0; if (kp.some(Boolean)) cands.push(kp.slice()); }
        }
        const show = kp => { const out = []; let wd = 0; parts.forEach((q, i) => { if (kp[i] === 1) { out.push(q); wd += pw[i]; } else if (kp[i] === 2) { out.push(cw[i][0]); wd += cw[i][1]; } else if (kp[i] === 3) { out.push(tf[i][0]); wd += tf[i][1]; } else if (out[out.length - 1] !== '…') { out.push('…'); wd += ell; } }); return [out.join(', '), wd + sep * (out.length - 1)]; };
        let best = null, bestT = false;
        const tryFit = (txt, wd) => { if (wd <= avail + .05) best = [txt]; else if (spare > 0 && wd <= avail + spare + .05) { best = [txt]; bestT = true; } };
        const allIn = show(k0);
        if (anyT || allIn[1] > avail + .05) tryFit(anyT ? allIn[0] : null, allIn[1]);   /* the whole call (changes tight), maybe in borrowed room */
        for (const kp of cands) { if (best) break; const [txt, wd] = show(kp); tryFit(txt, wd); }
        /* last resort, when not even one whole arg fits: the first arg's leading whole words, "lint_frame(1s: low-res …)",
           never a change (its values stay whole or not at all) and never a cut inside a word */
        if (!best && !delta[0]) {
          const q = parts[0]; let sp = q.lastIndexOf(' ');
          while (sp > 0 && !best) { const head = q.slice(0, sp); if (/\S/.test(head)) tryFit(`${head} …`, wOf(tn, 0, sp) + wOf(tn, sp, sp + 1) + ell); sp = q.lastIndexOf(' ', sp - 1); }
        }
        if (!best) tryFit('…', ell);
        /* narrower still (laid out, not summed: the name itself is at the edge here): "(…)", then the name alone with no
           parens ("preview_material  300ms"); the call stays whole in the title */
        if (!best) {
          const sg = row.querySelector('.sig'), fits = () => sg.scrollWidth <= sg.clientWidth + .5 && a.scrollWidth <= a.clientWidth + .5;
          if (spare > 0) row.classList.add('tight');
          a.textContent = '…'; if (fits()) { best = ['…']; bestT = spare > 0; }
          else { sg.classList.add('bare'); best = ['']; bestT = spare > 0; }   /* even if the name alone still overflows: the least cut */
        }
        t = best ? best[0] : '…'; tight = bestT;
      }
      c._fit = { k: row.dataset.fk, t, tight };
    }
    box.remove();
  },
};
export const POST = {
  cie(el, { spec, w, h }) {
    const cv = el.querySelector('canvas.ciecv'); if (!cv) return;
    const [ox, oy, scx, scy] = cv.dataset.geo.split(',').map(Number), dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
    cv.getContext('2d').putImageData(cieFill(w, h, ox, oy, scx, scy, dpr, (spec.opts || {}).fillOp ?? .85), 0, 0);
  },
  viewer(el) {
    const cv = el.querySelector('canvas.fc'), im = el.querySelector('.frame img'); if (!cv || !im) return;
    if (im.complete && im.naturalWidth) paintFalseColor(cv, im); else im.addEventListener('load', () => paintFalseColor(cv, im), { once: true });
  },
  scopes(el, { spec, data, w, h }) {
    const cv = el.querySelector('canvas.scopecv'); if (!cv || !data.wave) return;
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
    const c = cv.getContext('2d'); c.scale(dpr, dpr);
    const g = scopeGeom(spec.opts || {}, w, h);
    const cols = data.wave.cols, nb = data.wave.bins, cw = (g.wx1 - g.wx0) / cols.length, bh = (g.wy1 - g.wy0) / nb;
    // the blurred passes are drawn plain into one offscreen layer and blurred ONCE: a canvas filter applies per
    // draw call, so thousands of filtered fillRects cost ~20 s a render in software (measured headless, m05)
    const layer = () => { const o = document.createElement('canvas'); o.width = cv.width; o.height = cv.height; const oc = o.getContext('2d'); oc.scale(dpr, dpr); return [o, oc]; };
    const paint = (cx, alpha) => {
      for (let i = 0; i < cols.length; i++) {
        const col = cols[i];
        for (let b = 0; b < nb; b++) {
          const v = DIG.indexOf(col[b]); if (v <= 0) continue;
          cx.fillStyle = `rgba(236,214,196,${(v / 35 * alpha).toFixed(3)})`;
          cx.fillRect(g.wx0 + i * cw, g.wy1 - (b + 1) * bh, cw + .4, bh + .4);
        }
      }
    };
    const [wl, wc] = layer(); wc.globalCompositeOperation = 'lighter'; paint(wc, .35);
    c.globalCompositeOperation = 'lighter';
    c.filter = 'blur(2px)'; c.drawImage(wl, 0, 0, w, h); c.filter = 'none'; paint(c, .72);
    c.globalCompositeOperation = 'source-over';
    // vectorscope
    const V = data.vec || {}, X = q => g.vx + q[0] * g.vr, Y = q => g.vy + q[1] * g.vr;
    c.strokeStyle = '#2c2b29'; c.lineWidth = 1;
    c.beginPath(); c.moveTo(g.vx - g.vr, g.vy); c.lineTo(g.vx + g.vr, g.vy); c.moveTo(g.vx, g.vy - g.vr); c.lineTo(g.vx, g.vy + g.vr); c.stroke();
    c.strokeStyle = '#b98d77'; c.lineWidth = 2; c.beginPath(); c.arc(g.vx, g.vy, g.vr, 0, Math.PI * 2); c.stroke();
    if ((V.pts || []).length) { const [vl, vc] = layer(); vc.globalAlpha = .3;
      for (const q of V.pts) { vc.fillStyle = q[2]; vc.beginPath(); vc.arc(X(q), Y(q), 4.5, 0, Math.PI * 2); vc.fill(); }
      c.filter = 'blur(3px)'; c.drawImage(vl, 0, 0, w, h); c.filter = 'none'; }
    c.globalAlpha = 1; c.globalCompositeOperation = 'source-over';
    if (V.skin) { c.strokeStyle = '#efe7da'; c.lineWidth = 2; c.beginPath(); c.moveTo(X(V.skin[0]), Y(V.skin[0])); c.lineTo(X(V.skin[1]), Y(V.skin[1])); c.stroke(); }
    c.strokeStyle = '#a88a76'; c.lineWidth = 1.3;
    for (const q of V.targets || []) { c.beginPath(); c.arc(X(q), Y(q), 3.4, 0, Math.PI * 2); c.stroke(); }
  },
};

// ---------- CIE 1931 ----------
const LOCUS = [[.1741, .0050], [.1740, .0050], [.1738, .0049], [.1736, .0049], [.1733, .0048], [.1730, .0048], [.1726, .0048], [.1721, .0048], [.1714, .0051], [.1703, .0058], [.1689, .0069], [.1669, .0086], [.1644, .0109], [.1611, .0138], [.1566, .0177], [.1510, .0227], [.1440, .0297], [.1355, .0399], [.1241, .0578], [.1096, .0868], [.0913, .1327], [.0687, .2007], [.0454, .2950], [.0235, .4127], [.0082, .5384], [.0039, .6548], [.0139, .7502], [.0389, .8120], [.0743, .8338], [.1142, .8262], [.1547, .8059], [.1929, .7816], [.2296, .7543], [.2658, .7243], [.3016, .6923], [.3373, .6589], [.3731, .6245], [.4087, .5896], [.4441, .5547], [.4788, .5202], [.5125, .4866], [.5448, .4544], [.5752, .4242], [.6029, .3965], [.6270, .3725], [.6482, .3514], [.6658, .3340], [.6801, .3197], [.6915, .3083], [.7006, .2993], [.7079, .2920], [.7140, .2859], [.7190, .2809], [.7230, .2770], [.7260, .2740], [.7283, .2717], [.7300, .2700], [.7311, .2689], [.7320, .2680], [.7327, .2673], [.7334, .2666], [.7340, .2660], [.7344, .2656], [.7346, .2654], [.7347, .2653]];
function cie({ spec, data, w, h }) {
  const o = spec.opts || {};
  const pad = o.pad ?? [16, 16, 30, 16];
  const W = w - pad[1] - pad[3], H = h - pad[0] - pad[2];
  const sc0 = Math.min(W / .8, H / .9);
  const scx = o.scx ?? sc0, scy = o.scy ?? sc0;
  const ox = o.ox ?? pad[3] + (W - .8 * scx) / 2, oy = o.oy ?? pad[0] + .9 * scy;
  const P = ([x, y]) => [px(ox + x * scx), px(oy - y * scy)];
  const path = LOCUS.map((p, i) => (i ? 'L' : 'M') + P(p).join(' ')).join('') + 'Z';
  const bx = ox, by = oy - .9 * scy, bw = .8 * scx, bh = .9 * scy;
  let s = `<defs><clipPath id="hs"><path d="${path}"/></clipPath>
    <linearGradient id="cx" x1="0" x2="1" y1="0" y2="0"><stop offset="0" stop-color="#2a3cff"/><stop offset=".35" stop-color="#25c8d0"/><stop offset=".55" stop-color="#7fe05a"/><stop offset=".75" stop-color="#f3d23c"/><stop offset="1" stop-color="#ff3a2a"/></linearGradient>
    <linearGradient id="cy" x1="0" x2="0" y1="1" y2="0"><stop offset="0" stop-color="#5a2bff" stop-opacity=".9"/><stop offset=".5" stop-color="#ffffff" stop-opacity=".0"/><stop offset="1" stop-color="#30ff60" stop-opacity=".7"/></linearGradient>
    <radialGradient id="cw" cx="${px(P([.3127, .329])[0])}" cy="${px(P([.3127, .329])[1])}" r="${px(.16 * scx)}" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#fff" stop-opacity=".4"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient></defs>`;
  if (o.grid) { for (let v = 0; v <= .9001; v += .1) { const [x] = P([v, 0]), [, y] = P([0, v]); if (v <= .8001) s += `<line x1="${x}" x2="${x}" y1="${px(by)}" y2="${px(oy)}" stroke="${o.gridC || '#24241f'}"/>`; s += `<line x1="${px(bx)}" x2="${px(bx + (o.gridW ?? bw))}" y1="${y}" y2="${y}" stroke="${o.gridC || '#24241f'}"/>`; } }
  // the horseshoe fill is real chromaticity (xy -> XYZ -> sRGB, max channel = 1), painted by POST.cie
  s += `<path d="${path}" fill="none" stroke="#d9d4c9" stroke-width="1"/>`;
  for (const g of (data.gamuts || [])) {
    const lp = g.lpt ? P(g.lpt) : [P(g.pts[1])[0] + 6, P(g.pts[1])[1] + 2];
    s += `<polygon points="${g.pts.map(p => P(p).join(',')).join(' ')}" fill="none" stroke="${g.color || 'var(--cream)'}" stroke-width="${g.w ?? 1.3}"${g.dash ? ' stroke-dasharray="3 3"' : ''}/>` + (g.label ? `<text x="${lp[0]}" y="${lp[1]}" font-size="${o.gFs ?? 9.5}" fill="var(--text)">${E(g.label)}</text>` : '');
  }
  for (const p of (data.points || [])) s += p[2] === 'ring' ? `<circle cx="${P(p)[0]}" cy="${P(p)[1]}" r="3" fill="none" stroke="#f4f0e8" stroke-width="1.4"/>` : `<circle cx="${P(p)[0]}" cy="${P(p)[1]}" r="3.5" fill="var(--ember)"/>`;
  s += `<text x="${o.labX ?? w / 2}" y="${h - (o.labB ?? 12)}" font-size="${o.labFs ?? 10.5}" fill="var(--dim)" text-anchor="middle" font-family="${o.labFont ?? 'inherit'}">${E(data.label || 'CIE 1931')}</text>`;
  return head(spec, data) + `<canvas class="ciecv" data-geo="${[ox, oy, scx, scy].join(',')}" style="position:absolute;inset:0;width:${w}px;height:${h}px;pointer-events:none"></canvas>` +
    `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
}
const CIEC = new Map(), DIM = .8, SAT = 1.6; // the mockup's horseshoe: ~20% under full brightness, a smaller white core
function cieFill(w, h, ox, oy, scx, scy, dpr, op) {
  const key = [w, h, ox, oy, scx, scy, dpr, op].join(); let d = CIEC.get(key); if (d) return d;
  const W = Math.round(w * dpr), H = Math.round(h * dpr), c = document.createElement('canvas'); c.width = W; c.height = H;
  const g = c.getContext('2d'); d = g.createImageData(W, H); const p = d.data;
  const path = new Path2D(LOCUS.map((q, i) => (i ? 'L' : 'M') + (ox + q[0] * scx) * dpr + ' ' + (oy - q[1] * scy) * dpr).join('') + 'Z');
  const gam = v => v <= .0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - .055;
  for (let j = 0; j < H; j++) for (let i = 0; i < W; i++) {
    if (!g.isPointInPath(path, i + .5, j + .5)) continue;
    const x = ((i + .5) / dpr - ox) / scx, y = (oy - (j + .5) / dpr) / scy; if (y <= 1e-4) continue;
    const X = x / y, Z = (1 - x - y) / y;
    let r = 3.2406 * X - 1.5372 - .4986 * Z, gg = -.9689 * X + 1.8758 + .0415 * Z, b = .0557 * X - .2040 + 1.0570 * Z;
    r = Math.max(0, r); gg = Math.max(0, gg); b = Math.max(0, b); const m = Math.max(r, gg, b) || 1;
    const k = (j * W + i) * 4; p[k] = 255 * DIM * gam((r / m) ** SAT); p[k + 1] = 255 * DIM * gam((gg / m) ** SAT); p[k + 2] = 255 * DIM * gam((b / m) ** SAT); p[k + 3] = 255 * op;
  }
  if (CIEC.size > 4) CIEC.clear(); CIEC.set(key, d); return d;
}

// ---------- loudness ----------
function loudness({ spec, data, w, h }) {
  // measured on m04: big integrated LUFS, a target line running into the meters, true peak / LRA rows,
  // two meter ladders with L/R labels top and bottom, a peak-hold mark and a status tick under each top label
  const o = spec.opts || {};
  const mx = o.mx ?? 178, mw = o.mw ?? 14, mg = o.mg ?? 9, my0 = o.my0 ?? 40, my1 = o.my1 ?? 234, H = my1 - my0, rx = mx - 12;
  let html = head(spec, data);
  html += `<div class="cream${F(data)}" style="position:absolute;left:15px;top:${o.bigY ?? 64}px;line-height:1;white-space:nowrap"><span style="font-family:var(--mono);font-size:${o.bigFs ?? 33}px;letter-spacing:-.03em">${E(data.lufs)}</span><span style="font-family:var(--sans);font-size:${o.unitFs ?? 17}px;margin-left:7px">LUFS</span></div>`;
  // the line under the big number: "target <n>" only when a target exists (normalize_loudness's target_lufs, router.js);
  // otherwise the big number is a measurement (measure_loudness's integrated loudness) and the line names it "Integrated",
  // never a bare "target" over a measured value. 16 px = 9.55 px effective at 640 x 360 (x 0.597015).
  const ty = o.targetY ?? 115, tgt = data.target != null && data.target !== '' ? +data.target : null;
  html += tgt != null
    ? `<div style="position:absolute;left:15px;top:${ty - 10}px;height:20px;line-height:20px;font-size:${o.rowFs ?? 15}px;color:var(--text);white-space:nowrap">target&nbsp;&nbsp;<span style="font-family:var(--mono)">${E(data.target)}</span></div>`
    : `<div class="lufs-int" style="position:absolute;left:15px;top:${ty - 10}px;height:20px;line-height:20px;font-size:${o.intFs ?? 16}px;color:var(--text);white-space:nowrap">Integrated</div>`;
  html += `<div style="position:absolute;left:${o.tlx ?? 96}px;width:${rx - (o.tlx ?? 96)}px;top:${ty}px;height:1px;background:#8d887e"></div>`;
  html += `<div style="position:absolute;left:15px;width:${rx - 15}px;top:${o.divY ?? 178}px;height:1px;background:var(--line2)"></div>`;
  (data.rows || []).forEach((r, i) => html += `<div style="position:absolute;left:15px;width:${rx - 18}px;top:${(o.rowY ?? 191) + i * (o.rowStep ?? 27)}px;height:20px;display:flex;justify-content:space-between;align-items:center;white-space:nowrap;letter-spacing:-.02em;font-size:${o.rowFs ?? 15}px"><span style="color:var(--text)">${E(r[0])}</span><span class="mono cream${F(r)}">${E(r[1])}</span></div>`);
  (data.meters || []).forEach((m, i) => {
    const x = mx + i * (mw + mg), T = performance.now();
    // ballistics: a hit jumps the bar, then it falls back like a VU (~300 ms); the peak mark holds 700 ms, then falls
    let v = m.v ?? 0, pk = m.peak;
    if (m.hit && T - m.hit.t < METER_MS) v = m.hit.base + (m.hit.lvl - m.hit.base) * Math.exp(-(T - m.hit.t) / 300);
    if (m.peakHold && T - m.peakHold.t < METER_MS) { const a = T - m.peakHold.t; pk = a < 700 ? m.peakHold.lvl : m.peakHold.lvl + ((m.peak ?? v) - m.peakHold.lvl) * Math.min(1, (a - 700) / 800); }
    v = Math.max(0, Math.min(1, v));
    html += `<div class="meter" style="position:absolute;left:${x}px;top:${my0}px;width:${mw}px;height:${H}px;background:#2b2926;overflow:hidden">` +
      `<div style="position:absolute;left:0;right:0;bottom:0;height:${px(v * H)}px;background:#e5dcc6"></div>` +
      (pk != null ? `<div style="position:absolute;left:0;right:0;height:3px;bottom:${px(pk * H)}px;background:#e5dcc6;border-bottom:1.5px solid #3a3631"></div>` : '') + '</div>';
    html += `<div class="dim" style="position:absolute;left:${x}px;width:${mw}px;text-align:center;top:${o.labT ?? 13}px;font-size:${o.labFs ?? 13}px;line-height:16px;color:var(--text)">${E(m.l)}</div>` +
      `<div style="position:absolute;left:${x + 2}px;width:${mw - 4}px;top:${o.ledY ?? 33}px;height:2px;background:${m.led || '#8d887e'}"></div>` +
      `<div style="position:absolute;left:${x}px;width:${mw}px;text-align:center;top:${my1 + (o.labB ?? 8)}px;font-size:${o.labFs ?? 13}px;line-height:16px;color:var(--text)">${E(m.l)}</div>`;
  });
  return html;
}

// ---------- grading bottom: wheels + parade ----------
// GUI-FIX-2: a wheel's conic ring (72 arcs, a colour interpolated per arc) depends only on its geometry and palette; built
// once per geometry and reused, it no longer costs the hop onto the grading screen ~15 ms of string building (item 7).
const CONIC = new Map();
function wheels({ spec, data, w, h }) {
  const o = spec.opts || {};
  const pw = o.paradeW ?? Math.round(w * .36);
  const ww = w - pw, n = (data.wheels || []).length || 3;
  const r = o.r ?? Math.min(ww / n / 2 - 22, (h - 70) / 2), sw = o.ringW ?? 3;
  // ring: a conic sweep drawn as short arcs (SVG has no conic gradient); stops are [angle°, colour] counter-clockwise from 3 o'clock
  const RING = [[0, '#bcbcde'], [45, '#cea8db'], [90, '#d8a9cd'], [135, '#e3c4c1'], [180, '#dbd4b8'], [225, '#c5e1bb'], [270, '#b6d8bf'], [315, '#abd0d6'], [360, '#bcbcde']];
  const RINGE = [[0, '#dca98c'], [45, '#f99271'], [90, '#f3743d'], [135, '#f2ac7b'], [180, '#d8ca99'], [225, '#c1d397'], [270, '#bad09f'], [315, '#c7be9f'], [360, '#dca98c']];
  const hex = c => [1, 3, 5].map(k => parseInt(c.slice(k, k + 2), 16));
  const at = (st, a) => { let k = 0; while (k < st.length - 2 && st[k + 1][0] <= a) k++; const [a0, c0] = st[k], [a1, c1] = st[k + 1], t = (a - a0) / (a1 - a0), A = hex(c0), B = hex(c1); return `rgb(${A.map((v, j) => Math.round(v + (B[j] - v) * t)).join(',')})`; };
  const conic = (cx, cy, st) => { const key = `${cx},${cy},${r},${sw},${st === RINGE ? 1 : 0}`, hit = CONIC.get(key); if (hit) return hit; let s = ''; const N = 72; for (let k = 0; k < N; k++) { const a0 = k * 360 / N, a1 = (k + 1.15) * 360 / N, P = a => [px(cx + Math.cos(a * Math.PI / 180) * r), px(cy - Math.sin(a * Math.PI / 180) * r)]; const [x0, y0] = P(a0), [x1, y1] = P(a1); s += `<path d="M${x0} ${y0}A${r} ${r} 0 0 0 ${x1} ${y1}" stroke="${at(st, (k + .5) * 360 / N)}"/>`; } s = `<g fill="none" stroke-width="${sw}">${s}</g>`; if (CONIC.size > 64) CONIC.clear(); CONIC.set(key, s); return s; };
  let s = `<defs><radialGradient id="whc" cx="50%" cy="50%" r="50%"><stop offset="0" stop-color="#1b1a19"/><stop offset="1" stop-color="#121211"/></radialGradient>
   <linearGradient id="ring" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7a5ad8"/><stop offset=".35" stop-color="#3fa3a0"/><stop offset=".7" stop-color="#9fbf6a"/><stop offset="1" stop-color="#e8c79a"/></linearGradient>
   <linearGradient id="ringE" x1="0" y1="1" x2="1" y2="0"><stop offset="0" stop-color="#e8c79a"/><stop offset="1" stop-color="#ff5a23"/></linearGradient></defs>`;
  const lfs = o.labFs ?? 12, vfs = o.valFs ?? 11.5;
  const WH = data.wheels || [{ name: 'Lift', vals: ['0.00', '0.00', '0.00'] }, { name: 'Gamma', vals: ['0.00', '0.00', '0.00'] }, { name: 'Gain', vals: ['0.00', '0.00', '0.00'] }];   // neutral until a grade lands
  WH.forEach((W, i) => {
    const cx = o.cx ? o.cx[i] : ww / n * (i + .5), cy = (o.cy ?? (r + 34));
    s += `<text x="${cx}" y="${o.labY ?? 18}" font-size="${lfs}" fill="var(--wl-c,var(--text))" text-anchor="middle">${E(W.name)}</text>`;
    if (o.conic) s += `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${o.disc ?? '#0e0e0e'}"/>` + conic(cx, cy, W.em ? RINGE : RING);
    else s += `<circle cx="${cx}" cy="${cy}" r="${r}" fill="url(#whc)" stroke="url(#${W.em ? 'ringE' : 'ring'})" stroke-width="${sw}"/>`;
    const ci = r - sw / 2 - (o.cross ?? 8);
    s += `<line x1="${cx - ci}" x2="${cx + ci}" y1="${cy}" y2="${cy}" stroke="${o.crossC ?? '#262624'}"/><line x1="${cx}" x2="${cx}" y1="${cy - ci}" y2="${cy + ci}" stroke="${o.crossC ?? '#262624'}"/>`;
    const [dx, dy] = W.dot || [0, 0];
    s += `<circle cx="${px(cx + dx * r)}" cy="${px(cy - dy * r)}" r="${o.dotR ?? 4}" fill="${W.em ? 'var(--ember)' : 'var(--cream)'}"/>`;
    if (W.ring != null) { const a = W.ring * Math.PI / 180; s += `<circle cx="${px(cx + Math.cos(a) * r)}" cy="${px(cy - Math.sin(a) * r)}" r="${o.knobR ?? 4.5}" fill="var(--ember)"${o.conic ? ' stroke="#0e0e0e" stroke-width="1.5"' : ''}/>`; }
    s += `<text x="${cx}" y="${o.valY ?? (cy + r + 22)}" font-size="${vfs}" text-anchor="middle">` + W.vals.map((v, k) => `<tspan dx="${k ? (o.valGap ?? 10) : 0}" fill="${(W.em && k === 0) ? 'var(--ember-text)' : (W.cols && W.cols[k]) || 'var(--wv-c,var(--text))'}">${E(v)}</tspan>`).join('') + '</text>';
  });
  // parade: the engine's scope image when the data carries one, else the drawn luma traces
  const x0 = o.px0 ?? (ww + 34), x1 = w - (o.pxr ?? 12), y0 = o.py0 ?? 22, y1 = h - (o.pyb ?? 22);
  if (!o.noSep) s += `<line x1="${ww}" x2="${ww}" y1="14" y2="${h - 14}" stroke="var(--line)"/>`;
  const PL = data.parade && data.parade.live, pT = PL ? 0 : (o.imgPadT ?? 0), pB = PL ? 0 : (o.imgPadB ?? 0);
  if (data.parade && data.parade.src) s += `<image href="${E(data.parade.src)}" x="${x0}" y="${y0 - pT}" width="${x1 - x0}" height="${y1 - y0 + pT + pB}" preserveAspectRatio="none"${PL ? ' style="image-rendering:auto;opacity:.92"' : ''}/>`;
  s += [1023, 768, 512, 256, 0].map((v, i) => `<text x="${x0 - (o.plabGap ?? 6)}" y="${px(y0 + i * (y1 - y0) / 4 + (o.plabDy ?? 3))}" font-size="${o.plabFs ?? 9.5}" fill="var(--dim)" text-anchor="end" style="font-family:var(--plab-font,inherit)">${v}</text>` + (data.parade && data.parade.src && !PL ? '' : `<line x1="${x0}" x2="${x1}" y1="${px(y0 + i * (y1 - y0) / 4)}" y2="${px(y0 + i * (y1 - y0) / 4)}" stroke="#1a1a19"/>`)).join('');
  if (!(data.parade && data.parade.src)) {
    const cw = (x1 - x0) / 3;
    const cols = ['#e2553f', '#5fbf6a', '#4f7fe0'], lifts = [.05, 0, -.04];
    cols.forEach((c, k) => s += `<svg x="${x0 + k * cw + 3}" y="${y0}" width="${cw - 6}" height="${y1 - y0}">${lumaTrace(cw - 6, y1 - y0, 11 + k, c, .55, u => .42 + lifts[k] + .2 * Math.sin(u * 5 + k))}</svg>`);
  }
  return head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
}

// ---------- node stack (modelling) ----------
function stack({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  const y0 = o.y0 ?? 56, step = o.step ?? 64, cw = o.cw ?? 150, nh = o.nodeH ?? 30;
  (data.nodes || []).forEach((n, i) => {
    const y = y0 + i * step;
    if (o.center) {   // m08: nodes sized to their name, centred, joined by a hairline
      if (i) html += `<div style="position:absolute;left:${px(w / 2 - .5)}px;top:${y - step + nh}px;width:1px;height:${step - nh}px;background:var(--stack-line,#45443f)"></div>`;
      html += `<div class="snode${n.em ? ' em' : ''}${F(n)}" style="position:absolute;left:50%;transform:translateX(-50%);top:${y}px;height:${nh}px;font-size:${o.fs ?? 12}px">${E(n.name)}</div>`;
      return;
    }
    if (i) html += `<div style="position:absolute;left:${15 + cw / 2}px;top:${y - step + 30}px;width:1px;height:${step - 30}px;background:var(--faint)"></div>`;
    html += `<div style="position:absolute;left:15px;top:${y}px;width:${cw}px;height:30px;border:1px solid ${n.em ? 'var(--ember)' : 'var(--line)'};border-radius:4px;background:${n.em ? 'var(--ember-tint)' : 'var(--raise)'};display:flex;align-items:center;padding:0 10px;font-family:var(--mono);font-size:12px;color:var(--cream)">${E(n.name)}</div>`;
  });
  return html;
}
// ---------- profile / history (modelling) ----------
function profile({ spec, data, w, h }) {
  const o = spec.opts || {};
  if (o.ax != null) {   // m08 measured layout: axis, baseline, smooth profile with hollow diamonds, cursor, 80px history tiles
    const X = x => o.ax + x * (o.x1 - o.ax), Y = y => o.base - y * (o.base - o.top);
    const pts = (data.pts || []).map(([x, y]) => [X(x), Y(y)]);
    let s = `<line x1="${o.ax}" x2="${o.ax}" y1="${o.top - 6}" y2="${o.base}" stroke="var(--prof-axis,#24241f)" stroke-width="1"/><line x1="${o.ax}" x2="${o.x1}" y1="${o.base}" y2="${o.base}" stroke="var(--prof-base,#363531)" stroke-width="1.5"/>`;
    if (data.cursor != null) s += `<line x1="${px(X(data.cursor))}" x2="${px(X(data.cursor))}" y1="${o.top - 6}" y2="${o.base}" stroke="#55534d" stroke-width="1"/>`;
    if (pts.length > 1) s += `<path d="${o.smooth ? catmull(pts) : pts.map((q, i) => (i ? 'L' : 'M') + px(q[0]) + ' ' + px(q[1])).join('')}" fill="none" stroke="var(--cream)" stroke-width="1.5"/>`;
    pts.forEach(q => s += `<path d="M${px(q[0])} ${px(q[1] - 4.5)}l4.5 4.5-4.5 4.5-4.5-4.5z" fill="${o.hollow ? 'var(--panel)' : 'var(--cream)'}" stroke="var(--cream)" stroke-width="1.3"/>`);
    let html = head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
    (data.thumbs || []).forEach((t, i) => { const x = o.tx + i * (o.tw + o.gap);
      html += `<div class="ptile${t.sel ? ' sel' : ''}${F(t)}" style="left:${px(x)}px;top:${o.ty}px;width:${o.tw}px;height:${o.th}px">${t.src ? `<img src="${E(t.src)}" alt="">` : ''}</div>` +
        `<div class="ptlab" style="left:${px(x - 10)}px;width:${o.tw + 20}px;top:${o.labY - 11}px">${E(t.label)}</div>`; });
    return html;
  }
  const gw = o.gw ?? Math.round(w * .38);
  let s = `<line x1="40" x2="40" y1="40" y2="${h - 14}" stroke="#2a2a28"/>`;
  const pts = (data.pts || []).map(([x, y]) => [40 + x * (gw - 60), h - 18 - y * (h - 60)]);
  s += `<path d="${pts.map((p, i) => (i ? 'L' : 'M') + px(p[0]) + ' ' + px(p[1])).join('')}" fill="none" stroke="var(--cream)" stroke-width="1.4"/>`;
  pts.forEach(p => s += `<path d="M${px(p[0])} ${px(p[1] - 4)} l4 4 l-4 4 l-4 -4z" fill="var(--cream)"/>`);
  if (data.cursor != null) { const x = 40 + data.cursor * (gw - 60); s += `<line x1="${x}" x2="${x}" y1="36" y2="${h - 12}" stroke="#8a867d"/>`; }
  let html = head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
  const items = data.thumbs || []; const tx0 = gw + 20, tw = (w - tx0 - 15 - 12 * (items.length - 1)) / Math.max(1, items.length);
  items.forEach((t, i) => html += `<div style="position:absolute;left:${px(tx0 + i * (tw + 12))}px;width:${px(tw)}px;top:${o.ty ?? 36}px;height:${o.th ?? 74}px" class="thumb ${t.sel ? 'sel' : ''}">${t.src ? `<img src="${E(t.src)}">` : ''}</div><div class="mono dim" style="position:absolute;left:${px(tx0 + i * (tw + 12))}px;width:${px(tw)}px;top:${(o.ty ?? 36) + (o.th ?? 74) + 6}px;text-align:center;font-size:11px">${E(t.label)}</div>`);
  return html;
}
// ---------- simple list rows (lights, emitters) ----------
function list({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  (data.rows || []).forEach((r, i) => {
    const y = (o.y0 ?? 44) + i * ((o.rowH ?? 30) + (o.gap ?? 0));
    html += `<div style="position:absolute;left:${o.chips ? 10 : 15}px;right:${o.chips ? 10 : 15}px;top:${y}px;height:${o.rowH ?? 30}px;display:flex;align-items:center;gap:10px;font-size:${o.fs ?? 12.5}px;padding:0 ${o.chips ? (o.padX ?? 9) : 0}px;${o.chips ? 'background:var(--raise);border-radius:4px;' : ''}${o.lines ? 'border-bottom:1px solid var(--line2);' : ''}">` +
      (r.icon ? `<span style="width:16px;text-align:center;color:${r.dim ? 'var(--faint)' : 'var(--cream)'}">${r.icon === 'bulb' ? '<svg width="12" height="14" viewBox="0 0 12 14"><path d="M6 1a4.5 4.5 0 0 0-2.6 8.2V11h5.2V9.2A4.5 4.5 0 0 0 6 1z" fill="' + (o.iconFill ? 'currentColor' : 'none') + '" stroke="currentColor" stroke-width="1.2"/><path d="M4 12.8h4" stroke="currentColor" stroke-width="1.2"/></svg>' : E(r.icon)}</span>` : '') +
      `<span style="font-family:var(--label-font)" class="${r.dim ? 'dim' : 'cream'}">${E(r.k)}</span><span class="mono ${r.em ? 'em' : o.vCream ? 'cream' : 'dim'}" style="margin-left:auto">${E(r.v)}</span></div>`;
  });
  return html;
}
// ---------- scene tree ----------
function tree({ spec, data, w, h }) {
  const o = spec.opts || {};
  let html = head(spec, data);
  // per-type icons traced from the m13 mockup (17 x 15): camera with reel + lens, bulb (key/rim), bulb with rays (fill), stacked plates (mesh), floor plane
  const SW = 'fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"';
  const bulb = (cx, cy, r) => `<path d="M${cx - r * .62} ${cy + r * .78}A${r} ${r} 0 1 1 ${cx + r * .62} ${cy + r * .78}L${cx + r * .45} ${cy + r * 1.25}H${cx - r * .45}Z" ${SW}/><path d="M${cx - r * .4} ${cy + r * 1.75}H${cx + r * .4}" ${SW}/>`;
  const ic = { cam: `<circle cx="5" cy="3.6" r="2.5" ${SW}/><circle cx="5" cy="3.6" r=".5" fill="currentColor"/><rect x="1.4" y="6.2" width="9.6" height="7.4" rx="1.8" ${SW}/><path d="M11.2 9.9L15.8 7.2V13L11.2 10.9" ${SW}/>`,
    light: bulb(8.5, 5.6, 4.1),
    fill: bulb(8.5, 6.4, 3.3) + [[8.5, .9], [3.6, 2.6], [13.4, 2.6], [2, 6.6], [15, 6.6]].map(([x, y]) => `<circle cx="${x}" cy="${y}" r=".75" fill="currentColor"/>`).join(''),
    mesh: `<ellipse cx="8.5" cy="3" rx="6" ry="1.9" ${SW}/><path d="M2.6 3.4C3.4 5.4 5.6 6.2 8.5 6.2S13.6 5.4 14.4 3.4" ${SW}/><ellipse cx="8.5" cy="7.4" rx="6.3" ry="1.6" ${SW}/><path d="M3.2 12.2C3.6 10.4 5.6 9.7 8.5 9.7S13.4 10.4 13.8 12.2" ${SW}/><ellipse cx="8.5" cy="12.4" rx="6.4" ry="1.7" ${SW}/>`,
    plane: `<path d="M1.4 12.6L4.6 8.4H12.4L15.6 12.6Z" ${SW}/>` };
  if (o.headLine) html += `<i style="position:absolute;left:0;right:0;top:${o.headLine}px;height:1px;background:var(--line)"></i>`;
  const items = data.items || [];
  if (o.guide != null && items.length > 1) { const y1 = (o.y0 ?? 44) + (o.rowH ?? 30) * .5, y2 = (o.y0 ?? 44) + (items.length - 1) * (o.rowH ?? 30) + (o.rowH ?? 30) / 2 - 2;
    html += `<i style="position:absolute;left:${o.guide}px;top:${px(y1 + (o.rowH ?? 30) / 2)}px;height:${px(y2 - y1 - (o.rowH ?? 30) / 2)}px;width:1px;background:#3a3935"></i>`;
    items.forEach((it, i) => { if (i) html += `<i style="position:absolute;left:${o.guide}px;top:${px((o.y0 ?? 44) + i * (o.rowH ?? 30) + (o.rowH ?? 30) / 2 - 2)}px;width:8px;height:1px;background:#3a3935"></i>`; }); }
  items.forEach((it, i) => {
    const y = (o.y0 ?? 44) + i * (o.rowH ?? 30);
    html += `<div style="position:absolute;left:${o.selL ?? 10}px;right:${o.selR ?? 10}px;top:${y}px;height:${(o.rowH ?? 30) - 4}px;display:flex;align-items:center;gap:${o.gap ?? 9}px;padding-left:${(o.padL ?? 10) + (it.d || 0) * 16}px;border-radius:${o.selRad ?? 4}px;${it.sel ? `background:${o.selBg ?? 'var(--raise2)'};` : ''}font-family:${o.font ?? 'var(--mono)'};font-size:${o.fs ?? 12}px">` +
      `<svg width="${o.iconW ?? 17}" height="${(o.iconW ?? 17) * 15 / 17}" viewBox="0 0 17 15" style="flex:none;color:${it.sel || o.iconCream ? 'var(--cream)' : 'var(--dim)'}">${ic[it.icon] || ''}</svg><span class="${it.sel ? 'cream' : 'text'}" style="color:${it.sel || o.textCream ? 'var(--cream)' : 'var(--text)'}">${E(it.name)}</span></div>`;
  });
  return html;
}
// ---------- light rig (lighting) ----------
function lightrig({ spec, data, w, h }) {
  const o = spec.opts || {};
  // m11 measured layout when o.cx is given (diagram centre/radius, table columns, thumb grid); the older
  // compact layout otherwise
  if (o.cx != null) {
    const cx = o.cx, cy = o.cy, R = o.R, fs = o.fs ?? 17;
    let s = `<circle cx="${cx}" cy="${cy}" r="${R}" fill="none" stroke="${o.ringC ?? '#3a3935'}" stroke-width="1.5"/>`;
    s += `<rect x="${cx - 11}" y="${cy - 17}" width="22" height="30" rx="3" fill="${o.objC ?? '#6d6a64'}"/>`;
    for (const L of (data.lights || [])) {
      const a = L.a * Math.PI / 180, ca = Math.cos(a), sa = Math.sin(a), x = cx + ca * R, y = cy - sa * R, col = L.em ? 'var(--ember)' : 'var(--cream)';
      const up = sa > .7, lx = up ? x : x + (ca >= 0 ? 14 : -14), ly = up ? y - 14 : y + 20;
      s += `<line x1="${px(x)}" y1="${px(y)}" x2="${cx}" y2="${cy}" stroke="${col}" stroke-width="1.5" opacity="${L.em ? 1 : .55}"/><circle cx="${px(x)}" cy="${px(y)}" r="${o.dotR ?? 7}" fill="${col}"/>` +
        `<text x="${px(lx)}" y="${px(ly)}" style="font-family:var(--sans);font-size:${fs}px" fill="${L.em ? 'var(--ember)' : 'var(--dim)'}" text-anchor="${up ? 'middle' : ca >= 0 ? 'start' : 'end'}">${E(L.name)}</text>`;
    }
    let html = head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
    const C = o.cols, hy = o.hy, ry = o.ry, rh = o.rh;
    const cell = (x, y, txt, al, cls, f) => `<span class="${cls}" style="position:absolute;${al === 'l' ? `left:${x}px;` : ''}${al === 'r' ? `right:${w - x}px;` : al === 'c' ? `left:${x - 60}px;width:120px;text-align:center;` : ''}top:${px(y - 12)}px;line-height:24px;font-size:${f}px;font-family:var(--sans);white-space:nowrap">${E(txt)}</span>`;
    html += cell(C[0], hy, 'LIGHT', 'l', 'dim', o.hfs ?? 14) + cell(C[1], hy, 'TEMP', 'c', 'dim', o.hfs ?? 14) + cell(C[2], hy, 'INTENSITY', 'c', 'dim', o.hfs ?? 14) + cell(C[3], hy, 'FALLOFF', 'r', 'dim', o.hfs ?? 14);
    html += `<i style="position:absolute;left:${C[0]}px;right:${w - C[3]}px;top:${px(hy + 19)}px;height:1px;background:var(--line)"></i>`;
    (data.rows || []).forEach((r, i) => {
      const y = ry + i * rh;
      html += cell(C[0], y, r[0], 'l', 'cream', fs) + (r[1] ? cell(C[1], y, r[1], 'c', r[4] ? 'cream' : 'dim', fs) : '');
      if (r[2] != null && r[2] !== '') {
        const [b0, b1] = o.bar, vx = b0 + Math.min(Math.max(+r[3] || 0, .08), o.vmax ?? .45) * (b1 - b0);
        html += `<i style="position:absolute;left:${b0}px;width:${px(Math.max(0, vx - 20 - b0))}px;top:${px(y)}px;height:2px;background:var(--cream)"></i>` +
          `<i style="position:absolute;left:${px(vx + 20)}px;width:${px(Math.max(0, b1 - vx - 20))}px;top:${px(y)}px;height:2px;background:${o.barDim ?? '#3a3935'}"></i>` + cell(vx, y, r[2], 'c', 'cream', fs);
      }
      if (r[5] != null && r[5] !== '') html += cell(C[3], y, r[5], 'r', 'cream', fs);
      if (i < (data.rows || []).length - 1) html += `<i style="position:absolute;left:${C[0]}px;right:${w - C[3]}px;top:${px(y + rh / 2)}px;height:1px;background:var(--line2)"></i>`;
    });
    const T = o.thumbs; (data.thumbs || []).forEach((t, i) => {
      const x = T.x + (i % 2) * (T.w + T.gx), y = T.y + Math.floor(i / 2) * (T.h + T.gy);
      html += `<div class="thumb ${t.sel ? 'sel' : ''}" style="position:absolute;left:${x}px;top:${y}px;width:${T.w}px;height:${T.h}px;border-radius:${T.r ?? 6}px">${t.src ? `<img src="${E(t.src)}">` : ''}</div>` +
        `<span class="${t.sel ? 'cream' : 'dim'}" style="position:absolute;left:${x - 10}px;width:${T.w + 20}px;text-align:center;top:${px(y + T.h + T.ly - 12)}px;line-height:24px;font-size:${fs}px;font-family:var(--sans)">${E(t.label)}</span>`;
    });
    return html;
  }
  const dw = o.dw ?? 190, tw = o.tw ?? 250;
  let s = '';
  const cx = 15 + dw / 2, cy = (h + 20) / 2, R = Math.min(dw / 2 - 10, (h - 70) / 2);
  s += `<circle cx="${cx}" cy="${cy}" r="${R}" fill="none" stroke="#2a2a28"/><circle cx="${cx}" cy="${cy}" r="${R * .55}" fill="none" stroke="#1d1d1c"/>`;
  s += `<rect x="${cx - 6}" y="${cy - 10}" width="12" height="20" rx="3" fill="#8a867d"/>`;
  for (const L of (data.lights || [])) {
    const a = L.a * Math.PI / 180, x = cx + Math.cos(a) * R, y = cy - Math.sin(a) * R;
    s += `<line x1="${px(x)}" y1="${px(y)}" x2="${cx}" y2="${cy}" stroke="${L.em ? 'var(--ember)' : '#5a5650'}" stroke-dasharray="2 3"/><circle cx="${px(x)}" cy="${px(y)}" r="6" fill="${L.em ? 'var(--ember)' : 'var(--cream)'}"/><text x="${px(x + (Math.cos(a) >= 0 ? 10 : -10))}" y="${px(y + 4)}" font-size="10.5" fill="var(--dim)" text-anchor="${Math.cos(a) >= 0 ? 'start' : 'end'}">${E(L.name)}</text>`;
  }
  let html = head(spec, data) + `<svg style="position:absolute;inset:0" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${s}</svg>`;
  const tx = 15 + dw + 30, tr = w - (o.thumbsW ?? 250) - 30;
  const cols = 'grid-template-columns:1.2fr 1fr 1.4fr .7fr';
  html += `<div class="tbl" style="left:${tx}px;right:${w - tr}px;top:${o.ty ?? 44}px"><div class="tr h" style="${cols}"><span>LIGHT</span><span>TEMP</span><span>INTENSITY</span><span style="text-align:right">FALLOFF</span></div>` +
    (data.rows || []).map(r => `<div class="tr" style="${cols};height:34px"><span class="mono">${E(r[0])}</span><span class="mono dim">${E(r[1])}</span><span style="display:flex;align-items:center;gap:8px"><span style="flex:1;height:2px;background:#2a2a28;position:relative"><i style="position:absolute;left:0;top:0;bottom:0;width:${r[3] * 100}%;background:${r[4] ? 'var(--ember)' : '#bdb8ae'}"></i></span><span class="mono" style="width:3ch;text-align:right">${E(r[2])}</span></span><span class="mono dim" style="text-align:right">${E(r[5])}</span></div>`).join('') + '</div>';
  const th = data.thumbs || []; const gx = w - (o.thumbsW ?? 250) - 15, gw = (o.thumbsW ?? 250), cw = (gw - 10) / 2, chh = (h - 40 - 10) / 2;
  th.forEach((t, i) => { const x = gx + (i % 2) * (cw + 10), y = 20 + Math.floor(i / 2) * (chh + 10); html += `<div class="thumb ${t.sel ? 'sel' : ''}" style="position:absolute;left:${px(x)}px;top:${px(y)}px;width:${px(cw)}px;height:${px(chh)}px">${t.src ? `<img src="${E(t.src)}">` : ''}<span class="mono" style="position:absolute;left:6px;bottom:4px;font-size:10.5px;color:var(--cream)">${E(t.label)}</span></div>`; });
  return html;
}
// ---------- renders table (export) ----------
function renders({ spec, data, w, h }) {
  const o = spec.opts || {};
  // m14 measured layout when o.colW is given: px columns, sans header, every row cream (queued / "-" dim), the
  // status glyph in its own slot so the words line up with the header, a rule above a right-aligned footer
  const M = !!o.colW;
  const cols = `grid-template-columns:${M ? o.colW.map(c => typeof c === 'number' ? c + 'px' : c).join(' ') : (o.cols || '2.2fr 1.6fr 1fr 1.6fr 1fr')}`;
  const icoW = o.icoW ?? 22;
  const status = st => { const m = /^([●✓✕])\s*(.*)$/.exec(st); return M ? (m ? `<i class="rico">${E(m[1])} </i>${E(m[2])}` : `<i class="rico"></i>${E(st)}`) : E(st); };
  let html = head(spec, data);
  html += `<div class="tbl${M ? ' meas' : ''}" style="top:${o.top ?? 42}px${M ? `;left:${o.left ?? 13}px;right:${o.right ?? 12}px;--rico-w:${icoW}px` : ''}">` + `<div class="tr h" style="${cols}${M ? `;height:${o.headH ?? 28.5}px` : ''}">${(data.head || []).map((x, i) => `<span${M && i === 3 ? ' class="rst"' : ''}>${E(x)}</span>`).join('')}</div>` +
    (data.rows || []).map(r => `<div class="tr" style="${cols};height:${r._job ? (o.jobH ?? 44) : (o.rowH ?? 30)}px">` + r.map((c, i) => {
      if (i === 3) { const st = String(c); const em = st.startsWith('●'); return `<span class="mono ${em ? 'em' : st.startsWith('✕') ? 'err' : M && !/^[✓●]/.test(st) ? 'dim' : M ? 'cream' : 'dim'}">${status(st)}</span>`; }
      // a live job row (engine export): fixed 44px from its first frame so nothing shifts when it lands;
      // 16:9 thumb of the engine's last frame + output path under the name, each only when sent
      if (i === 0 && r._job) return `<span class="rjob">${r._thumb ? `<img class="rthumb" src="${E(r._thumb)}">` : '<i class="rthumb empty"><svg width="16" height="12" viewBox="0 0 16 12" fill="none" stroke="currentColor" stroke-width="1.1"><rect x=".6" y=".6" width="14.8" height="10.8" rx="1.6"/><path d="M4 .6v10.8M12 .6v10.8M.6 4h3.4M.6 8h3.4M12 4h3.4M12 8h3.4"/></svg></i>'}<span class="rname"><span class="mono cream">${E(c)}</span>${r._path ? `<span class="mono dim rpath">${E(r._path)}</span>` : ''}</span></span>`;
      return `<span class="${M ? (String(c) === '-' ? 'mono dim' : 'mono cream') : i ? 'mono dim' : 'mono cream'}">${E(c)}</span>`;
    }).join('') + '</div>').join('') + '</div>';
  if (M && o.footLine != null && data.foot) html += `<i style="position:absolute;left:${o.left ?? 13}px;right:${o.right ?? 12}px;top:${o.footLine}px;height:1px;background:var(--line)"></i>`;
  if (data.foot) { const f = E(data.foot), em = data.footEm ? E(data.footEm) : null;
    html += `<div class="pfoot${M ? ' meas' : ''}" style="left:auto;right:${M ? (o.right ?? 12) + 1 : 15}px">${em && f.includes(em) ? f.replace(em, `<span class="cream">${em}</span>`) : f}</div>`; }
  return html;
}

export const KINDS = { agent, viewer, kv, timeline, keys, curves, transcript, history, diffbatch, media, scopes, cie, loudness, wheels, stack, profile, list, tree, lightrig, renders };
