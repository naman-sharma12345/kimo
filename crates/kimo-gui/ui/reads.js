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
  get_scopes: R => [['Luma mean', short(R.luma_mean)], ['Avg RGB', rgb(R.avg_rgb)], ['Clipped whites', pct(R.clipped_whites_pct)], ['Crushed blacks', pct(R.crushed_blacks_pct)]],
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
  validate_captions: R => [['Captions', R.clean === true ? 'clean' : R.clean === false ? 'issues' : null], ['Profile', short(R.profile)], ['Groups', Array.isArray(R.groups) ? String(R.groups.length) : null], ['Skipped checks', Array.isArray(R.skipped) ? String(R.skipped.length) : null]],
};
const mb = n => { n = num(n); if (n == null) return '0 B'; const u = ['B', 'KB', 'MB', 'GB']; let i = 0; while (n >= 1024 && i < 3) { n /= 1024; i++; } return (i ? Math.round(n * 10) / 10 : Math.round(n)) + ' ' + u[i]; };

export function readRows(tool, R) {
  let rows = [];
  try { rows = (T[tool] ? T[tool](R || {}) : genericRows(R)).filter(r => r && r[1] != null && r[1] !== ''); } catch (e) { rows = []; }
  if (!rows.length) rows = genericRows(R);
  return rows.slice(0, 4).map(([k, v]) => [String(k), String(v)]);
}

// a mutating call no panel code drew: its own args first (names, never raw ids), then what the engine answered.
// A third element marks the row that also goes on the clip as a chip (text animators, path, style, weight).
const CHIP = { add_text_animator: A => A.preset ?? A.kind ?? 'animator', set_text_path: () => 'on path', set_text_style: () => 'styled', set_text_weight: A => (A.weight != null ? `w${A.weight}` : 'weight') };
const CALL = {
  add_text_animator: (A, R) => [['Animator', String(A.preset ?? A.kind ?? 'custom'), true], ['Range', A.range ?? A.based_on ?? null], ['Fill', A.fill != null ? (rgb(A.fill) ?? String(A.fill)) : null], ['Ease', A.ease ?? null]],
  set_text_path: A => [['Text path', Array.isArray(A.points) ? `${A.points.length} points` : A.shape ? String(A.shape) : A.remove ? 'removed' : 'set', true], ['Start', A.start != null ? short(A.start) : null], ['Flip', A.flip != null ? short(A.flip) : null]],
  set_text_style: (A, R, C) => [['Text style', 'applied', true], ...argRows(A, 2), ['Props', fxCount(C)]],
  set_text_weight: (A, R, C) => [['Weight', A.weight != null ? short(A.weight) : 'default', true], ['Props', fxCount(C)]],
  set_audio_effect_key: A => [[`Key ${A.coefficient ?? ''}`.trim(), `${short(A.value) ?? ''}${A.time != null ? ` @ ${short(A.time)}s` : ''}`], ['Effect', A.effect_index != null ? `#${A.effect_index}` : null]],
  remove_track_matte: () => [['Track matte', 'removed']],
  validate_captions: (A, R) => T.validate_captions(R || {}),
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
    : tool === 'pick_key_color' ? [0, 1, 2].map(j => [`Key ${'RGB'[j]}`, Array.isArray(R.sampled_rgb) && num(R.sampled_rgb[j]) != null ? r2(R.sampled_rgb[j]) : null])
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
  else if (tool === 'pick_key_color') { if (rgb(R.sampled_rgb)) parts.push(`key ${rgb(R.sampled_rgb)}`); }
  else if (tool === 'match_color') { if (rgb(R.src_mean) && rgb(R.ref_mean)) parts.push(`mean ${rgb(R.src_mean)} → ${rgb(R.ref_mean)}`); }
  else if (tool === 'match_grade') { if (rgb(R.source_lab_mean) && rgb(R.reference_lab_mean)) parts.push(`Lab ${rgb(R.source_lab_mean)} → ${rgb(R.reference_lab_mean)}`); }
  else return null;
  return parts.length ? `${tool} · ${parts.join(' · ')}` : null;
}
