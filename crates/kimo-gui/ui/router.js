// Tool-call router. Every engine event, in either wire format, lands here
// before the reducer:
//   unified (the engine contract, README "Unified tool events"):
//     {tool_name, call_id, phase: "start"|"progress"|"complete"|"error",
//      target: {track_id, clip_id, param}, payload: {...}, ts}
//   legacy (demo feed / older engines): {type: "tool_start"|"tool_end"|...}
// A unified event becomes the legacy rail events (so the rail always gets an
// entry) plus a family effect on the panel the tool touches: timeline clips,
// track headers, inspector rows, the viewer. Tools with no specific mapping
// still get the rail, an inspector diff of their payload and a HUD pulse, so
// no tool call is ever invisible. Pure data: nothing here touches the DOM.

import { READONLY, inferMode } from './modes.js';
import { COV_C, pipelineOps, projectOps, qaOps, space, C_MUT, cSnap, cRestore, cPush } from './project_ops.js';
import { COV_A, covAOps, covASee, covAPre, covAHistory } from './cov_a.js';
import { COVB, covbOps, covbLearn, covbHistory } from './cov_b.js';
import { readRows, callRows, CHIP, colourRows, scopeLine } from './reads.js';
import { fix2Pre, fix2Post } from './fix2.js';   /* GUI-FIX-2 */

export const FAMILIES = [
  ['pipeline', COV_C.pipeline], ['project', COV_C.project], ['qa', COV_C.qa],   /* project_ops.js: colour pipeline rows, m02 history / diff / batch / media, QA + cache in the HUD toast + rail row */
  ['edit', COV_A],   /* markers, cut / beat detection, beat binding, silence cuts, time mode, sync, multicam, burn-in, raw transition, event matching: cov_a.js */
  ['ingest', /^(import|ingest|add_media|relink|load_media|add_clip|add_track|insert)/],
  ['edit', /^(cut|split|blade|razor|trim|extend|ripple|slip|slide|move_clip|delete_clip|lift|roll)|^(duplicate_clip|swap_clips|loop_clip|freeze_frame|delete_track|set_speed|rate_stretch|set_speed_ramp|group_clips|ungroup_clip|distribute_clips|set_blend|set_echo|set_flow_blur|add_camera_shake|remove_audio_effect|set_track|project_set)$/],
  ['effects', /^(add_effect|apply_effect|set_effect|add_transition|remove_effect|add_speed|set_posterize|add_adjustment|add_managed_linear_effect$)/],
  ['transform', /^(set_property|set_clip_property|set_props?$|transform|set_transform|keyframe|set_keyframe|add_keyframe|scale|rotate|position|set_motion|add_null|set_parent|set_expression)/],
  ['color', /^(grade|color|colour|lut|apply_lut|set_lut|ocio|render_scope|analyze_exposure|wheel|curve|node_|hdr|render_ocio|render_exposure|render_gamut|set_grade_node|set_grade_graph|pick_white_balance|match_grade|match_color|set_white_balance|set_grade_output|remove_grade_node|generate_lut_node|add_acescct_primaries|create_shared_grade_graph|bind_shared_grade_graphs|remove_shared_grade_graph|set_node_param|set_node_mask|apply_asc_cdl|set_color_curve|set_color_wheels|set_grade_scopes|add_node$|connect_nodes|disconnect_nodes)/],
  ['audio', /^(set_|add_|apply_|auto_|measure_|normalize_|analyze_)?(mix|audio|gain|fader|mute|solo|pan|compress|eq|limiter|limit|loudness|duck|stem|bus|route_)|^track_beats$/],
  ['text', /^(caption|subtitle|add_caption|add_subtitle|edit_caption|transcript_edit|transcri|add_title|title|text|set_text|add_text|set_caption|validate_captions)/],
  ['3d', /^(add_mesh|mesh|model|extrude|bevel|light|camera|particle|emit|simulate|scene|render_3d|add_light|set_light|solve_camera|track|set_camera|add_scene3d|instantiate_gltf|generate_mesh_primitive|contour_sdf_graph|set_mesh_pin|derive_vertex_tangents|fit_track_homography|ecc_refine_track|decompose_track_homography|corner_disagreement|lk_residual_audit|validate_planar_track|undistort_clip|stabilize|apply_camera_solve|pin_layer|set_corner_pin|add_emitter|add_particles|add_shard_burst|set_material$|remove_material$|preview_material$|set_relight$|remove_light$)/],
  ['graphics', /^(add_shape|add_path_shape|set_path|add_path_key|clear_path_keys|add_shape_modifier|remove_shape_modifier|add_modifier|remove_modifier|set_layer_style|remove_layer_style|add_box_expand|add_detection_box|add_hud_frame|add_counter|add_prompt_box|add_glitch_confetti)$/],
  ['roto', /^(add_mask|set_mask|set_mask_keyframe|set_mask_path_key|set_mask_prop_key|update_mask|reorder_mask|remove_mask|remove_mask_layer|roto_propagate|refine_matte|set_track_matte|remove_track_matte)$/],
  ['export', /^(export|render|queue_render|encode|deliver|join_export$|preview_range$|collect_project$|contact_sheet$|cancel_job$|resume_job$)/],
  ['motion', /^(apply_motion_preset|bake_property|copy_curve|move_key|remove_key|remove_motion_path|set_animator_keyframe|simplify_curve|smooth_curve|motion_diff|motion_extract)$/],   // graph editor: curves, keys, motion (cov_b.js)
  ['measure', /^(solve_constraints|compute_text_bounds|raycast_mesh_raw|compile_scene3d_descriptor|preview_subject_match)$/],   // read-only measures drawn where they apply (cov_b.js)
];
export function family(name) {
  const n = String(name || '').toLowerCase();
  if (READONLY.test(n)) return 'inspect';
  for (const [f, re] of FAMILIES) if (re.test(n)) return f;
  return 'other';
}

/* one clock reading per frame: app.js freezes it for the frame's drain + render (thousands of flash checks on a 1k-clip
   timeline read it), and outside a frame it is the live clock */
let frameNow = 0;
export const freezeNow = t => { frameNow = t; };
const now = () => frameNow || performance.now();
const FLASH_MS = 700;
export const flashing = t => t != null && now() - t < FLASH_MS;

// A value as the inspector shows it. Numbers and percent strings both parse:
// "104%" stays 104%, a scale of 1.12 reads 112%, other numbers lose float noise.
// Never returns "undefined" / "NaN" / "null".
export function fmtVal(v, param = '') {
  if (v == null) return '';
  if (typeof v === 'boolean') return v ? 'on' : 'off';
  const s = String(v).trim(), pct = /^(-?\d+(?:\.\d+)?)\s*%$/.exec(s);
  const n = pct ? +pct[1] : (typeof v === 'number' || /^-?\d+(?:\.\d+)?$/.test(s) ? +s : NaN);
  if (!Number.isFinite(n)) return Array.isArray(v) ? v.map(x => fmtVal(x)).join(', ') : (typeof v === 'object' ? '' : s);
  const round = x => String(Math.round(x * 1000) / 1000);
  if (pct) return round(n) + '%';
  if (/scale|opacity|zoom/i.test(param) && Math.abs(n) <= 20) return round(n * 100) + '%';
  return round(n);
}

function views(state) { return state.views[state.mode] || (state.views[state.mode] = {}); }
function panels(state, MODES, kinds) {
  const M = MODES[state.mode]; const V = views(state); const out = [];
  for (const [name, spec] of Object.entries(M.panels)) if (kinds.includes(spec.kind)) out.push(V[name] || (V[name] = {}));
  return out;
}
function findTrack(tl, id) {
  if (!tl || !tl.tracks) return null;
  return tl.tracks.find(t => t.id === id || t.label === id) || null;
}
function findClip(tl, target) {
  for (const t of (tl && tl.tracks) || []) {
    if (target.track_id && !(t.id === target.track_id || t.label === target.track_id)) continue;
    const i = (t.clips || []).findIndex(c => c.id === target.clip_id || c.label === target.clip_id);
    if (i >= 0) return [t, i];
  }
  return [null, -1];
}

// Inspector diff: show what a tool changed, in place if the row exists.
function inspectorDiff(state, MODES, ev, fam) {
  const P = ev.payload || {}, T = ev.target || {};
  const kv = panels(state, MODES, ['kv'])[0]; if (!kv) return false;
  kv.blocks = kv.blocks || [];
  const param = T.param || P.param;
  const label = k => String(k).replace(/_/g, ' ').replace(/^./, c => c.toUpperCase());
  const set = (k, to, from) => {
    const row = kv.blocks.find(b => b.k && b.k.toLowerCase() === String(k).toLowerCase().replace(/_/g, ' '));
    const fx = (v, kk) => (/^normalize_/.test(String(ev.tool_name)) && /volume/i.test(String(kk)) && typeof v === 'number' ? 'x' + fmtVal(v) : fmtVal(v, kk));
    if (row) { if (from != null || row.v != null) row.from = from != null ? fx(from, k) : row.v; row.v = fx(to, k); row._flash = now(); return true; }
    return false;
  };
  if (!(P.key && P.key.ease)) {   /* an ease change has no value row to update: it gets its own row below */
    if (param != null && (P.to != null || P.value != null) && fmtVal(P.to ?? P.value, param) !== '') { if (set(param, P.to ?? P.value, P.from)) return true; }   /* a list / object value has no row text: rewriting a row with '' is no visible change */
    if (/^(x|y|pos_x|pos_y)$/i.test(String(param)) && kv.blocks.some(b => b.k && /^position$/i.test(b.k))) return true;   // xfOps wrote Position
  }
  // Unknown or unmapped: a small section for the latest call's payload (replaces the previous one).
  const i = kv.blocks.findIndex(b => b._live); if (i >= 0) kv.blocks.splice(i);
  // what changed first (param / from / to / effect / label / text), bookkeeping (timing, bounds, diffs) never
  const FIRST = ['param', 'from', 'to', 'value', 'effect', 'label', 'text', 'on', 'at', 'clip', 'track'];
  const HIDE = /^(dur_ms|duration_ms|duration|start|end|s|e|changed|mutated|result|images|ts|ts_ms|stage|elapsed_ms|progress|pct)$/;
  const rank = k => { const i = FIRST.indexOf(k); return i < 0 ? FIRST.length : i; };
  const rows = Object.entries(P).filter(([k, v]) => v != null && typeof v !== 'object' && !HIDE.test(k) && !RAW_ID.test(String(v))).sort((a, b) => rank(a[0]) - rank(b[0])).slice(0, 4);
  const val = P.to ?? P.value, head = param != null && val != null && fmtVal(val, param) !== '';
  const rest = head ? rows.filter(([k]) => !['param', 'from', 'to', 'value'].includes(k)).slice(0, 3) : rows.filter(([k]) => !(k === 'param' && param != null && val != null));   /* a param whose value has no row text (a list) is not a change by itself */
  if (!head && !rest.length && !P.key) return false;
  kv.blocks.push({ sec: ev.tool_name, _live: true });
  // the change itself leads: "Amount 0 → 0.35"; a keyframe's own key, "Scale @2s 100% → 125%"
  // normalize_loudness lands a linear volume factor (2.505 for +8 dB): marked "x2.505" so it never reads as LUFS or dB
  const fv = (v, k) => (/^normalize_/.test(String(ev.tool_name)) && /volume/i.test(String(k)) && typeof v === 'number' ? 'x' + fmtVal(v) : fmtVal(v, k));
  if (P.key) kv.blocks.push({ k: label(P.key.label), v: P.key.to, from: P.key.from, _live: true, _flash: now() });
  else if (head) kv.blocks.push({ k: label(param), v: fv(val, param), ...(P.from != null ? { from: fv(P.from, param) } : {}), _live: true, _flash: now() });
  for (const [k, v] of rest) kv.blocks.push({ k: label(k), v: fmtVal(v, k), _live: true, _flash: now() });
  return true;
}


// Grading (m15): the 3-way balance follows the call's own lift/gamma/gain numbers — color_wheels / grade
// effects (lift_r…gain_b), grade-graph nodes carrying the same params, and engines that send lift/gamma/gain
// directly. Each wheel's dot is its colour balance (channel deltas around the hue ring), the values are the
// R/G/B deltas from neutral, and the wheel the call changed turns ember. White-balance picks and shot matches
// draw the regions they sampled on the viewer.
const WHEEL_DEF = { lift: 0, gamma: 1, gain: 1 };
const HUE = { r: 100, g: 220, b: 340 };   // where each channel pulls on the ring (degrees, ccw from 3 o'clock)
function gradeOps(state, MODES, ev) {
  if (ev.phase !== 'complete') return false;
  const P = ev.payload || {}, tn = String(ev.tool_name).toLowerCase();
  const A = P.args || (state._gradeArgs && state._gradeArgs[ev.call_id]) || {};
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const prm = (A.params && typeof A.params === 'object' ? A.params : null) || (A.node && A.node.params) || A;
  const kind = String(A.kind ?? (A.node && A.node.kind) ?? '').toLowerCase();
  let hit = false;
  const wheelMode = Object.entries(MODES).find(([, M]) => Object.values(M.panels).some(sp => sp.kind === 'wheels'));
  if (wheelMode) {
    const V = state.views[wheelMode[0]] || (state.views[wheelMode[0]] = {});
    const key = Object.entries(MODES[wheelMode[0]].panels).find(([, sp]) => sp.kind === 'wheels')[0];
    const B = V[key] || (V[key] = {});
    const G = B._g || (B._g = { lift: { r: 0, g: 0, b: 0 }, gamma: { r: 1, g: 1, b: 1 }, gain: { r: 1, g: 1, b: 1 } });
    let changed = null; const touched = new Set();
    for (const w of ['lift', 'gamma', 'gain']) {
      for (const c of ['r', 'g', 'b']) {
        const v = numOf(prm[`${w}_${c}`]);
        if (v != null) { G[w][c] = v; changed = w; touched.add(w); }
      }
      const m = numOf(prm[w]);   // a master value: same delta on all three channels
      if (m != null && (kind === '' || /grade|wheel|lgg|primar/.test(kind) || tn === 'grade')) {
        const base = String(prm[w]).trim().match(/^[+-]/) ? WHEEL_DEF[w] + m : m;
        for (const c of ['r', 'g', 'b']) G[w][c] = base; changed = w; touched.add(w);
      }
    }
    // the Grade list: one row per adjusted control, updated in place, the latest change in ember
    const kvKey = Object.entries(MODES[wheelMode[0]].panels).find(([, sp]) => sp.kind === 'kv');
    if (kvKey) {
      const KV = V[kvKey[0]] || (V[kvKey[0]] = {}); KV.blocks = (KV.blocks || []).filter(b => b._grade);
      const setRow = (k, v) => { let b = KV.blocks.find(x => x.k === k); if (!b) { b = { k, _grade: true }; KV.blocks.push(b); } b.v = v; b.ch = true; b._flash = now(); for (const x of KV.blocks) if (x !== b && x._row !== ev.call_id) x.ch = false; b._row = ev.call_id; };
      const one = numOf(prm.amount ?? prm.value);
      const NAMES = { temperature: 'Temperature', tint: 'Tint', contrast: 'Contrast', saturation: 'Saturation', exposure: 'Exposure' };
      if (NAMES[kind] && one != null) { setRow(NAMES[kind], fmtVal(one)); hit = true; }
      if (A.clip_id != null && !KV.sub) KV.sub = String(A.clip_id);
    }
    // grade graph (m15 Nodes): node ids/kinds from the call, the output selected; plus rows for CDL / curve / scopes
    if (kvKey) {
      const KV = V[kvKey[0]]; KV.blocks = KV.blocks || [];
      const setRow = (k, v) => { let b = KV.blocks.find(x => x.k === k); if (!b) { b = { k, _grade: true }; KV.blocks.push(b); } b.v = v; b.ch = true; b._flash = now(); for (const x of KV.blocks) if (x !== b) x.ch = false; };
      const nodes = KV._nodes || (KV._nodes = []);
      const nid = A.node_id ?? (A.node && (A.node.id ?? A.node.node_id)) ?? A.id;
      const nk = (A.node && (A.node.kind ?? A.node.effect)) ?? A.kind;
      const addNode = (id, label) => { if (id == null) return; let n = nodes.find(x => x.id === String(id)); if (!n) { n = { id: String(id) }; nodes.push(n); } n.label = label ?? n.label ?? String(id); return n; };
      let graphHit = true;
      if (tn === 'set_grade_node' || tn === 'add_node') addNode(nid ?? nk, String(nid ?? nk));
      else if (tn === 'generate_lut_node') addNode(nid ?? 'lut', String(nid ?? 'lut'));
      else if (tn === 'remove_grade_node') { const k = nodes.findIndex(x => x.id === String(nid)); if (k >= 0) nodes.splice(k, 1); }
      else if (tn === 'set_grade_output') { for (const x of nodes) x.out = false; const n = addNode(nid ?? 'input'); n.out = true; }
      else if (tn === 'set_node_param') { addNode(nid); if (A.parameter != null) setRow(String(A.parameter).slice(0, 12), fmtVal(numOf(A.property && A.property.base) ?? A.property?.value ?? '')); }
      else if (tn === 'set_node_mask') { addNode(nid); setRow('Mask', A.mask ? String(A.mask.kind ?? A.mask.type ?? 'on') : 'off'); }
      else if (tn === 'connect_nodes' || tn === 'disconnect_nodes') { const a = A.from ?? A.source ?? A.src, b = A.to ?? A.target ?? A.dst; if (a != null) addNode(a); if (b != null) addNode(b); if (a != null && b != null) setRow('Edge', `${a} ${tn === 'connect_nodes' ? '→' : '×'} ${b}`); }
      else if (tn === 'create_shared_grade_graph') addNode(`shared:${A.id}`, `shared · ${A.id}`);
      else if (tn === 'remove_shared_grade_graph') { const k = nodes.findIndex(x => x.id === `shared:${A.id}`); if (k >= 0) nodes.splice(k, 1); }
      else if (tn === 'bind_shared_grade_graphs' && Array.isArray(A.graphs)) setRow('Shared', A.graphs.length ? A.graphs.map(String).join(', ') : 'none');
      else if (tn === 'apply_asc_cdl' && A.path) setRow('CDL', String(A.path).split(/[\\/]/).pop());
      else if (tn === 'set_color_curve' && Array.isArray(A.points)) { const ch = String(A.channel ?? 'm'), hl = { hue_sat: 'Hue vs Sat', hue_hue: 'Hue vs Hue', lum_sat: 'Lum vs Sat' }[ch]; setRow(hl ?? `Curve ${ch}`, `${A.points.length} pts`); const pts = A.points.filter(q => Array.isArray(q) && numOf(q[0]) != null && numOf(q[1]) != null).map(q => [numOf(q[0]), numOf(q[1])]); if (!hl && pts.length >= 2) KV._curve = { pts, ch }; }
      else if (tn === 'set_color_wheels') { const w = k => Array.isArray(A[k]) && A[k].length === 4 ? `${k[0].toUpperCase()}${k.slice(1, 3)} ${A[k].map(v => numOf(v) ?? '?').join('/')}` : null; setRow('Wheels', ['lift', 'gamma', 'gain', 'offset'].map(w).filter(Boolean).join(' · ') || (A.space ? String(A.space) : 'neutral')); }
      else if (tn === 'set_grade_scopes' && A.scopes && typeof A.scopes === 'object') { const g = A.scopes.groups, r = A.scopes.graphs; setRow('Scopes', [Array.isArray(g) ? `${g.length} groups` : null, r && typeof r === 'object' ? `${Object.keys(r).length} graphs` : null].filter(Boolean).join(' · ') || 'set'); }
      else if (tn === 'add_acescct_primaries') { const pr = A.params || {}; for (const k of ['lift', 'gamma', 'gain', 'offset']) if (numOf(pr[k]) != null) setRow(k[0].toUpperCase() + k.slice(1), fmtVal(pr[k])); }
      else graphHit = false;
      if (graphHit) {
        hit = true;
        const last = nodes.find(x => x.id === String(nid));
        KV.blocks = KV.blocks.filter(b => !b._nodes);
        if (nodes.length) KV.blocks.push({ sec: 'Nodes', _grade: true, _nodes: true }, { list: nodes.map((x, i) => `${String(i + 1).padStart(2, '0')}  ${x.label}`), sel: Math.max(0, nodes.indexOf(last ?? nodes.find(x => x.out) ?? nodes[nodes.length - 1])), _grade: true, _nodes: true });
        if (KV._curve) { KV.blocks = KV.blocks.filter(b => !b._curveBlk); KV.blocks.push({ curve: KV._curve.pts, ch: KV._curve.ch, _grade: true, _curveBlk: true }); }
      }
    }
    if (changed) {
      const fmt = d => (d > 0.0049 ? '+' : d < -0.0049 ? '-' : '') + Math.abs(d).toFixed(2);
      B.wheels = ['lift', 'gamma', 'gain'].map(w => {
        const d = ['r', 'g', 'b'].map(c => G[w][c] - WHEEL_DEF[w]), mean = (d[0] + d[1] + d[2]) / 3;
        let x = 0, y = 0; ['r', 'g', 'b'].forEach((c, k) => { const a = HUE[c] * Math.PI / 180; x += (d[k] - mean) * Math.cos(a); y += (d[k] - mean) * Math.sin(a); });
        const mag = Math.hypot(x, y), k = Math.min(.82, mag * 6) / (mag || 1);
        const em = w === changed;
        return { name: w[0].toUpperCase() + w.slice(1), dot: [x * k, y * k], em, ring: em && Math.abs(mean) > 1e-6 ? (mean > 0 ? 70 : 250) : null, vals: d.map(fmt) };
      });
      B._flash = now(); hit = true;
      if (kvKey) { const KV = V[kvKey[0]]; for (const W of B.wheels) if (touched.has(W.name.toLowerCase())) { let b = KV.blocks.find(x => x.k === W.name); if (!b) { b = { k: W.name, _grade: true }; KV.blocks.push(b); } b.v = W.vals.join(' '); b.ch = W.em; b._flash = now(); } for (const x of KV.blocks) if (!touched.has(String(x.k).toLowerCase())) x.ch = false; }
    }
  }
  // regions the call sampled: the neutral patch of a white-balance pick, the source/reference of a match
  const [mode, V2] = modeViews(state, MODES, ev);
  const vw = V2.viewer || (V2.viewer = {});
  const box = (r, label) => Array.isArray(r) && r.length === 4 && r.every(v => numOf(v) != null) ? { rect: r.map(numOf), label } : null;
  const regs = [];
  if (tn === 'pick_white_balance' && [A.x, A.y, A.w, A.h].every(v => numOf(v) != null)) regs.push(box([A.x, A.y, A.w, A.h], 'neutral'));
  if (tn === 'match_grade' || tn === 'match_color') { const s1 = box(A.source_region, 'source'), s2 = box(A.ref_region, A.ref_clip_id ? `ref · ${A.ref_clip_id}` : 'ref'); if (s1) regs.push(s1); if (s2) regs.push(s2); }
  if (regs.length) { vw.overlay = (vw.overlay || []).filter(o => o._kind !== tn).concat(regs.filter(Boolean).map(o => ({ ...o, _call: ev.call_id, _kind: tn }))); hit = true; }
  if ((tn === 'match_grade' || tn === 'match_color' || tn === 'pick_white_balance') && !regs.length) { vw.wipe = 1; hit = true; }
  // GUI-FIX-1: what the pick / match computed, in the Grade list of the screen it lands on and the Color scopes' stats line
  if (tn === 'match_grade' || tn === 'match_color' || tn === 'pick_white_balance') {
    const tl = timelineView(state, MODES), nm = id => { if (id == null) return null; const [t, i] = tl ? findClip(tl, { clip_id: id }) : [null, -1]; const lb = t ? t.clips[i].label : null; return lb && !RAW_ID.test(String(lb)) ? lb : (RAW_ID.test(String(id)) ? null : String(id)); };
    const rows = colourRows(tn, A, R, nm(A.ref_clip_id));
    const K = (MODES[mode] && Object.entries(MODES[mode].panels).find(([, sp]) => sp.kind === 'kv')) || null;
    if (rows.length && K) { const KV = V2[K[0]] || (V2[K[0]] = {}); KV.blocks = (KV.blocks || []).filter(b => !b._cread); for (const b of KV.blocks) b.ch = false; const t = now(); KV.blocks.push({ sec: tn, _grade: true, _cread: true }, ...rows.map(([k, v]) => ({ k, v, ch: true, _grade: true, _cread: true, _flash: t }))); hit = true; }
    scopeStats(state, MODES, scopeLine(tn, R));
  }
  return hit;
}
function scopeStats(state, MODES, text) {
  if (!text) return;
  for (const [m, M] of Object.entries(MODES)) for (const [k, sp] of Object.entries(M.panels)) if (sp.kind === 'scopes') { const V = state.views[m] || (state.views[m] = {}); (V[k] || (V[k] = {})).stats = { text, _flash: now() }; }
}

// Graphics (m09): HUD frames, detection boxes, counters, prompt boxes, shapes and paths land on the viewer
// at the geometry the call carries (normalized rects top-left origin; pixel offsets mapped through the session
// size), the layer joins the Inspector's Shape layers list, and layer styles write their own rows.
function graphicsOps(state, MODES, ev) {
  const P = ev.payload || {}, tn = String(ev.tool_name).toLowerCase();
  if (ev.phase === 'start') { (state._gfxArgs || (state._gfxArgs = {}))[ev.call_id] = P.args || {}; return false; }
  if (ev.phase !== 'complete') return false;
  const A = P.args || (state._gfxArgs && state._gfxArgs[ev.call_id]) || {}; delete (state._gfxArgs || {})[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const [mode, V] = modeViews(state, MODES, ev);
  const vw = V.viewer || (V.viewer = {});
  const S = state.session || {}, W = S.width, H = S.height;
  const rect4 = r => Array.isArray(r) && r.length === 4 && r.every(v => numOf(v) != null) ? r.map(numOf) : null;
  const add = o => { vw.overlay = (vw.overlay || []).filter(x => x._call !== ev.call_id).concat([{ ...o, _call: ev.call_id, _kind: tn }]); };
  let hit = false, layer = null;
  if (tn === 'add_detection_box' && [A.x, A.y, A.w, A.h].every(v => numOf(v) != null)) {
    layer = `det_${A.label ?? 'box'}`; add({ rect: [A.x, A.y, A.w, A.h].map(numOf), label: [A.label, numOf(A.confidence) != null ? numOf(A.confidence).toFixed(2) : null].filter(v => v != null).join(' '), em: true }); hit = true;
  } else if (tn === 'add_box_expand') {
    const a = rect4(A.from), b = rect4(A.to) || [0, 0, 1, 1];
    if (a) { add({ rect: a, label: A.label ?? null, em: true, to: b }); hit = true; layer = 'box_expand'; }
  } else if (tn === 'add_hud_frame') { add({ brackets: true, label: A.label ?? 'BOXES' }); hit = true; layer = 'hud'; }
  else if (tn === 'add_counter' && (numOf(A.to) != null || numOf(A.from) != null)) {
    const x = .5 + (W && numOf(A.x) != null ? numOf(A.x) / W : 0), y = .5 + (H && numOf(A.y) != null ? numOf(A.y) / H : 0);
    const f = v => (A.prefix ?? '') + (numOf(v) ?? 0).toFixed(numOf(A.decimals) ?? 0) + (A.suffix ?? '');
    add({ text: `${f(A.from ?? 0)} → ${f(A.to)}`, at: [x, y] }); hit = true; layer = 'counter';
  } else if (tn === 'add_prompt_box' && A.text != null) {
    const y = .5 + (numOf(A.y) != null ? numOf(A.y) / 720 : 0);   // engine: px offset from centre at 720p
    add({ text: String(A.text).slice(0, 48), at: [.5, Math.min(.92, Math.max(.08, y))], box: true }); hit = true; layer = 'prompt';
  } else if (tn === 'add_shape' && W && H) {
    const w = (numOf(A.width) ?? 200) / W, h = (numOf(A.height) ?? 200) / H, sh = String(A.shape ?? 'rect');
    add({ shape: sh, rect: [.5 - w / 2, .5 - h / 2, w, h] }); hit = true; layer = sh;
  } else if ((tn === 'add_path_shape' || tn === 'set_path' || tn === 'add_path_key') && Array.isArray(A.points) && A.points.length >= 2) {
    let pts = A.points.map(q => Array.isArray(q) ? [numOf(q[0]), numOf(q[1])] : (q && typeof q === 'object' ? [numOf(q.x), numOf(q.y)] : null)).filter(q => q && q[0] != null && q[1] != null);
    if (pts.length >= 2) {
      const norm = pts.every(([x, y]) => Math.abs(x) <= 1.5 && Math.abs(y) <= 1.5);
      if (!norm && W && H) pts = pts.map(([x, y]) => [x / W, y / H]);
      if (norm || (W && H)) { add({ path: pts, closed: !!A.closed }); hit = true; layer = 'path'; }
    }
  } else if (tn === 'add_glitch_confetti') {
    const n = Math.min(60, numOf(A.squares) ?? 24), seed = numOf(A.seed) ?? 1;
    add({ squares: n, seed }); hit = true; layer = 'confetti';
  }
  // Inspector: Shape layers list and Layer style rows
  const kvp = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'kv');
  if (kvp) {
    const K = V[kvp[0]] || (V[kvp[0]] = {}); K.blocks = (K.blocks || []).filter(b => b._gfx);
    const rid = R.clip_id ?? R.id ?? A.clip_id, nm = String(rid != null && !RAW_ID.test(String(rid)) ? rid : (typeof A.label === 'string' && A.label.trim() ? A.label.trim().slice(0, 24) : layer ?? ''));   // never a raw engine id
    if (layer && nm) {
      // the rows the m09 mockup draws above the list (Layer style: Stroke, Stroke color, Opacity), only from this layer's own args
      const hex = c => Array.isArray(c) && c.length >= 3 && c.every(x => numOf(x) != null) ? '#' + c.slice(0, 3).map(x => { const n = numOf(x); return Math.round(Math.max(0, Math.min(255, n <= 1 ? n * 255 : n))).toString(16).padStart(2, '0'); }).join('') : (typeof c === 'string' && c.length < 24 ? c : null);
      const sw = numOf(A.stroke_width), sc = hex(A.stroke_color ?? (/^add_(hud_frame|detection_box)$/.test(tn) ? A.color : undefined)), op = numOf(A.opacity);
      const rows = [sw != null && { k: 'Stroke', v: `${fmtVal(sw)} px` }, sc && { k: 'Stroke color', v: sc }, op != null && { k: 'Opacity', v: `${Math.round(op <= 1 ? op * 100 : op)}%` }].filter(Boolean).map(x => ({ ...x, _gfx: true, _layerRow: true, _flash: now() }));
      K.blocks = K.blocks.filter(b => !b._layerRow && !b._layerSec);
      if (rows.length) K.blocks.unshift({ sec: 'Layer style', _gfx: true, _layerSec: true }, ...rows);
      K.sub = `${nm} - ${layer === 'prompt' ? 'prompt box' : layer === 'counter' ? 'counter' : 'shape layer'}`;
      let li = K.blocks.find(b => b._layers);
      if (!li) { K.blocks.push({ sec: 'Shape layers', _gfx: true }); li = { list: [], sel: 0, _gfx: true, _layers: true }; K.blocks.push(li); }
      if (!li.list.includes(nm)) li.list.push(nm); li.sel = li.list.indexOf(nm); if (li.list.length > 8) li.list.splice(0, li.list.length - 8);
    }
    if ((tn === 'set_layer_style' || tn === 'add_shape_modifier' || tn === 'add_modifier') && (A.params || A.style || A.kind)) {
      const sec = tn === 'set_layer_style' ? 'Layer style' : 'Modifiers';
      let at = K.blocks.findIndex(b => b.sec === sec); if (at < 0) { K.blocks.unshift({ sec, _gfx: true }); at = 0; }
      const rows = Object.entries(A.params || {}).filter(([, v]) => numOf(v) != null).slice(0, 5).map(([k, v]) => ({ k: `${A.style ?? A.kind ?? ''} ${k}`.trim(), v: fmtVal(v, k), ch: true, _gfx: true, _flash: now() }));
      if (!rows.length) rows.push({ k: String(A.style ?? A.kind), v: 'on', ch: true, _gfx: true, _flash: now() });
      for (const b of K.blocks) b.ch = false;
      K.blocks.splice(at + 1, 0, ...rows.filter(r => !K.blocks.some(b => b.k === r.k && (b.v = r.v, b.ch = true, b._flash = now()))));
      if (A.clip_id != null) K.sub = `${A.clip_id} - shape layer`; hit = true;
    }
    if (tn === 'remove_layer_style' || tn === 'remove_shape_modifier' || tn === 'remove_modifier' || tn === 'clear_path_keys') { K.blocks.push({ k: tn.replace(/_/g, ' '), v: String(A.effect_id ?? A.clip_id ?? ''), ch: true, _gfx: true, _flash: now() }); hit = true; }
  }
  // the layer's clip on its track when the engine's telemetry did not already report the insert
  if (P.s == null && P.clip == null && A.track_id != null && numOf(A.start) != null) { P.clip = String(R.clip_id ?? R.id ?? layer ?? tn); P.s = numOf(A.start); P.e = numOf(A.start) + (numOf(A.duration) ?? 4); P.track = A.track_id; P.label = P.label ?? (RAW_ID.test(P.clip) ? (typeof A.label === 'string' && A.label.trim() ? A.label.trim().slice(0, 24) : layer ?? P.clip) : P.clip); ev.payload = P; }
  if (hit) ev._kvDone = true;
  return hit;
}

function hud(state, ev, fam, text) {
  const shown = fam === 'inspect' ? fam : ((state.explicitMode ? null : inferMode(String(ev.tool_name))) || state.mode || fam);   // the tag names the screen, not the router family
  const SHORT = { tracking: 'track', grading: 'grade', modelling: 'model', lighting: 'light', scene3d: 'scene', mixing: 'mix', effects: 'fx', particles: 'particles', captions: 'captions', graphics: 'graphics' };
  state.hud = { fam: SHORT[shown] || shown, family: fam, tool: ev.tool_name, text: text || ev.tool_name, phase: ev.phase, t: now() };
}

// Timeline effects of edit / ingest / effects / audio / text tools.
// The timeline a call lands on: the current mode's, else the edit mode's
// (so a real session that hops modes still builds one timeline).
function timelineView(state, MODES) {
  if (state.liveTL) return state.liveTL;
  const here = panels(state, MODES, ['timeline'])[0];
  if (here && here.tracks && here.tracks.length) return here;       // a scenario's own timeline
  // cold (engine) session: ONE timeline object, installed in every mode that draws a timeline,
  // so hopping modes as the tools change never shows a different or empty timeline
  const tl = state.liveTL = here || {};
  for (const [mode, M] of Object.entries(MODES)) for (const [name, spec] of Object.entries(M.panels)) {
    if (spec.kind !== 'timeline') continue;
    const V = state.views[mode] || (state.views[mode] = {});
    if (!V[name] || !(V[name].tracks && V[name].tracks.length)) V[name] = tl;
  }
  return tl;
}
// A real session starts empty: tracks and clips are created when a call
// first names them. New tracks append below existing ones (fixed row height
// inside a fixed panel), so nothing already on screen moves.
function ensureTrack(tl, id, hint) {
  tl.tracks = tl.tracks || [];
  let t = findTrack(tl, id);
  if (!t && id != null) {
    const audio = /audio|^a\d|music|dialog|sfx|stem/i.test(String(hint || id));
    const n = tl.tracks.filter(x => (x.kind || 'v') === (audio ? 'a' : 'v')).length + 1;
    t = { id, label: /^track\d+$/i.test(id) || !isNaN(id) ? (audio ? 'A' : 'V') + n : String(id), kind: audio ? 'a' : 'v', clips: [], _flash: now() };
    tl.tracks.push(t); tl._auto = tl._auto ?? !tl.ruler;
  }
  return t;
}
// An auto (engine-built) timeline's ruler grows to fit its clips; a scenario's ruler is left alone.
function fitRuler(tl) {
  if (!tl._auto) return;
  let e = 0; for (const t of tl.tracks || []) for (const c of t.clips || []) e = Math.max(e, +c.e || 0);
  const t1 = Math.max(10, Math.ceil(e * 1.15));
  if (!tl.ruler || tl.ruler.t1 < t1) tl.ruler = { t0: 0, t1, step: t1 <= 12 ? 2 : t1 <= 30 ? 5 : t1 <= 90 ? 10 : 30, edge: true };
}
// Clip names a person reads: a file name when the call carried a path, the text of a text clip,
// the engine's own label when it is a real name, else "V1 · Clip 02" (never a raw engine id).
const RAW_ID = /^(c|clip|t|track|fx|effect|node|n|layer|l|asset|media)[_-]?\d+(?:[_-][a-z]{0,5}\d+|[_-][a-z])*$/i;   /* also an id derived from one: c475_s0, clip2_b */
// The rail's copy of a call's args: a raw clip id becomes that clip's name on the live timeline, or is
// left out when the window has no name for it (never shown raw, never invented)
// A picked call's state diff (the Inspector's Diff section): before -> after for each target field the engine
// reported in its complete payload (`changed`, spectator.rs diff()), else the vocabulary's from/to. Clip start /
// duration read as the clip's In / Out; keyframe lists as the keys that changed; clips and tracks by their
// names on the live timeline (a raw id with no name is left out). Nothing is filled in that the engine did not send.
const secs = v => { const n = +v; return Number.isFinite(n) ? `${Math.round(n * 100) / 100}s` : ''; };
const DASH = '—';
function callDiff(state, ev) {
  const P = ev.payload || {}, T = ev.target || {}, C = P.changed && typeof P.changed === 'object' ? P.changed : null, out = [];
  const tl = state.liveTL, cap = s => s.replace(/_/g, ' ').replace(/^./, x => x.toUpperCase());
  const push = (k, from, to) => { if (from === to || (!from && !to)) return; out.push({ k, from: from || DASH, v: to || DASH }); };
  const clipName = id => { if (id == null) return null; if (!RAW_ID.test(String(id))) return String(id); const [t, i] = tl ? findClip(tl, { clip_id: id }) : [null, -1]; const lb = t ? t.clips[i].label : null; return lb && !RAW_ID.test(String(lb)) ? String(lb) : null; };
  const trackName = id => { if (id == null) return null; if (!RAW_ID.test(String(id))) return String(id); const t = tl ? findTrack(tl, id) : null; return t && t.label && !RAW_ID.test(String(t.label)) ? String(t.label) : null; };
  const keyStr = (k, p) => k && typeof k === 'object' && k.t != null ? `${secs(k.t)} ${fmtVal(k.v, p)}`.trim() : '';
  const keysDiff = (label, p, a, b) => {
    const A = Array.isArray(a) ? a : [], B = Array.isArray(b) ? b : [], at = new Map(A.map(k => [+k.t, k]));
    for (const k of B) { const o = at.get(+k.t); at.delete(+k.t); const f = o ? keyStr(o, p) : '', t = keyStr(k, p); if (f !== t) push(`${label} key`, f, t); }
    for (const o of at.values()) push(`${label} key`, keyStr(o, p), '');
  };
  if (C) {
    const F = C.clip && typeof C.clip === 'object' ? C.clip : {};
    // clip timing as In / Out (timeline seconds); needs the clip's own s/e from the payload to place "after"
    const sF = F.start, dF = F.duration, s1 = numOf(P.s), e1 = numOf(P.e);
    if ((sF || dF) && s1 != null && e1 != null) {
      const s0 = sF ? numOf(sF[0]) : s1, d0 = dF ? numOf(dF[0]) : e1 - s1;
      if (s0 != null) push('In', secs(s0), secs(s1));
      if (s0 != null && d0 != null) push('Out', secs(s0 + d0), secs(e1));
    } else { if (sF) push('Start', secs(sF[0]), secs(sF[1])); if (dF) push('Duration', secs(dF[0]), secs(dF[1])); }
    for (const [k, ft] of Object.entries(F)) {
      if (k === 'start' || k === 'duration' || !Array.isArray(ft)) continue;
      const segs = k.split('.'), last = segs[segs.length - 1];
      if (/\]$/.test(k) || /^(id|expr|bez|ease)$/.test(last)) continue;
      const emp = x => x == null || (Array.isArray(x) && !x.length); if (last !== 'keys' && emp(ft[0]) && emp(ft[1])) continue;   // a list that only appeared / emptied (its items are their own rows)            // list containers and bookkeeping
      if (/\]\.kind$/.test(k)) { push(cap(segs[0].replace(/\[.*$/, '').replace(/s$/, '')), fmtVal(ft[0]), fmtVal(ft[1])); continue; }   // effect added / removed
      const words = segs.map(s => s.replace(/\[.*\]$/, '')).filter(s => !/^(props|params|base|effects|masks|source)$/.test(s) || segs.length === 1);
      if (last === 'keys') { const p = words.filter(w => w !== 'keys').pop() || 'value'; keysDiff(cap(p), p, ft[0], ft[1]); continue; }
      const p = words[words.length - 1] || last, lbl = cap(k.startsWith('source.') ? 'source ' + words.join(' ') : words.join(' '));
      const fv = x => x != null && typeof x === 'object' ? (Array.isArray(x) ? (x.length && typeof x[0] !== 'object' ? fmtVal(x) : `[${x.length}]`) : '') : /(^|_)(point|time|start|end|in|out)$/.test(p) ? secs(x) : fmtVal(x, p);
      push(lbl, fv(ft[0]), fv(ft[1]));
    }
    for (const a of C.clips_added || []) { const n = clipName(a.clip_id) || (a.clip_id === T.clip_id || a.clip_id === P.clip ? (P.label && !RAW_ID.test(String(P.label)) ? String(P.label) : null) : null); if (n) push('Clip', '', n); }
    for (const a of C.clips_removed || []) { const n = clipName(a.clip_id); if (n) push('Clip', n, ''); }
    for (const m of C.clips_moved || []) { const n = clipName(m.clip_id), f = trackName(m.from), t = trackName(m.to); if (f && t) push(n ? `${n} track` : 'Track', f, t); }
    for (const t of C.tracks_added || []) { const n = trackName(t); if (n) push('Track', '', n); }
    for (const t of C.tracks_removed || []) { const n = trackName(t); if (n) push('Track', n, ''); }
    if (Array.isArray(C.duration)) push('Length', secs(C.duration[0]), secs(C.duration[1]));
  } else {
    const param = T.param || P.param;
    if (param != null && P.from !== undefined && P.to !== undefined) push(cap(String(param)), fmtVal(P.from, param), fmtVal(P.to, param));
  }
  return out.length ? out.slice(0, 10) : null;
}
const CLIP_KEYS = ['clip_id', 'clip', 'clip_a', 'clip_b', 'matte_clip_id', 'from_clip', 'to_clip', 'target_clip_id', 'ref_clip_id'];
function railArgs(state, a) {
  if (!a || typeof a !== 'object' || Array.isArray(a)) return a;
  let o = null;
  for (const k of CLIP_KEYS) { const v = a[k]; if (typeof v !== 'string' || !RAW_ID.test(v)) continue;
    const [t, i] = state.liveTL ? findClip(state.liveTL, { clip_id: v }) : [null, -1], lb = t ? t.clips[i].label : null;
    o = o || { ...a }; if (lb && !RAW_ID.test(String(lb))) o[k] = String(lb); else delete o[k]; }
  for (const k of ['asset_id', 'asset']) { const v = a[k]; if (typeof v !== 'string' || !RAW_ID.test(v)) continue;   /* a media id reads as its file name, else is left out */
    const nm = state.assets && state.assets[v]; o = o || { ...a }; if (nm) o[k] = nm; else delete o[k]; }
  for (const k of ['track_id', 'track']) { const v = a[k]; if (typeof v !== 'string' || !RAW_ID.test(v)) continue;   // raw track ids the same way ('track1' -> 'V1')
    const t = state.liveTL ? findTrack(state.liveTL, v) : null, lb = t ? t.label : null;
    o = o || { ...a }; if (lb && !RAW_ID.test(String(lb))) o[k] = String(lb); else delete o[k]; }
  return o || a;
}
// Media by name: import_media's file name is kept under the asset id its result returns, so a clip placed by
// asset_id (add_clip source_type media) reads "shot_0009.mp4", not "asset2" or "V1 · Clip 01"; the rail's asset_id
// arg reads the same name. The call's payload gets the name as its path (a copy; the event itself is not touched).
function assetNames(ev, state) {
  const P = ev.payload || {}, A = P.args || {}, base = p => String(p).split(/[\\/]/).pop();
  if (ev.phase === 'start') {
    if (typeof A.path === 'string' && A.path) (state._paths || (state._paths = {}))[ev.call_id] = base(A.path);
    if (typeof A.asset_id === 'string') (state._assetArg || (state._assetArg = {}))[ev.call_id] = A.asset_id;
    return ev;
  }
  if (ev.phase === 'progress') return ev;
  const nm = state._paths && state._paths[ev.call_id], R = P.result;
  if (nm && R && typeof R.asset_id === 'string') (state.assets || (state.assets = {}))[R.asset_id] = nm;
  if (state._paths) delete state._paths[ev.call_id];
  const aid = state._assetArg && state._assetArg[ev.call_id]; if (state._assetArg) delete state._assetArg[ev.call_id];
  const an = aid != null && state.assets && state.assets[aid];
  return an && P.path == null ? { ...ev, payload: { ...P, path: an } } : ev;
}
// A split's right-hand piece: the source clip's name and a split ordinal, "shot_0009.mp4 · split 1" (a second
// split of the same source is "· split 2"); the engine's new id is never shown
function splitName(tl, label) {
  const b = String(label ?? '').replace(/ · split \d+$/, '');
  const n = (tl._splitN || (tl._splitN = {}))[b] = (tl._splitN[b] || 0) + 1;
  return b ? `${b} · split ${n}` : label;
}
function clipLabel(P, t, fam) {
  const A = P.args || {};
  if (fam === 'text' && P.text) return P.text;
  if (P.label && !RAW_ID.test(String(P.label))) return String(P.label);
  const path = P.path ?? P.file ?? P.src ?? A.path ?? A.file ?? A.source ?? A.src;
  if (typeof path === 'string' && path) return path.split(/[\\/]/).pop();
  return `${t.label} · Clip ${String((t.clips || []).length + 1).padStart(2, '0')}`;
}
function insertClip(tl, t, c) {
  const s = +c.s, e = +c.e;
  // overwrite edit: whatever sat under the new clip is trimmed back to it
  t.clips = (t.clips || []).filter(k => !(k.s >= s && k.e <= e) || k.id === c.id);
  for (const k of t.clips) { if (k.id === c.id) continue; if (k.s < s && k.e > s) k.e = s; else if (k.s < e && k.e > e) k.s = e; }
  const i = t.clips.findIndex(k => k.id != null && k.id === c.id);
  if (i >= 0) Object.assign(t.clips[i], c); else t.clips.push(c);
  t.clips.sort((a, b) => a.s - b.s);
}

// Timeline edit ops that act on whole clips/tracks (duplicate, swap, loop, freeze, speed, group,
// distribute, blend/echo/flow-blur badges, camera shake layer, track rename/lock, project size). Every
// change comes from the call's own args (and the new id in its result); a clip the window has never seen
// is left alone, so nothing is guessed.
function editOps2(state, MODES, tl, ev, A) {
  const n = String(ev.tool_name).toLowerCase(), P = ev.payload || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const at = id => findClip(tl, { clip_id: id });
  const nid = (fb) => String(R.clip_id ?? R.id ?? fb);
  const badge = (c, b, re) => { c.fx = [...(c.fx || []).filter(x => !(re && re.test(x)) && x !== b), b]; c._flash = now(); };
  const N = numOf;
  if (n === 'project_set') {
    const w = N(A.width), h = N(A.height), fps = N(A.fps);
    if (w) state.session.width = w; if (h) state.session.height = h; if (fps) state.session.fps = fps;
    const [, V] = modeViews(state, MODES, ev); const vw = V.viewer || (V.viewer = {});
    const W = state.session.width, H = state.session.height, F = state.session.fps;
    if (W && H) { vw.res = `${W} x ${H}${F ? ` ${fmtVal(F)}p` : ''}`; vw._flash = now(); return true; }
    return false;
  }
  if (n === 'set_track') {
    const t = findTrack(tl, A.track_id); if (!t) return false;
    if (typeof A.name === 'string' && A.name.trim()) t.label = A.name.trim().slice(0, 16);
    if (A.locked != null || A.lock != null) t.lock = !!(A.locked ?? A.lock);
    t._flash = now(); return true;
  }
  if (n === 'delete_track') { const i = (tl.tracks || []).findIndex(t => t.id === A.track_id || t.label === A.track_id); if (i < 0) return false; tl.tracks.splice(i, 1); return true; }
  if (n === 'add_camera_shake') {
    const s = N(A.start), d = N(A.duration); if (s == null || A.track_id == null) return false;
    const t = ensureTrack(tl, A.track_id); const c = { id: nid('shake'), label: 'camera shake', s, e: s + (d ?? 2), fx: ['impact_shake'], _flash: now() };
    if (Array.isArray(A.hits)) c.keys = A.hits.map(x => N(Array.isArray(x) ? x[0] : x && typeof x === 'object' ? x.t ?? x.time : x)).filter(x => x != null).map(x => ({ t: x < s ? s + x : x }));
    insertClip(tl, t, c); t._flash = now(); return true;
  }
  if (n === 'group_clips' && Array.isArray(A.clip_ids) && A.clip_ids.length) {
    const got = A.clip_ids.map(id => at(id)).filter(([t]) => t); if (!got.length) return false;
    const kids = got.map(([t, i]) => ({ track: t.id, clip: t.clips[i] }));
    const s = Math.min(...kids.map(k => k.clip.s)), e = Math.max(...kids.map(k => k.clip.e));
    for (const k of kids) { const t = findTrack(tl, k.track); t.clips = t.clips.filter(c => c !== k.clip); }
    const t = ensureTrack(tl, A.track_id ?? kids[0].track);
    insertClip(tl, t, { id: nid('group'), label: `group · ${kids.length} clips`, s, e, cls: 'grp', _kids: kids, _flash: now() }); return true;
  }
  if (n === 'distribute_clips') {
    const t = findTrack(tl, A.track_id); if (!t || !(t.clips || []).length) return false;
    const gap = N(A.gap) ?? 0; let x = N(A.start) ?? t.clips[0].s;
    for (const c of [...t.clips].sort((a, b) => a.s - b.s)) { const d = c.e - c.s; c.s = x; c.e = x + d; x = c.e + gap; c._flash = now(); }
    return true;
  }
  if (n === 'swap_clips') {
    const [ta, ia] = at(A.clip_a), [tb, ib] = at(A.clip_b); if (!ta || ta !== tb) return false;
    let a = ta.clips[ia], b = ta.clips[ib]; if (b.s < a.s) [a, b] = [b, a];
    const gap = b.s - a.e, da = a.e - a.s, db = b.e - b.s, s0 = a.s;
    b.s = s0; b.e = s0 + db; a.s = b.e + gap; a.e = a.s + da; a._flash = b._flash = now(); ta.clips.sort((x, y) => x.s - y.s); return true;
  }
  const [t, i] = at(A.clip_id ?? (ev.target || {}).clip_id); if (!t) return false;
  const c = t.clips[i];
  if (n === 'duplicate_clip') {
    const s = N(A.start); if (s == null) return false; const tt = A.track_id != null ? ensureTrack(tl, A.track_id) : t;
    const cp = { ...c, id: nid(c.id + '_dup'), s, e: s + (c.e - c.s), _flash: now() }; insertClip(tl, tt, cp); tt._flash = now(); return true;
  }
  if (n === 'loop_clip') {
    const k = Math.min(32, Math.max(1, Math.round(N(A.times) ?? 1))), gap = N(A.gap) ?? 0, d = c.e - c.s; let s = c.e + gap;
    const ids = Array.isArray(R.clip_ids) ? R.clip_ids : [];
    for (let j = 0; j < k; j++, s += d + gap) insertClip(tl, t, { ...c, id: String(ids[j] ?? `${c.id}_loop${j + 1}`), s, e: s + d, _flash: now() });
    return true;
  }
  if (n === 'ungroup_clip') {
    if (!c._kids) return false; t.clips.splice(i, 1);
    for (const k of c._kids) insertClip(tl, ensureTrack(tl, k.track), { ...k.clip, _flash: now() }); return true;
  }
  if (n === 'freeze_frame') { badge(c, A.at != null ? `freeze ${fmtVal(N(A.at))}s` : 'freeze', /^freeze/); return true; }
  if (n === 'set_speed') { const v = N(A.speed); if (v == null) return false; badge(c, `${fmtVal(v)}×`, /^[\d.]+×$|^ramp$/); return true; }
  if (n === 'rate_stretch') { const d = N(A.duration); if (d == null) return false; c.e = c.s + d; c._flash = now(); return true; }
  if (n === 'set_speed_ramp') {
    const keys = Array.isArray(A.keys) ? A.keys.map(k => N(Array.isArray(k) ? k[0] : k && typeof k === 'object' ? k.t : null)).filter(x => x != null) : [];
    if (!keys.length) { c.fx = (c.fx || []).filter(x => x !== 'ramp'); c.keys = (c.keys || []).filter(k => !k._ramp); c._flash = now(); return true; }
    badge(c, 'ramp', /^[\d.]+×$/); c.keys = [...(c.keys || []).filter(k => !k._ramp), ...keys.map(x => ({ t: c.s + x, _ramp: true }))]; return true;
  }
  if (n === 'set_blend') { if (!A.mode) return false; badge(c, String(A.mode), /^(normal|add|multiply|screen|overlay)$/); return true; }
  if (n === 'set_echo') { badge(c, N(A.count) != null ? `echo ×${Math.round(N(A.count))}` : 'echo', /^echo/); return true; }
  if (n === 'set_flow_blur') { badge(c, 'flow blur', /^flow blur/); return true; }
  if (n === 'remove_audio_effect') { const fx = A.effect_id ?? A.effect ?? A.name; if (fx == null || !(c.fx || []).length) return false; c.fx = c.fx.filter(x => x !== fx); c._flash = now(); return true; }
  return false;
}

// Mask / roto family (m12): the mask's shape on the viewer (path points or rect/ellipse box), its keys on
// Mask keys (one lane per mask property, diamonds at the keyed frames; propagation fills the run between
// its keys), Refine matte / Key / Quality rows from the call's own args or result. Nothing is guessed.
function maskOps(state, MODES, ev) {
  const P = ev.payload || {}, tn = String(ev.tool_name).toLowerCase();
  if (ev.phase === 'start') (state._maskArgs || (state._maskArgs = {}))[ev.call_id] = P.args || {};
  const A = P.args || (state._maskArgs && state._maskArgs[ev.call_id]) || {};
  if (ev.phase !== 'complete') return false;
  delete (state._maskArgs || {})[ev.call_id];
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const [mode, V] = modeViews(state, MODES, ev); const PP = MODES[mode] ? MODES[mode].panels : {};
  const vw = V.viewer || (V.viewer = {});
  const mid = String(A.mask_id ?? R.mask_id ?? A.name ?? (tn === 'set_mask' || tn === 'remove_mask' ? 'mask' : 'mask'));
  const kvp = Object.entries(PP).find(([, sp]) => sp.kind === 'kv'), K = kvp ? (V[kvp[0]] || (V[kvp[0]] = {})) : null;
  const kp = Object.entries(PP).find(([, sp]) => sp.kind === 'keys'), KK = kp ? (V[kp[0]] || (V[kp[0]] = {})) : null;
  const fps = numOf(state.session.fps) ?? 24;
  let hit = false;
  const rowsSet = (sec, rows) => { if (!K) return; K.blocks = (K.blocks || []).filter(b => b._mask); let at = K.blocks.findIndex(b => b.sec === sec);
    if (at < 0) { K.blocks.push({ sec, _mask: true }); at = K.blocks.length - 1; }
    for (const [k, v] of rows) { if (v == null || v === '') continue; let r = K.blocks.find(b => b.k === k && b._sec === sec); const vv = String(v);
      if (!r) { r = { k, _mask: true, _sec: sec }; let end = at + 1; while (end < K.blocks.length && K.blocks[end].sec === undefined) end++; K.blocks.splice(end, 0, r); }
      if (r.v != null && r.v !== vv) r.from = r.v; r.v = vv; r._flash = now(); hit = true; } };
  const lane = (label, t, opt = {}) => { if (!KK) return; KK.lanes = (KK.lanes || []).filter(l => l._mask); let L = KK.lanes.find(l => l.label === label);
    if (!L) { L = { label, keys: [], size: 4.5, keyColor: '#f1e2c2', _mask: true }; KK.lanes.push(L); } if (t != null && !L.keys.includes(t)) L.keys.push(t); L.keys.sort((a, b) => a - b); Object.assign(L, opt);
    if (KK.lanes.length > 4) KK.lanes.splice(0, KK.lanes.length - 4);
    const all = KK.lanes.flatMap(l => [...(l.keys || []), ...((l.area || []).map(p => p[0]))]); if (all.length) { const lo = Math.min(...all), hi = Math.max(...all), pad = Math.max(2, (hi - lo) * .06), step = hi - lo > 120 ? 50 : hi - lo > 40 ? 10 : 5;
      KK.ruler = { t0: lo - pad, t1: hi + pad, step, first: Math.ceil((lo - pad) / step) * step, fmt: 'n', ticks: false }; if (t != null) KK.playhead = t; }
    hit = true; };
  const fr = t => numOf(t) != null ? Math.round(numOf(t) * fps) : null;
  const pts = Array.isArray(A.points) ? A.points.map(q => Array.isArray(q) ? [numOf(q[0]), numOf(q[1])] : q && typeof q === 'object' ? [numOf(q.x), numOf(q.y)] : null).filter(q => q && q[0] != null && q[1] != null) : [];
  const norm = pts.length && pts.every(([x, y]) => Math.abs(x) <= 1.5 && Math.abs(y) <= 1.5);
  const P2 = norm ? pts : (state.session.width ? pts.map(([x, y]) => [x / state.session.width, y / state.session.height]) : []);
  const ov = () => (vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'mask:' + mid));
  if (/^(add_mask|update_mask|set_mask)$/.test(tn)) {
    const pr = A.params && typeof A.params === 'object' ? A.params : A;
    const cx = numOf(pr.cx), cy = numOf(pr.cy), w = numOf(pr.w), h = numOf(pr.h), kind = String(A.kind ?? A.shape ?? (P2.length ? 'path' : 'rect'));
    if (P2.length >= 2) { const o = ov(); o.push({ path: P2, closed: A.closed !== false, _kind: 'mask:' + mid }); hit = true; }
    else if (cx != null && cy != null && w != null && h != null) { const o = ov(); o.push({ shape: /ellipse|circle/.test(kind) ? 'ellipse' : 'rect', rect: [cx - w / 2, cy - h / 2, w, h], _kind: 'mask:' + mid }); hit = true; }
    if (K) { K.sub = `${mid} - ${kind}${P2.length ? ` · ${P2.length} pts` : ''}`; }
    rowsSet('Refine matte', [['Feather', numOf(A.feather) != null ? `${fmtVal(numOf(A.feather))} px` : null], ['Choke', numOf(A.expand) != null ? fmtVal(-numOf(A.expand)) : null]]);   // only the rows m12 draws (choke = negative expand)
    const qs = ['hue_soft', 'sat_soft', 'lum_soft'].map(k => A.params && typeof A.params === 'object' ? numOf(A.params[k]) : null);
    if (qs.some(v => v != null)) rowsSet('Refine matte', [['Qualifier soft h/s/l', qs.map((v, i) => v == null ? '-' : i ? fmtVal(v) : `${fmtVal(v)}°`).join(' / ')]]);
  }
  if (tn === 'set_mask_path_key') { lane(`${mid} path`, fr(A.time)); if (P2.length >= 2) { const o = ov(); o.push({ path: P2, closed: true, _kind: 'mask:' + mid }); } }
  if (tn === 'set_mask_prop_key' || tn === 'set_mask_keyframe') { const pr = String(A.property ?? 'value'); lane(`${mid} ${pr}`, fr(A.time)); if (numOf(A.value) != null && /^(feather|expand)$/.test(pr)) rowsSet('Refine matte', [[pr === 'feather' ? 'Feather' : 'Choke', pr === 'feather' ? `${fmtVal(numOf(A.value))} px` : fmtVal(-numOf(A.value))]]); }
  if (tn === 'roto_propagate' && KK) { const L = (KK.lanes || []).find(l => l.label === `${mid} path`); if (L && L.keys.length >= 2) { const lo = L.keys[0], hi = L.keys[L.keys.length - 1]; L.bar = [[lo, hi, '#3a3935']]; hit = true; }
  }
  if (tn === 'refine_matte') { rowsSet('Refine matte', [['Bg model', A.auto_bg != null ? (A.auto_bg ? 'auto' : 'manual') : null]]); if (K && !K.sub && A.clip_id != null) K.sub = 'refine matte';
    const mae = numOf(R.mae ?? R.matte_mae); if (mae != null) rowsSet('Quality', [['Matte MAE', mae.toFixed(2)]]); }
  if (tn === 'set_track_matte' && KK) {   // the matte holds for the whole clip: a span over the clip's range (from the edit timeline), else over the lanes' current range
    const tl = state.liveTL, [ct, ci] = tl && A.clip_id != null ? findClip(tl, { clip_id: A.clip_id }) : [null, -1], c = ct ? ct.clips[ci] : null;
    const lo = c ? fr(c.s) : KK.ruler ? KK.ruler.t0 + (KK.ruler.t1 - KK.ruler.t0) * .06 : null, hi = c ? fr(c.e) : KK.ruler ? KK.ruler.t1 - (KK.ruler.t1 - KK.ruler.t0) * .06 : null;
    lane(`track matte${A.mode ? ' ' + String(A.mode) : ''}`, null, lo != null && hi != null && hi > lo ? { area: [[lo, 1], [hi, 1]], color: '#6f6c66', stroke: '#e8e0cc' } : {});
    if (lo != null && hi != null) { const L = KK.lanes.find(l => l.label.startsWith('track matte')); if (L && L.keys.length === 0 && KK.ruler && (lo < KK.ruler.t0 || hi > KK.ruler.t1)) lane(L.label, null); }   // refit the ruler to include the span
  }
  if (tn === 'remove_track_matte') { const cnt = () => ((K && K.blocks) || []).length + ((KK && KK.lanes) || []).length, n0 = cnt(); if (K) K.blocks = (K.blocks || []).filter(b => b.sec !== 'Key' && b._sec !== 'Key'); if (KK) KK.lanes = (KK.lanes || []).filter(l => !l.label.startsWith('track matte')); hit = cnt() !== n0; }   /* nothing drawn to take away: the Inspector says it (router callSection) */
  if (tn === 'reorder_mask' && numOf(A.index) != null && KK && KK.lanes) {   // the mask's lanes move to its new place in the stack order
    const mine = KK.lanes.filter(l => l.label.startsWith(mid + ' ')), rest = KK.lanes.filter(l => !l.label.startsWith(mid + ' ')); if (mine.length) { const at = Math.max(0, Math.min(rest.length, Math.round(numOf(A.index)))); KK.lanes = [...rest.slice(0, at), ...mine, ...rest.slice(at)]; for (const l of mine) l._flash = now(); hit = true; } }
  if (tn === 'remove_mask' || tn === 'remove_mask_layer') { ov(); if (tn === 'remove_mask') vw.overlay = (vw.overlay || []).filter(o => !String(o._kind || '').startsWith('mask:'));
    if (KK) KK.lanes = (KK.lanes || []).filter(l => !(tn === 'remove_mask' || l.label.startsWith(mid + ' '))); if (K && (tn === 'remove_mask' || String(K.sub || '').startsWith(mid))) { K.blocks = (K.blocks || []).filter(b => !b._mask); K.sub = ''; } hit = true; }
  if (hit) ev._kvDone = true;
  return hit;
}

function timelineOps(state, MODES, ev, fam) {
  const tl = timelineView(state, MODES); if (!tl) return false;
  const A = (ev.payload || {}).args || (state._editArgs && state._editArgs[ev.call_id]) || {};
  if (fam === 'edit') delete (state._editArgs || {})[ev.call_id];
  if (fam === 'edit' && editOps2(state, MODES, tl, ev, A)) {
    fitRuler(tl);
    // the m16 inspector heads with the touched clip's name (its rows fill only from args that carry them)
    const [t2, i2] = findClip(tl, { clip_id: A.clip_id ?? A.clip_a ?? (ev.target || {}).clip_id }); const kv = panels(state, MODES, ['kv'])[0];
    if (kv && t2) { const lb = t2.clips[i2].label; if (lb && !RAW_ID.test(String(lb))) kv.sub = String(lb); }
    if (kv) {
      // the op's own rows in the existing Inspector: what it touched (clip names, track) and its values
      kv.blocks = kv.blocks || []; const j = kv.blocks.findIndex(b => b._live); if (j >= 0) kv.blocks.splice(j);
      const nameOf = id => { const [tt, ii] = findClip(tl, { clip_id: id }); const lb = tt ? tt.clips[ii].label : null; return lb && !RAW_ID.test(String(lb)) ? String(lb) : null; };
      const ids = [A.clip_id, A.clip_a, A.clip_b, ...(Array.isArray(A.clip_ids) ? A.clip_ids : [])].filter(x => x != null);
      const names = [...new Set(ids.map(nameOf).filter(Boolean))];
      const trk = A.track_id != null ? findTrack(tl, A.track_id) : null;
      const s = v => `${fmtVal(numOf(v))} s`, rows = [];
      if (names.length) rows.push([names.length > 1 ? 'Clips' : 'Clip', names.length > 2 ? `${names[0]} +${names.length - 1}` : names.join(', ')]);
      if (trk) rows.push(['Track', String(trk.label)]);
      const add = (k, v) => { if (v != null && v !== '') rows.push([k, v]); };
      if (numOf(A.speed) != null) add('Speed', `${fmtVal(numOf(A.speed))}×`);
      if (ev.tool_name === 'rate_stretch' && numOf(A.duration) != null) add('Duration', s(A.duration));
      if (numOf(A.times) != null) add('Copies', String(Math.round(numOf(A.times))));
      if (numOf(A.gap) != null) add('Gap', s(A.gap));
      if (numOf(A.start) != null && ev.tool_name !== 'add_camera_shake') add('Start', s(A.start));
      if (Array.isArray(A.keys)) add('Ramp keys', `${A.keys.length} keys`);
      if (Array.isArray(A.clip_ids)) add('Grouped', `${A.clip_ids.length} clips`);
      if (A.mode != null) add('Mode', String(A.mode));
      if (numOf(A.count) != null) add('Count', String(Math.round(numOf(A.count))));
      if (numOf(A.at) != null) add('At', s(A.at));
      if (numOf(A.amount) != null) add('Amount', fmtVal(numOf(A.amount)));
      if (ev.tool_name === 'add_camera_shake') { if (numOf(A.amplitude) != null) add('Amplitude', fmtVal(numOf(A.amplitude))); if (numOf(A.duration) != null) add('Duration', s(A.duration)); }
      if (ev.tool_name === 'set_track' && typeof A.name === 'string') add('Name', A.name);
      if (ev.tool_name === 'project_set') { if (numOf(A.width) && numOf(A.height)) add('Size', `${A.width} x ${A.height}`); if (numOf(A.fps) != null) add('Frame rate', `${fmtVal(numOf(A.fps))} fps`); }
      if (rows.length) { kv.blocks.push({ sec: String(ev.tool_name), _live: true }); for (const [k, v] of rows.slice(0, 6)) kv.blocks.push({ k, v, _live: true, _flash: now() }); }
    }
    ev._kvDone = true; return true;
  }
  const hit = timelineOps1(tl, ev, fam);
  if (hit) {
    fitRuler(tl);
    const P = ev.payload || {};
    // playhead follows the engine's own offsets: a cut point, else the start of the clip just touched
    const at = P.at ?? (fam === 'ingest' || fam === 'text' ? P.e : null);
    if (at != null && Number.isFinite(+at)) tl.playhead = +at;
  }
  return hit;
}
function timelineOps1(tl, ev, fam) {
  const P = ev.payload || {}, T = ev.target || {}, n = String(ev.tool_name).toLowerCase();
  const ph = P.at ?? tl.playhead;
  // add_track: an empty lane appears (engine reports it under changed.tracks_added)
  const added = (P.changed && P.changed.tracks_added) || [];
  if (/^add_track/.test(n) || (added.length && P.s == null)) {
    const ids = added.length ? added.map(x => (x && typeof x === 'object') ? (x.track_id ?? x.id) : x) : [T.track_id || P.track || 'track' + ((tl.tracks || []).length + 1)];
    for (const id of ids) ensureTrack(tl, id, P.kind || P.type);
    return true;
  }
  // any insert the engine reports (clip + track + s/e): import, add_clip, add_text, add_audio...
  if (fam !== 'edit' && P.clip != null && P.s != null && (P.track != null || T.track_id != null)) {
    const t = ensureTrack(tl, P.track ?? T.track_id, P.kind || P.source_type || n);
    insertClip(tl, t, { id: P.clip, label: clipLabel(P, t, fam), s: +P.s, e: +(P.e ?? +P.s + 4), thumb: P.thumb, ...(fam === 'text' ? { cls: 'cap' } : {}), _flash: now() });
    t._flash = now(); return true;
  }
  if (!tl.tracks) tl.tracks = [];
  if (fam === 'ingest' && P.s != null) {
    const t = ensureTrack(tl, T.track_id || P.track || 'V' + (tl.tracks.length + 1));
    insertClip(tl, t, { id: T.clip_id || P.clip, label: clipLabel(P, t, fam), s: +P.s, e: +(P.e ?? +P.s + 4), thumb: P.thumb, _flash: now() });
    t._flash = now(); return true;
  }
  let [t, i] = findClip(tl, T);
  // a call on a clip this window has not seen yet (joined mid-session): the engine sent its bounds, so draw it first
  if (!t && T.clip_id && T.track_id && (fam === 'edit' || fam === 'effects') && P.s != null && P.e != null && !/^(split|cut|blade|razor)/.test(n)) {
    t = ensureTrack(tl, T.track_id); insertClip(tl, t, { id: T.clip_id, label: clipLabel({}, t, fam), s: +P.s, e: +P.e }); i = t.clips.findIndex(c => c.id === T.clip_id);
  }
  if (fam === 'edit' && t) {
    const c = t.clips[i];
    if (/^(cut|split|blade|razor)/.test(n) && ph > c.s && ph < c.e) {
      const right = { ...c, id: P.clip != null && P.clip !== c.id ? P.clip : (c.id || c.label) + '_b', label: splitName(tl, c.label), s: ph, _flash: now() };
      delete right.thumb;
      c.e = ph; c._flash = now();
      t.clips.splice(i + 1, 0, right);
      tl.razor = { at: ph, t: now() };
      if (P.ripple) for (const k of t.clips.slice(i + 2)) { k.s += +P.ripple; k.e += +P.ripple; }
      return true;
    }
    if (/^(trim|extend|roll|slip)/.test(n)) {
      if (P.s != null || P.in != null) c.s = +(P.s ?? P.in);
      if (P.e != null || P.out != null) c.e = +(P.e ?? P.out);
      c._flash = now(); return true;
    }
    if (/^(ripple|delete|lift)/.test(n)) {
      const gap = c.e - c.s; t.clips.splice(i, 1);
      if (!/^lift/.test(n)) for (const k of t.clips.slice(i)) { k.s -= gap; k.e -= gap; k._flash = now(); }
      return true;
    }
    if (/^(move|slide)/.test(n) && P.s != null) { const d = +P.s - c.s; c.s += d; c.e += d; c._flash = now(); return true; }
  }
  if (fam === 'effects' && t && /^(add|apply)_/.test(n)) {
    const c = t.clips[i]; const fx = P.effect || P.name || P.type || n.replace(/^(add|apply)_(effect_?)?/, '');
    c.fx = [...(c.fx || []).filter(x => x !== fx), fx]; c._flash = now(); return true;
  }
  if (fam === 'audio' && /^(mute|solo)/.test(n)) {
    const tr = findTrack(tl, T.track_id); if (!tr) return false;
    const k = n.startsWith('mute') ? 'mute' : 'solo'; tr[k] = P.on ?? P.value ?? !tr[k]; tr._flash = now(); return true;
  }
  if (fam === 'text' && P.text != null && P.s != null) {
    const tr = findTrack(tl, T.track_id || 'CAP'); if (!tr) return false;
    tr.clips = tr.clips || []; tr.clips.push({ s: +P.s, e: +(P.e ?? +P.s + 2), label: P.text, cls: 'cap', _flash: now() }); return true;
  }
  return false;
}


// Audio family on the mixing screen's own panels (m04): EQ nodes, compressor curve + gain
// reduction, Pan row, loudness readout and meter ballistics. Nothing here adds a panel.
const num = v => { const n = parseFloat(v); return Number.isFinite(n) ? n : null; };
export function fmtPan(v) { const n = num(v); if (n == null) return String(v ?? ''); const p = Math.round(Math.abs(n) <= 1 ? n * 100 : n); return p === 0 ? 'C' : (p < 0 ? 'L' : 'R') + Math.abs(p); }
function audioOps(state, MODES, ev) {
  const P = ev.payload || {}, A = P.args || {}, n = String(ev.tool_name).toLowerCase(), t = now();
  const V = views(state), M = MODES[state.mode]; let hit = false;
  const get = k => P[k] ?? A[k];
  const kvs = Object.entries(M.panels).filter(([, s]) => s.kind === 'kv').map(([k]) => V[k]).filter(Boolean);
  const blocks = kvs.flatMap(k => k.blocks || []);
  const eq = blocks.find(b => b.widget === 'eq'), comp = blocks.find(b => b.widget === 'comp');
  if (eq && (/eq/.test(n) || /eq/.test(String(get('effect') ?? '')))) {
    const f = num(get('freq') ?? get('f') ?? get('frequency')), g = num(get('gain') ?? get('g') ?? get('to')), q = num(get('q'));
    const bands = (eq.bands || []).filter(B => B.g != null && !/pass/.test(B.type || ''));
    let B = get('band') != null ? bands[+get('band')] : null;
    if (!B && f != null) B = bands.reduce((best, x) => (!best || Math.abs(Math.log(x.f / f)) < Math.abs(Math.log(best.f / f)) ? x : best), null);
    B = B || bands.find(x => x.sel) || bands[0];
    if (B) { if (f != null) B.f = f; if (g != null) B.g = g; if (q != null) B.q = q; bands.forEach(x => { x.sel = x === B; }); B._flash = t; eq._flash = t; hit = true; }
  }
  if (comp && /compress|comp(?!ute)|limit/.test(n)) {
    const th = num(get('threshold')), ra = num(get('ratio')), inp = num(get('input_db') ?? get('level'));
    if (th != null && th !== comp.threshold) { comp.threshold = th; comp._flashT = t; }
    if (ra != null && ra !== comp.ratio) { comp.ratio = ra; comp._flashR = t; }
    comp.inDb = inp ?? Math.min(0, (comp.threshold ?? -18) + 8);   // where the signal sits on the curve while the call runs
    comp._grT = t; hit = true;
  }
  if (/pan/.test(n)) {
    const v = get('pan') ?? get('value') ?? get('to');
    const row = blocks.find(b => b.k && /^pan$/i.test(b.k));
    if (row && v != null) { const nv = fmtPan(v); if (nv !== row.v) { row.from = row.v; row.v = nv; } row._flash = t; hit = true; }
  }
  // loudness readout + meters: every audio call moves the meters (a hit then decay); loudness tools set the number
  const L = Object.entries(M.panels).filter(([, s]) => s.kind === 'loudness').map(([k]) => V[k] || (V[k] = {}))[0];
  if (L) {
    // the big number is loudness in LUFS, never a gain: normalize_loudness's landed value (P.to, e.g. 2.505) is the
    // clip's volume factor, so the readout is the engine's result (measured + gain = where the clip now sits, or the
    // target), and the gain gets its own row in dB (GUI-FIX-1; it read "2.5 LUFS" before)
    const R = P.result && typeof P.result === 'object' ? P.result : {};
    const meas = num(R.measured_lufs), gdb = num(R.gain_db);
    const lufs = num(R.integrated_lufs ?? (meas != null && gdb != null ? meas + gdb : null) ?? R.target_lufs ?? R.lufs ?? P.lufs ?? (/lufs|loud/i.test(String(P.param ?? '')) ? P.to : null) ?? get('target_lufs') ?? get('lufs'));
    if (/loud|normaliz|lufs/.test(n) && lufs != null) { L.lufs = lufs.toFixed(1); L._flash = t; }
    if (/loud|normaliz|lufs/.test(n)) {
      const tg = num(R.target_lufs ?? get('target_lufs')); if (tg != null && /normaliz/.test(n)) L.target = tg.toFixed(1);
      const sg = x => (x > 0.049 ? '+' : x < -0.049 ? '-' : '') + Math.abs(x).toFixed(1);
      const add = [meas != null && ['Input', `${sg(meas)} LUFS`], gdb != null && ['Gain', `${sg(gdb)} dB`], num(R.true_peak_dbfs) != null && ['Peak', `${sg(R.true_peak_dbfs)} dBTP`], num(R.true_peak_dbfs) == null && num(R.sample_peak_dbfs) != null && ['Peak', `${sg(R.sample_peak_dbfs)} dBFS`]].filter(Boolean);
      if (add.length) { const keep = (L.rows || []).filter(r => !add.some(a => a[0] === r[0])); L.rows = [...add, ...keep].slice(0, 3); for (const r of add) r._flash = t; L._flash = t; }
    }
    const gain = num(get('gain') ?? get('gain_db')) ?? 0;
    for (const m of (L.meters || [])) {
      const base = m.v ?? .5, lvl = Math.max(0, Math.min(1, base + .12 + gain * .03));
      m.hit = { t, lvl, base };
      if (m.peak == null || lvl > m.peak) m.peakHold = { t, lvl };
    }
    state.animUntil = Math.max(state.animUntil || 0, t + METER_MS);
    hit = true;
  }
  return hit;
}
export const METER_MS = 1600;   // hit + VU decay + peak-hold fall: frames run only inside this window


// Export family on the export screen's own panels (m14): the viewer's progress bar (frame a / b,
// fps, ETA), the Export settings rows the call actually carried, and a job row in Renders that
// counts up while rendering and stays in the list (history) when it lands.
const mmss = ms => { const s = Math.max(0, Math.round(ms / 1000)); return String(Math.floor(s / 60)).padStart(2, '0') + ':' + String(s % 60).padStart(2, '0'); };
// "~/films/…/dune_film_final.mp4": first folder + file, the middle elided; a long file name loses its own middle
const midPath = (p, max = 26) => { const s = String(p).replace(/^\/(home|Users)\/[^/]+/, '~'); if (s.length <= max) return s; const seg = s.split('/'), file = seg.pop(), head = seg.filter(Boolean)[0];
  let out = head && head !== file ? `${s.startsWith('/') ? '/' : ''}${head}/…/${file}` : file; if (out.length <= max) return out; out = file; if (out.length <= max) return '…/' + out;
  const keep = max - 1, front = Math.ceil(keep / 2); return file.slice(0, front) + '…' + file.slice(file.length - (keep - front)); };
const shortPath = (p, max = 24) => { let s = String(p).replace(/^\/(home|Users)\/[^/]+/, '~'); const seg = s.split('/'); return s.length > max && seg.length > 3 ? '…/' + seg.slice(-(max > 24 ? 3 : 2)).join('/') : s; };
const fmtBytes = b => b >= 1e9 ? (b / 1e9).toFixed(2) + ' GB' : b >= 1e6 ? (b / 1e6).toFixed(1) + ' MB' : b >= 1e3 ? Math.round(b / 1e3) + ' KB' : b + ' B';
function exportOps(state, MODES, ev) {
  const M = MODES.export; if (!M) return false;
  const V = state.views.export || (state.views.export = {});
  const P = ev.payload || {}, A = P.args || {}, id = ev.call_id ?? ev.tool_name;
  const name = Object.keys(M.panels), kvName = name.find(k => M.panels[k].kind === 'kv'), rName = name.find(k => M.panels[k].kind === 'renders'), vName = name.find(k => M.panels[k].kind === 'viewer');
  const exp = V[kvName] || (V[kvName] = {}), ren = V[rName] || (V[rName] = {}), vw = V[vName] || (V[vName] = {});
  ren.head = ren.head || ['name', 'preset', 'length', 'status', 'started'];
  ren.rows = ren.rows || [];
  let row = ren.rows.find(r => r._call === id);
  const tn = ev.tool_name, R0 = P.result && typeof P.result === 'object' ? P.result : {};
  // cancel / resume act on an existing job: its row (matched by the engine's job id, else the live row)
  if (tn === 'cancel_job' || tn === 'resume_job') {
    const job = ren.rows.find(r => A.job_id != null && r._jobId === A.job_id) || ren.rows.find(r => r._live);
    if (!job) return false;
    if (tn === 'cancel_job' && ev.phase === 'complete') { const kept = Array.isArray(R0.completed_ranges) ? R0.completed_ranges.length : null; job[3] = '✕ cancelled' + (kept ? ` · ${kept} chunk${kept > 1 ? 's' : ''} kept` : ''); job._live = false; job._flash = now(); if (vw.progress) vw.progress = { ...vw.progress, right: 'cancelled' }; }
    if (tn === 'resume_job' && ev.phase === 'start') { job[3] = '● resuming'; job._live = true; job._call = id; job._flash = now(); vw.progress = { pct: vw.progress ? vw.progress.pct : null, left: 'resuming', right: '' }; }
    if (tn === 'resume_job' && ev.phase !== 'start') { job._call = id; row = job; } else return true;
  }
  if (ev.phase === 'start') {
    const path = A.path ?? A.output ?? A.file ?? '';
    const base = String(path).split(/[\\/]/).pop() || ev.tool_name;
    const ext = (base.match(/\.([a-z0-9]+)$/i) || [])[1];
    const res = A.resolution ?? (A.width && A.height ? `${A.width} x ${A.height}` : null);
    const codec = A.codec ?? A.video_codec ?? null;   // only what the call carried: nothing is guessed
    const set = [['Format', A.container ?? A.format ?? (ext ? ext.toUpperCase() : null)], ['Video', codec], ['Resolution', res], ['Frame rate', A.fps], ['Quality', A.crf != null ? 'CRF ' + A.crf : A.quality], ['Audio', A.audio === false ? 'off' : A.audio_codec], ['Output', path ? midPath(path) : null]];   // the path with its middle elided, never a hard clip
    exp.blocks = exp.blocks || [];
    for (const [k, v] of set) { if (v == null || v === '') continue; const b = exp.blocks.find(x => x.k === k); if (b) { if (b.v !== String(v)) { if (k === 'Output') delete b.from; else b.from = b.v; b.v = String(v); if (k === 'Resolution' && b.from != null) { b.from = String(b.from).replace(/\s*x\s*/, 'x'); b.v = b.v.replace(/\s*x\s*/, 'x'); } } /* '1920x1080 → 1280x720' fits the row; a single size keeps the mockup's '1920 x 1080' */ b._flash = now(); } else exp.blocks.push({ k, v: String(v), _flash: now() }); }   // a new output is a new file, not a change of one: no from → to
    const nParts = Array.isArray(A.parts) ? A.parts.length : null, sc = numOf(A.scale);
    const preset = tn === 'join_export' ? (nParts ? `join ${nParts} parts` : 'join') : tn === 'preview_range' ? `preview${sc != null && sc !== 1 ? ' ' + sc + 'x' : ''}` : tn === 'collect_project' ? 'collect' : tn === 'contact_sheet' ? 'contact sheet'
      : [res && String(res).replace(/\s*x\s*/, 'x'), codec].filter(Boolean).join(' ') || A.preset || '';
    const label = tn === 'collect_project' && !path && A.dest_dir ? (String(A.dest_dir).split(/[\\/]/).filter(Boolean).pop() || base) : base;
    if (!row) { row = [label, preset, '', tn === 'collect_project' ? '● collecting' : tn === 'contact_sheet' ? '● rendering' : '● rendering', 'now']; row._call = id; row._t0 = now(); row._job = true; ren.rows.unshift(row); }
    row._live = true; if (path) row._path = shortPath(path, 48); if (vw.src) row._thumb = vw.src;
    if (!/^(collect_project|contact_sheet)$/.test(tn)) vw.progress = { pct: null, left: 'starting', right: '' };   /* indeterminate until the engine's first progress event: never a made-up 0% */   // a collect / sheet is not a frame render: no bar
    return true;
  }
  if (ev.phase === 'progress') {
    const pct = P.pct ?? (P.current_frame != null && P.total_frames ? 100 * P.current_frame / P.total_frames : null);
    if (pct == null) return false;
    const left = P.current_frame != null && P.total_frames ? `frame ${P.current_frame} / ${P.total_frames}` : `${Math.round(pct)}%`;
    const right = [P.fps != null ? `${Math.round(P.fps)} fps` : '', P.elapsed_ms != null ? `${mmss(P.elapsed_ms)} elapsed` : '', P.eta_ms != null ? `eta ${mmss(P.eta_ms)}` : ''].filter(Boolean).join('\u2003');
    vw.progress = { pct: Math.round(pct), left, right };
    if (row) row[3] = `● rendering ${Math.round(pct)}%`;
    return true;
  }
  const ok = ev.phase === 'complete', dur = P.dur_ms ?? ev.duration_ms ?? ev._dur;
  if (row) {
    // history row: the last frame the engine rendered, the output path, length and final size, each
    // only when the engine actually sent it (no placeholder)
    const R = P.result && typeof P.result === 'object' ? P.result : {};
    const bytes = [R.bytes, R.size_bytes, R.file_size, P.bytes, P.size_bytes].find(x => typeof x === 'number' && x > 0);
    row[3] = ok ? '✓ done' + (bytes ? ' · ' + fmtBytes(bytes) : '') : '✕ failed';
    if (ok && typeof R.duration === 'number') row[2] = mmss(R.duration * 1000);
    if (ok && R.path) row._path = shortPath(R.path, 48);
    if (R.job_id != null) row._jobId = R.job_id;
    if (vw.src) row._thumb = vw.src;
    if (dur != null) row[4] = mmss(dur); row._flash = now(); row._live = false;
  }
  if (vw.progress && !/^(collect_project|contact_sheet)$/.test(tn)) vw.progress = ok ? (vw.progress.left === 'starting' ? { ...vw.progress, pct: 100, left: 'done', right: dur != null ? `in ${mmss(dur)}` : '' } : { ...vw.progress, pct: 100, right: (dur != null ? `done in ${mmss(dur)}` : 'done') }) : { ...vw.progress, right: 'failed', halt: true };   /* a failed job's sweep stops */
  return true;
}

// Unified event -> legacy rail events + family effects. Returns the legacy
// events the reducer should apply (the rail), having already mutated views.
const COV_H = { now, flashing, fmtVal, RAW_ID, findClip, findTrack, ensureTrack, fitRuler, timelineView, modeViews, cRestore };
// what cov_b.js needs from here (passed in, not imported back: no import cycle)
const COVB_H = { now: () => now(), fmtVal, gamut: g => space(g, 0) || null, modeViews: (s, M, e) => modeViews(s, M, e),
  clipName: (s, M, id) => { const tl = s.liveTL || panels(s, M, ['timeline'])[0]; const [t, i] = tl ? findClip(tl, { clip_id: id }) : [null]; const lb = t ? t.clips[i].label : null; return lb && !RAW_ID.test(String(lb)) ? String(lb) : null; },
  clipObj: (s, id) => { const tl = s.liveTL; const [t, i] = tl ? findClip(tl, { clip_id: id }) : [null]; return t ? t.clips[i] : null; } };
/* A keyframe call's payload carries the whole key list before and after ({t, v, ease, bez} each), so a plain
   fmtVal of it read "x: → ,". keyChange turns it into the one key that changed, the way the rail and the Inspector
   read every other change: "scale @2s: 100% → 125%" (the curve's value there before → the new key's value),
   "x @0s ease: linear → .42 .0 .58 1" for a bezier ease. The key lists stay on the payload for the Graph. */
const isKey = k => k && typeof k === 'object' && !Array.isArray(k) && numOf(k.t) != null && 'v' in k;
const keyAt = (keys, t) => keys.find(k => Math.abs(numOf(k.t) - t) < 1e-6);
function curveAt(keys, t) {
  const ks = keys.filter(k => numOf(k.v) != null).sort((a, b) => numOf(a.t) - numOf(b.t)); if (!ks.length) return null;
  if (t <= numOf(ks[0].t)) return numOf(ks[0].v);
  for (let i = 1; i < ks.length; i++) { const a = ks[i - 1], b = ks[i], ta = numOf(a.t), tb = numOf(b.t); if (t <= tb) return tb > ta ? numOf(a.v) + (numOf(b.v) - numOf(a.v)) * (t - ta) / (tb - ta) : numOf(b.v); }
  return numOf(ks[ks.length - 1].v);
}
const CSS_EASE = { '0.25 0.1 0.25 1': 'ease', '0.42 0 1 1': 'ease-in', '0 0 0.58 1': 'ease-out', '0.42 0 0.58 1': 'ease-in-out' };
const easeOf = k => { if (!(Array.isArray(k.bez) && k.bez.length === 4)) return String(k.ease ?? 'linear'); const s = k.bez.map(v => fmtVal(v)).join(' '); return CSS_EASE[s] || 'bezier ' + s.replace(/(^| )0\./g, '$1.'); };   /* the standard curves by name */
const keySecs = t => `${Math.round(t * 1000) / 1000}s`;
const KEY_TOOLS = /^set_keyframe(_bezier)?$/;   /* cov_b.js reads move_key / remove_key / … key lists itself */
function keyChange(ev) {
  const P = ev.payload; if (!KEY_TOOLS.test(String(ev.tool_name)) || ev.phase !== 'complete' || !P || !Array.isArray(P.from) || !Array.isArray(P.to)) return ev;
  const F = P.from, To = P.to; if (![...F, ...To].length || ![...F, ...To].every(isKey)) return ev;
  const param = String((ev.target || {}).param ?? P.param ?? (P.args || {}).property ?? 'value'), A = P.args || {};
  /* the key the call named, else the first key that differs */
  const ts = [...new Set([...F, ...To].map(k => numOf(k.t)))].sort((a, b) => a - b);
  const differs = t => { const a = keyAt(F, t), b = keyAt(To, t); return !a || !b || numOf(a.v) !== numOf(b.v) || easeOf(a) !== easeOf(b); };
  const ta = numOf(A.time ?? A.t), t = ta != null && differs(ta) ? ta : ts.find(differs);
  const base = { ...P, from_keys: F, to_keys: To, from: undefined, to: undefined };
  if (t == null) return { ...ev, payload: { ...base, key: { label: `${param} keys`, from: String(F.length), to: String(To.length), t: null } } };
  const a = keyAt(F, t), b = keyAt(To, t);
  let key;
  if (a && b && numOf(a.v) === numOf(b.v)) key = { label: `${param} @${keySecs(t)} ease`, from: easeOf(a), to: easeOf(b), t, ease: true };
  else {
    const was = a ? numOf(a.v) : curveAt(F, t), now_ = b ? numOf(b.v) : curveAt(To, t);
    key = { label: `${param} @${keySecs(t)}`, from: was == null ? 'none' : fmtVal(was, param), to: now_ == null ? 'none' : fmtVal(now_, param), t };
    base.from = was ?? undefined; base.to = now_ ?? undefined;
  }
  return { ...ev, payload: { ...base, key } };
}
/* The Graph (m03's curves panel) for a keyframe call: the property's curve as the engine now holds it, evaluated
   segment by segment — a key with a bezier ease [x1, y1, x2, y2] runs the CSS cubic-bezier between it and the next
   key (sampled, so the drawn line IS the timing curve), with its two handles in ember; a linear key a straight line;
   a hold key a step. The curve before the call is dashed underneath, the changed key is ember. Only on a screen that
   has a Graph (set_keyframe_bezier opens m03); everything drawn comes from the engine's own key lists. */
const BEZ_NAMED = { ease: [0.25, 0.1, 0.25, 1], 'ease-in': [0.42, 0, 1, 1], 'ease-out': [0, 0, 0.58, 1], 'ease-in-out': [0.42, 0, 0.58, 1] };
function bezOf(k) {
  if (Array.isArray(k.bez) && k.bez.length === 4 && k.bez.every(v => numOf(v) != null)) return k.bez.map(numOf);
  return BEZ_NAMED[String(k.ease ?? '').toLowerCase().replace(/_/g, '-')] || null;
}
function cubicY([x1, y1, x2, y2], x) {   // CSS cubic-bezier: solve x(s) = x (Newton, bisection fallback), return y(s)
  const B = (a, b, s) => 3 * a * s * (1 - s) * (1 - s) + 3 * b * s * s * (1 - s) + s * s * s;
  const dB = (a, b, s) => 3 * a * (1 - s) * (1 - s) + 6 * (b - a) * s * (1 - s) + 3 * (1 - b) * s * s;
  let s = x;
  for (let i = 0; i < 8; i++) { const e = B(x1, x2, s) - x, d = dB(x1, x2, s); if (Math.abs(e) < 1e-6) break; if (Math.abs(d) < 1e-6) { s = NaN; break; } s -= e / d; }
  if (!(s >= 0 && s <= 1) || Math.abs(B(x1, x2, s) - x) > 1e-4) { let lo = 0, hi = 1; s = x; for (let i = 0; i < 40; i++) { if (B(x1, x2, s) < x) lo = s; else hi = s; s = (lo + hi) / 2; } }
  return B(y1, y2, s);
}
export function evalKeys(keys, n = 24) {   // [[t, v]] along the curve, plus the bezier handles [[t, v], [t, v]] (key → handle)
  const ks = keys.filter(k => numOf(k.t) != null && numOf(k.v) != null).sort((a, b) => numOf(a.t) - numOf(b.t));
  const pts = [], handles = [];
  ks.forEach((a, i) => {
    const ta = numOf(a.t), va = numOf(a.v); pts.push([ta, va]);
    const b = ks[i + 1]; if (!b) return;
    const tb = numOf(b.t), vb = numOf(b.v), dt = tb - ta, dv = vb - va, bz = bezOf(a), ease = String(a.ease ?? '').toLowerCase();
    if (/^(hold|step|constant)$/.test(ease)) { pts.push([tb, va]); return; }
    if (!bz || dt <= 0) return;
    for (let j = 1; j < n; j++) { const x = j / n; pts.push([ta + x * dt, va + cubicY(bz, x) * dv]); }
    handles.push([[ta, va], [ta + bz[0] * dt, va + bz[1] * dv]], [[ta + bz[2] * dt, va + bz[3] * dv], [tb, vb]]);
  });
  return { pts, handles, keys: ks.map(k => [numOf(k.t), numOf(k.v)]) };
}
const KG_PROP = { x: 'position x', y: 'position y', pos_x: 'position x', pos_y: 'position y' };
function keyGraph(state, MODES, ev) {
  const P = ev.payload || {}, T = ev.target || {};
  if (!Array.isArray(P.to_keys)) return false;
  const [mode, V] = modeViews(state, MODES, ev), PP = MODES[mode] ? MODES[mode].panels : {};
  const e = Object.entries(PP).find(([, sp]) => sp.kind === 'curves'); if (!e) return false;
  const G = V[e[0]] || (V[e[0]] = {});
  const cur = evalKeys(P.to_keys), prev = evalKeys(P.from_keys || []);
  const ts = [...cur.pts, ...prev.pts].map(p => p[0]); let t0 = ts.length ? Math.min(...ts) : 0, t1 = ts.length ? Math.max(...ts) : 1; if (t1 - t0 < 1e-6) { t0 -= .5; t1 += .5; }
  const raw = (t1 - t0) / 6, mag = Math.pow(10, Math.floor(Math.log10(raw))), step = [1, 2, 2.5, 5, 10].map(m => m * mag).find(x => x >= raw) || raw, pad = (t1 - t0) * .04;
  const vs = [...cur.pts, ...prev.pts, ...cur.handles.flat()].map(p => p[1]); let lo = Math.min(...vs), hi = Math.max(...vs); if (!(hi - lo > 1e-9)) { lo -= 1; hi += 1; }
  const N = v => .12 + (v - lo) / (hi - lo) * .76, nn = q => [q[0], N(q[1])];
  const kt = P.key && P.key.t;
  G.ruler = { t0: t0 - pad, t1: t1 + pad, step, first: Math.ceil((t0 - pad) / step - 1e-9) * step, fmt: 's' };
  G.curves = [];
  if (prev.pts.length) G.curves.push({ pts: prev.pts.length > 1 ? prev.pts.map(nn) : [nn(prev.pts[0]), nn(prev.pts[0])], keys: prev.keys.map(k => [k[0], N(k[1]), '#4d4a44']), color: '#4d4a44', w: 1.4, dash: '5 4', cls: 'kg-prev' });
  if (cur.pts.length) G.curves.push({ pts: cur.pts.length > 1 ? cur.pts.map(nn) : [nn(cur.pts[0]), nn(cur.pts[0])], em: true, cls: 'kg-cur', _raw: cur.pts, _handles: cur.handles,
    handles: cur.handles.map(h => h.map(nn)), keys: cur.keys.map(k => [k[0], N(k[1]), kt != null && Math.abs(k[0] - kt) < 1e-6 ? 'var(--ember)' : 'var(--cream)']) });
  const nm = (() => { const tl = state.liveTL || panels(state, MODES, ['timeline'])[0]; const [tr, i] = tl && T.clip_id != null ? findClip(tl, { clip_id: T.clip_id }) : [null]; const lb = tr ? tr.clips[i].label : null; return lb && !RAW_ID.test(String(lb)) ? String(lb) : null; })();
  const prop = KG_PROP[String(T.param ?? P.param ?? 'value').toLowerCase()] || String(T.param ?? P.param ?? 'value').replace(/_/g, ' ');
  G.lanes = [nm ? `${nm} · ${prop}` : prop];
  G.playhead = kt != null ? kt : null; G._kg = ev.call_id;
  return true;
}
export function route(ev, state, MODES) {
  if (!ev || ev.type || !ev.tool_name) return [ev];        // legacy or non-tool event: unchanged
  ev = keyChange(assetNames(ev, state));
  if (ev.tool_name === 'render_scope' && ev.call_id != null && (ev.phase === 'start' || !(state._scopeCalls || {})[ev.call_id])) (state._scopeCalls || (state._scopeCalls = {}))[ev.call_id] = String(((ev.payload || {}).args || {}).scope ?? 'scope');   /* its image goes to the scope dock (app.js) */
  if (COV_A.test(String(ev.tool_name))) ev = covAPre(state, ev, COV_H);   /* rail args read as names; raw args kept for covAOps */
  covbHistory(state, MODES, ev, COVB_H);   // before any op: the screen as it was before this step (cov_b.js)
  const fam = family(ev.tool_name), id = ev.call_id ?? ev.tool_name + '@' + (ev.ts ?? '');
  const out = [];
  fix2Pre(state, MODES, ev);   /* GUI-FIX-2: a feature-gated refusal's cargo feature, read from the engine's error (fix2.js) */
  if (fam === 'inspect') { const o = inspectOps(state, MODES, ev, id); fix2Post(state, MODES, ev); return o; }
  if (fam === 'transform' && (ev.phase === 'start' || ev.phase === 'complete')) xfOps(state, MODES, ev);
  if (ev.phase === 'complete' && KEY_TOOLS.test(String(ev.tool_name))) keyGraph(state, MODES, ev);   // the curve on the Graph, when the screen has one
  if (fam === '3d') sceneOps(state, MODES, ev);
  if (fam === 'graphics') graphicsOps(state, MODES, ev);
  if (fam === 'roto') maskOps(state, MODES, ev);
  const cBefore = ev.phase === 'complete' && ((ev.payload || {}).mutated === true && C_MUT.test(String(ev.tool_name)) || ev.tool_name === 'load_project') ? cSnap(state, MODES) : null;   /* load_project reports mutated false but is an engine undo step (history_list: next_undo load_project) */   /* lane C's undo step: its panels before the call (project_ops.js) */
  if (fam === 'pipeline') pipelineOps(state, MODES, ev);
  if (fam === 'project') projectOps(state, MODES, ev);
  if (fam === 'qa') qaOps(state, MODES, ev);
  if (ev.phase === 'start' && (fam === 'color' || fam === 'effects')) (state._gradeArgs || (state._gradeArgs = {}))[ev.call_id] = (ev.payload || {}).args || {};
  if ((fam === 'color' || fam === 'effects') && gradeOps(state, MODES, ev)) ev._kvDone = true;
  covbLearn(state, ev);   // key lists the engine reports, for the graph tools
  if (COVB.test(String(ev.tool_name)) && covbOps(state, MODES, ev, COVB_H)) ev._kvDone = true;
  if (ev.phase === 'start' && /^(open_project|load_project|new_project|save_project|project_set)$/.test(String(ev.tool_name))) {   // the topbar's project name, from the call's own args
    const pa = (ev.payload || {}).args || {}, nm = pa.name ?? pa.project ?? (typeof pa.path === 'string' ? pa.path.split(/[\\/]/).pop() : null);
    if (typeof nm === 'string' && nm.trim()) state.session.project = nm.trim().slice(0, 40);
  }
  if (ev.phase === 'start' && fam === 'edit') (state._editArgs || (state._editArgs = {}))[ev.call_id] = (ev.payload || {}).args || {};
  if (ev.phase === 'start' && ev.call_id != null) (state._callArgs || (state._callArgs = {}))[ev.call_id] = (ev.payload || {}).args || {};
  const T = ev.target || {}, P = ev.payload || {};
  const param = T.param || P.param, val = P.to ?? P.value;
  const args = { ...(T.clip_id ? { clip: T.clip_id } : {}), ...(param && val != null ? { [param]: fmtVal(val, param) } : {}) };
  if (ev.phase === 'start' && ev.ts_ms != null) (state._t0 || (state._t0 = {}))[id] = ev.ts_ms;
  if (ev.phase === 'start') {
    out.push({ type: 'tool_start', id, name: ev.tool_name, args: P.args ? railArgs(state, P.args) : (Object.keys(args).length ? railArgs(state, args) : undefined) });
    if (fam === 'export') exportOps(state, MODES, ev);
    hud(state, ev, fam);
  } else if (ev.phase === 'progress') {
    const pct = P.pct ?? (P.done != null && P.total ? 100 * P.done / P.total : null);
    out.push({ type: 'tool_progress', id, progress: P.done != null ? { done: P.done, total: P.total } : { pct } });
    if (fam === 'audio' && /compress|comp(?!ute)|limit/.test(String(ev.tool_name).toLowerCase())) audioOps(state, MODES, ev);
    if (fam === 'export') exportOps(state, MODES, ev);
    if (fam === 'color' && pct != null) { const v = views(state); v.viewer = v.viewer || {}; v.viewer.wipe = pct / 100; }
    hud(state, ev, fam, `${ev.tool_name} ${pct != null ? Math.round(pct) + '%' : ''}`.trim());
  } else if (ev.phase === 'complete' || ev.phase === 'error') {
    const err = ev.phase === 'error';
    const t0 = state._t0 && state._t0[id]; if (t0 != null) delete state._t0[id];
    const dur = numOf(P.dur_ms) ?? numOf(ev.duration_ms) ?? (t0 != null && ev.ts_ms != null && ev.ts_ms >= t0 ? ev.ts_ms - t0 : null); ev._dur = dur;
    // the landed value replaces the rail's start-time args: "scale: 104% → 112%"
    // normalize_loudness lands a linear volume factor: the rail reads it "x1 → x2.505", never a bare number (GUI-FIX-1)
    const lfv = v => (/^normalize_/.test(String(ev.tool_name)) && /volume/i.test(String(param)) && typeof v === 'number' ? 'x' + fmtVal(v) : fmtVal(v, param));
    const landed = !err && P.key ? { ...(T.clip_id ? { clip: T.clip_id } : {}), [P.key.label]: `${P.key.from} → ${P.key.to}` } : !err && param && val != null && typeof val !== 'object' && fmtVal(val, param) !== '' ?   /* a key list (to: [{t, v}…]) is not a landed value */ { ...(T.clip_id ? { clip: T.clip_id } : {}), [param]: (P.from != null ? lfv(P.from) + ' → ' : '') + lfv(val) } : undefined;
    out.push({ type: 'tool_end', id, name: ev.tool_name, dur_ms: dur, status: err ? 'error' : 'ok', error: P.error, ...(P.result !== undefined ? { result: P.result } : {}), ...(landed ? { args: railArgs(state, landed) } : ev._railArgs ? { args: ev._railArgs } : {}) });
    if (!err) {
      const v = views(state); if (v.viewer && v.viewer.wipe != null) v.viewer.wipe = 1;
      if (/false_?colou?r/.test(String(ev.tool_name).toLowerCase())) { v.viewer = v.viewer || {}; v.viewer.falsecolor = { split: P.split ?? .5 }; }
      if (ev.tool_name === 'add_captions') covASee(state, ev);
      const tnA = String(ev.tool_name), isA = COV_A.test(tnA), hitA = covAHistory(state, MODES, ev, COV_H, () => isA && covAOps(state, MODES, ev, COV_H));   /* COV-A's own visual + its undo / redo snapshots (cov_a.js) */
      if (hitA && isA) ev._kvDone = true;   /* timelineOps leaves these alone */
      const restoredA = hitA && (tnA === 'undo' || tnA === 'redo');   /* the snapshot is the whole timeline: the step's engine clip list is not applied on top of it */
      const hit = (restoredA || ev._tlDone || timelineOps(state, MODES, ev, fam)) | (fam === 'audio' ? audioOps(state, MODES, ev) : false) | (fam === 'export' ? exportOps(state, MODES, ev) : false);
      const df = callDiff(state, ev); if (df) { const te = out.find(x => x.type === 'tool_end'); if (te) te.diff = df; }   // the picked call's Inspector Diff
      const diff = ev._kvDone || inspectorDiff(state, MODES, ev, fam);   // sceneOps already wrote the inspector
      if (fam === 'qa' && ev._readRows) readCard(state, MODES, ev.tool_name, ev._readRows);   // QA answers stay on the info panel too
      const sect = !diff && !ev._readRows && (!hit || fam === 'audio') && callSection(state, MODES, ev);   /* audio's meter hit decays: not a lasting visual */
      if (!hit && !diff && !sect) hud(state, ev, fam, ev.tool_name + ' ✓'); else hud(state, ev, fam, ev._hudText || ev.tool_name + ' ✓');
      if (cBefore) cPush(state, cBefore, cSnap(state, MODES), String(ev.tool_name), P.result);
    } else { if (fam === 'export') exportOps(state, MODES, ev); hud(state, ev, fam, ev._hudErr || ev.tool_name + ' failed'); }
    if (state._callArgs) delete state._callArgs[ev.call_id];
  }
  fix2Post(state, MODES, ev);   /* GUI-FIX-2: track overlay, outliner card, mask chip, Media / Renders rows (fix2.js) */
  return out;
}


// Read-only family (get_/list_/inspect_/probe_/sample_/query…): the call changes nothing, so nothing
// on the canvas or timeline is mutated and the workspace does not move. What the spectator sees: the
// rail entry, the HUD pill in its quiet "read" state with what came back ("get_transform · x 40, y 0"),
// and a cream peek ring on the element the call read (its clip, track label or inspector row).
const summary = r => {
  if (r == null || r === '') return '';
  if (Array.isArray(r)) return r.length + (r.length === 1 ? ' item' : ' items');
  if (typeof r !== 'object') { const s = String(r); return s.length > 36 ? s.slice(0, 35) + '…' : s; }
  const parts = [];
  for (const [k, v] of Object.entries(r)) {
    if (v == null || /^(ok|path|kimo_build|build|ts|ts_ms|dur_ms)$/.test(k)) continue;
    if (Array.isArray(v)) parts.push(`${k.replace(/_/g, ' ')} ×${v.length}`);
    else if (typeof v !== 'object') { const f = fmtVal(v, k); if (f !== '' && f.length <= 18) parts.push(`${k.replace(/_/g, ' ')} ${f}`); }
    if (parts.length === 2) break;
  }
  return parts.join(', ');
};
// GUI-FIX-1: a read's answer as a quiet card pinned to the bottom of the current screen's info panel (kv). It is
// not an Inspector row (a read changes no value), the next read replaces it, and the screen never hops.
function readCard(state, MODES, tool, rows, title) {   /* title: the card's label ('read' unless given; GUI-FIX-2 'outliner') */
  if (!rows || !rows.length) return false;
  const M = MODES[state.mode]; if (!M) return false;
  const V = views(state);
  // the Color screen's info panel is the pipeline list, a pure state display that a read leaves byte-identical
  // (cov_c contract): there the answer goes on the scopes' stats line instead
  if (Object.values(M.panels).some(sp => sp.kind === 'scopes') && Object.keys(M.panels).includes('pipe')) {
    const sk = Object.entries(M.panels).find(([, sp]) => sp.kind === 'scopes')[0];
    (V[sk] || (V[sk] = {})).stats = { text: `${tool} · ` + rows.slice(0, 3).map(r => `${r[0].toLowerCase()} ${r[1]}`).join(' · '), _flash: now() };
    return true;
  }
  const e = Object.entries(M.panels).find(([, s]) => s.kind === 'kv'); if (!e) return false;
  const K = V[e[0]] || (V[e[0]] = {});
  K.read = { tool: String(tool), rows: rows.slice(0, 4), _flash: now(), ...(title ? { title } : {}) };
  return true;
}
// GUI-FIX-1: a successful mutating call that no panel code drew (text styling and animators, an audio-effect key, a
// matte removal, a physics bake...) gets a short section in the Inspector of the screen it lands on, from its own
// args and result; text calls also put a chip on their clip in the timeline.
function callSection(state, MODES, ev) {
  const A = (state._callArgs && state._callArgs[ev.call_id]) || (ev.payload || {}).args || {}, T = ev.target || {};
  const tl = timelineView(state, MODES);
  const name = id => { if (id == null) return null; if (!RAW_ID.test(String(id))) return String(id); const [t, i] = tl ? findClip(tl, { clip_id: id }) : [null, -1]; const lb = t ? t.clips[i].label : null; return lb && !RAW_ID.test(String(lb)) ? lb : null; };
  const rows = callRows(String(ev.tool_name), A, (ev.payload || {}).result, (ev.payload || {}).changed, name(T.clip_id ?? A.clip_id));
  if (!rows.length) return false;
  const [, V] = modeViews(state, MODES, ev), m = (state.explicitMode ? null : inferMode(String(ev.tool_name))) || state.mode;
  const e = MODES[m] && Object.entries(MODES[m].panels).find(([, s]) => s.kind === 'kv'); if (!e) return false;
  const K = V[e[0]] || (V[e[0]] = {}); K.blocks = K.blocks || [];
  const i = K.blocks.findIndex(b => b._live); if (i >= 0) K.blocks.splice(i);
  const t = now();
  K.blocks.push({ sec: String(ev.tool_name), _live: true }, ...rows.map(r => ({ k: r[0], v: r[1], ...(r[2] ? { chip: true } : {}), ch: true, _live: true, _flash: t })));
  const chipOf = CHIP[String(ev.tool_name)], chip = chipOf && rows.some(r => r[2]) ? [null, String(chipOf(A))] : null;
  if (chip && tl && (T.clip_id ?? A.clip_id) != null) { const [tr, k] = findClip(tl, { clip_id: T.clip_id ?? A.clip_id, track_id: T.track_id }); if (tr) { const c = tr.clips[k]; c.fx = [...(c.fx || []).filter(x => x !== chip[1]), chip[1]]; c._flash = t; } }
  return true;
}
function inspectOps(state, MODES, ev, id) {
  const P = ev.payload || {}, T = ev.target || {}, A = P.args || {};
  const out = [];
  if (ev.phase === 'start' && ev.ts_ms != null) (state._t0 || (state._t0 = {}))[id] = ev.ts_ms;
  if (ev.phase === 'start') {
    out.push({ type: 'tool_start', id, name: ev.tool_name, args: Object.keys(A).length ? A : undefined, read: true });
    hud(state, ev, 'inspect', ev.tool_name);
    peek(state, MODES, { clip_id: T.clip_id ?? A.clip_id, track_id: T.track_id ?? A.track_id, param: T.param ?? A.param ?? A.property });
  } else if (ev.phase === 'progress') {
    const pct = P.pct;
    out.push({ type: 'tool_progress', id, progress: { pct } });
  } else {
    const err = ev.phase === 'error', dur = P.dur_ms ?? ev.duration_ms, sm = err ? '' : summary(P.result);
    out.push({ type: 'tool_end', id, name: ev.tool_name, dur_ms: dur, status: err ? 'error' : 'ok', error: P.error, read: true });
    hud(state, ev, 'inspect', ev.tool_name + (err ? ' failed' : sm ? ' · ' + sm : ' ✓'));
    if (!err) { readCard(state, MODES, ev.tool_name, readRows(String(ev.tool_name), P.result)); scopeStats(state, MODES, scopeLine(String(ev.tool_name), P.result)); }   // the answer stays on the screen's info panel
  }
  return out;
}
function peek(state, MODES, t) {
  const tl = state.liveTL || panels(state, MODES, ['timeline'])[0];
  if (t.clip_id != null) { const [tr, i] = findClip(tl, { clip_id: t.clip_id, track_id: t.track_id }); if (tr) tr.clips[i]._peek = now(); }
  else if (t.track_id != null) { const tr = findTrack(tl, t.track_id); if (tr) tr._peek = now(); }
  if (t.param != null) {
    const kv = panels(state, MODES, ['kv'])[0], k = String(t.param).toLowerCase().replace(/_/g, ' ');
    const row = kv && (kv.blocks || []).find(b => b.k && b.k.toLowerCase() === k); if (row) row._peek = now();
  }
}


// Transform family (set_property/set_transform x y scale rotation opacity, keyframes, bezier ease,
// motion path, motion blur, nulls, parenting, expressions): a gizmo on the viewer — the layer's
// bounding box with 8 handles and its anchor, the previous pose dashed, an arrow when it moved, and a
// readout of the change — plus keyframe diamonds on the clip in the timeline. Geometry comes only from
// the call: x/y are project pixels, placed when the project size is known (session/frame events'
// width/height), otherwise the move is shown in the readout and the box stays centred.
export const GIZMO_MS = 2400;
const XF = { x: 'x', y: 'y', pos_x: 'x', pos_y: 'y', scale: 'scale', rotation: 'rotation', rot_z: 'rotation', rotate: 'rotation', opacity: 'opacity', rot_x: 'rot_x', rot_y: 'rot_y', pos_z: 'pos_z' };
const numOf = v => { if (v == null || typeof v === 'object') return null; const s = String(v).trim(), n = parseFloat(s); return Number.isFinite(n) ? (s.endsWith('%') ? n / 100 : n) : null; };
function xfOps(state, MODES, ev) {
  const P = ev.payload || {}, T = ev.target || {}, A = P.args || (state._xfArgs && state._xfArgs[ev.call_id]) || {};
  if (ev.phase === 'start') { (state._xfArgs || (state._xfArgs = {}))[ev.call_id] = A; }
  const tn = String(ev.tool_name).toLowerCase(), clip = T.clip_id ?? A.clip_id ?? A.clip ?? A.id ?? null;
  const store = state.xform || (state.xform = {}), key = clip ?? '_';
  const cur = store[key] || (store[key] = { x: 0, y: 0, scale: 1, rotation: 0 });
  const prev = { ...cur };
  const g = { t: now(), tool: ev.tool_name, phase: ev.phase, clip, prev, next: cur, read: [] };
  const param = String(T.param ?? P.param ?? A.property ?? A.param ?? '').toLowerCase(), k = XF[param];
  if (ev.phase === 'complete') {
    delete (state._xfArgs || {})[ev.call_id];
    // a single field change (set_property / set_transform param=…, set_keyframe value)
    if (k) {
      const f = numOf(P.from), to = numOf(P.to ?? P.value ?? A.value);
      if (f != null) prev[k] = f;
      if (to != null) cur[k] = to;
      // the inspector's Position row reads x, y together ("0, 0 → 260, 0")
      if ((k === 'x' || k === 'y') && to != null) { const kv = panels(state, MODES, ['kv'])[0], row = kv && (kv.blocks || []).find(b => b.k && /^position$/i.test(b.k)); if (row) { row.from = `${fmtVal(prev.x)}, ${fmtVal(prev.y)}`; row.v = `${fmtVal(cur.x)}, ${fmtVal(cur.y)}`; row._flash = now(); g.posRow = true; } }
      if (to != null) g.read.push(`${param} ${P.from != null ? fmtVal(P.from, param) + ' → ' : ''}${fmtVal(P.to ?? P.value ?? A.value, param)}`);
    }
    // a pose in the args (set_transform {x, y, scale, rotation}, add_null starting pose)
    for (const [ak, sk] of Object.entries(XF)) if (ak !== param && A[ak] != null && numOf(A[ak]) != null && !/keyframe/.test(tn)) { cur[sk] = numOf(A[ak]); g.read.push(`${ak} ${fmtVal(A[ak], ak)}`); }
  }
  // the family's own marks
  if (/add_null/.test(tn)) { g.null = true; g.read.unshift('null'); }
  if (/set_parent/.test(tn)) { const pid = A.parent_id ?? A.parent; g.parent = pid ? store[pid] || { x: 0, y: 0, scale: 1, rotation: 0 } : null; g.read.unshift(pid ? `parent → ${pid}` : 'unparented'); }
  if (/motion_blur/.test(tn)) { const sa = numOf(A.shutter_angle ?? A.shutter ?? A.angle); if (sa != null) { g.blur = sa; g.read.unshift(`shutter ${sa}°`); } }
  if (/expression/.test(tn)) { const op = A.op ?? A.expression ?? A.kind; const ps = ['freq', 'amp', 'mode', 'phase'].filter(x => A[x] != null).map(x => `${x} ${fmtVal(A[x], x)}`).join(', '); if (op) g.read.unshift(`${param ? param + ' = ' : ''}${op}(${ps})`); g.expr = true; }
  if (/bezier/.test(tn)) { const b = A.bezier ?? A.curve ?? A.handles; if (Array.isArray(b) && b.length === 4 && b.every(v => numOf(v) != null)) { g.bez = b.map(numOf); g.read.unshift(`ease [${b.map(v => fmtVal(v)).join(', ')}]`); } }
  if (/motion_path/.test(tn) && Array.isArray(A.points)) { g.path = A.points.filter(p => Array.isArray(p) && p.length >= 2).map(p => [numOf(p[0]), numOf(p[1])]).filter(p => p[0] != null && p[1] != null); g.read.unshift(`motion path · ${g.path.length} points`); }
  // keyframe diamonds on the clip lane (clip-local times)
  if (/keyframe/.test(tn) && ev.phase === 'complete') {
    const times = Array.isArray(A.keys) ? A.keys.map(q => numOf(q && (q.time ?? q.t))).filter(v => v != null) : [numOf(A.time ?? A.t ?? A.at)].filter(v => v != null);
    const tl = state.liveTL || panels(state, MODES, ['timeline'])[0];
    const [tr, i] = clip != null ? findClip(tl, { clip_id: clip, track_id: T.track_id }) : [null, -1];
    if (tr && times.length) { const c = tr.clips[i]; c.keys = c.keys || []; for (const tt of times) if (!c.keys.some(q => Math.abs(q.t - (c.s + tt)) < 1e-6)) c.keys.push({ t: c.s + tt, em: !!g.bez }); else if (g.bez) c.keys.find(q => Math.abs(q.t - (c.s + tt)) < 1e-6).em = true; c._flash = now(); }
    if (!g.read.length || !k) g.read.push(`${times.length} key${times.length === 1 ? '' : 's'}${param ? ' · ' + param : ''}`);
  }
  if (/^(rot_x|rot_y|pos_z)$/.test(k || '') || ['rot_x', 'rot_y', 'pos_z'].some(q => A[q] != null)) g.three = true;
  if (!g.read.length) g.read.push(ev.tool_name);
  state.gizmo = g;   // nothing animates in JS: the viewer draws it while fresh and CSS fades it
}


// 3D / tracking / lighting / mesh family, on the panels of the screen the call opens (m07 tracking,
// m08 modelling, m11 particles, m12 lighting, m13 scene): everything drawn comes from the call's own
// arguments or result — track quads / ROI / mesh grid / point ring / matte seed on the viewer, a light
// glyph at its frame position plus its row in Lights, the mesh op appended to the Stack, a physics
// body per clip, the solve's reprojection error. Nothing is drawn for a value the call did not carry.
const E2 = x => String(x).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const quad8 = c => Array.isArray(c) && c.length >= 8 && c.slice(0, 8).every(v => numOf(v) != null) ? [0, 2, 4, 6].map(i => [numOf(c[i]), numOf(c[i + 1])]) : null;
function modeViews(state, MODES, ev) {
  const m = (state.explicitMode ? null : inferMode(String(ev.tool_name))) || state.mode;   // the screen app.js will show for this call
  return [m, MODES[m] ? (state.views[m] || (state.views[m] = {})) : views(state)];
}
function sceneOps(state, MODES, ev) {
  const P = ev.payload || {}, T = ev.target || {}, tn = String(ev.tool_name).toLowerCase();
  if (ev.phase === 'start') (state._sceneArgs || (state._sceneArgs = {}))[ev.call_id] = P.args || {};
  const A = P.args || (state._sceneArgs && state._sceneArgs[ev.call_id]) || {};
  const R = P.result && typeof P.result === 'object' ? P.result : {};
  const [mode, V] = modeViews(state, MODES, ev);
  const vw = V.viewer || (V.viewer = {});
  const ov = () => (vw.overlay = (vw.overlay || []).filter(o => o._call !== ev.call_id && !(o._kind && o._kind === tn)));
  let hit = false;
  if (ev.phase === 'start' || ev.phase === 'complete') {
    // trackers: the quad / ROI / mesh grid / point / seed outline they were given
    if (/^track_(planar|mesh|features)/.test(tn)) {
      const q = quad8(A.corners) || quad8(A.roi) || (Array.isArray(A.anchors) && A.anchors[0] && quad8(A.anchors[0].corners));
      if (q) { const o = ov(); o.push({ quad: q, grid: /mesh/.test(tn) ? [numOf(A.nx) ?? 7, numOf(A.ny) ?? 5] : null, roi: /features/.test(tn), _call: ev.call_id, _kind: tn }); hit = true; }
    }
    if (tn === 'track_point' && numOf(A.x) != null && numOf(A.y) != null) { const o = ov(); o.push({ pt: [numOf(A.x), numOf(A.y)], r: numOf(A.radius) ?? .12, _call: ev.call_id, _kind: tn }); hit = true; }
    if (tn === 'track_matte' && Array.isArray(A.seeds) && A.seeds[0] && Array.isArray(A.seeds[0].points)) {
      const pts = A.seeds[0].points.map(p => Array.isArray(p) ? [numOf(p[0]), numOf(p[1])] : null).filter(p => p && p[0] != null && p[1] != null);
      if (pts.length >= 3) { const o = ov(); o.push({ poly: pts, _call: ev.call_id, _kind: tn }); hit = true; }
    }
    // homography fits / refinements / audits (m07): the quad they work on, from their own normalised corners / ROI,
    // and on completion the numbers they report on the viewer's left meta line
    if (/^(fit_track_homography|ecc_refine_track|decompose_track_homography|corner_disagreement|lk_residual_audit|validate_planar_track)$/.test(tn)) {
      const q = quad8(A.corners) || quad8(A.roi) || (Array.isArray(A.native_quads) && quad8(A.native_quads[0])) || quad8(R.corners) || quad8(R.roi);
      if (q) { const o = ov(); o.push({ quad: q, roi: /ecc|lk_|corner_dis/.test(tn), _call: ev.call_id, _kind: tn }); hit = true; }
      if (ev.phase === 'complete') {
        const lab = { fit_track_homography: 'fit', ecc_refine_track: 'ecc', decompose_track_homography: 'decompose', corner_disagreement: 'corners', lk_residual_audit: 'lk audit', validate_planar_track: 'validate' }[tn];
        const KEYS = [['rms_px', 'px RMS'], ['p95_px', 'px p95'], ['max_px', 'px max'], ['mean_px', 'px mean'], ['rotation_deg', '°'], ['correlation', 'ncc'], ['inliers', 'inliers'], ['failed', 'failed']];
        const parts = KEYS.map(([k, u]) => numOf(R[k]) != null ? `${fmtVal(R[k])} ${u}` : null).filter(Boolean).slice(0, 2);
        if (typeof R.ok === 'boolean') parts.push(R.ok ? 'ok' : 'failed');
        if (parts.length) { vw.left = `${lab} ${parts.join(' · ')}`; hit = true; }
      }
    }
    // lens / stabilize (m07): the Brown k1/k2 the call removes, drawn as the warped grid it straightens;
    // the stabilizer's crop when the result reports its zoom
    if (tn === 'undistort_clip' && (numOf(A.k1) != null || numOf(A.k2) != null)) {
      const o = ov(); o.push({ lens: [numOf(A.k1) ?? 0, numOf(A.k2) ?? 0], c: [numOf(A.cx) ?? .5, numOf(A.cy) ?? .5].map(v => v > 1 && state.session && state.session.width ? null : v), _call: ev.call_id, _kind: tn });
      vw.left = ['undistort', numOf(A.k1) != null ? `k1 ${fmtVal(A.k1)}` : null, numOf(A.k2) != null ? `k2 ${fmtVal(A.k2)}` : null].filter(Boolean).join(' · '); hit = true;
    }
    if (tn === 'stabilize') {
      const z = numOf(R.zoom ?? R.scale ?? R.crop_scale);
      if (z != null && z >= 1) { const o = ov(); o.push({ crop: 1 / z, _call: ev.call_id, _kind: tn }); }
      vw.left = ['stabilize', numOf(A.smooth) != null ? `smooth ${fmtVal(A.smooth)}` : null, A.lock ? 'locked' : null, A.rotation === false ? 'no rotation' : null, z != null ? `zoom ${fmtVal(z)}×` : null].filter(Boolean).join(' · '); hit = true;
    }
    if (tn === 'apply_camera_solve' && ev.phase === 'complete') {
      const n = numOf(R.keys ?? R.keyframes ?? R.count);
      vw.left = ['camera solve → keys', A.solve_id != null ? String(A.solve_id) : null, n != null ? `${fmtVal(n)} keys` : null, A.fov === false ? 'no fov' : null].filter(Boolean).join(' · '); hit = true;
    }
    // corner pin / pin_layer: the pinned quad (pin_layer quads are project pixels; corner-pin offsets are pixels from the frame corners)
    if ((tn === 'pin_layer' || tn === 'set_corner_pin') && state.session && state.session.width && state.session.height) {
      const W = state.session.width, H = state.session.height;
      let q = null;
      if (tn === 'pin_layer') { const f = quad8(A.quad) || (Array.isArray(A.keys) && A.keys[0] && quad8(A.keys[0].quad)); q = f && f.map(([x, y]) => [x / W, y / H]); }
      else if (Array.isArray(A.corners) && A.corners.length === 4) { const base = [[0, 0], [1, 0], [1, 1], [0, 1]]; q = A.corners.map((d, i) => Array.isArray(d) && numOf(d[0]) != null && numOf(d[1]) != null ? [base[i][0] + numOf(d[0]) / W, base[i][1] + numOf(d[1]) / H] : null); if (q.some(v => !v)) q = null; }
      if (q) { const o = ov(); o.push({ quad: q, _call: ev.call_id, _kind: tn }); hit = true; }
    }
    // particles (m10): the emitter joins Emitters with its rate/count, gets a Simulation lane with a dot at its
    // start and end frame, and writes its own numbers into the Inspector in the mockup's sections — all from args
    if ((tn === 'add_emitter' || tn === 'add_particles' || tn === 'add_shard_burst') && ev.phase === 'complete') {
      const C = (A.config && typeof A.config === 'object') ? { ...A, ...A.config } : A;
      const name = String(C.name ?? A.preset ?? A.kind ?? (tn === 'add_shard_burst' ? 'shards' : C.shape ? `${C.shape} emitter` : 'emitter'));
      const rate = numOf(C.rate ?? C.rate_per_s ?? C.spawn_rate), count = numOf(C.count ?? C.shards ?? C.num_shards);
      const v = rate != null ? `${Math.round(rate).toLocaleString('en-US')}/s` : count != null ? Math.round(count).toLocaleString('en-US') : '';
      const P = MODES[mode] ? MODES[mode].panels : {};
      const lists = Object.entries(P).filter(([, sp]) => sp.kind === 'list');
      if (lists[0]) { const L = V[lists[0][0]] || (V[lists[0][0]] = {}); L.rows = (L.rows || []).filter(x => x._id); let r = L.rows.find(x => x._id === name); if (!r) { r = { _id: name }; L.rows.push(r); } r.k = name; r.v = v; r.em = true; r._flash = now(); for (const x of L.rows) if (x !== r) x.em = false; if (L.rows.length > 4) L.rows.splice(0, L.rows.length - 4); hit = true; }
      const kp = Object.entries(P).find(([, sp]) => sp.kind === 'keys');
      const fps = numOf(state.session.fps) ?? 24, s0 = numOf(A.start), dur = numOf(A.duration);
      if (kp && s0 != null) {
        const KK = V[kp[0]] || (V[kp[0]] = {}); KK.lanes = (KK.lanes || []).filter(l => l._pt);
        const f0 = Math.round(s0 * fps), f1 = dur != null ? Math.round((s0 + dur) * fps) : null;
        let ln = KK.lanes.find(l => l.label === name); if (!ln) { ln = { label: name, dots: [], r: 3.4, dotColor: '#8a8780', _pt: true }; KK.lanes.push(ln); }
        ln.dots = f1 != null && f1 !== f0 ? [f0, f1] : [f0]; if (KK.lanes.length > 3) KK.lanes.splice(0, KK.lanes.length - 3);
        const end = Math.max(...KK.lanes.flatMap(l => l.dots), 1), step = end > 240 ? 120 : end > 96 ? 48 : 24;
        KK.ruler = { t0: 0, t1: Math.ceil(end / step) * step, step, fmt: 'n' }; KK.playhead = f0; hit = true;
      }
      const ins = Object.entries(P).find(([, sp]) => sp.kind === 'kv');
      if (ins) {
        const K = V[ins[0]] || (V[ins[0]] = {}); K.sub = tn === 'add_shard_burst' ? 'shard_burst' : name;
        const num = x => numOf(Array.isArray(x) ? x[1] : x);
        const secs = [[tn === 'add_shard_burst' ? 'Fracture' : 'Emitter', [['Shards', tn === 'add_shard_burst' ? count : null], ['Seed', C.seed], ['Rate', rate != null ? `${Math.round(rate).toLocaleString('en-US')}/s` : null]]],
          ['Physics', [['Gravity', C.gravity], ['Restitution', C.restitution ?? C.bounce], ['Drag', C.drag], ['Turbulence', C.turbulence], ['Wind', C.wind]]],
          ['Particles', [['Count', tn !== 'add_shard_burst' ? count : null], ['Lifetime', C.life ?? C.lifetime], ['Size', C.size ?? C.size0]]]];
        const blocks = [];
        for (const [sec, rows] of secs) { const rr = rows.filter(([, x]) => x != null && (typeof x === 'string' || num(x) != null)); if (!rr.length) continue; blocks.push({ sec }); for (const [k, x] of rr) blocks.push({ k, v: typeof x === 'string' ? x : k === 'Lifetime' ? `${fmtVal(num(x))} s` : (k === 'Shards' || k === 'Count') ? Math.round(num(x)).toLocaleString('en-US') : fmtVal(num(x), k.toLowerCase()), chip: true, _flash: now() }); }
        if (blocks.length) { K.blocks = blocks; ev._kvDone = true; ev._ptKv = true; hit = true; }
      }
    }
    // lights: glyph at its frame position, row in the Lights list
    if (tn === 'set_light' || tn === 'light_rig') {
      const lists = Object.entries(MODES[mode] ? MODES[mode].panels : {}).filter(([, sp]) => sp.kind === 'list').map(([k]) => V[k] || (V[k] = {}));
      const L = lists[0];
      const add = (id, kind, v, pos, col) => {
        if (L) { L.rows = L.rows || []; let r = L.rows.find(x => x._id === id); if (!r) { r = { _id: id, icon: 'bulb' }; L.rows.push(r); } r.k = id; r.v = v; r.em = true; r._flash = now(); for (const x of L.rows) if (x !== r) x.em = false; }
        if (pos) { const o = ov(); o.push({ light: pos, kind, color: col, label: id, _call: ev.call_id + id, _kind: 'light:' + id }); }
      };
      if (tn === 'set_light') {
        const pos = Array.isArray(A.pos) && numOf(A.pos[0]) != null && numOf(A.pos[1]) != null ? [numOf(A.pos[0]), numOf(A.pos[1])] : null;
        const col = Array.isArray(A.color) && A.color.length >= 3 ? A.color.slice(0, 3).map(numOf) : null;
        const tK = numOf(A.temperature ?? A.kelvin), v = tK != null ? `${Math.round(tK)}K` : [A.kind, numOf(A.intensity) != null ? fmtVal(A.intensity) : null].filter(Boolean).join(' · ');   // m11 Lights shows the colour temperature
        if (A.id != null || A.kind != null) { add(String(A.id ?? A.kind), A.kind, v, pos, col); hit = true;
          // the 3D screen's Scene tree carries the light by its id (it lives in the project the camera sees)
          const SV = MODES.scene3d ? (state.views.scene3d || (state.views.scene3d = {})) : null, trl = MODES.scene3d ? Object.entries(MODES.scene3d.panels).find(([, sp]) => sp.kind === 'tree') : null;
          if (trl && SV) { const S = SV[trl[0]] || (SV[trl[0]] = {}); S.items = S.items || []; const nm = String(A.id ?? A.kind); if (!S.items.some(x => x.name === nm)) { const at = S.items.findIndex(x => x.icon !== 'cam' && x.icon !== 'light' && x.icon !== 'fill'); S.items.splice(at < 0 ? S.items.length : at, 0, { name: nm, icon: /fill/i.test(nm) ? 'fill' : 'light', _flash: now() }); if (S.items.length > 14) S.items.length = 14; } } }
      } else if (A.preset && ev.phase === 'complete') {
        const pre = String(A.prefix ?? 'rig'), sub = Array.isArray(A.subject) ? A.subject.map(numOf) : null;
        const ids = Array.isArray(R.lights) ? R.lights.map(x => typeof x === 'string' ? x : x && x.id).filter(Boolean) : ['key', 'fill', 'rim'].map(r => `${pre}_${r}`);
        ids.forEach(id => add(id, null, String(A.preset), null, null));
        if (sub && sub[0] != null) { const o = ov(); o.push({ light: sub, kind: 'subject', label: String(A.preset), _call: ev.call_id, _kind: tn }); }
        hit = true;
      }
    }
    // materials / relight / remove_light (m11): the Inspector's Material rows from set_material's own args,
    // a removed light leaves Lights and the rig table, relight names its layer on the viewer tag
    if (ev.phase === 'complete' && /^(set_material|remove_material|preview_material|set_relight|remove_light)$/.test(tn)) {
      const PP = MODES[mode] ? MODES[mode].panels : {};
      const kvn2 = Object.entries(PP).find(([, sp]) => sp.kind === 'kv');
      if (tn === 'set_material' && kvn2) {
        const K = V[kvn2[0]] || (V[kvn2[0]] = {}); const nm = String(A.name ?? 'material');
        const alb = Array.isArray(A.albedo) && A.albedo.length >= 3 ? A.albedo.slice(0, 3).map(x => (numOf(x) ?? 0).toFixed(2)).join('  ') : null;
        const F = [['Albedo', alb], ['Roughness', numOf(A.roughness)], ['Metallic', numOf(A.metal)], ['IOR', numOf(A.ior)], ['Clearcoat', numOf(A.clearcoat)]].filter(([, v]) => v != null);
        const keep = (K.blocks || []).filter(b => b.widget);
        K.sub = nm; K.blocks = [...keep, { sec: 'Material' }, ...F.map(([k, v]) => ({ k, v: typeof v === 'number' ? v.toFixed(2) : v, _flash: now() }))]; ev._kvDone = true; hit = true;
      }
      if (tn === 'remove_material' && kvn2) { const K = V[kvn2[0]] || (V[kvn2[0]] = {}); if (K.sub === String(A.name)) { K.blocks = (K.blocks || []).filter(b => b.widget); K.sub = ''; } ev._kvDone = true; hit = true; }
      if (tn === 'preview_material') { const p = R.path; if (typeof p === 'string') { vw.left = `preview · ${String(A.material ?? '')}${numOf(R.mean) != null ? ` · mean ${fmtVal(R.mean)}` : ''}`.trim(); hit = true; } }
      if (tn === 'set_relight') { const lay = A.clip_id; if (lay != null) { vw.tag = `relight ${String(lay)}`; hit = true; } }
      if (tn === 'remove_light' && A.id != null) {
        const id = String(A.id);
        for (const [k, sp] of Object.entries(PP)) { const D = V[k]; if (!D) continue; if (sp.kind === 'list' && D.rows) D.rows = D.rows.filter(x => x._id !== id && x.k !== id); if (sp.kind === 'lightrig') { if (D.rows) D.rows = D.rows.filter(x => x[0] !== id); if (D.lights) D.lights = D.lights.filter(x => x.name !== id && x._id !== id); } }
        vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'light:' + id); hit = true;
      }
    }
    // mesh ops: a node on the Stack (modelling), newest in ember
    if (/^(mesh_|generate_mesh_primitive|contour_sdf_graph|set_mesh_pin|derive_vertex_tangents)/.test(tn) && ev.phase === 'complete') {
      const stk = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'stack');
      if (stk) { const S = V[stk[0]] || (V[stk[0]] = {}); S.nodes = S.nodes || []; for (const n of S.nodes) n.em = false; const nm = { mesh_lathe: 'lathe', mesh_subdivide: 'subdivide', mesh_boolean: 'boolean ' + (A.op ?? A.operation ?? 'diff'), mesh_extrude_path: 'extrude path', generate_mesh_primitive: String(A.shape ?? (A.primitive && typeof A.primitive === 'object' ? A.primitive.kind : A.primitive) ?? A.kind ?? 'primitive')   /* GUI-FIX-2: primitive is {kind,…}, not a string */, contour_sdf_graph: 'contour', set_mesh_pin: 'pin', derive_vertex_tangents: 'tangents' }[tn] || tn.replace(/^mesh_/, ''); S.nodes.push({ name: nm, em: true, _flash: now() }); if (S.nodes.length > 7) S.nodes.splice(0, S.nodes.length - 7); hit = true; }
    }
    // camera (m13): orbit ring with the camera on it at its pan, a frustum wedge opening at its fov,
    // a diamond per keyframed pan, the axis triad turned by the pan; scene objects into the Scene tree
    if (/^set_camera/.test(tn) && ev.phase === 'complete') {
      const cam = state.cam || (state.cam = {}), keys = state.camKeys || (state.camKeys = []);
      if (tn === 'set_camera') for (const k of ['cam_x', 'cam_y', 'cam_z', 'cam_pan', 'cam_tilt', 'cam_roll', 'cam_fov']) { if (numOf(A[k]) != null) cam[k] = numOf(A[k]); }
      else if (A.property && numOf(A.value) != null) { cam[A.property] = numOf(A.value); keys.push({ prop: A.property, t: numOf(A.time), v: numOf(A.value) }); if (keys.length > 24) keys.shift(); }
      vw.overlay = (vw.overlay || []).filter(x => x._kind !== 'camera');
      // Keyframes panel: one lane per keyed camera property, diamonds at the keys' timeline times, newest in ember
      const kp = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'keys');
      const timed = keys.filter(k => k.t != null);
      if (kp && timed.length) {
        const KK = V[kp[0]] || (V[kp[0]] = {}), last = timed[timed.length - 1], props = [...new Set(timed.map(k => k.prop))];
        KK.ruler = { t0: 0, t1: Math.max(4, Math.ceil(Math.max(...timed.map(k => k.t)) + 1)), step: 1, fmt: 's', align: 'left' };
        KK.lanes = props.map(pr => ({ label: `<span class=cream>camera</span> <span class=dim>${E2(pr.replace(/^cam_/, ''))}</span>`, html: true, line: true, sel: pr === last.prop, keys: timed.filter(k => k.prop === pr).map(k => k === last ? { t: k.t, em: true } : k.t) }));
        KK.playhead = last.t;
      }
      const o = ov(); o.push({ cam: { ...cam }, keys: keys.filter(k => k.prop === 'cam_pan').map(k => k.v), _call: ev.call_id, _kind: 'camera' });
      const kvn = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'kv');
      if (kvn) { ev._kvDone = true; const K = V[kvn[0]] || (V[kvn[0]] = {}); K.sub = 'camera';
        // m13's rows: Transform (Position x/y/z, Rotation tilt/pan/roll, FOV), each only from the values the calls set
        const keep = (K.blocks || []).filter(b => b._cam); K.blocks = keep.length ? keep : [{ sec: 'Transform', _cam: true }];
        const f = k => cam[k] != null ? fmtVal(cam[k]) : null;
        const pos = ['cam_x', 'cam_y', 'cam_z'].some(k => cam[k] != null) ? ['X', 'Y', 'Z'].map((l, i) => f(['cam_x', 'cam_y', 'cam_z'][i]) != null ? `${l} ${f(['cam_x', 'cam_y', 'cam_z'][i])}` : null).filter(Boolean).join('  ') : null;
        const rot = ['cam_tilt', 'cam_pan', 'cam_roll'].some(k => cam[k] != null) ? ['cam_tilt', 'cam_pan', 'cam_roll'].map(k => f(k) ?? '0').join(', ') : null;
        for (const [lab, v] of [['Position', pos], ['Rotation', rot], ['FOV', f('cam_fov')]]) { if (v == null) continue; let r = K.blocks.find(b => b.k === lab); if (!r) { r = { k: lab, _cam: true }; K.blocks.push(r); } if (r.v !== v) { if (r.v != null && lab !== 'Position') r.from = r.v; r.v = v; r._flash = now(); } }
        const ord = ['Transform', 'Position', 'Rotation', 'FOV']; K.blocks.sort((a, b) => ord.indexOf(a.sec ?? a.k) - ord.indexOf(b.sec ?? b.k)); }
      // the Scene tree carries the camera
      const trc = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'tree');
      if (trc) { const S = V[trc[0]] || (V[trc[0]] = {}); S.items = S.items || []; let it = S.items.find(x => x.icon === 'cam'); if (!it) { it = { name: 'camera', icon: 'cam' }; S.items.unshift(it); } for (const x of S.items) x.sel = x === it; it._flash = now(); }
      hit = true;
    }
    if (tn === 'add_scene3d' && ev.phase === 'complete' && A.config && typeof A.config === 'object') {
      const tr = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'tree');
      const objs = Array.isArray(A.config.objects) ? A.config.objects : [], lts = Array.isArray(A.config.lights) ? A.config.lights : [];
      if (tr && (objs.length || lts.length)) { const S = V[tr[0]] || (V[tr[0]] = {}); S.items = S.items || []; for (const x of S.items) x.sel = false;
        objs.forEach((ob, i) => S.items.push({ name: String(ob.name ?? ob.id ?? `${ob.kind ?? 'object'} ${i + 1}`), icon: ob.kind === 'plane' ? 'plane' : 'mesh', d: ob.parent ? 1 : 0, sel: i === objs.length - 1, _flash: now() }));
        lts.forEach((l, i) => { const nm = String(l.name ?? `${l.kind ?? 'light'} ${i + 1}`); S.items.push({ name: nm, icon: /fill/i.test(nm) ? 'fill' : 'light' }); });
        if (S.items.length > 14) S.items.splice(0, S.items.length - 14); hit = true; }
    }
    // physics bodies: one ring per body at its pixel offset from the canvas centre (needs the project size)
    if (tn === 'simulate_physics' && Array.isArray(A.bodies) && state.session.width) {
      const o = ov(); for (const b of A.bodies) if (b && numOf(b.x) != null && numOf(b.y) != null) o.push({ body: [.5 + numOf(b.x) / state.session.width, .5 + numOf(b.y) / state.session.height], r: (numOf(b.radius_px) ?? 20) / state.session.width, label: b.clip_id, _call: ev.call_id, _kind: tn }); hit = true;
    }
  }
  if (ev.phase === 'complete') {
    // Inspector: the call's own args for what it targeted (light / track / camera / mesh), no placeholders
    const kvn = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'kv');
    if (kvn && !/^set_camera/.test(tn) && !ev._ptKv) {
      const K = V[kvn[0]] || (V[kvn[0]] = {}); K.blocks = K.blocks || [];
      const i = K.blocks.findIndex(b => b._live); if (i >= 0) K.blocks.splice(i);
      const fmtA = v => Array.isArray(v) ? (v.length <= 4 && v.every(x => numOf(x) != null) ? v.map(x => fmtVal(x)).join(', ') : null) : (typeof v === 'object' || v == null ? null : fmtVal(v));
      const lab = k => String(k).replace(/_/g, ' ').replace(/^./, c => c.toUpperCase());
      const rows = Object.entries(A).filter(([k]) => !/^(clip_id|target_clip_id|return_|verts0)/.test(k)).map(([k, v]) => [k, fmtA(v)]).filter(([, v]) => v != null && v !== '').slice(0, 6);
      if (rows.length) { K.blocks.push({ sec: String(A.id ?? A.clip_id ?? ev.tool_name), _live: true }); for (const [k, v] of rows) K.blocks.push({ k: lab(k), v, _live: true, _flash: now() }); hit = true; ev._kvDone = true; }
    }
    // Light rig table: one row per light the calls named, only the columns its args carry
    if (tn === 'set_light' && (A.id != null || A.kind != null)) {
      const rg = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'lightrig');
      if (rg) { const G2 = V[rg[0]] || (V[rg[0]] = {}); G2.rows = G2.rows || []; const id = String(A.id ?? A.kind); let r = G2.rows.find(x => x[0] === id);
        const inten = numOf(A.intensity), temp = numOf(A.temperature ?? A.kelvin);
        if (!r) { r = [id, '', '', 0, false, '']; G2.rows.push(r); }
        r[1] = temp != null ? Math.round(temp) + 'K' : r[1]; if (inten != null) { r[2] = fmtVal(inten); r._i = inten; } if (numOf(A.falloff) != null) r[5] = fmtVal(A.falloff);
        if (numOf(A.falloff) != null) r[5] = numOf(A.falloff).toFixed(1);
        const mx = Math.max(...G2.rows.map(x => x._i || 0)) || 1; for (const x of G2.rows) { x[3] = (x._i || 0) / mx; x[4] = x === r; }
        // the rig diagram: the light on the ring at the angle of its frame position around the subject
        if (Array.isArray(A.pos) && numOf(A.pos[0]) != null && numOf(A.pos[1]) != null) { G2.lights = (G2.lights || []).filter(l => l.name !== id); for (const l of G2.lights) l.em = false; G2.lights.push({ name: id.replace(/^rig_/, ''), _id: id, a: Math.round(Math.atan2(.5 - numOf(A.pos[1]), numOf(A.pos[0]) - .5) * 180 / Math.PI), em: true }); }
        hit = true; }
    }
    // Track residual graph: per-frame residuals when the result carries them (else the mockup's empty state)
    const res = [R.frame_rms, R.residuals, R.residuals_px, R.per_frame_rms].find(a => Array.isArray(a) && a.length > 1 && a.every(v => numOf(v) != null));
    if (res && /track|solve/.test(tn)) { const kp = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'keys'); if (kp) { const KK = V[kp[0]] || (V[kp[0]] = {}); const n = res.length, mx = Math.max(...res.map(numOf)); KK.ruler = { t0: 0, t1: n - 1, step: Math.max(1, Math.round(n / 6)), fmt: 'n' }; KK.graph = { pts: res.map(numOf), max: Math.max(1, Math.ceil(mx)), thr: KK.graph && KK.graph.thr }; hit = true; } }
    // a solve / track reports its error: the viewer's left meta line (m07 "solve 0.70 px RMS")
    const rms = numOf(R.rms_px ?? R.rms ?? R.reprojection_rms ?? R.mean_error_px ?? (Array.isArray(R.solves) && R.solves[0] ? R.solves[0].rms_px : null));
    if (rms != null && /solve|track/.test(tn)) { vw.left = `${/solve/.test(tn) ? 'solve' : 'track'} ${rms.toFixed(2)} px RMS`; hit = true; }
    if (tn === 'particle_simulate_raw') { const L = Object.entries(MODES[mode] ? MODES[mode].panels : {}).find(([, sp]) => sp.kind === 'list'); const n = numOf(A.steps); if (L && n != null) { const X2 = V[L[0]] || (V[L[0]] = {}); X2.rows = X2.rows || []; let r = X2.rows.find(x => x._id === 'raw'); if (!r) { r = { _id: 'raw', k: 'raw sim' }; X2.rows.push(r); } r.v = `${fmtVal(n)} steps`; r._flash = now(); hit = true; } }
    delete (state._sceneArgs || {})[ev.call_id];
  }
  return hit;
}

/* helpers project_ops.js reads (it imports them back: ES modules allow the cycle, nothing runs at load) */
export { now as _now, RAW_ID, findClip, findTrack, timelineView, numOf, fitRuler, readCard };
