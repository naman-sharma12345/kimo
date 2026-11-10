// GUI-FIX-2 (2026-10-17): what the 131-tool audit (gui_cov2.md) found landing only in the rail, a generic read card or
// the HUD toast now lands where it belongs. Everything drawn comes from the call's own args / result / `changed`,
// never a guess and never a raw id:
//   1. tracking (m07): the quad / feature points / residual vectors / per-corner disagreement rings / long-gap boxes on
//      the Tracking viewer, plus one "track" chip with the numbers the call answered;
//   2. mesh / material / camera / particle calls: a scene outliner card (newest entry first) pinned to the info panel
//      of the screen the call lands on;
//   3. roto_propagate: the propagated mask path on the viewer and a "mask" chip (keys, frames filled, snap);
//   4. a render the engine refuses: the viewer's out-of-date chip reads "render refused: <reason>". The reason is the
//      engine's own `frame_skipped` reason when it sends one (demo mode). Headless mode sends no reason (preview.rs drops
//      the frame silently), so for the grade graph the GUI mirrors the graph from each call's `changed` and applies the
//      engine's completeness rule for reachable nodes (kimo-core color/node_graph.rs Graph::apply); the node reads as
//      its effect name. The chip only shows while the viewer really is behind the project (no newer frame arrived);
//   5. relink_media -> a Media row; render_scene3d_sequence -> a Renders row (Export screen); template_describe -> a
//      read card;
//   6. a refusal for a cargo feature that is not compiled in: a "feature not compiled: <feature>" badge on the viewer
//      the feature names read from the engine's error text.
import { inferMode } from './modes.js';
import { _now as now, numOf, RAW_ID, readCard } from './router.js';

const r2 = x => String(Math.round(x * 100) / 100);
const r3 = x => String(Math.round(x * 1000) / 1000);
const pxs = x => (numOf(x) == null ? null : (Math.abs(x) < 0.01 && x !== 0 ? Number(x).toExponential(1) : r2(x)) + ' px');
const RAWISH = /^(c|clip|t|track|fx|effect|node|n|layer|l|asset|media|an|marker|mask|m|cam|light|obj|mat|material|fit|solve)[_-]?\d+$/i;   // an engine handle, never shown
const clean = s => (s != null && s !== '' && !RAW_ID.test(String(s)) && !RAWISH.test(String(s)) ? String(s) : null);
// a list the event emitter truncated ("…+4" as the last item) still counts every element
const cnt = a => { if (!Array.isArray(a)) return null; const m = a.length && typeof a[a.length - 1] === 'string' && /^…\+(\d+)$/.exec(a[a.length - 1]); return m ? a.length - 1 + Number(m[1]) : a.length; };
const quad8 = c => Array.isArray(c) && c.length >= 8 && c.slice(0, 8).every(v => numOf(v) != null) ? [0, 2, 4, 6].map(i => [numOf(c[i]), numOf(c[i + 1])]) : null;
function screenOf(state, MODES, tool) {
  const m = (state.explicitMode ? null : inferMode(String(tool))) || state.mode;
  return MODES[m] ? m : state.mode;
}
function panelOf(state, MODES, mode, kind) {
  const M = MODES[mode]; if (!M) return null;
  const e = Object.entries(M.panels).find(([, sp]) => sp.kind === kind); if (!e) return null;
  const V = state.views[mode] || (state.views[mode] = {});
  return V[e[0]] || (V[e[0]] = {});
}
// one chip of a kind on a viewer, top-left, under any chip another family placed there
function chip(vw, kind, text, cls) {
  const others = (vw.tags || []).filter(t => t._b2 !== kind && !t.pos);
  const mine = { text, _b2: kind, cls, y: 10 + 34 * others.filter(t => (t.y ?? 10) < 200).length };
  vw.tags = [...(vw.tags || []).filter(t => t._b2 !== kind), mine];
}
const dropChip = (vw, kind) => { if (vw && vw.tags) vw.tags = vw.tags.filter(t => t._b2 !== kind); };

// ---------- 6. feature-gated refusals ----------
const FEATURES = ['native-ocio', 'mesh-import', 'material-textures', 'flat-sah', 'gpu3d', 'surface-contour'];   // kimo-core Cargo.toml [features]
export function gatedFeatures(err) {
  const s = String(err ?? '');
  if (!/feature|requires/i.test(s)) return [];
  return FEATURES.filter(f => s.includes(f));
}
export function fix2Pre(state, MODES, ev) {
  if (ev.phase === 'start') {   // a new call: the previous refusal's badge has been answered by the agent moving on
    for (const V of Object.values(state.views || {})) for (const v of Object.values(V || {})) if (v && v.tags && v.tags.some(t => t._b2 === 'gated')) dropChip(v, 'gated');
    return;
  }
  if (ev.phase !== 'error') return;
  const f = gatedFeatures((ev.payload || {}).error);
  if (!f.length) return;
  const text = `feature not compiled: ${f.join(', ')}`;
  ev._gated = text;
}
function gatedPost(state, MODES, ev) {
  if (!ev._gated) return false;
  const mode = screenOf(state, MODES, ev.tool_name), vw = panelOf(state, MODES, mode, 'viewer');
  if (vw) { chip(vw, 'gated', ev._gated, 'gated'); return true; }
  return readCard(state, MODES, ev.tool_name, [['Refused', ev._gated]]);
}

// ---------- 1. tracking ----------
const TRACK = /^(track_features|fit_track_homography|decompose_track_homography|ecc_refine_track|corner_disagreement|lk_residual_audit|track_planar_anchored|validate_planar_track|track_feature_residuals|track_jitter_metrics|track_long_gap)$/;
function trackPost(state, MODES, ev) {
  const tn = String(ev.tool_name); if (!TRACK.test(tn) || ev.phase !== 'complete') return false;
  const vw = panelOf(state, MODES, screenOf(state, MODES, tn), 'viewer') || panelOf(state, MODES, 'tracking', 'viewer'); if (!vw) return false;   // a read-only tracker (long gap, residuals, jitter) draws on the screen it is read from
  const P = ev.payload || {}, A = (state._b2Args && state._b2Args[ev.call_id]) || P.args || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const S = state.session || {}, W = numOf(S.width) || 640, H = numOf(S.height) || 360;
  const T = state._trk || (state._trk = {});
  const norm = (p, w = W, h = H) => (Array.isArray(p) && numOf(p[0]) != null && numOf(p[1]) != null ? [numOf(p[0]) / w, numOf(p[1]) / h] : null);
  // the quad this call works on: its own corners / ROI / first anchor, else the engine's px corners, else the last fit's
  let q = quad8(A.corners) || quad8(A.roi) || (Array.isArray(A.anchors) && A.anchors[0] && quad8(A.anchors[0].corners));
  if (!q) { const qp = quad8(R.corners0_px) || quad8(R.roi_px); if (qp) q = qp.map(p => norm(p)); }
  if (q) T.quad = q; else q = T.quad || null;
  const ov = { _call: ev.call_id, _kind: 'b2track' };
  if (q) ov.quad = q;
  let text = null;
  const f = R.summary || {};
  switch (tn) {
    case 'track_features':
      ov.roi = true;
      text = ['features', numOf(R.features) != null ? `${R.features} found` : null, numOf(R.survived_all_frames) != null ? `${R.survived_all_frames} survived` : null, numOf(R.frames) != null ? `${R.frames} frames` : null].filter(Boolean).join(' · ');
      break;
    case 'fit_track_homography':
      text = ['fit', numOf(f.fit_pooled_frame_median_px) != null ? `median ${pxs(f.fit_pooled_frame_median_px)}` : null, numOf(f.fit_worst_frame_p95_px) != null ? `worst p95 ${pxs(f.fit_worst_frame_p95_px)}` : null, numOf(f.held_out_features) != null ? `${f.held_out_features} held out` : null].filter(Boolean).join(' · ');
      break;
    case 'decompose_track_homography': {
      const p = norm(R.point_px); if (p) { ov.pts = [{ p }]; }
      const rot = f.rotation_deg || {}, sm = f.scale_major || {};
      text = ['decompose', numOf(rot.last) != null ? `rot ${r3(rot.last)}°` : null, numOf(sm.last) != null ? `scale ${r3(sm.last)}` : null, Array.isArray(f.orientation_flip_frames) ? `${f.orientation_flip_frames.length} flips` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'ecc_refine_track': {
      const m = R.corner_move_px || {};
      text = ['ecc', numOf(R.refined) != null ? `${R.refined}/${R.frames ?? '?'} refined` : null, numOf(m.max) != null ? `corner move max ${pxs(m.max)}` : null, Array.isArray(R.failed_frames) ? `${R.failed_frames.length} failed` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'corner_disagreement': {
      const pc = Array.isArray(R.per_corner) ? R.per_corner : [];
      if (q && pc.length) ov.rings = pc.filter(c => c && numOf(c.corner) != null && q[c.corner] && numOf(c.max) != null).map(c => ({ p: q[c.corner], r: 6 + 10 * Math.min(3, c.max) }));
      const mx = Math.max(...pc.map(c => numOf(c && c.max) ?? 0)), wf = Array.isArray(R.worst_frames) && R.worst_frames[0];
      text = ['corners', pc.length ? `max ${pxs(mx)}` : null, wf && numOf(wf.frame) != null ? `worst frame ${wf.frame}` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'lk_residual_audit':
      ov.roi = true;
      text = ['lk audit', numOf(R.features_detected) != null ? `${R.features_detected} features` : null, numOf(R.median_of_frame_medians_px) != null ? `median ${pxs(R.median_of_frame_medians_px)}` : null, numOf(R.worst_p95_px) != null ? `worst p95 ${pxs(R.worst_p95_px)}` : null].filter(Boolean).join(' · ');
      break;
    case 'track_planar_anchored': {
      const g = Array.isArray(R.gaps) ? R.gaps.filter(x => x && typeof x === 'object') : [], need = g.filter(x => x.needs_anchor);
      const cl = Math.max(0, ...g.map(x => numOf(x.forward_closure_px) ?? 0));
      text = ['anchored', numOf(R.frames) != null ? `${R.frames} frames` : null, (gn => `${gn} gap${gn === 1 ? ' needs' : 's need'} an anchor`)(numOf(R.gaps_needing_anchor) ?? need.length), need[0] && numOf(need[0].suggest_anchor_frame) != null ? `try frame ${need[0].suggest_anchor_frame}` : null, g.length ? `closure ${pxs(cl)}` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'validate_planar_track': {
      const d = R.disagreement_px || {};
      text = ['validate', numOf(R.flagged_frames) != null ? `${R.flagged_frames}/${R.tested_frames ?? '?'} flagged` : null, numOf(d.max) != null ? `max ${pxs(d.max)}` : null, numOf(R.untested_frames) ? `${R.untested_frames} untested` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'track_feature_residuals': {
      // the largest residuals: the tracked point, and the residual vector magnified so a sub-pixel error is visible
      const L = Array.isArray(R.largest) ? R.largest.filter(x => x && typeof x === 'object') : [], MAG = 20;
      ov.pts = L.map(x => { const p = norm(x.tracked) || norm(x.p0), pr = norm(x.predicted); if (!p) return null; const v = pr ? [(p[0] - pr[0]) * MAG, (p[1] - pr[1]) * MAG] : null; return { p, v }; }).filter(Boolean);
      const ls = R.length_stats_px || {};
      text = ['residuals', numOf(R.frame) != null ? `frame ${R.frame}` : null, numOf(R.compared) != null ? `${R.compared} ${R.set === 'heldout' ? 'held out' : 'compared'}` : null, numOf(ls.max) != null ? `max ${pxs(ls.max)}` : null, ov.pts.length ? `vectors ×${MAG}` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'track_jitter_metrics': {
      const F = R.fit || {}, ac = F.accel_px || {}, st = F.step_px || {};
      const pc = Array.isArray(F.per_corner) ? F.per_corner : [];
      if (q && pc.length) ov.rings = pc.filter(c => c && numOf(c.corner) != null && q[c.corner] && numOf(c.accel_p95) != null).map(c => ({ p: q[c.corner], r: 6 + 10 * Math.min(3, c.accel_p95) }));
      text = ['jitter', numOf(ac.p95) != null ? `accel p95 ${pxs(ac.p95)}` : null, numOf(st.max) != null ? `step max ${pxs(st.max)}` : null].filter(Boolean).join(' · ');
      break;
    }
    case 'track_long_gap': {
      const as = Array.isArray(R.analysis_size) ? R.analysis_size : [W, H], sw = numOf(as[0]) || W, sh = numOf(as[1]) || H;
      const sm = Array.isArray(R.samples) ? R.samples.filter(x => x && typeof x === 'object' && x.bbox) : [];
      ov.boxes = sm.map(x => [numOf(x.bbox.x) / sw, numOf(x.bbox.y) / sh, numOf(x.bbox.w) / sw, numOf(x.bbox.h) / sh]).filter(b => b.every(v => v != null && Number.isFinite(v)));
      if (!ov.quad && ov.boxes.length) delete ov.quad;
      if (ov.boxes.length) delete ov.quad;   // its box is the target, not the planar quad
      const n = cnt(R.samples), lost = sm.filter(x => /lost/.test(String(x.state))).length;
      text = ['long gap', n != null ? `${n} samples` : null, sm.length ? `${sm.filter(x => /visible|seed/.test(String(x.state))).length}/${sm.length} listed visible` : null, lost ? `${lost} lost` : null].filter(Boolean).join(' · ');
      break;
    }
  }
  vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'b2track' && !(o._call === ev.call_id));
  if (ov.quad || ov.pts || ov.boxes || ov.rings) vw.overlay.push(ov);
  if (text) chip(vw, 'track', text, 'trk2');
  return true;
}

// ---------- 2. scene outliner card ----------
const OUTL = /^(generate_mesh_primitive|mesh_boolean|mesh_extrude_path|mesh_lathe|mesh_subdivide|derive_vertex_tangents|contour_sdf_graph|remove_material|apply_camera_solve|sample_scene3d_animation|inspect_mesh_buffers|inspect_mesh_primitive|particle_simulate_raw)$/;
const vt = (v, t) => [v != null ? `${v} v` : null, t != null ? `${t} t` : null].filter(Boolean).join(' · ');
function outlinerEntry(tn, A, R) {
  const G = R.geometry || {}, I = R.inspection || {}, prim = (A.primitive && A.primitive.kind) || (G.primitive && G.primitive.kind) || (R.primitive && R.primitive.kind) || (A.geometry && A.geometry.primitive && A.geometry.primitive.kind);
  switch (tn) {
    case 'generate_mesh_primitive': return ['Mesh', [clean(prim) ?? 'primitive', vt(cnt(G.vertices), cnt(G.indices))].filter(Boolean).join(' · ')];
    case 'mesh_boolean': return ['Mesh', [`boolean ${clean(A.op ?? A.operation) ?? ''}`.trim(), vt(numOf(I.vertices), numOf(I.triangles)), numOf(I.volume) != null ? `vol ${r3(I.volume)}` : null].filter(Boolean).join(' · ')];
    case 'mesh_extrude_path': return ['Mesh', ['extrude path', vt(cnt(G.vertices), cnt(G.indices))].filter(Boolean).join(' · ')];
    case 'mesh_lathe': return ['Mesh', ['lathe', numOf(A.segments) != null ? `${A.segments} seg` : null, vt(cnt(G.vertices), cnt(G.indices))].filter(Boolean).join(' · ')];
    case 'mesh_subdivide': return ['Mesh', ['subdivided', vt(numOf(I.final_vertices), numOf(I.final_triangles))].filter(Boolean).join(' · ')];
    case 'derive_vertex_tangents': return ['Mesh', ['tangents', vt(cnt(R.vertices), cnt(R.indices))].filter(Boolean).join(' · ')];
    case 'contour_sdf_graph': { const st = R.stream || {}; return ['Mesh', ['SDF contour', numOf(R.resolution) != null ? `res ${R.resolution}` : null, vt(cnt(st.vertices), cnt(st.indices))].filter(Boolean).join(' · ')]; }
    case 'remove_material': { const nm = clean(R.removed ?? A.name); return ['Material', nm ? `${nm} removed` : 'removed']; }
    case 'apply_camera_solve': return ['Camera', ['solve applied', numOf(R.keys) != null ? `${R.keys} keys` : null, Array.isArray(R.span) && numOf(R.span[1]) != null ? `${r2(R.span[1] - (numOf(R.span[0]) ?? 0))} s` : null].filter(Boolean).join(' · ')];
    case 'sample_scene3d_animation': return ['Animation', [numOf(R.nodes_animated) != null ? `${R.nodes_animated} node${R.nodes_animated === 1 ? '' : 's'}` : null, R.camera_animated === false ? 'camera still' : R.camera_animated ? 'camera moves' : null, numOf(R.lights_animated) != null ? `${R.lights_animated} lights` : null].filter(Boolean).join(' · ')];
    case 'inspect_mesh_buffers': return ['Mesh', ['buffers', vt(numOf(R.vertices), numOf(R.triangles))].filter(Boolean).join(' · ')];
    case 'inspect_mesh_primitive': return ['Mesh', [clean(prim) ?? 'primitive', vt(numOf(R.vertex_count), numOf(R.triangle_count)), numOf(R.stride_bytes) != null ? `${R.stride_bytes} B/v` : null].filter(Boolean).join(' · ')];
    case 'particle_simulate_raw': { const s = Array.isArray(R.states) ? R.states.filter(x => x && typeof x === 'object') : []; const last = s[s.length - 1]; return ['Particles', [`${cnt(R.states) ?? 0} states`, last && Array.isArray(last.position) && numOf(last.position[1]) != null ? `y ${r2(last.position[1])}` : null].filter(Boolean).join(' · ')]; }
  }
  return null;
}
function outlinerPost(state, MODES, ev) {
  const tn = String(ev.tool_name); if (!OUTL.test(tn) || ev.phase !== 'complete') return false;
  const P = ev.payload || {}, A = (state._b2Args && state._b2Args[ev.call_id]) || P.args || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const e = outlinerEntry(tn, A, R); if (!e || !e[1]) return false;
  const L = state._outl || (state._outl = []);
  L.unshift(e); if (L.length > 4) L.length = 4;
  return readCard(state, MODES, tn, L.map(x => [x[0], x[1]]), 'outliner');
}

// ---------- 3. roto_propagate ----------
function maskPost(state, MODES, ev) {
  if (ev.tool_name !== 'roto_propagate' || ev.phase !== 'complete') return false;
  const P = ev.payload || {}, R = P.result && typeof P.result === 'object' ? P.result : {}, C = (P.changed && P.changed.clip) || {};
  const vw = panelOf(state, MODES, screenOf(state, MODES, ev.tool_name), 'viewer'); if (!vw) return false;
  // the propagated path: the last key of the mask's new path_keys (normalised points), drawn closed
  let path = null;
  for (const [k, v] of Object.entries(C)) {
    if (!/path_keys$/.test(k) || !Array.isArray(v) || !Array.isArray(v[1])) continue;
    const keys = v[1].filter(x => x && typeof x === 'object' && Array.isArray(x.points)), last = keys[keys.length - 1];
    if (last) path = last.points.filter(p => p && numOf(p.x) != null && numOf(p.y) != null).map(p => [numOf(p.x), numOf(p.y)]);
  }
  vw.overlay = (vw.overlay || []).filter(o => o._kind !== 'b2mask');
  if (path && path.length >= 3) vw.overlay.push({ path, closed: true, _kind: 'b2mask', _call: ev.call_id });
  const text = ['mask propagated', numOf(R.keys) != null ? `${R.keys} keys` : null, numOf(R.frames_filled) != null ? `${R.frames_filled} frames filled` : null, numOf(R.mean_snap_px) != null ? `snap ${pxs(R.mean_snap_px)}` : null].filter(Boolean).join(' · ');
  chip(vw, 'mask', text, 'mask2');
  return true;
}

// ---------- 4. grade-graph mirror (render refusal reason in headless mode) ----------
// headless fallback; remove when the engine emits a frame_skipped reason in headless, v5.1 engine task.
// The chip it feeds carries data-source="inferred" (this rule) vs "engine" (a frame_skipped reason, app.js).
function graphLearn(state, ev) {
  if (ev.phase !== 'complete') return;
  const P = ev.payload || {}, C = P.changed && P.changed.clip;
  if (!C || typeof C !== 'object') return;
  const keys = Object.keys(C).filter(k => /^grade_graph(\.|$)/.test(k)); if (!keys.length) return;
  const cid = (ev.target && ev.target.clip_id) ?? ((state._b2Args && state._b2Args[ev.call_id]) || P.args || {}).clip_id;
  if (cid == null) return;
  const GG = state._gg || (state._gg = {});
  let g = GG[cid] || (GG[cid] = { nodes: {}, output: 'input' });
  for (const k of keys) {
    const to = Array.isArray(C[k]) ? C[k][1] : undefined;
    if (k === 'grade_graph') { if (to == null) g = GG[cid] = { nodes: {}, output: 'input' }; continue; }
    let m;
    if (k === 'grade_graph.output') { if (typeof to === 'string') g.output = to; continue; }
    if ((m = /^grade_graph\.nodes\.([^.]+)$/.exec(k))) { if (to == null) delete g.nodes[m[1]]; else if (typeof to === 'object') g.nodes[m[1]] = { kind: to.kind, inputs: Array.isArray(to.inputs) ? to.inputs : [], fx: to.effect && to.effect.kind }; continue; }
    if ((m = /^grade_graph\.nodes\.([^.]+)\.(kind|inputs|effect\.kind)$/.exec(k))) { const n = g.nodes[m[1]] || (g.nodes[m[1]] = { inputs: [] }); if (m[2] === 'kind') n.kind = to; else if (m[2] === 'inputs') n.inputs = Array.isArray(to) ? to.filter(x => typeof x === 'string') : []; else n.fx = to; }
  }
  // kimo-core node_graph.rs apply(): the reachable nodes in evaluation order; a grade node needs exactly one input,
  // a mixer (parallel / layer) at least two. The first that fails is the engine's refusal.
  let reason = null;
  if (g.output !== 'input' && g.nodes[g.output]) {
    const done = new Set(), active = new Set(), order = [];
    const visit = id => { if (id === 'input' || done.has(id) || active.has(id) || !g.nodes[id]) return; active.add(id); for (const s of g.nodes[id].inputs || []) visit(s); active.delete(id); done.add(id); order.push(id); };
    visit(g.output);
    for (const id of order) { const n = g.nodes[id], c = (n.inputs || []).length; if (n.kind === 'grade' ? c !== 1 : c < 2) { reason = `incomplete reachable node · ${n.fx ? String(n.fx).replace(/_/g, ' ') : n.kind === 'grade' ? 'grade' : String(n.kind ?? 'mixer')}`; break; } }
  }
  const RF = state._refuse || (state._refuse = {});
  if (reason) RF[cid] = reason; else delete RF[cid];
  const any = Object.values(RF)[0];
  state.preview.refused = any || null; state.preview.refusedSrc = any ? 'inferred' : null;
}

// ---------- 5. toast-only tools ----------
const base = p => String(p ?? '').split(/[\\/]/).filter(Boolean).pop() || '';
function relinkPost(state, MODES, ev) {
  if (ev.tool_name !== 'relink_media' || ev.phase !== 'complete') return false;
  const P = ev.payload || {}, A = (state._b2Args && state._b2Args[ev.call_id]) || P.args || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const M = panelOf(state, MODES, 'batch', 'media'); if (!M) return false;
  const dry = R.applied === false, n = numOf(dry ? R.would_relink : R.relinked) ?? 0;
  const items = (M.items || []).filter(x => !x._b2);
  const its = Array.isArray(R.items) ? R.items.filter(x => x && typeof x === 'object') : [];
  const rows = its.slice(0, 3).map(x => ({ name: base(x.old_path ?? x.from ?? x.path ?? x.name) || 'media', note: base(x.new_path ?? x.to ?? x.found ?? '') || String(x.status ?? ''), link: true, em: true, _b2: true }));
  const sum = R.no_history ? 'nothing missing' : `${n} ${dry ? 'would relink' : 'relinked'}`;
  rows.unshift({ name: `relink${dry ? ' · dry run' : ''}`, note: [sum, numOf(R.not_found) ? `${R.not_found} not found` : null, numOf(R.ambiguous) ? `${R.ambiguous} ambiguous` : null].filter(Boolean).join(' · '), link: true, em: true, _b2: true });
  M.items = [...rows, ...items].slice(0, 5);
  M.foot = `relink ${dry ? 'dry run' : 'applied'} · ${sum}`; M._footFlash = now();
  return true;
}
function seqPost(state, MODES, ev) {
  if (ev.tool_name !== 'render_scene3d_sequence' || !(ev.phase === 'complete' || ev.phase === 'error')) return false;
  const M = MODES.export; if (!M) return false;
  const P = ev.payload || {}, A = (state._b2Args && state._b2Args[ev.call_id]) || P.args || {}, R = P.result && typeof P.result === 'object' ? P.result : {};
  const ren = panelOf(state, MODES, 'export', 'renders'); if (!ren) return false;
  ren.head = ren.head || ['name', 'preset', 'length', 'status', 'started'];
  ren.rows = (ren.rows || []).filter(r => r._call !== ev.call_id);
  const err = ev.phase === 'error', n = numOf(R.count) ?? cnt(R.frames), dir = R.dir ?? A.dir;
  const fr0 = Array.isArray(R.frames) && R.frames[0] && typeof R.frames[0] === 'object' ? R.frames[0] : null;
  const row = [base(dir) || 'scene3d sequence', ['scene3d', numOf(A.width) && numOf(A.height) ? `${A.width}x${A.height}` : numOf(A.height) ? `${A.height} px` : null, fr0 && fr0.render && fr0.render.backend ? 'cpu' : null].filter(Boolean).join(' '), n != null ? `${n} frame${n === 1 ? '' : 's'}` : '-', err ? '✕ failed' : '✓ done', 'now'];
  row._call = ev.call_id; row._job = true; row._flash = now(); if (dir) row._path = String(dir).replace(/^\/(home|Users)\/[^/]+/, '~').split('/').slice(-3).join('/');
  ren.rows.unshift(row); if (ren.rows.length > 6) ren.rows.length = 6;
  return true;
}
function templatePost(state, MODES, ev) {
  if (ev.tool_name !== 'template_describe' || ev.phase !== 'complete') return false;
  const R = (ev.payload || {}).result; if (!R || typeof R !== 'object') return false;
  const ops = Array.isArray(R.ops) ? R.ops.filter(x => x && typeof x === 'object') : [], prm = Array.isArray(R.params) ? R.params.filter(x => x && typeof x === 'object') : [];
  const rows = [['Template', clean(R.name) ?? 'template'], ['Ops', `${(k => `${k} op${k === 1 ? '' : 's'}`)(numOf(R.op_count) ?? ops.length)}${ops.length ? ' · ' + [...new Set(ops.map(o => String(o.tool ?? '')).filter(Boolean))].slice(0, 2).join(', ') : ''}`], ['Params', prm.length ? prm.slice(0, 2).map(p => `${p.name}${p.default != null ? ' = ' + p.default : ''}`).join(', ') : 'none']];
  if (readCard(state, MODES, 'template_describe', rows)) return true;
  // the Batch / History screen (m02) has no Inspector: the answer joins the Diff list's templates lines, as a read ('=')
  const D = panelOf(state, MODES, state.mode, 'diffbatch'); if (!D) return false;
  if (D._ctx !== 'templates') { D.diff = []; D._ctx = 'templates'; D.meta = ''; }
  D.diff = (D.diff || []).concat([['=', `template ${rows[0][1]} · ${rows[1][1]} · ${rows[2][1]}`]]).slice(-6); D.titleSmall = 'templates'; D._flash = now();
  return true;
}

// after the family code ran (so a card / chip here is the last word for these tools)
export function fix2Post(state, MODES, ev) {
  if (ev.phase === 'start') { (state._b2Args || (state._b2Args = {}))[ev.call_id] = (ev.payload || {}).args || {}; return false; }
  let hit = false;
  try {
    graphLearn(state, ev);
    hit = gatedPost(state, MODES, ev) || trackPost(state, MODES, ev) || outlinerPost(state, MODES, ev) || maskPost(state, MODES, ev) || relinkPost(state, MODES, ev) || seqPost(state, MODES, ev) || templatePost(state, MODES, ev);
  } finally {
    if ((ev.phase === 'complete' || ev.phase === 'error') && state._b2Args) delete state._b2Args[ev.call_id];
  }
  return hit;
}
