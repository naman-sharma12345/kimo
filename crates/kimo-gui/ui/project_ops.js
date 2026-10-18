// COV-C: dedicated visuals for the colour-pipeline, project / history / template / cache and frame-QA tools.
// Every reaction lands in an element the mockups already have:
//   pipeline (m05 Color pipeline list): input / working / view / output rows, a clip's input override row,
//     the HDR rows (MaxCLL / MaxFALL / Peak), each changed row from -> to;
//   project (m02): the History list (checkpoints, restore, undo / redo, save, load), the Diff list
//     (project_diff, template_define / template_delete), the Batch table (batch, recipe_apply,
//     template_apply), the Media list + its footer (load_project dependencies, save_project);
//     set_meta renames the clip / track on the timeline and writes Name / Role / Tags in the Inspector (m16);
//   qa: diff_frames, compare_ranges, lint_frame, flash_series, audio_spectrum and the frame cache answer in the HUD status toast and keep the result on their
//     (flash_series also lists its luminance transition stamps, first-crossing times, on the info panel's Stamps row, and
//      with flash_limit the busiest 1 s window: flashes @ start, windows over the limit, flashing area, luminance and red)
//     own rail row (Naman 7:24), no mode hop, nothing on the canvas.
// Never a raw id: clips read as their timeline names, checkpoints and templates by the names the call gave.
import { _now as now, RAW_ID, findClip, findTrack, timelineView, numOf, fitRuler } from './router.js';

export const COV_C = {
  pipeline: /^(set_color_pipeline|set_clip_input_color|set_hdr_mastering|set_ocio_view_delivery)$/,
  project: /^(set_meta|load_project|save_project|project_diff|checkpoint|checkpoint_delete|restore|undo|redo|batch|recipe_apply|template_apply|template_define|template_delete)$/,
  qa: /^(diff_frames|compare_ranges|lint_frame|flash_series|audio_spectrum|cache_clear|cache_set_budget)$/,
};

// Kimo colour IDs (kimo-core color: gamut 0..7, transfer 0..8)
const GAMUT = ['Rec.709', 'Rec.2020', 'P3-D65', 'AP1', 'AP0', 'AWG3', 'SGamut3', 'SGamut3.Cine'];   /* short: the list is 258 px wide */
const TRANSFER = ['linear', 'sRGB', 'gamma 2.4', 'ACEScct', 'LogC3', 'S-Log3', 'PQ', 'HLG', 'BT.709'];
export function space(g, t) {
  g = numOf(g); t = numOf(t);
  if (g == null && t == null) return null;
  const G = GAMUT[g] ?? null, T = TRANSFER[t] ?? null;
  if (g === 3 && t === 0) return 'ACEScg';
  if (g === 3 && t === 3) return 'ACEScct';
  if (g === 0 && (t === 1 || t === 8)) return 'Rec.709 ' + T;
  if (g === 1 && (t === 6 || t === 7)) return 'Rec.2100 ' + T;
  if (g === 5 && t === 4) return 'ARRI LogC3';
  if (g === 6 && t === 5) return 'S-Log3';
  if (g === 7 && t === 5) return 'S-Log3 Cine';
  return [G, T].filter(Boolean).join(' ') || null;
}
const working = g => { g = numOf(g); return g == null ? null : g === 3 ? 'ACEScg' : (GAMUT[g] ?? 'gamut ' + g) + ' linear'; };
const nits = v => { const n = numOf(v); return n == null ? null : (Math.round(n * 1000) / 1000) + ' nits'; };
const base = p => String(p ?? '').split(/[\\/]/).pop();
const bytes = n => { n = numOf(n); if (n == null) return null; const u = ['B', 'KB', 'MB', 'GB']; let i = 0; while (n >= 1024 && i < 3) { n /= 1024; i++; } return (i ? Math.round(n * 10) / 10 : Math.round(n)) + ' ' + u[i]; };
const clean = s => (s != null && s !== '' && !RAW_ID.test(String(s)) ? String(s) : null);
function panel(state, MODES, mode, kind) {
  const M = MODES[mode]; if (!M) return null;
  const e = Object.entries(M.panels).find(([, sp]) => sp.kind === kind); if (!e) return null;
  const V = state.views[mode] || (state.views[mode] = {});
  return V[e[0]] || (V[e[0]] = {});
}
const clipName = (state, MODES, id) => {
  if (id == null) return null;
  const tl = timelineView(state, MODES); const s = String(id);
  if (tl) { const [t, i] = findClip(tl, { clip_id: s.replace(/^@/, '') }); if (t) return clean(t.clips[i].label); }
  return s.startsWith('@') ? s.slice(1) : clean(s);
};

// ---------- pipeline: the m05 Color pipeline list ----------
function pipeRows(state, MODES) {
  const kv = panel(state, MODES, 'color', 'kv'); if (!kv) return;
  const S = state._pipe, ch = S._ch || {}, t = now(), row = (k, v, key) => {   /* from -> to when both fit the 258 px row, else the new value flashes alone */
    const c = ch[key], fits = c && c.from != null && c.from !== v && String(k).length + String(c.from).length + String(v ?? '').length <= 23;
    return { k, v: v ?? '—', ...(c ? { _flash: t, ...(fits ? { from: c.from } : {}) } : {}), ...(c && c.dim ? { dim: true } : {}) };
  };
  const B = [row('input', S.input, 'input')];
  for (const [nm, v] of Object.entries(S.clips || {}).slice(-2)) B.push(row(nm, v, 'clip:' + nm));
  B.push(row('working', S.working, 'working'));
  if (S.view != null) B.push(S._viewErr ? { ...row('view', S.view, 'view'), dim: true } : { ...row('view', S.view, 'view'), emb: true, bar: true });
  B.push(row('output', S.output, 'output'));
  if (S.hdr) {
    B.push({ sec: 'HDR' });
    for (const [k, key] of [['MaxCLL', 'max_cll'], ['MaxFALL', 'max_fall'], ['Peak', 'max_nits'], ['Min', 'min_nits']]) B.push({ ...row(k, S.hdr[key], 'hdr:' + key), h: 34 });
  }
  kv.blocks = B; S._ch = {};
}
function setPipe(S, key, v) { const prev = key.startsWith('clip:') ? (S.clips || {})[key.slice(5)] : key.startsWith('hdr:') ? (S.hdr || {})[key.slice(4)] : S[key]; if (v == null) return; (S._ch || (S._ch = {}))[key] = { from: prev ?? null }; if (key.startsWith('clip:')) (S.clips || (S.clips = {}))[key.slice(5)] = v; else if (key.startsWith('hdr:')) (S.hdr || (S.hdr = {}))[key.slice(4)] = v; else S[key] = v; }
export function pipelineOps(state, MODES, ev) {
  const tn = String(ev.tool_name), P = ev.payload || {};
  if (ev.phase === 'start') { (state._covArgs || (state._covArgs = {}))[ev.call_id] = P.args || {}; return false; }
  if (ev.phase !== 'complete' && ev.phase !== 'error') return false;
  const A = (state._covArgs && state._covArgs[ev.call_id]) || P.args || {}; if (state._covArgs) delete state._covArgs[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {}, err = ev.phase === 'error';
  if (err && tn !== 'set_ocio_view_delivery') return false;   /* a refused call leaves the pipeline as it was */
  const S = state._pipe || (state._pipe = { input: null, working: null, view: null, output: null });
  if (tn === 'set_color_pipeline') {
    if (err) return false;
    const st = (R.settings && typeof R.settings === 'object' ? R.settings : null) || A.settings || {};
    if (st.enabled === false) { setPipe(S, 'working', 'off (display-referred)'); }
    else {
      setPipe(S, 'input', space(st.input_gamut, st.input_transfer));
      setPipe(S, 'working', working(st.working_gamut));
      setPipe(S, 'output', space(st.output_gamut, st.output_transfer));
    }
    if (st.mastering && typeof st.mastering === 'object') for (const k of ['max_cll', 'max_fall', 'max_nits', 'min_nits']) setPipe(S, 'hdr:' + k, nits(st.mastering[k]));
  } else if (tn === 'set_clip_input_color') {
    if (err) return false;
    const T = ev.target || {}, cn = clipName(state, MODES, A.clip_id ?? T.clip_id), nm = cn ? cn.replace(/\.[a-z0-9]{2,4}$/i, '').slice(0, 12) : 'clip input';
    const v = space(R.gamut ?? A.gamut, R.transfer ?? A.transfer); if (!v) return false;
    if (!(S.clips && S.clips[nm] != null)) { (S._ch || (S._ch = {}))['clip:' + nm] = { from: S.input ?? 'project input' }; (S.clips || (S.clips = {}))[nm] = v; } else setPipe(S, 'clip:' + nm, v);
  } else if (tn === 'set_hdr_mastering') {
    if (err) return false;
    const m = (R.mastering && typeof R.mastering === 'object' ? R.mastering : null) || A.metadata || {};
    if (A.clear || R.mastering === null) { for (const k of ['max_cll', 'max_fall', 'max_nits', 'min_nits']) if (S.hdr) setPipe(S, 'hdr:' + k, 'cleared'); if (!S.hdr) return false; }
    else for (const k of ['max_cll', 'max_fall', 'max_nits', 'min_nits']) setPipe(S, 'hdr:' + k, nits(m[k]));
  } else if (tn === 'set_ocio_view_delivery') {
    const sel = A.selection || {}, view = typeof sel.view === 'string' && sel.view ? sel.view : (typeof sel.display === 'string' && sel.display ? sel.display : null);
    if (!view) return false;
    if (err) { (S._ch || (S._ch = {})).view = { from: null, dim: true }; S.view = '✗ ' + view; S._viewErr = true;
      if (/native-ocio/.test(String(P.error ?? ''))) ev._hudErr = 'set_ocio_view_delivery · engine-disabled: native OCIO is off in this build'; }   /* the engine's own reply: the view is not applied, nothing else changes */
    else {
      if (S._viewErr) { S.view = null; S._viewErr = false; } setPipe(S, 'view', view);
      const og = numOf(sel.output_gamut), out = space(og === 1 ? 1 : og === 0 ? 0 : null, sel.output_transfer); if (out) setPipe(S, 'output', out);
    }
  } else return false;
  pipeRows(state, MODES); ev._kvDone = true; return true;
}

// ---------- project (m02 History / Diff / Batch / Media, m16 timeline + Inspector for set_meta) ----------
function agoAll(items, t) {
  for (const it of items) { const s = (t - it._t) / 1000; it.ago = s < 60 ? 'now' : s < 3600 ? Math.floor(s / 60) + 'm' : Math.floor(s / 3600) + 'h'; it.live = false; }
}
function histPush(state, MODES, item, opts = {}) {
  const H = panel(state, MODES, 'batch', 'history'); if (!H) return false;
  const t = now(); let items = (H.items || []).filter(x => !x.dim);   /* a row shown as deleted leaves on the next change */
  if (opts.reset) items = [];
  if (opts.replace) items = items.filter(x => !(x._cp && x.id === item.id));
  if (item._step && items[0] && items[0]._step) items.shift();   /* undo / redo: one row is the stack position, the latest call wins */
  item = { ...item, _t: t, _flash: t };
  items.unshift(item); agoAll(items, t); item.live = true;
  for (const x of items) x.sel = x === (opts.sel || item);
  const src = (state.views.batch && state.views.batch.viewer && state.views.batch.viewer.src) || null; if (src && !item.src) item.src = src;
  while (items.length > 5) { let j = -1; for (let i = items.length - 1; i > 0; i--) if (!items[i]._cp) { j = i; break; } items.splice(j > 0 ? j : items.length - 1, 1); }   /* checkpoints outlive step rows */
  H.items = items; return true;
}
const rev = s => { s = String(s ?? ''); let m;
  if (s === 'current' || s === '') return 'now';
  if ((m = /^checkpoint:(.+)$/.exec(s))) return m[1];
  if ((m = /^undo:(\d+)$/.exec(s))) return +m[1] === 1 ? '1 step back' : m[1] + ' steps back';
  if ((m = /^file:(.+)$/.exec(s))) return base(m[1]);
  return clean(s) || 'revision'; };
function diffLines(state, MODES, ctx, lines, meta) {
  const D = panel(state, MODES, 'batch', 'diffbatch'); if (!D) return false;
  const t = now();
  if (D._ctx !== ctx) { D.diff = []; D._ctx = ctx; D.meta = ''; }
  D.diff = (D.diff || []).concat(lines).slice(-6); D.titleSmall = ctx; if (meta != null) D.meta = meta; D._flash = t; return true;
}
function batchTable(state, MODES, title, meta, rows) {
  const D = panel(state, MODES, 'batch', 'diffbatch'); if (!D) return false;
  D.batch = { title, meta, rows: rows.slice(0, 6) }; D._flash = now(); return true;
}
function mediaFoot(state, MODES, foot, items) {
  const M = panel(state, MODES, 'batch', 'media'); if (!M) return false;
  M.foot = foot; M._footFlash = now(); if (items) M.items = items; return true;
}
export function projectOps(state, MODES, ev) {
  const tn = String(ev.tool_name), P = ev.payload || {};
  if (ev.phase === 'start') {
    (state._covArgs || (state._covArgs = {}))[ev.call_id] = P.args || {};
    if (tn === 'set_meta' && P.args && typeof P.args.id === 'string') {   /* the rail's copy: the id reads as the clip / track's name, else is left out */
      const A0 = P.args, id = A0.id.replace(/^@/, ''), tl = timelineView(state, MODES);
      const lb = A0.target === 'track' ? (tl && findTrack(tl, id) ? clean(findTrack(tl, id).label) : null) : clipName(state, MODES, A0.id);
      const { id: _, ...rest } = A0; ev.payload = { ...P, args: lb ? { ...rest, [A0.target === 'track' ? 'track' : 'clip']: lb } : rest };
    }
    return false;
  }
  if (ev.phase !== 'complete' && ev.phase !== 'error') return false;
  const A = (state._covArgs && state._covArgs[ev.call_id]) || P.args || {}; if (state._covArgs) delete state._covArgs[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {}, err = ev.phase === 'error';
  const n = v => Math.round(numOf(v) ?? 0), pl = (k, w) => `${k} ${w}${k === 1 ? '' : 's'}`;
  let hit = false;
  if (err) {   /* a refused call changes nothing on screen (COV-C2): its rail row turns to the error state with the engine's words and the HUD says it */
    if (tn === 'batch' && /rolled back/.test(String(P.error ?? ''))) ev._hudErr = 'batch failed · rolled back, nothing applied';
    return false;
  }
  switch (tn) {
    case 'checkpoint': {
      const name = R.name ?? A.name; if (!name) break;
      const H = panel(state, MODES, 'batch', 'history'), was = H && (H.items || []).some(x => x._cp && x.id === name);
      (state._cps || (state._cps = {}))[String(name)] = projSnap(state, MODES);   /* what the project looked like, for restore */
      hit = histPush(state, MODES, { id: String(name), name: was ? 'checkpoint · replaced' : 'checkpoint', _cp: true }, { replace: true }); break;
    }
    case 'checkpoint_delete': {
      if (state._cps) delete state._cps[String(A.name)];
      const H = panel(state, MODES, 'batch', 'history'); if (!H) break;
      const it = (H.items || []).find(x => x._cp && x.id === A.name);
      if (it) { it.name = 'deleted'; it.dim = true; it.live = false; it._cp = false; it._flash = now(); hit = true; }
      else hit = histPush(state, MODES, { id: String(A.name ?? 'checkpoint'), name: 'deleted', dim: true });
      if (it && it.sel) { it.sel = false; const nx = H.items.find(x => !x.dim); if (nx) { nx.sel = true; nx.live = true; } }
      break;
    }
    case 'restore': {
      const name = R.restored ?? A.name; if (!name) break;
      const H = panel(state, MODES, 'batch', 'history'), cp = H && (H.items || []).find(x => x._cp && x.id === name);
      hit = histPush(state, MODES, { id: 'restore', name: String(name), src: cp && cp.src }, { sel: cp });
      if (cp) { cp._flash = now(); } ev._hudText = `restore · back to ${name}`;
      const S = state._cps && state._cps[String(name)]; if (S) { projApply(state, MODES, S); ev._tlDone = true; }   /* the timeline and the pipeline go back to the checkpoint */
      break;
    }
    case 'undo': case 'redo': {
      const k = n(R[tn === 'undo' ? 'undone' : 'redone'] ?? A.steps ?? 1);
      hit = histPush(state, MODES, { _step: true, id: k > 1 ? `${tn} ${k} steps` : tn, name: `${n(R.undo_left)} left` });   /* the History column fits ~13 characters */
      ev._hudText = `${tn} ${pl(k, 'step')} · ${n(R.undo_left)} left to undo, ${n(R.redo_available)} to redo`; break;
    }
    case 'save_project': {
      const f = base(R.path ?? A.path) || 'project';
      const hist = R.history != null || A.with_history;
      hit = histPush(state, MODES, { id: f, name: 'saved' });
      (state._saved || (state._saved = {}))[String(R.path ?? A.path)] = projSnap(state, MODES);   /* a later load_project of this file shows it again */
      ev._hudText = `save_project · ${f}${hist ? ` + history: ${pl(n(R.undo_steps), 'undo step')}, ${pl(n(R.checkpoints), 'checkpoint')}` : ''}`;
      mediaFoot(state, MODES, `save_project: ${f}${hist && R.history_bytes != null ? `, history ${bytes(R.history_bytes)}` : ''}`); break;
    }
    case 'load_project': {
      const f = base(A.path) || 'project';
      hit = histPush(state, MODES, { id: f, name: 'loaded' }, { reset: true });
      const S = state._saved && state._saved[String(A.path)]; if (S) { projApply(state, MODES, S); ev._tlDone = true; }   /* a file this window saw saved: its timeline and pipeline */
      ev._hudText = `load_project · ${f}${R.undo_steps != null ? `: ${pl(n(R.undo_steps), 'undo step')}, ${pl(n(R.checkpoints), 'checkpoint')}` : ''}`;
      const D = R.dependencies && typeof R.dependencies === 'object' ? R.dependencies : null;
      if (D) {
        const bad = (Array.isArray(D.not_ok) ? D.not_ok : []).filter(x => x && typeof x === 'object');
        const by = new Map(); for (const x of bad) { const nm = base(x.path ?? x.resolved) || String(x.kind || 'file'), k = nm + '|' + (x.kind || '') + '|' + (x.status || ''); const g = by.get(k) || { name: nm, kind: x.kind, status: x.status || 'missing', n: 0 }; g.n++; by.set(k, g); }
        const items = [...by.values()].slice(0, 4).map(g => ({ name: g.name, note: `${g.kind ? g.kind + ' ' : ''}${g.status}${g.n > 1 ? ' ×' + g.n : ''}`, em: true }));
        mediaFoot(state, MODES, `load_project: ${pl(n(D.total), 'file')}, ${n(D.missing)} missing${n(D.changed) ? `, ${n(D.changed)} changed` : ''}`, items.length ? items : undefined);
      } else mediaFoot(state, MODES, `load_project: ${f}, ${pl(n(R.tracks), 'track')}`);
      break;
    }
    case 'project_diff': {
      const ctx = `${rev(A.a)} to ${rev(A.b ?? 'current')}`, L = [];
      const nm = x => (x && typeof x === 'object' ? clean(x.name) || clipName(state, MODES, x.id) : clean(x));
      for (const x of (Array.isArray(R.added) ? R.added : [])) { const s = nm(x); if (s) L.push(['+', s]); }
      for (const x of (Array.isArray(R.removed) ? R.removed : [])) { const s = nm(x); if (s) L.push(['-', s]); }
      for (const x of (Array.isArray(R.changed) ? R.changed : [])) { const s = nm(x); const k = Array.isArray(x && x.changes) ? x.changes.length : 0; if (s) L.push(['~', k ? `${s}  ${pl(k, 'change')}` : s]); }
      const T = R.tracks && typeof R.tracks === 'object' ? R.tracks : {};
      for (const x of (Array.isArray(T.renamed) ? T.renamed : [])) if (x && clean(x.after)) L.push(['~', `track ${clean(x.before) || 'unnamed'} -> ${x.after}`]);
      for (const x of (Array.isArray(T.added) ? T.added : [])) { const s = nm(x); if (s) L.push(['+', 'track ' + s]); }
      for (const x of (Array.isArray(T.removed) ? T.removed : [])) { const s = nm(x); if (s) L.push(['-', 'track ' + s]); }
      if (R.identical || !L.length) L.push(['~', R.identical ? 'identical' : 'no named changes']);
      const C = R.counts || {};
      const D = panel(state, MODES, 'batch', 'diffbatch'); if (D) D._ctx = null;
      hit = diffLines(state, MODES, ctx, L, `+${n(C.added)} ~${n(C.changed)} -${n(C.removed)}`); break;
    }
    case 'template_define': {
      const name = R.name ?? A.name; if (!name && !A.library_path) break;
      const ps = Array.isArray(R.params) ? R.params : (Array.isArray(A.params) ? A.params.map(p => p && p.name).filter(Boolean) : []);
      const ops = n(R.op_count ?? (Array.isArray(A.ops) ? A.ops.length : 0));
      hit = diffLines(state, MODES, 'templates', [[R.replaced ? '~' : '+', `template ${name ?? base(A.library_path)}${ps.length ? ` (${ps.join(', ')})` : ''} · ${pl(ops, 'op')}`]]); break;
    }
    case 'template_delete': {
      const name = R.deleted ?? A.name; if (!name) break;
      hit = diffLines(state, MODES, 'templates', [['-', `template ${name}`]]); break;
    }
    case 'batch': {
      const ops = Array.isArray(A.ops) ? A.ops : [], res = Array.isArray(R.results) ? R.results.filter(x => x && typeof x === 'object') : [];
      const warn = new Set((Array.isArray(R.warnings) ? R.warnings : []).map(w => w && w.index));
      const rows = (res.length ? res : ops.map((o, i) => ({ index: i, tool: o && o.tool }))).map(x => {
        const op = ops[x.index] || {}, a = op.args || {};
        const who = clipName(state, MODES, a.clip_id) || clean(a.name) || clean(a.track_id) || `op ${(x.index ?? 0) + 1}`;
        return [who, String(x.tool ?? op.tool ?? ''), R.dry_run ? 'dry run' : warn.has(x.index) ? 'warned' : 'done'];
      });
      hit = batchTable(state, MODES, clean(A.label) ? 'Batch ' + A.label : 'Batch', `${n(R.applied ?? rows.length)} / ${ops.length || rows.length}`, rows);
      if (!R.dry_run) { batchTimeline(state, MODES, ops, res); ev._tlDone = true; } break;
    }
    case 'recipe_apply': {
      const name = (R.recipe && R.recipe.name) || A.name || 'recipe';
      const roles = R.roles && typeof R.roles === 'object' ? Object.keys(R.roles) : [];
      const rows = roles.map(k => [k.split('/').pop(), String(name), R.dry_run ? 'dry run' : 'added']);
      hit = batchTable(state, MODES, 'Recipe ' + name, `${pl(n(R.applied), 'op')}`, rows.length ? rows : [[String(name), 'recipe', R.dry_run ? 'dry run' : 'done']]);
      if (!R.dry_run) { const pr = A.params && typeof A.params === 'object' ? A.params : {}, txt = [pr.title, pr.text, pr.name].find(v => typeof v === 'string' && clean(v));
        layerClips(state, MODES, P, R.created && Array.isArray(R.created.clip_ids) ? R.created.clip_ids : [], `${name}${txt ? ' · ' + txt : ''}`); ev._tlDone = true; }
      break;
    }
    case 'template_apply': {
      const name = R.template ?? A.name ?? 'template';
      const created = Array.isArray(R.created) ? R.created : (R.created && Array.isArray(R.created.clip_ids) ? R.created.clip_ids : []);
      const label = clean(P.label) || (R.args && typeof R.args === 'object' ? Object.values(R.args).find(v => typeof v === 'string' && clean(v)) : null);
      const rows = created.map((c, i) => [clipName(state, MODES, c) || (i === 0 && label) || `clip ${i + 1}`, String(name), R.dry_run ? 'dry run' : 'added']);
      hit = batchTable(state, MODES, 'Template ' + name, pl(n(R.applied_ops ?? R.count), 'op'), rows.length ? rows : [[label || String(name), String(name), R.dry_run ? 'dry run' : 'done']]);
      if (!R.dry_run) { layerClips(state, MODES, P, created, label || String(name)); ev._tlDone = true; }
      break;
    }
    case 'set_meta': hit = metaOps(state, MODES, A, R); break;
  }
  if (hit) ev._kvDone = true;
  return hit;
}
function metaOps(state, MODES, A, R) {
  const tl = timelineView(state, MODES); const t = now();
  const target = R.target ?? A.target ?? 'clip', id = String(R.id ?? A.id ?? '').replace(/^@/, '');
  const name = R.name !== undefined ? R.name : A.name;
  let hit = false, from = null, nm = null;
  if (target === 'track') {
    const tr = tl && (findTrack(tl, id) || (tl.tracks || []).find(x => x.name === id)); if (!tr) return false;
    from = clean(tr.label); if (typeof name === 'string' && name) { tr.label = name.slice(0, 16); tr.name = name; } tr._flash = t; nm = tr.label; hit = true;
  } else {
    let [tr, i] = tl ? findClip(tl, { clip_id: id }) : [null, -1];
    if (!tr && tl) for (const x of tl.tracks || []) { const j = (x.clips || []).findIndex(c => c.name === id); if (j >= 0) { tr = x; i = j; break; } }
    if (!tr) return false;
    const c = tr.clips[i]; from = clean(c.label);
    if (typeof name === 'string' && name) { c.label = name; c.name = name; } c._flash = t; nm = clean(c.label); hit = true;
  }
  const kvE = Object.entries(MODES.edit ? MODES.edit.panels : {}).find(([, sp]) => sp.kind === 'kv');
  if (kvE) {
    const V = state.views.edit || (state.views.edit = {}), kv = V[kvE[0]] || (V[kvE[0]] = {});
    kv.blocks = (kv.blocks || []); const j = kv.blocks.findIndex(b => b._live); if (j >= 0) kv.blocks.splice(j);
    if (nm) kv.sub = nm;
    kv.blocks.push({ sec: target === 'track' ? 'set_meta · track' : 'set_meta', _live: true });
    if (typeof name === 'string' && name) kv.blocks.push({ k: 'Name', v: name, ...(from && from !== name ? { from } : {}), _live: true, _flash: t });
    else if (name === null) kv.blocks.push({ k: 'Name', v: 'cleared', _live: true, _flash: t });
    const role = R.role !== undefined ? R.role : A.role; if (role !== undefined) kv.blocks.push({ k: 'Role', v: role == null ? 'cleared' : String(role), _live: true, _flash: t });
    const tags = Array.isArray(R.tags) ? R.tags : Array.isArray(A.tags) ? A.tags : null;
    if (tags) kv.blocks.push({ k: 'Tags', v: tags.length ? tags.join(', ') : 'none', _live: true, _flash: t });
    else { const add = Array.isArray(A.add_tags) ? A.add_tags.map(x => '+' + x) : [], rm = Array.isArray(A.remove_tags) ? A.remove_tags.map(x => '-' + x) : []; if (add.length || rm.length) kv.blocks.push({ k: 'Tags', v: [...add, ...rm].join(' '), _live: true, _flash: t }); }
  }
  return hit;
}

// ---------- qa / cache: the HUD status toast and the call's own rail row (Naman 7:24: no viewer or layout change) ----------
// diff_frames / lint_frame read, cache_clear / cache_set_budget tune memory: nothing on the canvas moves and the workspace
// does not hop. The toast says what came back ("diff_frames · now vs graded: 77% changed, major") and the rail row keeps it
// in place of the start-time args, so the result stays readable after the toast fades.
const sec = v => { const x = numOf(v); return x == null ? '' : ` ${Math.round(x * 100) / 100}s`; };
// diff_frames quality object -> one line of raw numbers (PSNR dB, SSIM, MS-SSIM); null PSNR reads "identical"
function qualityLine(Q) {
  const fx = (v, d) => (typeof v === 'number' && Number.isFinite(v) ? v.toFixed(d) : null), parts = [];
  if ('psnr_db' in Q) parts.push(`PSNR ${fx(Q.psnr_db, 2) != null ? fx(Q.psnr_db, 2) + ' dB' : Q.identical === true ? 'identical' : 'none'}`);
  if ('ssim' in Q) parts.push(`SSIM ${fx(Q.ssim, 4) ?? 'none'}`);
  if ('ms_ssim' in Q) parts.push(`MS-SSIM ${fx(Q.ms_ssim, 4) ?? 'none'}`);
  return parts.length ? parts.join(' · ') : null;
}
function side(state, MODES, s) {
  if (!s || typeof s !== 'object') return 'now';
  const src = String(s.source ?? 'composite'); let m, w;
  if (src === 'composite') w = 'now';
  else if ((m = /^snapshot:(.+)$/.exec(src))) w = rev(m[1]);
  else if ((m = /^clip:(.+)$/.exec(src))) w = clipName(state, MODES, m[1]) || 'clip';
  else if ((m = /^file:(.+)$/.exec(src))) w = base(m[1]);
  else w = rev(src);
  return w + (s.time != null ? sec(s.time) : s.frame != null ? ` f${Math.round(numOf(s.frame))}` : '');
}
const LINT = { out_of_bounds: 'out of bounds', invisible_layer: 'invisible layer', low_res_upscale: 'low-res upscale', static_frame: 'static frame' };
export function qaOps(state, MODES, ev) {
  const tn = String(ev.tool_name), P = ev.payload || {};
  if (ev.phase === 'start') { (state._covArgs || (state._covArgs = {}))[ev.call_id] = P.args || {}; return false; }
  if (ev.phase !== 'complete') return false;
  const A = (state._covArgs && state._covArgs[ev.call_id]) || P.args || {}; if (state._covArgs) delete state._covArgs[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const n = v => Math.round(numOf(v) ?? 0), pl = (k, w) => `${k} ${w}${k === 1 ? '' : 's'}`;
  let rail = null, toast = null;
  if (tn === 'diff_frames') {
    const a = side(state, MODES, R.a || A.a), b = side(state, MODES, R.b || A.b), D = R.delta || {}, pct = numOf(D.changed_percent);
    const what = D.bit_identical ? 'identical' : pct != null ? `${pct < 1 ? Math.round(pct * 100) / 100 : Math.round(pct * 10) / 10}% changed${R.verdict_hint ? ', ' + R.verdict_hint : ''}` : null;
    if (!what) return false;
    const psnr = numOf((R.metrics || {}).psnr_db);
    rail = { changed: D.bit_identical ? 'none' : what.replace(/ changed/, '') }; toast = `${tn} · ${a} vs ${b}: ${what}${psnr != null && !D.bit_identical ? ` (PSNR ${Math.round(psnr * 10) / 10} dB)` : ''}`;
    // v6 quality object (only when the call passed quality): raw luma PSNR / SSIM / MS-SSIM scalars, null PSNR = identical, never a verdict
    const Q = R.quality && typeof R.quality === 'object' ? R.quality : null;
    ev._dfQuality = Q ? qualityLine(Q) : null;
  } else if (tn === 'compare_ranges') {
    // A/B over time: only the summary's top-level scalars are read (the t_s / psnr_db / ssim curves arrive cut to 8 entries plus a
    // "…+N" marker in the feed). A null PSNR mean with every sample identical reads "identical"; nothing here is a verdict.
    const S = R.summary && typeof R.summary === 'object' ? R.summary : {}, pts = numOf(R.points); if (pts == null) return false;
    const sd = (s, st) => side(state, MODES, { source: s && s.source, time: numOf(st) ?? 0 });
    const range = `${sd(R.a || A.a, (R.a || {}).start)} vs ${sd(R.b || A.b, (R.b || {}).start)} · ${sec(numOf(R.duration) ?? 0).trim()}, ${pl(n(pts), 'sample')}`;
    const fx = (v, d) => (typeof v === 'number' && Number.isFinite(v) ? v.toFixed(d) : null);
    const row = (k, unit, d) => { const m = S[k]; if (!m || typeof m !== 'object') return null; const mean = fx(m.mean, d), min = fx(m.min, d), p5 = fx(m.p5, d);
      if (mean == null) return numOf(R.identical_count) === pts ? 'identical' : 'none';
      const at = numOf(m.min_at_s); return `mean ${mean}${unit}, min ${min}${unit}${at != null ? ' at ' + sec(at).trim() : ''}, p5 ${p5}${unit}`; };
    const ps = row('psnr_db', ' dB', 2), ss = row('ssim', '', 4), ms = row('ms_ssim', '', 4), idn = n(R.identical_count);
    rail = { [`${pl(n(pts), 'sample')}`]: ps != null ? `PSNR ${ps}` : ss != null ? `SSIM ${ss}` : `MS-SSIM ${ms}` };
    toast = `${tn} · ${range}: ${[ps != null && `PSNR ${ps}`, ss != null && `SSIM ${ss}`].filter(Boolean).join(' · ')}${idn ? ` (${idn} identical)` : ''}`;
    // four rows (readCard keeps 4), each allowed to wrap
    ev._crRows = [['Range', range, 'wrap'], ...[['PSNR', ps], ['SSIM', ss], ['MS-SSIM', ms]].filter(r => r[1] != null).map(r => [r[0], r[1] + (r[0] === 'PSNR' && idn && r[1] !== 'identical' ? ` (${idn} identical)` : ''), 'wrap'])].slice(0, 4);
  } else if (tn === 'lint_frame') {
    const t = A.time != null ? sec(A.time).trim() : Array.isArray(A.times) ? pl(A.times.length, 'time') : '';
    const S = R.summary || {}, by = S.counts_by_code && typeof S.counts_by_code === 'object' ? S.counts_by_code : {};
    const parts = Object.entries(by).filter(([, k]) => numOf(k)).map(([c, k]) => `${LINT[c] || c.replace(/_/g, ' ')} ×${n(k)}`);
    const what = parts.length ? parts.join(', ') : 'clean';
    rail = { [t || 'frame']: what }; toast = `${tn}${t ? ' ' + t : ''} · ${what}`;
  } else if (tn === 'flash_series') {
    // raw counts only, no verdict: the busiest window's transitions and the largest changed area, as the engine measured them
    const T = R.totals || {}, P0 = R.params_used || {}; if (R.samples == null) return false;
    const rg = R.range || {}, span = rg.start != null && rg.end != null ? `${sec(rg.start).trim()}–${sec(rg.end).trim()}` : '';
    const win = numOf(P0.window_s) ?? 1, area = numOf(T.max_changed_area_fraction);
    const what = `max ${n(T.max_lum_transitions_in_window)} luminance / ${n(T.max_red_transitions_in_window)} red transitions per ${Math.round(win * 100) / 100}s` + (area != null ? `, area ${Math.round(area * 100)}%` : '');
    // the info panel's Transitions row: the same three numbers in fewer words, stacked under its label (it wrapped to 6 lines)
    ev._flashTr = `max ${n(T.max_lum_transitions_in_window)} luminance · ${n(T.max_red_transitions_in_window)} red per ${Math.round(win * 100) / 100}s` + (area != null ? ` · area ${Math.round(area * 100)}%` : '');
    // stamps: each luminance transition sits on its first crossing of lum_delta inside the monotonic run that reaches the
    // swing's extreme (strict-run guard), so these times are where the change starts; up to 8 listed, then "+N more".
    // Read from the payload's lum_stamps [{t, dir}] (kimo-mcp spectator.rs builds it from the FULL result): the result's
    // frames arrays arrive cut to 8 entries plus a "…+N" marker, so they cannot carry a stamp. A "…+N" string or any
    // non-finite time is skipped, never read as 0.
    const LS = Array.isArray(P.lum_stamps) ? P.lum_stamps : [];
    const st = LS.map(s => { const t = s && typeof s === 'object' && typeof s.t === 'number' && Number.isFinite(s.t) ? s.t : null, d = s ? s.dir : null;
      return t != null && (d === 1 || d === -1) ? `${d > 0 ? '↑' : '↓'}${Math.round(t * 100) / 100}s` : null; }).filter(Boolean);
    const tot = Number.isFinite(P.lum_stamps_total) ? Math.max(P.lum_stamps_total, st.length) : st.length;
    ev._flashStamps = st.length ? st.slice(0, 8).join(' ') + (tot > 8 ? ` +${tot - 8} more` : '') : Array.isArray(P.lum_stamps) ? 'none' : 'not in feed';
    rail = { [span || pl(n(R.samples), 'frame')]: what }; toast = `${tn}${span ? ' ' + span : ''} · ${pl(n(R.samples), 'frame')}: ${what}`;
    // v6 flash rule numbers (only when the call gave flash_limit): top-level scalars worst_window_* / windows_over_limit /
    // flash_area_pct_max and the red_ twins; the busiest 1 s window with its start time, how many windows exceed the limit
    // card text kept to two lines at full card width: "5 @ 5.92s, 27>3, 98%" = 5 flashes in the worst 1 s window starting 5.92 s,
    // 27 windows over the limit 3, largest flash area 98% (the row wrapped to 8 lines beside its label and pushed the card above a 640x360 window)
    const lim = numOf(R.flash_limit);
    if (lim != null) { const one = (c, s0, o, a) => { const cn = numOf(c), sv = numOf(s0), ov = numOf(o), ar = numOf(a); if (cn == null) return null;
        return `${cn}${sv != null ? ` @ ${Math.round(sv * 100) / 100}s` : ''}${ov != null ? `, ${ov}>${Math.round(lim * 100) / 100}` : ''}${ar != null && cn > 0 ? `, ${Math.round(ar)}%` : ''}`; };
      const lw = one(R.worst_window_flashes, R.worst_window_start, R.windows_over_limit, R.flash_area_pct_max), rw = one(R.worst_red_window_flashes, R.worst_red_window_start, R.red_windows_over_limit, R.red_flash_area_pct_max);
      ev._flashRule = lw != null || rw != null ? `${lw ?? '–'} · red ${rw ?? '–'}` : null; }
  } else if (tn === 'audio_spectrum') {
    // raw levels only, from the result's top-level scalars (peak_hz / peak_dbfs / total_dbfs / loudest_band): the bands and
    // spectrum arrays arrive cut to 8 entries plus a "…+N" marker, so no row reads them. null (silence) reads "silent", never 0.
    const fin = v => (typeof v === 'number' && Number.isFinite(v) ? v : null);
    const hz = v => (v >= 1000 ? `${Math.round(v / 10) / 100} kHz` : `${Math.round(v)} Hz`), dbs = v => `${v < 0 ? '−' : ''}${Math.abs(Math.round(v * 10) / 10).toFixed(1)} dBFS`;
    const rg = R.range || {}, span = fin(rg.start) != null && fin(rg.end) != null ? `${sec(rg.start).trim()}–${sec(rg.end).trim()}` : '';
    const ph = fin(R.peak_hz), pd = fin(R.peak_dbfs), td = fin(R.total_dbfs), LB = R.loudest_band && typeof R.loudest_band === 'object' ? R.loudest_band : {};
    const lo = fin(LB.lo_hz), hi = fin(LB.hi_hz), ld = fin(LB.dbfs);
    if (fin(R.fft_size) == null) return false;
    const peak = ph != null && pd != null ? `${hz(ph)}, ${dbs(pd)}` : 'silent';
    const span2 = (a, b) => (b < 1000 ? `${Math.round(a)}–${Math.round(b)} Hz` : a >= 1000 ? `${Math.round(a / 10) / 100}–${Math.round(b / 10) / 100} kHz` : `${hz(a)}–${hz(b)}`);
    const band = lo != null && hi != null && ld != null ? `${span2(lo, hi)}, ${dbs(ld)}` : 'none';
    const who = A.clip_id ? clipName(state, MODES, A.clip_id) || 'clip' : 'mix';
    rail = { [span || who]: `peak ${peak}` }; toast = `${tn} ${who}${span ? ' ' + span : ''} · peak ${peak}, loudest band ${band}`;
    // four rows (readCard keeps 4), each allowed to wrap so no value is cut by an ellipsis
    ev._specRows = [['Range', `${who}${span ? ' ' + span : ''}`, 'wrap'], ['Peak', peak, 'wrap'], ['Loudest band', band, 'wrap'], ['Total', td != null ? dbs(td) : 'silent', 'wrap']];
  } else if (tn === 'cache_set_budget') {
    const cap = numOf(R.cap_bytes) ?? (numOf(A.megabytes) != null ? numOf(A.megabytes) * 1048576 : null); if (cap == null) return false;
    const prev = state._cacheCap; state._cacheCap = cap;
    const cv = cap === 0 ? 'off' : bytes(cap), pv = prev != null && prev !== cap ? (prev === 0 ? 'off' : bytes(prev)) + ' → ' : '';
    rail = { budget: pv + cv }; toast = `frame cache ${pv}${cv}, ${bytes(R.bytes ?? 0)} used`;
  } else if (tn === 'cache_clear') {
    rail = { freed: `${bytes(R.freed_bytes ?? 0)}, ${pl(n(R.cleared_entries), 'frame')}` }; toast = `frame cache cleared · ${pl(n(R.cleared_entries), 'frame')}, ${bytes(R.freed_bytes ?? 0)} freed`;
  } else return false;
  ev._railArgs = rail; ev._hudText = toast; ev._kvDone = true;
  // GUI-FIX-1: the same answer stays on the current screen's info panel (router.js readCard), not only in the toast
  if (tn === 'diff_frames') { ev._readRows = [['Compared', `${side(state, MODES, R.a || A.a)} vs ${side(state, MODES, R.b || A.b)}`, 'wrap'], ['Changed', rail.changed]]; if (ev._dfQuality) ev._readRows.push(['Quality', ev._dfQuality, 'wrap']); }
  else if (tn === 'compare_ranges') ev._readRows = ev._crRows;
  else if (tn === 'lint_frame') { ev._readRows = [['Frame', Object.keys(rail)[0]], ['Findings', Object.values(rail)[0]]];
    // v6 quality numbers (lint_frame quality): the extremes from summary.quality, only when the call asked for them
    const Q = R.summary && R.summary.quality && typeof R.summary.quality === 'object' ? R.summary.quality : null, pc = x => numOf(x) != null ? `${Math.round(x * 1000) / 10}%` : null;
    if (Q) { const cr = pc(Q.max_crushed_frac), cl = pc(Q.max_clipped_frac), pl0 = numOf(Q.max_band_plateau_px), lv = numOf(Q.min_luma_levels), id = numOf(Q.identical_to_prev);
      if (cr != null && cl != null) ev._readRows.push(['Crushed / clipped', `max ${cr} / ${cl}`, 'stack']);
      const bf = [pl0 != null ? `plateau ${Math.round(pl0 * 10) / 10} px` : null, lv != null ? `${lv} levels` : null, id != null ? `${id} same as prev` : null].filter(Boolean).join(' · ');
      if (bf) ev._readRows.push(['Banding / frozen', bf, 'stack']); } }
  else if (tn === 'flash_series') { ev._readRows = [['Range', Object.keys(rail)[0]], ['Transitions', ev._flashTr, 'stack'], ['Stamps', ev._flashStamps, 'wrap']]; if (ev._flashRule) ev._readRows.push(['Worst 1 s flashes', ev._flashRule, 'stack']);
    // v6 flash pin: area_basis field names the worst 10 degree field on the same row (the card keeps 4 rows; top-level worst_field_rect only)
    const fr = R && R.area_basis === 'field' && Array.isArray(R.worst_field_rect) && R.worst_field_rect.length === 4 && R.worst_field_rect.every(v => typeof v === 'number' && Number.isFinite(v)) ? R.worst_field_rect : null;
    if (ev._flashRule && fr) ev._readRows[ev._readRows.length - 1][1] += ` · field ${fr[0]},${fr[1]} ${fr[2]}×${fr[3]}`; }
  else if (tn === 'audio_spectrum') ev._readRows = ev._specRows;
  else if (tn === 'cache_set_budget') ev._readRows = [['Frame cache', rail.budget], ['Used', bytes(R.bytes ?? 0)]];
  else ev._readRows = [['Freed', bytes(R.freed_bytes ?? 0)], ['Frames', String(n(R.cleared_entries))]];
  return true;
}


// ---------- COV-C2: undo / redo, restore and load for this lane's own state ----------
// The engine's undo / redo / restore / load_project events carry no state, so the window keeps what it drew:
// a mutating lane-C call stores the lane's panels before and after it on the shared undo stack (cov_a.js
// covAHistory, aligned with the engine by undo_left), a checkpoint / save_project keeps the project's timeline
// and pipeline under its name / path, and undo, redo, restore and load_project put those back.
export const C_MUT = /^(set_color_pipeline|set_clip_input_color|set_hdr_mastering|set_meta|restore|batch|recipe_apply|template_apply|template_define|template_delete|load_project)$/;
const noFlash = (k, v) => (k === '_flash' || k === '_footFlash' ? undefined : v);
const clone = x => (x == null ? null : JSON.parse(JSON.stringify(x, noFlash)));
function tlSnap(state, MODES) { const tl = timelineView(state, MODES) || {}; return clone({ tracks: tl.tracks || [], markers: (tl.markers || []).filter(m => !m.gone), mc: tl._mc || {} }); }
function projSnap(state, MODES) { return { pipe: clone(state._pipe || null), tl: tlSnap(state, MODES) }; }
const VIEWS = [['color', 'kv'], ['edit', 'kv'], ['batch', 'diffbatch'], ['batch', 'media']];
export function cSnap(state, MODES) { return { ...projSnap(state, MODES), v: VIEWS.map(([m, k]) => clone(panel(state, MODES, m, k))) }; }
function tlApply(state, MODES, s, t) {
  const tl = timelineView(state, MODES); if (!tl || !s) return;
  const key = tr => JSON.stringify((tr.clips || []).map(c => [c.id, c.s, c.e, c.label, c.fx, (c.keys || []).length]));
  const before = new Map((tl.tracks || []).map(tr => [tr.id, key(tr) + '|' + tr.label]));
  tl.tracks = clone(s.tracks);
  for (const tr of tl.tracks) if (before.get(tr.id) !== key(tr) + '|' + tr.label) { tr._flash = t; for (const c of tr.clips || []) c._flash = t; }
  const gone = (tl.markers || []).filter(m => !m.gone && !s.markers.some(x => x.id === m.id)).map(m => ({ ...m, gone: t }));
  tl.markers = [...clone(s.markers), ...gone]; tl._mc = clone(s.mc);
  fitRuler(tl);
}
function pipeApply(state, MODES, pipe) {
  if (!pipe) { if (state._pipe) { state._pipe = null; const kv = panel(state, MODES, 'color', 'kv'); if (kv) kv.blocks = []; } return; }   /* back to a project with no pipeline set: the list empties */
  const cur = state._pipe || {}, nx = clone(pipe) || { input: null, working: null, view: null, output: null };
  const ch = {};   /* every row whose value moves flashes, from -> to where it fits */
  for (const k of ['input', 'working', 'view', 'output']) if ((cur[k] ?? null) !== (nx[k] ?? null)) ch[k] = { from: cur[k] ?? null };
  for (const [k, v] of Object.entries(nx.clips || {})) if ((cur.clips || {})[k] !== v) ch['clip:' + k] = { from: (cur.clips || {})[k] ?? null };
  for (const [k, v] of Object.entries(nx.hdr || {})) if ((cur.hdr || {})[k] !== v) ch['hdr:' + k] = { from: (cur.hdr || {})[k] ?? null };
  nx._ch = ch; state._pipe = nx;
  if (pipe || state._pipe) pipeRows(state, MODES);
}
function projApply(state, MODES, S) { const t = now(); tlApply(state, MODES, S.tl, t); if (S.pipe || state._pipe) pipeApply(state, MODES, S.pipe); }
export function cRestore(state, MODES, S) {   /* undo / redo of a lane-C step: its panels exactly as they were */
  const t = now();
  tlApply(state, MODES, S.tl, t);
  state._pipe = clone(S.pipe);
  VIEWS.forEach(([m, k], i) => {
    const o = panel(state, MODES, m, k); if (!o) return;
    const was = JSON.stringify(o, noFlash), nx = S.v[i] || {};
    for (const key of Object.keys(o)) delete o[key];
    Object.assign(o, clone(nx));
    if (was !== JSON.stringify(o, noFlash)) { if (k === 'media') o._footFlash = t; else if (k === 'kv') for (const b of o.blocks || []) { if (!b.sec) b._flash = t; } else o._flash = t; }
  });
}
export function cPush(state, before, after, tool, R) {   /* the step covAHistory just pushed for this call gets the lane's snapshots */
  const h = state._covHist || (state._covHist = { undo: [], redo: [] });
  if (tool === 'load_project') {   /* the engine's stack is now the file's history plus the load itself: steps this window never saw stay blank */
    const k = Math.max(0, Math.round(numOf(R && R.undo_steps) ?? 0));
    h.undo = [...Array(Math.min(k, 47)).fill(null), { tool, c: true, before, after }]; h.redo = []; return;
  }
  if (!h.undo.length || h.undo[h.undo.length - 1] !== null) return;
  h.undo[h.undo.length - 1] = { tool, c: true, before, after };
}
function batchTimeline(state, MODES, ops, res) {   /* trims / moves the batch applied, from the engine's own per-op results */
  const tl = timelineView(state, MODES); if (!tl) return; const t = now();
  for (const r of res) {
    const op = ops[r.index] || {}, a = op.args || {}, b = r.result_brief && typeof r.result_brief === 'object' ? r.result_brief : {};
    const [tr, i] = findClip(tl, { clip_id: a.clip_id }); if (!tr) continue; const c = tr.clips[i];
    if (op.tool === 'trim_clip' && numOf(b.duration) != null) { c.e = c.s + numOf(b.duration); c._flash = t; }
    else if (op.tool === 'move_clip' && numOf(a.start ?? a.to) != null) { const d = c.e - c.s; c.s = numOf(a.start ?? a.to); c.e = c.s + d; c._flash = t; tr.clips.sort((x, y) => x.s - y.s); }
  }
  fitRuler(tl);
}
function layerClips(state, MODES, P, ids, label) {   /* template / recipe output: layered OVER the track's clips (the engine stacks them; nothing underneath is cut) */
  const tl = timelineView(state, MODES), tr = tl && findTrack(tl, P.track); if (!tr) return;
  const s = numOf(P.s), e = numOf(P.e); if (s == null || e == null || !ids.length) return;
  tr.clips = [...(tr.clips || []), { id: String(ids[0]), ids: ids.map(String), s, e, label, layer: true, ...(ids.length > 1 ? { fx: [ids.length + ' layers'] } : {}), _flash: now() }].sort((a, b) => a.s - b.s);
  tr._flash = now(); fitRuler(tl);
}
