// Coverage lane B: the graph-editor, motion and measurement tools, each on an element its screen
// already has (no new panel). Everything drawn comes from the call's own args, its result, or the
// engine's own before/after key lists (payload.changed / from / to); nothing is guessed.
//   m03 Graph (curves panel + Inspector): move_key, remove_key, smooth_curve, simplify_curve,
//     bake_property, apply_motion_preset, copy_curve, set_animator_keyframe, motion_extract, motion_diff
//   viewer transform gizmo: remove_motion_path (the removed path, dashed, fading with the gizmo)
//   viewer overlay: solve_constraints (solved boxes), compute_text_bounds (line boxes, ink box)
//   m08 Stack + Inspector: raycast_mesh_raw · m13 Scene tree + Inspector: compile_scene3d_descriptor
//   m06 fx badge + Inspector: add_managed_linear_effect · m15 Inspector: preview_subject_match
// The engine's spectator stream lists at most 8 keys of a long key list and ends it with "…+N";
// the curves draw the keys it listed and the lane / Inspector say how many keys there are in all.

export const COVB = /^(apply_motion_preset|bake_property|copy_curve|move_key|remove_key|remove_motion_path|set_animator_keyframe|simplify_curve|smooth_curve|motion_diff|motion_extract|solve_constraints|add_managed_linear_effect|compute_text_bounds|raycast_mesh_raw|compile_scene3d_descriptor|preview_subject_match)$/;

const PROP = { x: 'position x', y: 'position y', pos_x: 'position x', pos_y: 'position y', scale: 'scale', rotation: 'rotation', rot_z: 'rotation', opacity: 'opacity' };
const propLabel = p => PROP[String(p).toLowerCase()] || String(p).replace(/_/g, ' ');
const isNum = v => typeof v === 'number' && Number.isFinite(v);
const keyList = L => {   // the engine's key list (possibly cut short) -> { keys: [[t, v, ease]], more }
  const out = { keys: [], more: 0 };
  if (!Array.isArray(L)) return out;
  for (const k of L) {
    if (typeof k === 'string') { const m = /\+(\d+)/.exec(k); if (m) out.more += +m[1]; continue; }
    if (k && isNum(k.t) && isNum(k.v)) out.keys.push([k.t, k.v, k.ease]);
  }
  return out;
};
const PROPKEY = /^props\.([\w]+)\.keys$/, ANIMKEY = /^text_fx\.animators\[([^\]]+)\]\.(\w+)\.keys$/;

// Remember every key list the engine reports, so later graph calls (copy_curve) know their source.
export function covbLearn(state, ev) {
  if (ev.phase === 'start' && ev.tool_name === 'add_text_animator') { const A = (ev.payload || {}).args || {}; (state._covbAnimArgs || (state._covbAnimArgs = {}))[ev.call_id] = A; }
  if (ev.phase !== 'complete') return;
  const P = ev.payload || {}, T = ev.target || {}, ch = P.changed && P.changed.clip;
  if (ev.tool_name === 'add_text_animator') {   // the animator's preset name, for its lane label (never its id)
    const A = P.args || (state._covbAnimArgs || {})[ev.call_id] || {}, R = P.result || {}; delete (state._covbAnimArgs || {})[ev.call_id];
    if (R.animator_id != null && A.preset) (state._covbAnim || (state._covbAnim = {}))[R.animator_id] = String(A.preset);
  }
  if (ch) for (const [k, ba] of Object.entries(ch)) { const mm = /^mask\.layers\[([^\]]+)\]\.name$/.exec(k);   /* mask layer names, so a mask reads by name (preview_subject_match regions) */
    if (mm && Array.isArray(ba) && ba[1] != null && ba[1] !== '') (state._covbMasks || (state._covbMasks = {}))[mm[1]] = String(ba[1]); }
  if (!ch || T.clip_id == null) return;
  const C = state._covbCurves || (state._covbCurves = {});
  for (const [k, ba] of Object.entries(ch)) {
    if (!Array.isArray(ba)) continue;
    const m = PROPKEY.exec(k), a = ANIMKEY.exec(k); if (!m && !a) continue;
    const id = `${T.clip_id}|${m ? m[1] : 'anim:' + a[1] + ':' + a[2]}`, kl = keyList(ba[1]);
    if (kl.keys.length || kl.more) C[id] = kl; else delete C[id];
  }
}

export function covbOps(state, MODES, ev, H) {
  const tn = String(ev.tool_name), P = ev.payload || {}, T = ev.target || {};
  const st = state._covbArgs || (state._covbArgs = {});
  if (ev.phase === 'start') { st[ev.call_id] = P.args || {}; return false; }
  if (ev.phase !== 'complete') { if (ev.phase === 'error') delete st[ev.call_id]; return false; }
  const A = P.args || st[ev.call_id] || {}; delete st[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const [mode, V] = H.modeViews(state, MODES, ev);
  const PP = MODES[mode] ? MODES[mode].panels : {};
  const pk = kind => { const e = Object.entries(PP).find(([, sp]) => sp.kind === kind); return e ? (V[e[0]] || (V[e[0]] = {})) : null; };
  const f = (v, p) => H.fmtVal(v, p), s = v => `${f(Math.round(v * 1000) / 1000)} s`;
  const clip = T.clip_id ?? A.clip_id ?? null, cname = id => id == null ? null : H.clipName(state, MODES, id);
  // Inspector rows for this call (the screen's own kv panel), "from → to" where the call changed a value
  const insp = (head, rows) => { const K = pk('kv'); if (!K) return false; K.blocks = (K.blocks || []).filter(b => !b._live && !b._covb);
    K.blocks.push({ sec: head, _live: true, _covb: true }); for (const r of rows.filter(r => r && r[1] != null && r[1] !== '').slice(0, 7)) K.blocks.push({ k: r[0], v: String(r[1]), ...(r[2] != null ? { from: String(r[2]) } : {}), _live: true, _covb: true, _flash: H.now() });
    ev._kvDone = true; return true; };
  const head = (...xs) => xs.filter(x => x != null && x !== '').join(' · ');

  // ---------------- graph editor (m03): curves + key squares + lane chips ----------------
  const before = (prop) => { const ch = P.changed && P.changed.clip; const ba = ch && ch[`props.${prop}.keys`]; return Array.isArray(ba) ? [keyList(ba[0]), keyList(ba[1])] : null; };
  const graph = (lanes, o = {}) => {   // lanes: [{label, cur, prev, em, marks: [[t, v, kind]]}] with times in seconds (or frames, o.frames)
    const G = pk('curves'); if (!G) return false;
    lanes = lanes.slice(0, 5);
    const ts = lanes.flatMap(L => [...(L.cur || []), ...(L.prev || []), ...(L.marks || [])].map(k => k[0])).concat(o.span || []).filter(isNum);
    let t0 = ts.length ? Math.min(...ts) : 0, t1 = ts.length ? Math.max(...ts) : 1; if (t1 - t0 < 1e-6) { t0 -= .5; t1 += .5; }
    const raw = (t1 - t0) / 6, mag = Math.pow(10, Math.floor(Math.log10(raw))), step = [1, 2, 2.5, 5, 10].map(m => m * mag).find(x => x >= raw) || raw, pad = (t1 - t0) * .04;
    G.ruler = { t0: t0 - pad, t1: t1 + pad, step, first: Math.ceil((t0 - pad) / step - 1e-9) * step, fmt: o.frames ? 'n' : 's' };
    G.curves = []; G.lanes = [];
    for (const L of lanes) {
      const vs = [...(L.cur || []), ...(L.prev || []), ...(L.marks || [])].map(k => k[1]).filter(isNum);
      let lo = vs.length ? Math.min(...vs) : 0, hi = vs.length ? Math.max(...vs) : 1; if (hi - lo < 1e-9) { lo -= 1; hi += 1; }
      const N = v => .12 + (v - lo) / (hi - lo) * .76;
      if (L.prev && L.prev.length) G.curves.push({ pts: L.prev.map(k => [k[0], N(k[1])]), keys: L.prev.length <= 40 ? L.prev.map(k => [k[0], N(k[1]), '#4d4a44']) : [], color: '#4d4a44', w: 1.4, dash: '5 4', cls: 'cvb-prev' });
      if (L.cur && L.cur.length) G.curves.push({ pts: L.cur.length > 1 ? L.cur.map(k => [k[0], N(k[1])]) : [[L.cur[0][0], N(L.cur[0][1])], [L.cur[0][0], N(L.cur[0][1])]], keys: L.cur.length <= 60 ? L.cur.map(k => [k[0], N(k[1])]) : [], em: !!L.em, color: L.em ? null : '#8a8780', cls: L.em ? 'cvb-cur cvb-em' : 'cvb-cur' });
      for (const [t, v, kind] of (L.marks || [])) if (isNum(t) && isNum(v)) G.curves.push({ pts: [[t, N(v)], [t, N(v)]], keys: [[t, N(v), kind === 'gone' ? 'none' : 'var(--ember)', kind === 'gone' ? 'var(--ember)' : null]], cls: 'cvb-mark cvb-' + kind });
      G.lanes.push(L.label);
    }
    G.playhead = o.playhead != null ? o.playhead : null; G._covb = ev.call_id;
    return true;
  };
  const near = (keys, t) => keys.reduce((b, k) => (b == null || Math.abs(k[0] - t) < Math.abs(b[0] - t) ? k : b), null);
  const prop = String(A.property ?? T.param ?? ''), nm = cname(clip);

  if (tn === 'move_key' || tn === 'remove_key' || tn === 'smooth_curve' || tn === 'simplify_curve' || tn === 'bake_property') {
    const ba = before(prop) || [keyList(P.from), keyList(P.to)];
    const [b, a] = ba, more = a.more ? ` · ${a.keys.length + a.more} keys` : '';
    const hit = (() => {
      if (tn === 'move_key') {
        const old = near(b.keys, numOrNull(A.time)), nt = numOrNull(A.new_time) ?? (old && old[0]), nv = near(a.keys, nt);
        graph([{ label: propLabel(prop), cur: a.keys, em: true, marks: [...(old ? [[old[0], old[1], 'gone']] : []), ...(nv ? [[nv[0], nv[1], 'new']] : [])] }], { playhead: nt });
        return insp(head(nm, 'move key'), [['Property', propLabel(prop)], old && nt != null && Math.abs(old[0] - nt) > 1e-9 ? ['Key time', s(nt), s(old[0])] : (old ? ['Key time', s(old[0])] : null), old && nv ? ['Value', f(nv[1]), Math.abs(old[1] - nv[1]) > 1e-9 ? f(old[1]) : null] : null, ['Keys', a.keys.length + a.more]]);
      }
      if (tn === 'remove_key') {
        const gone = b.keys.find(k => !a.keys.some(q => Math.abs(q[0] - k[0]) < 1e-6)) || near(b.keys, numOrNull(A.time));
        graph([{ label: propLabel(prop), cur: a.keys, prev: b.keys, em: true, marks: gone ? [[gone[0], gone[1], 'gone']] : [] }], { playhead: gone ? gone[0] : numOrNull(A.time) });
        return insp(head(nm, 'remove key'), [['Property', propLabel(prop)], gone ? ['Removed', `${s(gone[0])} · ${f(gone[1])}`] : null, ['Keys', a.keys.length + a.more, b.keys.length + b.more]]);
      }
      if (tn === 'smooth_curve') {
        const d = a.keys.map((k, i) => b.keys[i] ? Math.abs(k[1] - b.keys[i][1]) : 0), mx = d.length ? Math.max(...d) : null, at = mx != null ? a.keys[d.indexOf(mx)] : null;
        graph([{ label: propLabel(prop), cur: a.keys, prev: b.keys, em: true }], { playhead: at ? at[0] : null });
        return insp(head(nm, 'smooth curve'), [['Property', propLabel(prop)], ['Sigma', `${f(numOrNull(A.sigma) ?? 2)} keys`], ['Ends', R.pin_ends || A.pin_ends ? 'pinned' : 'free'], ['Keys', a.keys.length + a.more], at ? ['Max change', `${f(Math.round(mx * 100) / 100)} @ ${s(at[0])}`] : null, at && b.keys[d.indexOf(mx)] ? [`Value @ ${s(at[0])}`, f(at[1]), f(b.keys[d.indexOf(mx)][1])] : null]);
      }
      if (tn === 'simplify_curve') {
        const bn = numOrNull(R.before) ?? b.keys.length + b.more, an = numOrNull(R.after) ?? a.keys.length + a.more;
        graph([{ label: `${propLabel(prop)} · ${an} keys`, cur: a.keys, prev: b.keys, em: true }]);
        return insp(head(nm, 'simplify curve'), [['Property', propLabel(prop)], ['Keys', an, bn], ['Tolerance', f(numOrNull(A.tolerance) ?? .25)], numOrNull(R.max_error) != null ? ['Max error', f(R.max_error)] : null]);
      }
      /* bake_property: dense linear keys over [start, end]; the expression it replaced, dropped */
      const n = numOrNull(R.total_keys ?? R.keys) ?? a.keys.length + a.more, rg = Array.isArray(R.range) ? R.range : [numOrNull(A.start), numOrNull(A.end)];
      const ch = P.changed && P.changed.clip, ex = ch && ch[`props.${prop}.expr`], exOld = Array.isArray(ex) && Array.isArray(ex[0]) ? ex[0].map(o => o && o.op).filter(Boolean).join(' + ') : '';
      graph([{ label: `${propLabel(prop)} · ${n} keys`, cur: a.keys, prev: b.keys.length ? b.keys : null, em: true }], { span: rg.filter(isNum) });
      return insp(head(A.camera ? 'camera' : nm, 'bake'), [['Property', propLabel(prop)], ['Baked', `${n} keys · ${rg.filter(isNum).map(x => f(x)).join('–')} s`], ['Keys', n, b.keys.length + b.more], numOrNull(A.fps) != null ? ['Rate', `${f(A.fps)} keys/s`] : null, exOld ? ['Expression', 'dropped', exOld] : null, numOrNull(R.max_error) != null ? ['Max error', `${f(Math.round(R.max_error * 1000) / 1000)}${numOrNull(R.max_error_at) != null ? ' at ' + s(R.max_error_at) : ''}`] : null, R.outside_range_changed ? ['Outside range', 'edge held'] : null]);
    })();
    return hit;
  }
  if (tn === 'apply_motion_preset') {
    const ch = (P.changed && P.changed.clip) || {}, lanes = [];
    for (const [k, ba] of Object.entries(ch)) { const m = PROPKEY.exec(k); if (!m || !Array.isArray(ba)) continue; const a = keyList(ba[1]), b = keyList(ba[0]); lanes.push({ label: propLabel(m[1]), cur: a.keys, prev: b.keys.length ? b.keys : null, em: false, _p: m[1], _n: a.keys.length + a.more, _d: a.keys.length, _b: b.keys.length + b.more }); }
    const ord = ['x', 'y', 'scale', 'rotation', 'opacity']; lanes.sort((p, q) => ord.indexOf(p._p) - ord.indexOf(q._p)); if (lanes[0]) lanes[0].em = true;
    const ap = Array.isArray(R.applied) ? R.applied : [];
    const names = ap.map(x => x && x.preset).filter(Boolean), at = ap.length ? numOrNull(ap[0].at) : numOrNull(A.at), dur = ap.length ? numOrNull(ap[0].duration) : numOrNull(A.duration);
    if (lanes.length) graph(lanes, { playhead: at, span: at != null && dur != null ? [at, at + dur] : [] });
    return insp(head(nm, 'motion preset'), [['Preset', names.length ? names.join(' + ') : (A.preset ?? A.prompt)], at != null ? ['At', s(at)] : null, dur != null ? ['Duration', s(dur)] : null, ap.length && numOrNull(ap[0].frames) != null ? ['Frames', ap.map(x => x.frames).join(' + ')] : null, lanes.length ? ['Channels', lanes.map(l => l._p).join(', ')] : null, lanes.length ? ['Keys', `${lanes[0]._n} each${lanes[0]._d < lanes[0]._n ? ` (${lanes[0]._d} drawn)` : ''}`, lanes.every(l => l._b === lanes[0]._b) ? `${lanes[0]._b}` : null] : null, numOrNull(A.intensity) != null ? ['Intensity', f(A.intensity)] : null]);
  }
  if (tn === 'copy_curve') {
    const C = state._covbCurves || {}, src = C[`${A.from_clip}|${A.from_property}`], had = C[`${A.to_clip}|${A.to_property}`], hadN = had ? had.keys.length + had.more : null;
    const ts = numOrNull(A.time_scale) ?? 1, to = numOrNull(A.time_offset) ?? 0, vs = numOrNull(A.value_scale) ?? 1, vo = numOrNull(A.value_offset) ?? 0;
    let dst = null;
    if (src && src.keys.length && !src.more) {   /* the documented paste: reverse / invert, then time scale + offset, value scale + offset */
      const T0 = src.keys[0][0], T1 = src.keys[src.keys.length - 1][0], V0 = Math.min(...src.keys.map(k => k[1])), V1 = Math.max(...src.keys.map(k => k[1]));
      dst = src.keys.map(([t, v]) => [A.reverse ? T0 + T1 - t : t, A.invert ? V0 + V1 - v : v]).sort((p, q) => p[0] - q[0]).map(([t, v]) => [t * ts + to, v * vs + vo]);
      if (numOrNull(R.keys) != null && R.keys !== dst.length) dst = null;
      if (dst) { (state._covbCurves || (state._covbCurves = {}))[`${A.to_clip}|${A.to_property}`] = { keys: dst, more: 0 }; }
    }
    const fromN = cname(A.from_clip), toN = cname(A.to_clip);
    const lanes = [];
    if (src && src.keys.length) lanes.push({ label: propLabel(A.from_property), cur: src.keys, em: false });
    if (dst) lanes.push({ label: propLabel(A.to_property), cur: dst, em: true });
    if (lanes.length) graph(lanes);
    return insp(head(toN, 'paste curve'), [['From', head(fromN, propLabel(A.from_property))], ['To', head(toN, propLabel(A.to_property))], to ? ['Time offset', `${to > 0 ? '+' : ''}${s(to)}`] : null, ts !== 1 ? ['Time scale', `×${f(ts)}`] : null, vs !== 1 || vo ? ['Value', [vs !== 1 ? `×${f(vs)}` : '', vo ? `${vo > 0 ? '+' : ''}${f(vo)}` : ''].filter(Boolean).join(' ')] : null, A.reverse || A.invert ? ['Flip', [A.reverse ? 'reversed' : '', A.invert ? 'inverted' : ''].filter(Boolean).join(', ')] : null, numOrNull(R.keys) != null ? ['Keys', R.keys, hadN] : null]);
  }
  if (tn === 'set_animator_keyframe') {
    const ch = P.changed && P.changed.clip, k = ch && Object.keys(ch).find(x => ANIMKEY.test(x)), ba = k ? ch[k] : [P.from, P.to];
    const a = keyList(ba[1]), b = keyList(ba[0]), t = numOrNull(A.time), v = numOrNull(A.value);
    const pre = (state._covbAnim || {})[A.animator_id], lab = `${pre ? pre + ' · ' : 'animator · '}${String(A.prop)}`;
    graph([{ label: lab, cur: a.keys, prev: b.keys.length ? b.keys : null, em: true, marks: t != null && v != null ? [[t, v, 'new']] : [] }], { playhead: t });
    const old = b.keys.find(q => t != null && Math.abs(q[0] - t) < 1e-6);
    return insp(head(nm, 'text animator'), [['Animator', pre ?? null], ['Property', String(A.prop)], t != null ? ['Key time', s(t)] : null, v != null ? ['Value', f(v), old ? f(old[1]) : null] : null, A.ease ? ['Ease', String(A.ease).replace('_', '-')] : null, ['Keys', a.keys.length + a.more, b.keys.length + b.more]]);
  }
  if (tn === 'motion_extract') {
    const cv = keyList([]), rows = Array.isArray(R.curves) ? R.curves : [];
    const s0 = numOrNull(A.start) ?? 0, fr = rows.filter(r => Array.isArray(r) && r.length >= 4 && r.every(isNum));
    const more = rows.filter(r => typeof r === 'string').map(r => +((/\+(\d+)/.exec(r) || [])[1] || 0)).reduce((x, y) => x + y, 0);
    const ch = [['dx', 0, 'x (px)'], ['dy', 1, 'y (px)'], ['scale', 2, 'scale'], ['rot_deg', 3, 'rotation (°)']];
    const lanes = ch.map(([key, i, lab]) => ({ label: lab, cur: fr.map((r, j) => [s0 + j, r[i]]), em: i < 2 }));
    const n = numOrNull(R.frames) ?? fr.length + more, rg = R.range || {};
    graph(lanes, { frames: true, span: [s0, s0 + n - 1] });
    const rr = (k, p) => Array.isArray(rg[k]) && rg[k].length === 2 ? `${f(Math.round(rg[k][0] * 100) / 100, p)} – ${f(Math.round(rg[k][1] * 100) / 100, p)}` : null;
    return insp(head(nm, 'motion extract'), [['Frames', `${n}${fr.length < n ? ` (${fr.length} drawn)` : ''}`], ['X range', rr('dx') && rr('dx') + ' px'], ['Y range', rr('dy') && rr('dy') + ' px'], ['Scale range', rr('scale', 'scale')], ['Rotation', rr('rot_deg') && rr('rot_deg') + '°'], A.target_clip_id != null ? ['Applied to', (cname(A.target_clip_id) || 'clip') + (A.invert ? ' (inverted)' : '')] : null]);
  }
  if (tn === 'motion_diff') {
    const C2 = R.channels || {}, n = numOrNull(R.frames), s0 = numOrNull(A.start) ?? 0, wf = numOrNull(R.worst_translation_frame);
    const lab = { dx_px: ['x', ' px'], dy_px: ['y', ' px'], rot_deg: ['rotation', '°'], scale: ['scale', ''] };
    const ent = Object.entries(C2).filter(([k, v]) => lab[k] && v && isNum(v.rms_diff));
    const worst = ent.length ? ent.reduce((p, q) => (q[1].rms_diff / (q[0] === 'scale' ? .01 : 1) > p[1].rms_diff / (p[0] === 'scale' ? .01 : 1) ? q : p)) : null;
    const G = pk('curves');
    if (G && n != null) { graph([], { frames: true, span: [s0, s0 + n - 1], playhead: wf != null ? s0 + wf : null }); G.lanes = ent.map(([k, v]) => `${lab[k][0]} Δ ${k === 'scale' ? f(Math.round(v.rms_diff * 1e4) / 100) + '%' : f(Math.round(v.rms_diff * 100) / 100) + lab[k][1]}`).slice(0, 5); }
    return insp(head(cname(A.clip_a), 'vs', cname(A.clip_b)), [n != null ? ['Frames', n] : null, ...ent.slice(0, 4).map(([k, v]) => [`${lab[k][0][0].toUpperCase() + lab[k][0].slice(1)} RMS`, `${k === 'scale' ? f(Math.round(v.rms_diff * 1e4) / 100) + '%' : f(Math.round(v.rms_diff * 100) / 100) + lab[k][1]} · r ${f(Math.round((v.correlation ?? 0) * 100) / 100)}`]), wf != null ? ['Worst frame', `${wf}${numOrNull(R.worst_translation_px) != null ? ' · ' + f(Math.round(R.worst_translation_px * 100) / 100) + ' px' : ''}`] : null]);
  }
  if (tn === 'remove_motion_path') {
    const ch = (P.changed && P.changed.clip) || {}, pts = Array.isArray(ch['motion_path.points']) && Array.isArray(ch['motion_path.points'][0]) ? ch['motion_path.points'][0].filter(p => p && isNum(p.x) && isNum(p.y)).map(p => [p.x, p.y]) : [];
    state.gizmo = { t: H.now(), tool: tn, phase: ev.phase, clip, prev: { x: 0, y: 0, scale: 1, rotation: 0 }, next: { x: 0, y: 0, scale: 1, rotation: 0 }, read: [pts.length ? `motion path removed · ${pts.length} points` : 'motion path removed'], gonePath: pts };
    return insp(head(nm, 'motion path'), [['Motion path', 'none', pts.length ? `${pts.length} points` : 'on']]) || true;
  }

  // ---------------- measurement (read-only) on the viewer ----------------
  const vw = (() => { const v = pk('viewer'); return v; })();
  const W = state.session && state.session.width, Hh = state.session && state.session.height;
  if (tn === 'solve_constraints') {
    const bx = Array.isArray(R.box) && R.box.length === 4 ? R.box : (W && Hh ? [0, 0, W, Hh] : null), pos = Array.isArray(R.positions) ? R.positions : [];
    const bad = new Set((Array.isArray(R.violated) ? R.violated : []).flatMap(v => { const c = Array.isArray(A.constraints) ? A.constraints[v && typeof v === 'object' ? v.index ?? v.constraint : v] : null; return c && Array.isArray(c.items) ? c.items : []; }));
    if (vw && bx && bx[2] > 0 && bx[3] > 0) { vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'layout');
      for (const p of pos) if (p && [p.x, p.y, p.w, p.h].every(isNum)) vw.overlay.push({ rect: [(p.x - bx[0]) / bx[2], (p.y - bx[1]) / bx[3], p.w / bx[2], p.h / bx[3]], label: `${p.id} ${Math.round(p.w)}×${Math.round(p.h)}`, em: bad.has(p.id), _kind: 'layout', _call: ev.call_id }); }
    const sat = Array.isArray(R.satisfied) ? R.satisfied.length : null, vio = Array.isArray(R.violated) ? R.violated.length : null;
    return insp('layout solve', [['Items', pos.length], ['Box', bx ? `${Math.round(bx[2])}×${Math.round(bx[3])}` : null], sat != null ? ['Satisfied', `${sat} of ${sat + (vio || 0)}`] : null, vio ? ['Violated', vio] : null, ...pos.slice(0, 3).map(p => [String(p.id), `${Math.round(p.x)}, ${Math.round(p.y)}`])]);
  }
  if (tn === 'compute_text_bounds') {
    const cw = R.canvas && isNum(R.canvas.width) ? R.canvas.width : W, chh = R.canvas && isNum(R.canvas.height) ? R.canvas.height : Hh;
    const lines = Array.isArray(R.lines) ? R.lines.filter(l => l && [l.x, l.y, l.w, l.h].every(isNum)) : [];
    if (vw && cw && chh) { vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'textbounds');
      for (const l of lines) vw.overlay.push({ rect: [l.x / cw, l.y / chh, l.w / cw, l.h / chh], label: `${Math.round(l.w)}×${Math.round(l.h)}`, _kind: 'textbounds', _call: ev.call_id });
      if (Array.isArray(R.ink) && R.ink.length === 4 && R.ink.every(isNum)) vw.overlay.push({ rect: [R.ink[0] / cw, R.ink[1] / chh, (R.ink[2] - R.ink[0]) / cw, (R.ink[3] - R.ink[1]) / chh], em: true, _kind: 'textbounds', _call: ev.call_id }); }
    const fnt = R.font && R.font.file ? String(R.font.file).split(/[\\/]/).pop().replace(/\.(ttf|otf)$/i, '') : null;
    return insp(head(cname(R.clip_id ?? clip), 'text bounds'), [isNum(R.width) && isNum(R.height) ? ['Size', `${Math.round(R.width)} × ${Math.round(R.height)} px`] : null, Array.isArray(R.ink) ? ['Ink', `${R.ink.map(v => Math.round(v)).join(', ')}`] : null, numOrNull(R.line_count) != null ? ['Lines', R.line_count] : null, lines[0] && isNum(lines[0].baseline) ? ['Baseline', `${Math.round(lines[0].baseline)} px`] : null, fnt ? ['Font', fnt + (R.font.substituted ? ' (substituted)' : '')] : null, isNum(R.t) ? ['At', s(R.t)] : null]);
  }
  if (tn === 'raycast_mesh_raw') {
    const hit = R.hit && typeof R.hit === 'object' ? R.hit : null;
    const S = pk('stack');
    if (S) { S.nodes = S.nodes || []; for (const n of S.nodes) n.em = false; S.nodes.push({ name: hit ? `raycast · tri ${hit.triangle}` : 'raycast · miss', em: true, _flash: H.now() }); if (S.nodes.length > 7) S.nodes.splice(0, S.nodes.length - 7); }
    const ray = A.ray || {}, v3 = a => Array.isArray(a) && a.length === 3 ? a.map(x => f(x)).join(', ') : null;
    const tris = A.mesh && Array.isArray(A.mesh.indices) ? A.mesh.indices.length : null;
    return insp('raycast', [['Result', hit ? `hit triangle ${hit.triangle}` : 'miss'], hit && isNum(hit.t) ? ['Distance t', f(Math.round(hit.t * 1000) / 1000)] : null, hit && Array.isArray(hit.barycentric) ? ['Barycentric', hit.barycentric.map(x => f(Math.round(x * 1000) / 1000)).join(', ')] : null, ['Origin', v3(ray.origin)], ['Direction', v3(ray.direction ?? ray.dir)], tris != null ? ['Triangles', tris] : null, numOrNull(R.nodes) != null ? ['BVH nodes', R.nodes] : null]) || !!S;
  }
  if (tn === 'compile_scene3d_descriptor') {
    const sc = A.scene || {}, srcs = Array.isArray(sc.sources) ? sc.sources : [], nodes = Array.isArray(sc.nodes) ? sc.nodes : [], st2 = R.stats || {};
    const tr = pk('tree');
    if (tr && nodes.length) { tr.items = (tr.items || []).filter(x => !x._covb); for (const x of tr.items) x.sel = false;
      nodes.slice(0, 8).forEach((n, i) => { const so = srcs[n && n.source] || {}, nm2 = so.primitive && so.primitive.kind ? so.primitive.kind : (so.kind ? String(so.kind).replace(/_/g, ' ') : 'draw'); tr.items.push({ name: `${nm2} ${i + 1}`, icon: so.kind === 'primitive' && so.primitive && so.primitive.kind === 'plane' ? 'plane' : 'mesh', sel: i === nodes.length - 1, _covb: true, _flash: H.now() }); });
      if (tr.items.length > 14) tr.items.splice(0, tr.items.length - 14); }
    return insp('scene compile', [['Draws', st2.draws ?? (Array.isArray(R.draws) ? R.draws.length : null)], ['Nodes', st2.nodes], ['Sources', st2.sources != null ? `${st2.sources_resolved ?? st2.sources} of ${st2.sources} resolved` : null], ['Triangles', st2.triangles], ['Vertices', st2.vertices], ['Lights', Array.isArray(R.lights) ? R.lights.length : null]]) || !!tr;
  }
  if (tn === 'add_managed_linear_effect') {
    const kind = String(P.effect ?? A.kind ?? ''), pr = A.params && typeof A.params === 'object' ? Object.entries(A.params).filter(([, v]) => isNum(v)) : [];
    const chE = P.changed && P.changed.clip && P.changed.clip.effects, had = Array.isArray(chE) && Array.isArray(chE[0]) ? chE[0].map(e => String((e && e.kind) || 'effect').replace(/_/g, ' ')) : null;
    return insp(head(nm, kind.replace(/_/g, ' ')), [['Effect', kind.replace(/_/g, ' ')], R.state ? ['State', String(R.state).replace(/_/g, ' ')] : null, numOrNull(R.working_gamut) != null ? ['Working gamut', H.gamut(R.working_gamut)] : null, ...pr.slice(0, 3).map(([k, v]) => [k.replace(/_/g, ' ').replace(/^./, c => c.toUpperCase()), f(v, k)]), R.guard ? ['Guard', 'stale if gamut changes'] : null, had ? ['Effects', [...had, kind.replace(/_/g, ' ')].join(', '), had.length ? had.join(', ') : 'none'] : null]);
  }
  if (tn === 'preview_subject_match') {   // read-only (mutated: false): the region pairs by mask name, the engine's Lab stats source -> result
    const sr = A.source_regions || {}, rr = A.ref_regions || {}, M = state._covbMasks || {};
    const mn = id => id == null ? '—' : M[id] != null ? M[id] : 'mask';   /* a mask layer by its own name, never its id */
    const lab = (L, i) => Array.isArray(L) && L[i] && Array.isArray(L[i].lab_mean) && isNum(L[i].lab_mean[0]) ? L[i].lab_mean[0] : null, r3 = v => f(Math.round(v * 1000) / 1000);
    const rows = [];
    ['skin', 'lighting', 'background'].forEach((k, i) => { if (sr[k] == null && rr[k] == null) return; const K = k[0].toUpperCase() + k.slice(1);
      rows.push([K, `${mn(sr[k])} → ${mn(rr[k])}`]);
      const a = lab(R.source_stats, i), b = lab(R.result_stats, i), c = lab(R.reference_stats, i);
      if (a != null && b != null) rows.push([K + ' L*', r3(b) + (c != null ? ` (ref ${r3(c)})` : ''), r3(a)]); });
    return insp(head(cname(A.clip_id), 'match', cname(A.ref_clip_id)), [...rows, isNum(A.time) && isNum(A.ref_time) ? ['Frames', `${s(A.time)} ↔ ${s(A.ref_time)}`] : null, R.domain ? ['Domain', String(R.domain).replace(/_/g, ' ')] : null, ['Preview', P.mutated === false || R.mutated === false ? 'before / after only, nothing changed' : 'before / after']]);
  }
  return false;
}
function numOrNull(v) { if (v == null || typeof v === 'object' || typeof v === 'boolean') return null; const n = +v; return Number.isFinite(n) ? n : null; }

// ---------------- undo / redo: the GUI mirrors the engine's history for the steps this lane drew ----------------
// The engine's undo / redo events carry no state (changed: {}), only { undone | redone, undo_left, redo_available }.
// Every mutating call is one engine step. For this lane's tools the GUI keeps what that call's screen held just before
// the step (its panels, the touched clip on the shared timeline, the remembered key lists, the gizmo), so an undo puts
// the screen back exactly and a redo puts the step back exactly. Other tools' steps are counted (an empty entry) so
// the stack stays aligned with the engine's; a new step clears the redo side, as the engine does; load / restore
// replace the engine's history, so the mirror empties. Failed and read-only calls are not steps and change nothing.
const HMAX = 64, HRESET = /^(load_project|open_project|new_project|restore)$/;
const clone = o => Array.isArray(o) ? o.map(clone) : o && typeof o === 'object' && Object.getPrototypeOf(o) === Object.prototype ? Object.fromEntries(Object.entries(o).map(([k, v]) => [k, clone(v)])) : o;   /* plain data copied; strings (a frame's data URL) shared, not duplicated */
function hsnap(state, mode, clipId, H) {
  const V = state.views[mode] || {}, tl = state.liveTL, P = {};
  for (const [k, d] of Object.entries(V)) if (d !== tl) P[k] = clone(d);
  const c = clipId != null ? H.clipObj(state, clipId) : null;
  return { mode, clipId, P, clip: c ? clone(c) : null, curves: clone(state._covbCurves), anim: clone(state._covbAnim), gizmo: state.gizmo };
}
function hput(state, e, H) {   // e back on screen; returns what it replaced (the other side of the stack)
  const cur = hsnap(state, e.mode, e.clipId, H), V = state.views[e.mode] || (state.views[e.mode] = {}), tl = state.liveTL;
  for (const k of Object.keys(V)) if (V[k] !== tl && !(k in e.P)) delete V[k];
  for (const [k, d] of Object.entries(e.P)) if (V[k] !== tl) V[k] = d;
  const c = e.clip && e.clipId != null ? H.clipObj(state, e.clipId) : null;
  if (c) { for (const k of Object.keys(c)) delete c[k]; Object.assign(c, e.clip); }   /* in place: the timeline object is shared by every screen */
  state._covbCurves = e.curves; state._covbAnim = e.anim; state.gizmo = e.gizmo;
  return cur;
}
export function covbHistory(state, MODES, ev, H) {
  if (ev.phase !== 'complete') return;
  const tn = String(ev.tool_name), P = ev.payload || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const Hs = state._covbHist || (state._covbHist = { undo: [], redo: [] });
  if (HRESET.test(tn)) { Hs.undo = []; Hs.redo = []; return; }
  if (tn === 'undo' || tn === 'redo') {
    const from = tn === 'undo' ? Hs.undo : Hs.redo, to = tn === 'undo' ? Hs.redo : Hs.undo;
    const n = Math.max(0, Math.round(Number(R[tn === 'undo' ? 'undone' : 'redone']) || 0));
    for (let i = 0; i < n && from.length; i++) { const e = from.pop(); to.push(e ? hput(state, e, H) : null); }
    return;
  }
  if (P.mutated !== true) return;
  const T = ev.target || {}, A = P.args || (state._covbArgs || {})[ev.call_id] || {};
  Hs.undo.push(COVB.test(tn) ? hsnap(state, H.modeViews(state, MODES, ev)[0], T.clip_id ?? A.clip_id ?? A.to_clip ?? null, H) : null);
  if (Hs.undo.length > HMAX) Hs.undo.shift();
  Hs.redo = [];
}
