// COV-A: dedicated, reactive visuals for thirteen timeline tools that used to reach only the generic fallback
// (rail entry + Inspector diff + HUD pulse). Each one lands on an element the screen's mockup already has:
//   add_marker / remove_marker   an ember tick (a range marker: an ember band) on the timeline ruler; removal fades it
//   detect_cuts                  the cuts it found as cream ticks on the ruler, a dashed line through the tracks as they land
//   detect_beats                 beat ticks on the clips of the analysed file (m04 beat ticks), tempo on the Audio rows
//   bind_beats                   ember key diamonds on the clip at every hit; the pulse curve + beat diamonds on m03's Graph
//   remove_silence_cuts          the ranges ripple out of the track's clips, a razor line at each joint
//   set_time_mode                "reverse" / "freeze 1.25 s" badge on the clip (the speed / freeze badge row)
//   sync_clips                   the clips slide to their synced starts; Inspector shows each from -> to
//   add_multicam / switch_angle  the group's clip on its track reads "<file> · angle N" per switched span, razor at the switch,
//                                viewer tag "angle 2 · cam_b.mp4"
//   burn_in                      the caption group's clips carry "burn-in off" while off; viewer tag says which
//   transition_render_raw        viewer tag "crossfade · 40%", Inspector rows (operator, progress, edge, frame size)
//   match_events                 Inspector rows: pairs, within tolerance, mean / max offset, dropouts (its inputs are plain
//                                timestamp vectors, not timeline objects, so it draws nothing on the timeline)
// Pure data like the rest of the router: the timeline / Graph / viewer / Inspector renderers draw it. Names, never raw ids.

export const COV_A = /^(add_marker|remove_marker|add_multicam|switch_angle|sync_clips|detect_cuts|detect_beats|bind_beats|remove_silence_cuts|set_time_mode|match_events|transition_render_raw|burn_in)$/;
const CAP = 64;   /* ticks / keys drawn per call, whatever the engine returns */

const N = v => { if (v == null || typeof v === 'object' || v === '') return null; const n = +v; return Number.isFinite(n) ? n : null; };
const r2 = n => String(Math.round(n * 100) / 100);
const sec = n => `${r2(n)} s`;
const signed = n => (n > 0 ? '+' : n < 0 ? '−' : '') + r2(Math.abs(n)) + ' s';
const base = p => String(p ?? '').replace(/^(file|clip):/, '').split(/[\\/]/).pop();

function panelOf(state, MODES, H, ev, kind) {
  const [m, V] = H.modeViews(state, MODES, ev);
  const e = Object.entries((MODES[m] || MODES[state.mode] || { panels: {} }).panels).find(([, s]) => s.kind === kind);
  return e ? (V[e[0]] || (V[e[0]] = {})) : null;
}
function rows(kv, title, list, sub, H) {
  if (!kv) return;
  kv.blocks = kv.blocks || []; const j = kv.blocks.findIndex(b => b._live); if (j >= 0) kv.blocks.splice(j);
  if (sub) kv.sub = sub;
  kv.blocks.push({ sec: title, _live: true });
  for (const r of list.filter(Boolean).slice(0, 6)) kv.blocks.push({ k: r[0], v: r[1], ...(r[2] != null ? { from: r[2] } : {}), _live: true, _flash: H.now() });
}
function tag(state, MODES, H, ev, text) {
  const vw = panelOf(state, MODES, H, ev, 'viewer'); if (!vw) return;
  vw.tags = [...(vw.tags || []).filter(t => !t._cova), { text, _cova: true }]; vw._flash = H.now();
}
function clipName(H, tl, id) {
  const [t, i] = H.findClip(tl, { clip_id: id }); const lb = t ? t.clips[i].label : null;
  return lb && !H.RAW_ID.test(String(lb)) ? String(lb) : null;
}
function clipsOf(tl, file) {   /* clips cut from one media file: "cam_a.mp4", "cam_a.mp4 · split 1" */
  const out = []; if (!file) return out;
  for (const t of tl.tracks || []) for (const c of t.clips || []) { const l = String(c.label ?? ''); if (l === file || l.startsWith(file + ' · ')) out.push(c); }
  return out;
}
const capList = (a, f = x => x) => (Array.isArray(a) ? a : []).map(f).filter(x => x != null).slice(0, CAP);

// remember a caption group's clips (add_captions result) so burn_in can find them later
export function covASee(state, ev) {
  const R = (ev.payload || {}).result;
  if (ev.phase === 'complete' && R && typeof R.caption_id === 'string' && Array.isArray(R.clip_ids)) (state._capGroups || (state._capGroups = {}))[R.caption_id] = R.clip_ids.slice(0, 256);
}

// The rail's copy of a COV-A call's args, with every id it carries read as a name (the marker's name and time, the
// clips' names, the multicam group by its angles, a caption group by its cue count). The raw args are kept for covAOps.
export function covAPre(state, ev, H) {
  const P = ev.payload || {};
  if (ev.phase === 'complete') {   /* the landed value the rail shows: bind_beats' from/to are whole key lists, set_time_mode's param is an engine path */
    if (Array.isArray(P.to) || Array.isArray(P.from)) { const Q = { ...P }; delete Q.from; delete Q.to; delete Q.param; return { ...ev, payload: Q, target: { ...(ev.target || {}), param: null } }; }
    if (P.param === 'time_mode.mode') return { ...ev, payload: { ...P, param: 'mode' } };
    return ev;
  }
  if (ev.phase !== 'start') return ev;
  const A = P.args; if (!A || typeof A !== 'object') return ev;
  (state._covArgs || (state._covArgs = {}))[ev.call_id] = A;
  const tl = state.liveTL, nm = id => (tl && clipName(H, tl, id)) || null, o = { ...A };
  if ('marker_id' in o) { const m = tl && (tl.markers || []).find(x => x.id === o.marker_id); delete o.marker_id; if (m) o.marker = `${m.name || 'unnamed'} @ ${sec(m.t)}`; }
  if ('multicam_id' in o) { const g = tl && tl._mc && tl._mc[o.multicam_id]; delete o.multicam_id; if (g) o.multicam = g.names.join(' + '); }
  if ('caption_id' in o) { const ids = state._capGroups && state._capGroups[o.caption_id]; delete o.caption_id; if (ids) o.captions = `${ids.length} cues`; }
  for (const k of ['angles', 'clip_ids']) if (Array.isArray(o[k])) o[k] = o[k].map(id => nm(id) || null).filter(Boolean).join(', ') || undefined;
  if ('clip_id' in o && !nm(o.clip_id)) delete o.clip_id;
  if (typeof o.source === 'string' && /^(clip|file):/.test(o.source)) o.source = o.source.startsWith('clip:') ? (nm(o.source.slice(5)) || 'clip') : base(o.source);
  if (typeof o.path === 'string') o.path = base(o.path);
  for (const k of ['a', 'b']) if (Array.isArray(o[k])) o[k] = `${o[k].length} events`;
  for (const k of ['from', 'to']) if (o[k] && typeof o[k] === 'object' && N(o[k].w) && N(o[k].h)) o[k] = `${o[k].w} x ${o[k].h}`;
  if (o.operator && typeof o.operator === 'object') o.operator = String(o.operator.kind ?? o.operator.type ?? 'op');
  if (Array.isArray(o.ranges)) o.ranges = `${o.ranges.length} ranges`;
  if (Array.isArray(o.beats)) o.beats = `${o.beats.length} beats`;
  for (const k of Object.keys(o)) if (o[k] === undefined) delete o[k];
  return { ...ev, payload: { ...P, args: o } };
}

export function covAOps(state, MODES, ev, H) {
  const n = String(ev.tool_name), P = ev.payload || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const A = (state._covArgs && state._covArgs[ev.call_id]) || P.args || (state._editArgs && state._editArgs[ev.call_id]) || {};
  if (state._covArgs) delete state._covArgs[ev.call_id];
  const now = H.now(), tl = H.timelineView(state, MODES), kv = panelOf(state, MODES, H, ev, 'kv');
  const name = id => clipName(H, tl, id);

  if (n === 'add_marker') {
    const t = N(A.time); if (t == null) return false;
    const d = Math.max(0, N(A.duration) ?? 0), nm = typeof A.name === 'string' && A.name.trim() ? A.name.trim().slice(0, 24) : null;
    tl.markers = (tl.markers || []).filter(m => !m.gone && (R.marker_id == null || m.id !== R.marker_id));
    tl.markers.push({ id: R.marker_id, t, d, name: nm, color: A.color, _flash: now });
    if (tl.markers.length > CAP) tl.markers.splice(0, tl.markers.length - CAP);
    rows(kv, 'add_marker', [['Marker', nm || 'unnamed'], ['At', sec(t)], d ? ['Length', sec(d)] : null, typeof A.color === 'string' ? ['Color', A.color] : null], null, H);
    return true;
  }
  if (n === 'remove_marker') {
    const m = (tl.markers || []).find(x => x.id != null && x.id === A.marker_id && !x.gone);
    tl.markers = (tl.markers || []).filter(x => !x.gone || H.flashing(x.gone));
    if (m) m.gone = now;
    rows(kv, 'remove_marker', [['Marker', 'removed', m ? `${m.name || 'unnamed'} · ${sec(m.t)}` : null]], null, H);
    return true;
  }
  if (n === 'detect_cuts') {
    const src = String(A.source ?? 'timeline'), cuts = capList(R.cuts, c => (c && N(c.time) != null ? c : null));
    let off = 0, where = 'timeline';
    if (src.startsWith('file:')) { const cs = clipsOf(tl, base(src)); where = base(src); off = cs.length ? cs[0].s : 0; }
    else if (src.startsWith('clip:')) where = name(src.slice(5)) || 'clip';
    tl.cuts = { ts: cuts.map(c => ({ t: off + N(c.time), kind: String(c.kind ?? 'cut').replace(/_/g, ' ') })), t: now };
    const cnt = N(R.count) ?? cuts.length;
    rows(kv, 'detect_cuts', [['Cuts', cnt ? `${cnt} found` : 'none found'], ['Source', where],
      ...tl.cuts.ts.slice(0, 4).map(c => [c.kind.replace(/^./, x => x.toUpperCase()), sec(c.t)])], null, H);
    return true;
  }
  if (n === 'detect_beats') {
    const beats = capList(R.beats, N), file = base(A.path ?? P.path), cs = clipsOf(tl, file);
    for (const c of cs) { c.beats = beats.map(b => c.s + b).filter(t => t >= c.s && t <= c.e); c._flash = now; }
    if (cs.length) tl._beatT = now;
    const bpm = N(R.bpm);
    rows(kv, 'detect_beats', [['Beats', String(Array.isArray(R.beats) ? R.beats.length : beats.length)], bpm != null ? ['Tempo', `${Math.round(bpm * 10) / 10} bpm`] : null,
      file ? ['Source', file] : null, N(A.min_gap) != null ? ['Min gap', sec(N(A.min_gap))] : null], null, H);
    return true;
  }
  if (n === 'bind_beats') {
    const [t, i] = H.findClip(tl, { clip_id: A.clip_id }); const c = t ? t.clips[i] : null;
    const prop = String(A.property ?? 'value'), every = Math.max(1, Math.round(N(A.every) ?? 1)), curve = String(A.curve ?? 'pulse');
    let hits = [];
    if (Array.isArray(A.beats)) hits = capList(A.beats, N).filter((_, k) => k % every === 0);
    else if (N(A.bpm) > 0 && c) { const step = 60 / N(A.bpm) * every; for (let x = c.s + (N(A.offset) ?? 0); x < c.e - 1e-6 && hits.length < CAP; x += step) hits.push(x); }
    if (c) { hits = hits.filter(x => x >= c.s - 1e-6 && x < c.e - 1e-6); c.keys = [...(c.keys || []).filter(k => !k._beat), ...hits.map(x => ({ t: x, em: true, _beat: true }))]; c._flash = now; }
    const amt = N(A.amount) ?? .1, decay = N(A.decay) ?? (hits.length > 1 ? (hits[1] - hits[0]) / 4 : .25);
    // m03 Graph: the property's pulse over the clip, beat diamonds on top, the lane named for clip + property
    const g = panelOf(state, MODES, H, ev, 'curves');
    if (g && c) {
      const span = c.e - c.s, step = span <= 12 ? 2 : span <= 30 ? 5 : 10, lo = .3, hi = .8, mid = .55;
      const pts = [[c.s, curve === 'swing' ? mid : lo]];
      hits.forEach((x, k) => { const sgn = curve === 'swing' && k % 2 ? -1 : 1, pk = curve === 'swing' ? mid + sgn * .3 : hi, rest = curve === 'swing' ? mid : lo;
        const att = curve === 'kick' ? Math.min(.04, decay / 4) : 0;
        pts.push([x - (att || .002), rest], [x, pk]);   /* flat until the hit, then the jump */
        if (curve === 'flash') pts.push([x + decay * .9, pk]);
        pts.push([x + decay * .35, rest + (pk - rest) * .3], [x + decay, rest]); });
      pts.push([c.e, curve === 'swing' ? mid : lo]);
      g.ruler = { t0: c.s, t1: c.e, step, first: Math.ceil(c.s / step) * step };
      g.beats = { list: hits };
      g.curves = [{ pts: pts.filter(p => p[0] >= c.s && p[0] <= c.e), em: true, keys: hits.map((x, k) => [x, curve === 'swing' && k % 2 ? mid - .3 : curve === 'swing' ? mid + .3 : hi]) }];
      g.lanes = [`${name(A.clip_id) || 'clip'} · ${prop}`]; g._flash = now;
    }
    const amtTxt = (amt >= 0 ? '+' : '') + H.fmtVal(amt, prop);
    rows(kv, 'bind_beats', [c && name(A.clip_id) ? ['Clip', name(A.clip_id)] : null, ['Property', prop], ['Hit', `${amtTxt} · ${curve}`],
      ['Beats', String(N(R.beats) ?? hits.length) + (every > 1 ? ` · every ${every}` : '')], N(A.bpm) ? ['Tempo', `${r2(N(A.bpm))} bpm`] : null], null, H);
    return true;
  }
  if (n === 'remove_silence_cuts') {
    const t = H.findTrack(tl, A.track_id);
    const ranges = capList(A.ranges, r => Array.isArray(r) && N(r[0]) != null && N(r[1]) != null && N(r[1]) > N(r[0]) ? [N(r[0]), N(r[1])] : null).sort((a, b) => b[0] - a[0]);
    let total = 0;
    if (t) {
      for (const [a, b] of ranges) {
        const len = b - a, out = [];
        for (const k of t.clips || []) {
          if (k.e <= a) { out.push(k); continue; }
          if (k.s >= b) { k.s -= len; k.e -= len; k._flash = now; out.push(k); continue; }
          const e0 = k.e, head = k.s < a, tail = e0 > b;   /* a clip wholly inside the range goes */
          if (head && tail) { const r = { ...k, id: (k.id ?? k.label) + '_r', s: a, e: e0 - len, _flash: now }; delete r.thumb; k.e = a; k._flash = now; out.push(k, r); }
          else if (head) { k.e = a; k._flash = now; out.push(k); }
          else if (tail) { k.s = a; k.e = e0 - len; k._flash = now; out.push(k); }
        }
        t.clips = out.sort((p, q) => p.s - q.s); total += len;
      }
      /* each joint where it sits once every range is out: earlier ranges pull it left by their length */
      tl.joints = { at: ranges.map(([a], k) => a - ranges.slice(k + 1).reduce((s, r) => s + r[1] - r[0], 0)), t: now };
      t._flash = now;
    }
    rows(kv, 'remove_silence_cuts', [t ? ['Track', String(t.label)] : null, ['Removed', `${N(R.removed) ?? ranges.length} ${(N(R.removed) ?? ranges.length) === 1 ? 'range' : 'ranges'} · ${sec(total)}`],
      ...ranges.slice().reverse().slice(0, 3).map(([a, b]) => ['Range', `${r2(a)}–${r2(b)} s`])], null, H);
    return true;
  }
  if (n === 'set_time_mode') {
    const [t, i] = H.findClip(tl, { clip_id: A.clip_id }); const c = t ? t.clips[i] : null;
    const mode = String(A.mode ?? 'normal'), at = N(A.at), prev = c ? (c._tm || 'normal') : null;
    const label = mode === 'freeze' ? `freeze ${at != null ? sec(at) : ''}`.trim() : mode;
    if (c) { c.fx = (c.fx || []).filter(x => !/^(reverse|freeze)\b/.test(x)); if (mode !== 'normal') c.fx.push(label); if (!c.fx.length) delete c.fx; c._tm = label; c._flash = now; }
    rows(kv, 'set_time_mode', [c && name(A.clip_id) ? ['Clip', name(A.clip_id)] : null, ['Time', label, prev && prev !== label ? prev : null]], null, H);
    return true;
  }
  if (n === 'sync_clips') {
    const ids = Array.isArray(R.clip_ids) ? R.clip_ids : Array.isArray(A.clip_ids) ? A.clip_ids : [], offs = Array.isArray(R.offsets_seconds) ? R.offsets_seconds : [];
    const found = ids.map(id => { const [t, i] = H.findClip(tl, { clip_id: id }); return t ? t.clips[i] : null; });
    const have = found.filter(Boolean); const out = [];
    if (have.length && offs.length === ids.length) {
      const anchor = Math.min(...have.map(c => c.s)) + (N(R.origin_shift) ?? 0);
      found.forEach((c, k) => { if (!c || N(offs[k]) == null) return; const to = anchor + N(offs[k]), from = c.s, d = to - from;
        if (Math.abs(d) > 1e-9) { c.s += d; c.e += d; } c._flash = now; out.push([c.label && !H.RAW_ID.test(String(c.label)) ? String(c.label) : 'clip', sec(to), Math.abs(d) > 1e-9 ? sec(from) : null]); });
      for (const t of tl.tracks || []) if ((t.clips || []).some(c => found.includes(c))) t.clips.sort((a, b) => a.s - b.s);
    }
    rows(kv, 'sync_clips', [['Synced', `${ids.length} clips`], ['By', String(R.method || A.by || 'audio')], ...out.slice(0, 4)], null, H);
    return true;
  }
  if (n === 'add_multicam') {
    const ids = Array.isArray(A.angles) ? A.angles : [], names = ids.map(id => name(id) || 'clip');
    const s = N(R.start), d = N(R.duration), mid = R.multicam_id; if (s == null || d == null) return false;
    for (const t of tl.tracks || []) t.clips = (t.clips || []).filter(c => !ids.includes(c.id));   /* the engine moves the sources into the group */
    const t = H.ensureTrack(tl, R.track_id ?? 'multicam');
    const g = { s, e: s + d, names, audio: N(R.audio_angle) ?? N(A.audio_angle) ?? 0, cuts: [], track: t.id };
    (tl._mc || (tl._mc = {}))[mid] = g; mcClips(t, g, now, null);
    t._flash = now; H.fitRuler(tl);
    tag(state, MODES, H, ev, `multicam · ${names.length} angles`);
    rows(kv, 'add_multicam', [['Multicam', `${names.length} angles`], ['Angles', names.join(', ')], ['Audio', names[g.audio] || `angle ${g.audio + 1}`], ['Span', `${r2(s)}–${r2(s + d)} s`]], null, H);
    return true;
  }
  if (n === 'switch_angle') {
    const g = tl._mc && tl._mc[A.multicam_id], at = N(A.t), ang = Math.round(N(A.angle) ?? 0); if (!g || at == null) return false;
    const before = mcAngleAt(g, at);
    g.cuts = [...g.cuts.filter(c => Math.abs(c.t - at) > 1e-6), { t: at, angle: ang }].sort((a, b) => a.t - b.t);
    const t = H.findTrack(tl, g.track); if (t) mcClips(t, g, now, at);
    tl.razor = { at, t: now };
    const nm = g.names[ang] || `angle ${ang + 1}`;
    tag(state, MODES, H, ev, `angle ${ang + 1} · ${nm}`);
    rows(kv, 'switch_angle', [['Angle', `${ang + 1} · ${nm}`, before !== ang ? `${before + 1} · ${g.names[before] || ''}`.trim() : null], ['At', sec(at)], ['Audio', `${g.names[g.audio] || 'angle ' + (g.audio + 1)} (fixed)`]], null, H);
    return true;
  }
  if (n === 'burn_in') {
    const on = A.enabled ?? R.burn_in, ids = (state._capGroups && state._capGroups[A.caption_id]) || [];
    let k = 0;
    for (const id of ids) { const [t, i] = H.findClip(tl, { clip_id: id }); if (!t) continue; const c = t.clips[i]; k++;
      c.fx = (c.fx || []).filter(x => x !== 'burn-in off'); if (!on) c.fx.push('burn-in off'); if (!c.fx.length) delete c.fx; c._flash = now; }
    tag(state, MODES, H, ev, on ? 'captions burned in' : 'captions off in preview + export');
    rows(kv, 'burn_in', [['Burn-in', on ? 'on' : 'off', on ? 'off' : 'on'], ids.length ? ['Cues', String(ids.length)] : null], null, H);   /* the engine's cue count for the group, not only the cues drawn */
    return true;
  }
  if (n === 'transition_render_raw') {
    const op = A.operator && typeof A.operator === 'object' ? String(A.operator.kind ?? A.operator.type ?? 'transition') : 'transition';
    const pr = N(A.progress), pct = pr != null ? Math.round(pr * 100) : null, opn = op.replace(/_/g, ' ');
    tag(state, MODES, H, ev, `${opn}${pct != null ? ` · ${pct}%` : ''}`);
    rows(kv, 'transition_render_raw', [['Transition', opn], pct != null ? ['Progress', `${pct}%`] : null, A.edge ? ['Edge', String(A.edge)] : null,
      N(R.w) && N(R.h) ? ['Frame', `${R.w} x ${R.h}`] : null], null, H);
    return true;
  }
  if (n === 'match_events') {
    const st = R.stats && typeof R.stats === 'object' ? R.stats : {}, pairs = Array.isArray(R.pairs) ? R.pairs : [];
    const within = pairs.filter(p => p && p.within_tol).length, na = N(st.count_a) ?? (Array.isArray(A.a) ? A.a.length : null), nb = N(st.count_b) ?? (Array.isArray(A.b) ? A.b.length : null);
    const tol = N((R.params_used || {}).tolerance_s) ?? N(A.tolerance_s);
    rows(kv, 'match_events', [['Matched', `${N(st.matched) ?? pairs.length} of ${na ?? '?'} → ${nb ?? '?'}`], ['Within', `${within}${tol != null ? ` · ±${r2(tol)} s` : ''}`],
      N(st.mean_offset_s) != null ? ['Mean offset', signed(N(st.mean_offset_s))] : null, N(st.max_abs_offset_s) != null ? ['Max offset', sec(N(st.max_abs_offset_s))] : null,
      ['Mode', String((R.params_used || {}).mode ?? A.mode ?? 'nearest').replace(/_/g, ' ')]], null, H);
    return true;
  }
  return false;
}

// Undo / redo. The engine's undo / redo results say only how many steps moved and how many are left (undo_left), so the
// window keeps its own stack in step with the engine's: one entry per mutating call (P.mutated), a snapshot pair for the
// COV-A calls (the timeline's tracks / clips, markers, multicam groups before and after) and a blank for the rest.
// undo restores the "before" of each COV-A entry it steps over, redo the "after"; the stack length is re-aligned to the
// engine's undo_left, so a window that joined mid-session never restores a state it did not see.
// bind_beats also keeps m03's Graph panel (pulse curve, beat diamonds, lane) before / after, so undo takes the pulse off the Graph too.
const HMAX = 48;
function snap(tl) {
  return { tracks: (tl.tracks || []).map(t => ({ ...t, clips: (t.clips || []).map(c => ({ ...c, ...(c.fx ? { fx: [...c.fx] } : {}), ...(c.keys ? { keys: c.keys.map(k => ({ ...k })) } : {}) })) })),
           markers: (tl.markers || []).filter(m => !m.gone).map(m => ({ ...m })), mc: JSON.parse(JSON.stringify(tl._mc || {})) };
}
function restore(tl, s, now) {
  const before = new Map((tl.tracks || []).map(t => [t.id, JSON.stringify((t.clips || []).map(c => [c.id, c.s, c.e, c.label, c.fx, (c.keys || []).length]))]));
  tl.tracks = s.tracks.map(t => ({ ...t, clips: t.clips.map(c => ({ ...c, ...(c.fx ? { fx: [...c.fx] } : {}), ...(c.keys ? { keys: c.keys.map(k => ({ ...k })) } : {}) })) }));
  for (const t of tl.tracks) { const k = JSON.stringify(t.clips.map(c => [c.id, c.s, c.e, c.label, c.fx, (c.keys || []).length]));
    if (before.get(t.id) !== k) { t._flash = now; for (const c of t.clips) c._flash = now; } }
  const gone = (tl.markers || []).filter(m => !m.gone && !s.markers.some(x => x.id === m.id)).map(m => ({ ...m, gone: now }));
  tl.markers = [...s.markers.map(m => ({ ...m, ...((tl.markers || []).some(x => x.id === m.id && !x.gone) ? {} : { _flash: now }) })), ...gone];
  tl._mc = JSON.parse(JSON.stringify(s.mc));
}
export function covAHistory(state, MODES, ev, H, apply) {
  const P = ev.payload || {}, R = P.result && typeof P.result === 'object' ? P.result : {}, n = String(ev.tool_name);
  const h = state._covHist || (state._covHist = { undo: [], redo: [] });
  if (n === 'undo' || n === 'redo') {
    const tl = H.timelineView(state, MODES), from = n === 'undo' ? h.undo : h.redo, to = n === 'undo' ? h.redo : h.undo;
    let k = Math.max(0, Math.round(N(R[n === 'undo' ? 'undone' : 'redone']) ?? 1));
    let last = null;
    while (k-- > 0 && from.length) { const e = from.pop(); to.push(e); if (!e) continue; last = e; if (e.c) { H.cRestore(state, MODES, n === 'undo' ? e.before : e.after); continue; }   /* a lane-C step (project_ops.js) */
      restore(tl, n === 'undo' ? e.before : e.after, H.now());
      if (e.gp) { const g = JSON.parse(n === 'undo' ? e.gb : e.ga); for (const key of Object.keys(e.gp)) delete e.gp[key]; Object.assign(e.gp, g); } }
    const left = N(R.undo_left); if (left != null && h.undo.length > left) h.undo.splice(0, h.undo.length - left);   /* re-align with the engine */
    if (last) H.fitRuler(tl);
    return !!last;
  }
  if (P.mutated !== true) return apply();
  const cov = COV_A.test(n), tl = cov ? H.timelineView(state, MODES) : null, before = cov ? snap(tl) : null;
  const gp = n === 'bind_beats' ? panelOf(state, MODES, H, ev, 'curves') : null, gb = gp ? JSON.stringify(gp) : null;
  const hit = apply();
  h.undo.push(cov ? { tool: n, before, after: snap(tl), ...(gp ? { gp, gb, ga: JSON.stringify(gp) } : {}) } : null); h.redo.length = 0;
  if (h.undo.length > HMAX) h.undo.splice(0, h.undo.length - HMAX);
  return hit;
}

function mcAngleAt(g, t) { let a = 0; for (const c of g.cuts) if (c.t <= t + 1e-9) a = c.angle; return a; }
function mcClips(t, g, now, flashAt) {   /* the group's video as one clip per switched span */
  const pts = [g.s, ...g.cuts.map(c => c.t).filter(x => x > g.s && x < g.e), g.e], out = [];
  for (let k = 0; k < pts.length - 1; k++) {
    const a = mcAngleAt(g, pts[k]), prev = out[out.length - 1];
    if (prev && prev._ang === a) { prev.e = pts[k + 1]; continue; }
    out.push({ id: `mc:${g.track}:${k}`, s: pts[k], e: pts[k + 1], _ang: a, label: `${g.names[a] || 'clip'} · angle ${a + 1}`, ...(flashAt == null || Math.abs(pts[k] - flashAt) < 1e-6 ? { _flash: now } : {}) });
  }
  t.clips = [...(t.clips || []).filter(c => !String(c.id ?? '').startsWith(`mc:${g.track}:`) && !(c.s >= g.s - 1e-6 && c.e <= g.e + 1e-6)), ...out].sort((a, b) => a.s - b.s);
}

// The timeline's own markup for markers, detected cuts and silence joints (panels.js timeline() post layer).
export function covAPost(data, X, o, inView, flashing) {
  let s = '';
  const ry = o.rulerY ?? 6, ty = o.trackY ?? 30;
  for (const m of data.markers || []) {
    if (m.gone && !flashing(m.gone)) continue;
    const x = X(m.t); if (!inView(x)) continue;
    const w = m.d ? Math.max(2, X(m.t + m.d) - x) : 0;
    s += `<i class="tlmk${m.d ? ' rng' : ''}${m.gone ? ' gone' : flashing(m._flash) ? ' flash' : ''}" style="left:${Math.round(x * 10) / 10}px;top:${ry + 9}px;height:${Math.max(6, ty - ry - 11)}px${w ? `;width:${Math.round(w * 10) / 10}px` : ''}"></i>`;
  }
  if (data.cuts) { const fl = flashing(data.cuts.t);
    for (const c of data.cuts.ts || []) { const x = X(c.t); if (!inView(x)) continue;
      s += `<i class="tlcut${fl ? ' flash' : ''}" style="left:${Math.round(x * 10) / 10}px;top:${ry + 12}px;bottom:4px"></i>`; } }
  if (data.joints && flashing(data.joints.t)) for (const a of data.joints.at || []) { const x = X(a); if (inView(x)) s += `<div class="razor" style="left:${Math.round(x * 10) / 10}px;top:${ty}px;bottom:4px"></div>`; }
  return s;
}
