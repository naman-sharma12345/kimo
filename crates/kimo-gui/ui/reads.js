// GUI-FIX-1: what a read / inspect / QA call answered, kept on the screen after the HUD toast fades.
// The audit (gui_coverage.md, 2026-10-17) found 65 successful calls whose only trace was the rail row and the toast.
// A read changes nothing in the engine, so it never writes an Inspector row (rows are values the project holds):
// its answer lands in a quiet "read" card pinned to the bottom of the current screen's info panel (kv), replaced by
// the next read. Mutating calls with no visual of their own (text styling, an audio-effect key, a matte removal, a
// physics bake) get a short section in the Inspector of the screen they land on, from their own args and result.
// Colour reads and matches also write the Color screen's scope stats line. Never a raw id, never a full path.
import { fmtVal } from './router.js';

const RAW = /^(c|clip|t|track|fx|effect|node|n|layer|l|asset|media|an|marker|mask|m|cam|light|obj)[_-]?\d+(?:[_-][a-z]{0,5}\d+|[_-][a-z])*$/i;
const SKIP = /^(ok|path|kimo_build|build|ts|ts_ms|dur_ms|elapsed_ms|note|notes|domain|method|assumption|description|formula|differentiability|basis|as_of|inspection|contract|fallback_contract|nits_note|cache|clock|schema|schema_version|version_note)$|_id$|_ids$|_note$|_method$|_contract$/;
const num = v => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const r2 = x => String(Math.round(x * 100) / 100);
const pct = x => (num(x) == null ? null : (x < 10 ? r2(x) : String(Math.round(x * 10) / 10)) + '%');
const lab = k => String(k).replace(/_/g, ' ').replace(/^./, c => c.toUpperCase());
const base = s => String(s).split(/[\\/]/).pop();
const trunc = (s, n = 26) => (s.length > n ? s.slice(0, n - 1) + '…' : s);
const rgb = a => (Array.isArray(a) && a.length >= 3 && a.slice(0, 3).every(x => num(x) != null) ? a.slice(0, 3).map(r2).join(' ') : null);

// one value, short enough for a kv row
export function short(v, k = '') {
  if (v == null) return null;
  if (typeof v === 'boolean') return v ? 'yes' : 'no';
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) return null;
    if (/bytes|memory|ceiling/i.test(k)) return mb(v);
    if (v !== 0 && Math.abs(v) < 0.001) return v.toExponential(1);
    return fmtVal(Math.abs(v) >= 1e6 ? Math.round(v) : Math.round(v * 1000) / 1000, k);
  }
  if (typeof v === 'string') { if (!v || RAW.test(v)) return null; return trunc(/[\\/]/.test(v) && v.length > 20 ? base(v) : v); }
  if (Array.isArray(v)) {
    const sc = v.filter(x => x != null && typeof x !== 'object' && !(typeof x === 'string' && (RAW.test(x) || /^…\+\d+$/.test(x))));
    if (!v.length) return 'none';
    if (sc.length === v.length && sc.length <= 4 && sc.every(x => typeof x === 'number')) return sc.map(x => short(x)).join(' ');
    if (sc.length === v.length && typeof sc[0] === 'string') return trunc(sc.slice(0, 3).map(String).join(', ') + (v.length > 3 ? ` +${v.length - 3}` : ''));
    const named = v.map(x => x && typeof x === 'object' ? (x.name ?? x.kind ?? x.label ?? x.code ?? null) : null).filter(x => typeof x === 'string' && !RAW.test(x));
    if (named.length) return trunc(`${v.length} · ` + named.slice(0, 2).join(', ') + (v.length > 2 ? '…' : ''));
    return `×${v.length}`;
  }
  if (typeof v === 'object') { const n = Object.keys(v).length; return n ? `${n} ${n === 1 ? 'entry' : 'entries'}` : 'none'; }
  return null;
}

// generic: scalars first, then lists, then maps; at most n rows
export function genericRows(R, n = 4) {
  if (R == null) return [];
  if (Array.isArray(R)) { const s = short(R); return s ? [['Items', R.length + (s.startsWith('×') ? '' : ' · ' + s.replace(/^\d+ · /, ''))]] : []; }
  if (typeof R !== 'object') { const s = short(R); return s ? [['Result', s]] : []; }
  const out = [], rank = v => (v == null ? 9 : typeof v !== 'object' ? 0 : Array.isArray(v) ? 1 : 2);
  for (const [k, v] of Object.entries(R).sort((a, b) => rank(a[1]) - rank(b[1]))) {
    if (SKIP.test(k)) continue;
    if (typeof v === 'string' && v.length > 40) continue;   // prose (notes, formulas) is not a readout
    const s = short(v, k); if (s == null || s === '') continue;
    out.push([lab(k), s]); if (out.length >= n) break;
  }
  if (!out.length && Object.keys(R).length === 0) out.push(['Result', 'empty']);
  return out;
}

// tools whose answer has a better first line than the generic walk
const T = {
  get_scopes: R => {
    // v6 numeric arguments: percentiles add a luma spread and the chroma (vectorscope radius) mean; zones add the worst zone's clip / crush.
    // Top-level scalars and the first / last entry of the top-level percentile arrays only; without them the v5 rows stand.
    const P = Array.isArray(R.percentiles) ? R.percentiles : null, LP = Array.isArray(R.luma_percentiles) ? R.luma_percentiles : null;
    // the spectator caps long arrays (8 entries plus a "…+N" marker): pair P[k] with LP[k] at the last index where both are numbers
    let k = -1; if (P && LP) for (let i = 0; i < Math.min(P.length, LP.length); i++) { if (num(P[i]) != null && num(LP[i]) != null) k = i; else break; }
    const spread = k >= 0 ? [k ? `Luma p${r2(P[0])}–p${r2(P[k])}` : `Luma p${r2(P[0])}`, k ? `${r2(LP[0])} – ${r2(LP[k])}` : r2(LP[0])] : null;
    const zone = num(R.zone_clipped_whites_max_pct) != null && num(R.zone_crushed_blacks_max_pct) != null ? ['Zone max clip/crush', `${pct(R.zone_clipped_whites_max_pct)} / ${pct(R.zone_crushed_blacks_max_pct)}`, 'wrap'] : null;
    if (!spread && !zone) return [['Luma mean', short(R.luma_mean)], ['Avg RGB', rgb(R.avg_rgb)], ['Clipped whites', pct(R.clipped_whites_pct)], ['Crushed blacks', pct(R.crushed_blacks_pct)]];
    return [['Luma mean', short(R.luma_mean)], spread || ['Avg RGB', rgb(R.avg_rgb)], spread ? ['Chroma mean', num(R.chroma_mean) != null ? String(Math.round(R.chroma_mean * 1000) / 1000) : null] : ['Clipped whites', pct(R.clipped_whites_pct)], zone || ['Clipped whites', pct(R.clipped_whites_pct)]];
  },
  // v6 contrast_ratio: top-level scalars only (ratio, luminance_a / _b, ratio_protan / _deutan / _tritan when cvd was asked)
  contrast_ratio: R => { const cv = [['P', R.ratio_protan], ['D', R.ratio_deutan], ['T', R.ratio_tritan]].filter(c => num(c[1]) != null);
    return [['Ratio', num(R.ratio) != null ? `${r2(R.ratio)}:1` : null], ['Luminance a / b', num(R.luminance_a) != null && num(R.luminance_b) != null ? `${String(Math.round(R.luminance_a * 1000) / 1000)} / ${String(Math.round(R.luminance_b * 1000) / 1000)}` : null, 'wrap'],
      cv.length ? ['Under CVD', cv.map(c => `${c[0]} ${r2(c[1])}:1`).join(' · '), 'wrap'] : null, num(R.severity) != null && cv.length ? ['Severity', r2(R.severity)] : null].filter(Boolean); },
  // v6 edit_check: did the edit stay inside its allowed rects (the band). The card header carries the threshold (readHead);
  // 4 rows from the top-level counts and the largest region only: outside (with the largest delta there), inside, overall max, regions
  edit_check: R => { const O = R.outside && typeof R.outside === 'object' ? R.outside : {}, I = R.inside && typeof R.inside === 'object' ? R.inside : {};
    const g = Array.isArray(R.regions) && R.regions[0] && Array.isArray(R.regions[0].bbox) && R.regions[0].bbox.length === 4 && R.regions[0].bbox.every(v => num(v) != null) ? R.regions[0].bbox : null;
    return [['Changed outside band', num(O.changed_pixels) != null ? `${O.changed_pixels} px` + (num(O.max_delta) != null ? ` (max Δ there ${O.max_delta.toFixed(2)})` : '') + (num(R.allowed_count) === 0 ? ' · no band given' : '') : null, 'stack'],
      num(R.allowed_count) > 0 ? ['Changed inside band', num(I.changed_pixels) != null ? `${I.changed_pixels} px` : null, 'stack'] : null,
      ['Max Δ overall', num(R.max_delta) != null ? R.max_delta.toFixed(2) + (R.bit_identical === true ? ' · identical' : '') : null, 'wrap'],
      ['Regions', num(R.region_count) != null ? (R.region_count === 0 ? 'none' : `${R.region_count}` + (g ? ` · largest ${g[2]}×${g[3]} at ${g[0]},${g[1]}` : '')) : null, 'wrap']].filter(r => r && r[1] != null); },
  // v6 text_contrast_series: the worst frame of text over moving video, from top-level scalars only (the card shows 4 rows);
  // below the bar the fix shown is the scrim (suggested_fix / scrim_alpha_min), never a recolour
  text_contrast_series: R => Array.isArray(R.captions) ? [['Worst ratio', num(R.worst_ratio) != null ? `${r2(R.worst_ratio)}:1` + (typeof R.worst_caption === 'string' ? ` · ${R.worst_caption.slice(0, 40)}` : '') : null, 'wrap'],
      ['Worst at', num(R.worst_time) != null ? `${r2(R.worst_time)}s` + (num(R.worst_frame) != null ? ` · frame ${R.worst_frame}` : '') : null, 'wrap'],
      ['Captions', num(R.caption_count) != null ? `${R.caption_count}` + (num(R.captions_below) != null ? ` · ${R.captions_below} below bar` : '') + (R.suggested_fix === 'scrim' ? ' · scrim' : '') : null, 'wrap'],
      ['Cross a cut', num(R.captions_crossing_cut) != null ? (R.captions_crossing_cut === 0 ? 'none' : (Array.isArray(R.crossing_cut) ? R.crossing_cut.slice(0, 3).filter(c => c && typeof c.name === 'string').map(c => `${c.name.slice(0, 24)}` + (Array.isArray(c.cut_times) && num(c.cut_times[0]) != null ? ` @ ${r2(c.cut_times[0])}s` : '')).join(', ') : '') + (R.captions_crossing_cut > 3 ? ` and ${R.captions_crossing_cut - 3} more` : '')) : null, 'wrap']].filter(r => r && r[1] != null && r[1] !== '') :
    [['Worst ratio', num(R.worst_ratio) != null ? `${r2(R.worst_ratio)}:1` + (num(R.threshold) != null ? ` (bar ${r2(R.threshold)}:1${typeof R.size_class === 'string' ? ', ' + R.size_class.slice(0, 8) : ''})` : '') : null, 'wrap'],
    ['Worst at', num(R.worst_time) != null ? `${r2(R.worst_time)}s` + (num(R.worst_frame) != null ? ` · frame ${R.worst_frame}` : '') : null, 'wrap'],
    num(R.threshold) != null && num(R.frames_below) != null ? ['Below bar', `${R.frames_below} of ${num(R.frames_sampled) ?? '?'} frames` + (num(R.first_below_time) != null ? ` · first ${r2(R.first_below_time)}s` : '') + (R.suggested_fix === 'scrim' ? (num(R.scrim_alpha_min) != null ? ` · scrim ≥ ${Math.round(R.scrim_alpha_min * 100)}%` : ' · scrim') : ''), 'wrap'] : ['Frames', num(R.frames_sampled) != null ? String(R.frames_sampled) : null],
    ['Cuts', num(R.cut_count) != null ? (R.cut_count === 0 ? 'none inside' : `${R.cut_count} inside` + (Array.isArray(R.cut_times) && num(R.cut_times[0]) != null ? ` · first ${r2(R.cut_times[0])}s` : '')) : null, 'wrap']].filter(r => r && r[1] != null),
  // v6 query_safe_margins rects: worst-case overlap and worst crossed edge scalars (the card shows 4 rows)
  query_safe_margins: R => (Array.isArray(R.overlaps) ? [['Profile', short(R.profile)], ['Max outside safe', num(R.rects_outside_safe_max_pct) != null ? pct(R.rects_outside_safe_max_pct) : 'no safe zone', 'wrap'], ['Max in avoid', num(R.rects_in_avoid_max_pct) != null ? pct(R.rects_in_avoid_max_pct) : 'no avoid zone', 'wrap'],
    ['Worst edge', typeof R.worst_edge === 'string' && num(R.worst_edge_px) != null ? `${R.worst_edge} ${r2(R.worst_edge_px)} px` + (typeof R.worst_edge_rect === 'string' ? ` · ${R.worst_edge_rect.slice(0, 40)}` : '') : (num(R.worst_edge_px) === 0 ? 'none crossed' : 'no safe zone'), 'wrap']]
    // without rects (v5 arguments): the same 4 facts the generic rows showed, with Source wrapped (it clipped at 'documented-constant')
    : [['Profile', short(R.profile)], ['Zones', Array.isArray(R.zones) ? `${R.zones.length} · ` + R.zones.slice(0, 2).map(z => String((z && z.name) ?? '')).join(', ') + (R.zones.length > 2 ? '…' : '') : null, 'wrap'], ['Estimated', typeof R.estimated === 'boolean' ? (R.estimated ? 'yes' : 'no') : null], ['Source', typeof R.source === 'string' ? R.source : null, 'wrap']].filter(r => r[1] != null)),
  analyze_color: R => [['Mean RGB', rgb(R.mean)], ['Luma mean', Array.isArray(R.mean) ? short(R.mean[3]) : null], ['Luma min / max', Array.isArray(R.min) && Array.isArray(R.max) ? `${r2(R.min[3] ?? R.min[0])} / ${r2(R.max[3] ?? R.max[0])}` : null], ['Bins', short(R.bins)]],
  describe_frame: R => { const C = R.canvas || {}, S = R.frame_stats || {}; return [['Canvas', C.width ? `${C.width}×${C.height}` : null], ['Clipped whites', pct(S.clipped_white_pct)], ['Clipped blacks', pct(S.clipped_black_pct)], ['Dominant', Array.isArray(S.dominant_colors) ? S.dominant_colors.slice(0, 3).map(d => d.hex).filter(Boolean).join(' ') : null]]; },
  cache_stats: R => [['Frames', short(R.entries)], ['Used', `${mb(R.bytes)} of ${mb(R.cap_bytes)}`], ['Hit rate', pct((num(R.hit_rate) ?? 0) * 100)], ['Evictions', short(R.evictions)]],
  read_pixels: R => [['Pixels', num(R.count) != null ? short(Math.round(R.count / 4)) : null], ['First RGB', rgb(R.data)], ['Size', R.width ? `${R.width}×${R.height}` : null]],
  get_project: R => [['Assets', R.assets ? short(Object.keys(R.assets).length) : null], ['Tracks', Array.isArray(R.tracks) ? short(R.tracks.length) : R.tracks ? short(Object.keys(R.tracks).length) : null], ['Duration', num(R.duration) != null ? r2(R.duration) + ' s' : null], ['Canvas', R.width ? `${R.width}×${R.height}` : null]],
  kimo_info: R => [['Version', short(R.version)], ['Backends', R.backends ? Object.entries(R.backends).filter(([, b]) => b && b.available).map(([k]) => k).join(', ') || 'none' : null], ...genericRows(R, 2)],
  list_edit_points: R => [['Edit points', Array.isArray(R.points) ? `${R.points.length} · ${R.points.slice(0, 4).map(x => r2(x) + 's').join(' ')}` : null]],
  list_markers: R => [['Markers', Array.isArray(R.markers) ? `${R.markers.length}` + (R.markers.length ? ' · ' + R.markers.slice(0, 2).map(m => `${m.name ?? ''} ${r2(m.time ?? 0)}s`.trim()).join(', ') : '') : null]],
  evaluate_hlg_reference_display: R => [['Display', Array.isArray(R.display_rgb_nits) ? R.display_rgb_nits.slice(0, 3).map(x => Math.round(x)).join(' ') + ' nits' : null], ['Gamma', short(R.gamma)], ['Gamut', short(R.gamut)]],
  history_list: R => [['Undo', Array.isArray(R.undo) ? `${R.undo.length} steps` : null], ['Next undo', short(R.next_undo)], ['Redo', Array.isArray(R.redo) ? `${R.redo.length}` : null], ['Checkpoints', Array.isArray(R.checkpoints) ? `${R.checkpoints.length}` : null]],
  snap_time: R => [['Time', num(R.time) != null ? r2(R.time) + ' s' : null], ['Snapped', short(R.snapped)]],
  session_stats: R => [['Calls', short(R.calls_total)], ['Errors', short(R.errors_total)], ['Returned', mb(R.bytes_returned_total)]],
  inspect_color_pipeline: R => [['Input', [R.default_media_input_gamut, R.default_media_input_transfer].filter(Boolean).join(' ') || null], ['Working', short(R.working_space ?? R.working ?? R.working_gamut)], ['Output', [R.configured_output_gamut, R.configured_output_transfer].filter(Boolean).join(' ') || null], ...genericRows(R, 1)],
  // v6 render_flow stats: from flow_stats (canvas px per frame) 'Motion' is the mean vector (length and direction, 0° = right, 90° = down),
  // 'Speed' the p50 / p90 / max speeds, 'Moving' the share faster than moving_px; without stats the card is the v5 card (generic rows)
  render_flow: R => { const F = R.flow_stats && typeof R.flow_stats === 'object' ? R.flow_stats : null; if (!F) return genericRows(R);
    const r1 = x => String(Math.round(x * 10) / 10), mu = num(F.mean_u), mv = num(F.mean_v), d = num(F.mean_dir_deg), p50 = num(F.speed_p50), p90 = num(F.speed_p90), mx = num(F.speed_max), mf = num(F.moving_frac);
    return [['Motion', mu != null && mv != null ? `${r1(Math.hypot(mu, mv))} px/f${d != null ? ` · ${Math.round(d)}°` : ''}` : null],
      ['Speed', p50 != null && p90 != null && mx != null ? `p50 ${r1(p50)} · p90 ${r1(p90)} · max ${r1(mx)} px/f` : null, 'stack'],
      ['Moving', mf != null ? pct(mf * 100) : null]]; },
  // v6: with legibility (validate_captions legibility: true) the rows after Captions are the smallest caps, 'Min caps' (px and % of the
  // frame height), the lowest contrast and the highest reading speed, from the top-level legibility summary (short values: the
  // Inspector column is narrow); without it the card is the v5 card, row for row
  validate_captions: R => { const G = R.legibility && typeof R.legibility === 'object' ? R.legibility : null;
    const base = [['Captions', R.clean === true ? 'clean' : R.clean === false ? 'issues' : null], ['Profile', short(R.profile)]];
    if (!G) return [...base, ['Groups', Array.isArray(R.groups) ? String(R.groups.length) : null], ['Skipped checks', Array.isArray(R.skipped) ? String(R.skipped.length) : null]];
    const mc = num(G.min_cap_frac), px = num(G.min_cap_px), mr = num(G.min_contrast), cps = num(G.max_cps);
    return [base[0], ['Min caps', mc != null ? `${px != null ? Math.round(px * 10) / 10 + ' px · ' : ''}${pct(mc * 100)}` : null],
      ['Lowest contrast', mr != null ? `${mr.toFixed(2)}:1` : null], ['Max reading', cps != null ? `${cps.toFixed(1)} cps` : null]]; },
};
const mb = n => { n = num(n); if (n == null) return '0 B'; const u = ['B', 'KB', 'MB', 'GB']; let i = 0; while (n >= 1024 && i < 3) { n /= 1024; i++; } return (i ? Math.round(n * 10) / 10 : Math.round(n)) + ' ' + u[i]; };

// the read card's header: the tool name, plus the one argument a reader needs to read the rows right (edit_check: its threshold)
export function readHead(tool, R) {
  if (tool === 'edit_check' && R && num(R.threshold) != null) return `threshold ${r2(R.threshold)} · ${tool}`;   // threshold first: a narrow header cuts the end
  return tool;
}
export function readRows(tool, R) {
  let rows = [];
  try { rows = (T[tool] ? T[tool](R || {}) : genericRows(R)).filter(r => r && r[1] != null && r[1] !== ''); } catch (e) { rows = []; }
  if (!rows.length) rows = genericRows(R);
  return rows.slice(0, 4).map(([k, v, w]) => (w === 'wrap' || w === 'stack' ? [String(k), String(v), w] : [String(k), String(v)]));   // a row may ask to wrap (get_scopes zones)
}

// a mutating call no panel code drew: its own args first (names, never raw ids), then what the engine answered.
// A third element marks the row that also goes on the clip as a chip (text animators, path, style, weight).
const CHIP = { add_text_animator: A => A.preset ?? A.kind ?? 'animator', set_text_path: () => 'on path', set_text_style: () => 'styled', set_text_weight: A => (A.weight != null ? `w${A.weight}` : 'weight') };
const CALL = {
  add_text_animator: (A, R) => [['Animator', String(A.preset ?? A.kind ?? 'custom'), true], ['Range', A.range ?? A.based_on ?? null]],
  set_text_path: A => [['Text path', Array.isArray(A.points) ? `${A.points.length} points` : A.shape ? String(A.shape) : A.remove ? 'removed' : 'set', true], ['Start', A.start != null ? short(A.start) : null], ['Flip', A.flip != null ? short(A.flip) : null]],
  set_text_style: (A, R, C) => [['Text style', 'applied', true], ...argRows(A, 2), ['Props', fxCount(C)]],
  set_text_weight: (A, R, C) => [['Weight', A.weight != null ? short(A.weight) : 'default', true], ['Props', fxCount(C)]],
  set_audio_effect_key: A => [[`Key ${A.coefficient ?? ''}`.trim(), `${short(A.value) ?? ''}${A.time != null ? ` @ ${short(A.time)}s` : ''}`], ['Effect', A.effect_index != null ? `#${A.effect_index}` : null]],
  remove_track_matte: () => [['Track matte', 'removed']],
  validate_captions: (A, R) => T.validate_captions(R || {}).map(r => [r[0], r[1]]),   // Inspector rows carry no layout flag (the third field means emphasis here)
};
const fxCount = C => { const c = C && C.clip; if (!c || typeof c !== 'object') return null; const n = Object.keys(c).filter(k => /^text_fx/.test(k)).length; return n ? `${n} changed` : null; };
function argRows(A, n = 3) {
  const out = [];
  for (const [k, v] of Object.entries(A || {})) {
    if (/^(clip_id|track_id|id|ids|clip_ids|path)$|_id$/.test(k)) continue;
    const s = short(v, k); if (s == null || s === '' || s === 'none') continue;
    out.push([lab(k), s]); if (out.length >= n) break;
  }
  return out;
}
export function callRows(tool, A, R, C, clipName) {
  let rows = CALL[tool] ? CALL[tool](A || {}, R, C) : [...argRows(A, 3), ...genericRows(R, 2)];
  rows = rows.filter(r => r && r[1] != null && r[1] !== '').map(r => [String(r[0]), String(r[1]), !!r[2]]);
  if (rows.length && clipName) rows.unshift(['Clip', String(clipName), false]);
  return rows.slice(0, 4);
}
export { CHIP };

// colour picks / matches (grading screen) and the Color scopes' one-line stats
export function colourRows(tool, A, R, refName) {
  R = R || {};
  // the Grade list is narrow (m15): one number per row (luma of a mean, L of a Lab mean, one gain per channel)
  const luma = a => (Array.isArray(a) && a.length >= 3 && a.slice(0, 3).every(x => num(x) != null) ? r2(0.2126 * a[0] + 0.7152 * a[1] + 0.0722 * a[2]) : null);
  const L = a => (Array.isArray(a) && num(a[0]) != null ? r2(a[0]) : null), G = R.gains || [];
  const rows = tool === 'pick_white_balance' ? [['WB R', num(G[0]) != null ? r2(G[0]) : null], ['WB G', num(G[1]) != null ? r2(G[1]) : null], ['WB B', num(G[2]) != null ? r2(G[2]) : null]]
    : tool === 'match_color' ? [['Match', 'colour'], ['Ref', refName], ['Src luma', luma(R.src_mean)], ['Ref luma', luma(R.ref_mean)]]
    : tool === 'match_grade' ? [['Match', 'Lab'], ['Ref', refName], ['Src L', L(R.source_lab_mean)], ['Ref L', L(R.reference_lab_mean)]] : [];
  return rows.filter(r => r[1] != null && r[1] !== '').map(r => [String(r[0]), String(r[1])]);
}
export function scopeLine(tool, R) {
  if (!R || typeof R !== 'object' || Array.isArray(R)) return null;
  const parts = [];
  if (tool === 'get_scopes') { if (num(R.luma_mean) != null) parts.push(`luma ${r2(R.luma_mean)}`); if (rgb(R.avg_rgb)) parts.push(`rgb ${rgb(R.avg_rgb)}`); if (num(R.clipped_whites_pct) != null) parts.push(`clip ${pct(R.clipped_whites_pct)}`); if (num(R.crushed_blacks_pct) != null) parts.push(`crush ${pct(R.crushed_blacks_pct)}`); }
  else if (tool === 'analyze_color') { if (rgb(R.mean)) parts.push(`mean ${rgb(R.mean)}`); if (Array.isArray(R.mean) && num(R.mean[3]) != null) parts.push(`luma ${r2(R.mean[3])}`); if (Array.isArray(R.min) && Array.isArray(R.max)) parts.push(`range ${r2(R.min[3] ?? R.min[0])}–${r2(R.max[3] ?? R.max[0])}`); }
  else if (tool === 'describe_frame') { const S = R.frame_stats || {}; if (num(S.clipped_white_pct) != null) parts.push(`clip ${pct(S.clipped_white_pct)}`); if (num(S.clipped_black_pct) != null) parts.push(`crush ${pct(S.clipped_black_pct)}`); }
  else if (tool === 'pick_white_balance') { if (rgb(R.gains)) parts.push(`WB gains ${rgb(R.gains)}`); }
  else if (tool === 'match_color') { if (rgb(R.src_mean) && rgb(R.ref_mean)) parts.push(`mean ${rgb(R.src_mean)} → ${rgb(R.ref_mean)}`); }
  else if (tool === 'match_grade') { if (rgb(R.source_lab_mean) && rgb(R.reference_lab_mean)) parts.push(`Lab ${rgb(R.source_lab_mean)} → ${rgb(R.reference_lab_mean)}`); }
  else return null;
  return parts.length ? `${tool} · ${parts.join(' · ')}` : null;
}
