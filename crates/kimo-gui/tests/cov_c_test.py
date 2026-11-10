#!/usr/bin/env python3
"""COV-C reactive test: the colour-pipeline, project / history / template / cache and frame-QA tools.

Replays tests/ev_engine_cov_c.json (a real kimo-mcp capture: import, add_clip, checkpoint, the colour
pipeline, set_meta, project_diff, undo / redo, restore, checkpoint_delete, batch, templates, a recipe,
save / load, the frame cache, diff_frames, lint_frame) into a cold live page, plus the real lint_frame call
from tests/ev_cov_c.json (it replaced a synthetic event; no synthetic events remain). Then proof(): one real
engine session per tool (refusal, call, undo, redo; see the block above proof()). For every one of the 22 COV-C tools it asserts:
  - the tool is classified dedicated by ui/router.js family() (not the generic fallback),
  - a TOOL-SPECIFIC element in an existing mockup panel is absent before the call lands and present after
    (Color pipeline row, History row, Diff line, Batch table, Media row / footer, timeline label + Inspector);
    the read / tune tools (diff_frames, lint_frame, cache_clear, cache_set_budget; Naman 7:24: HUD status toast,
    rail row) must put their RESULT in the HUD toast and on their rail row, without a mode hop or a canvas change
    -- a bare rail row or a bare "tool ✓" HUD pulse does not count,
  - no raw id (clip3, track1, asset2, clip1000...) and no undefined / NaN / null in those panels,
and that the page goes idle afterwards (0 rAF over 3 s).

    python3 crates/kimo-gui/tests/cov_c_test.py     # exit 1 on failure
"""
import asyncio, functools, http.server, json, re, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads(Path(__file__).with_name('ev_engine_cov_c.json').read_text())
TOOLS = ['set_clip_input_color', 'set_color_pipeline', 'set_hdr_mastering', 'set_ocio_view_delivery', 'set_meta', 'load_project',
         'save_project', 'project_diff', 'checkpoint', 'checkpoint_delete', 'restore', 'undo', 'redo', 'batch', 'recipe_apply',
         'template_apply', 'template_define', 'template_delete', 'cache_clear', 'cache_set_budget', 'diff_frames', 'lint_frame']

PROBE = r"""
(() => {
  window.__raf = 0;
  const rAF = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = cb => rAF(t => { window.__raf++; cb(t); });
})();
"""
JS = r"""
window.ap = e => { __kimo.apply(e); window.__drainAt = __kimo.metrics.raf };   /* every apply: the frame count at enqueue; drained() waits past it */
window.T = sel => [...document.querySelectorAll(sel)].map(e => e.textContent.replace(/\s+/g, ' ').trim());
window.has = (sel, ...parts) => T(sel).some(s => parts.every(p => s.includes(p)));
window.hud = s => { const h = document.querySelector('[data-panel=viewer] .hud'); return !!h && h.textContent.includes(s); };
window.rail = (fn, s) => [...document.querySelectorAll('[data-panel=agent] .call')].some(r => r.textContent.includes(fn) && r.textContent.includes(s));
/* the QA rows (lint_frame, diff_frames, cache_*): the result is the row's point, but a long one cannot fit the rail.
   The rail drops whole args and, when not even one fits, keeps that arg's leading whole words ("1s: low-res …").
   So: the VISIBLE row text (innerText, not hidden textContent) has the tool name and a non-empty word-boundary
   prefix of the result, and the row's title holds the full result. Stricter than the textContent check it replaces,
   which passed on text clipped out of sight. The HUD toast, checked by hud(), shows the full result. */
window.railQA = (fn, s) => [...document.querySelectorAll('[data-panel=agent] .call')].some(r => {
  const vis = r.innerText.replace(/\n/g, '') /* flex items come back one per line */, t = (r.querySelector('.sig[title]') || r).getAttribute('title') || '', w = s.split(' ');
  const esc = x => x.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return vis.includes(fn + '(') && t.includes(s) && new RegExp('(^|[\\s(:,])' + esc(w[0]) + '(?=[\\s,…)]|$)').test(vis);
});
window.row = (k, v, from) => [...document.querySelectorAll('[data-panel=pipe] .row')].some(r => r.querySelector('.k').textContent === k && r.querySelector('.v').textContent.endsWith(v) && (from == null || ((r.querySelector('.from') || {}).textContent === from)));
"""
# per call: the predicate that must be false before the call completes and true after (in the panel the tool owns)
CHECK = {
  'checkpoint#1':        "has('[data-panel=hist] .hrow', 'before_grade', 'checkpoint', '●')",
  'set_color_pipeline':  "row('input', 'Rec.709 sRGB') && row('working', 'ACEScg') && row('output', 'Rec.2100 PQ')",
  'set_clip_input_color':"row('shot_0009', 'AP0 ACEScct') && !!document.querySelector('[data-panel=pipe] .row.flash')",
  'set_hdr_mastering':   "row('MaxCLL', '1000 nits') && row('MaxFALL', '400 nits') && row('Peak', '1000 nits') && T('[data-panel=pipe] .sec').includes('HDR')",
  'set_ocio_view_delivery#1': "row('view', '✗ ACES 1.0 SDR') && !!document.querySelector('[data-panel=pipe] .row.dim') && !document.querySelector('[data-panel=pipe] .row.emb')",
  'set_meta#1':          "T('[data-panel=tl] .clip .cl').includes('hero_wide') && has('[data-panel=insp] .row', 'Name', 'shot_0009.mp4', 'hero_wide') && has('[data-panel=insp] .row', 'Role', 'a-roll') && has('[data-panel=insp] .row', 'Tags', 'hero, day')",
  'set_meta#2':          "T('[data-panel=tl] .trk .lab').some(s => s.includes('dialogue'))",
  'checkpoint#2':        "has('[data-panel=hist] .hrow', 'graded', 'checkpoint', '●')",
  'project_diff':        "has('[data-panel=diff] .ph', 'before_grade to now') && has('[data-panel=diff] div', '~', 'hero_wide 5 changes') && has('[data-panel=diff] div', 'track unnamed -> dialogue')",
  'undo#1':              "has('[data-panel=hist] .hrow', 'undo', '9 left', '●') && hud('undo 1 step · 9 left to undo, 1 to redo')",
  'redo#1':              "has('[data-panel=hist] .hrow', 'redo', '10 left', '●') && !has('[data-panel=hist] .hrow', '9 left') && hud('redo 1 step · 10 left to undo, 0 to redo')",
  'undo#2':              "has('[data-panel=hist] .hrow', 'undo 2 steps', '8 left') && hud('undo 2 steps · 8 left to undo, 2 to redo')",
  'redo#2':              "has('[data-panel=hist] .hrow', 'redo 2 steps', '10 left') && hud('redo 2 steps')",
  'restore':             "has('[data-panel=hist] .hrow', 'restore', 'before_grade', '●') && hud('restore · back to before_grade') && [...document.querySelectorAll('[data-panel=hist] .hrow')].some(r => r.textContent.includes('before_grade') && r.textContent.includes('checkpoint') && r.querySelector('.thumb.sel'))",
  'checkpoint_delete':   "[...document.querySelectorAll('[data-panel=hist] .hrow')].some(r => r.textContent.includes('before_grade') && r.textContent.includes('deleted') && r.style.opacity === '0.45')",
  'batch':               "has('[data-panel=diff] div', 'Batch trim both', '2 / 2') && has('[data-panel=diff] div', 'shot_0009.mp4', 'trim_clip', 'warned') && has('[data-panel=diff] div', 'shot_0015.mp4', 'trim_clip')",
  'template_define':     "has('[data-panel=diff] div', '+', 'template lower_third (text, at) · 1 op')",
  'template_apply':      "has('[data-panel=diff] div', 'Template lower_third', '1 op') && has('[data-panel=diff] div', 'Jane Doe', 'lower_third', 'added')",
  'recipe_apply':        "has('[data-panel=diff] div', 'Recipe lower_third', '40 ops') && ['bar', 'accent', 'title', 'subtitle'].every(k => has('[data-panel=diff] div', k, 'lower_third', 'added'))",
  'template_delete':     "T('[data-panel=diff] div').some(s => s === '-template lower_third')",
  'save_project':        "has('[data-panel=hist] .hrow', 'proj.kimo', 'saved', '●') && hud('save_project · proj.kimo + history: 16 undo steps, 1 checkpoint') && has('[data-panel=media] .pfoot', 'save_project: proj.kimo, history')",
  'load_project':        "T('[data-panel=hist] .hrow').length === 1 && has('[data-panel=hist] .hrow', 'proj.kimo', 'loaded') && hud('load_project · proj.kimo: 16 undo steps, 1 checkpoint') && has('[data-panel=media] div', 'Montserrat', 'font missing ×3') && has('[data-panel=media] .pfoot', 'load_project: 5 files, 3 missing')",
  'cache_set_budget':    "hud('frame cache 512 MB, 0 B used') && railQA('cache_set_budget', 'budget: 512 MB')",
  'cache_clear':         "hud('frame cache cleared · 0 frames, 0 B freed') && railQA('cache_clear', 'freed: 0 B, 0 frames')",
  'diff_frames':         "hud('diff_frames · now 0.5s vs graded 0.5s: 77% changed, major (PSNR 10.5 dB)') && railQA('diff_frames', 'changed: 77%, major')",
  'lint_frame':          "hud('lint_frame 1s · low-res upscale ×1') && railQA('lint_frame', 'low-res upscale ×1')",
  # the real lint_frame call from the COV-C2 capture (it replaced a synthetic event)
  'lint_frame#2':        "hud('lint_frame 1s · low-res upscale ×2, out of bounds ×1') && railQA('lint_frame', 'low-res upscale ×2, out of bounds ×1')",
}
SYNTH = [e for e in json.loads(Path(__file__).with_name('ev_cov_c.json').read_text())['proof']['lint_frame']
         if e.get('tool_name') == 'lint_frame' and (e.get('payload') or {}).get('args', {}).get('time') == 1.0 or
            (e.get('tool_name') == 'lint_frame' and e.get('phase') == 'complete')]
QA = {'diff_frames', 'lint_frame', 'cache_clear', 'cache_set_budget'}
RAW = r"/\b(?:c|clip|t|track|fx|effect|node|n|layer|l|asset|media)[_-]?\d+(?:[_-][a-z]{0,5}\d+|[_-][a-z])*\b/i"
PANELS = '[data-panel=agent] .call, [data-panel=pipe], [data-panel=hist], [data-panel=diff], [data-panel=media], [data-panel=insp], [data-panel=viewer], [data-panel=tl] .clip, [data-panel=tl] .trk .lab'

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

# ---------------------------------------------------------------------------------------------------------------
# COV-C2 proof (Naman 9:02, "no skipping, no rough mocks"): tests/ev_cov_c.json holds one REAL kimo-mcp session per
# tool, captured from k4-kimo-mcp-bin over stdio (/tmp/covc2/cap.py): the engine's own refusal of the tool, the
# successful call, then the engine's undo and redo. Per tool:
#   (a) the affected entities by their engine ids in the window's model, and the exact values on screen, before -> after;
#   (b) the real failed call shows as failed (its rail row in the error state with the engine's words, the HUD says so)
#       and changes nothing: the model identical down to the flash stamps, the tool's panels byte-identical;
#   (c) undo puts the tool's panels back byte-for-byte to before the call, redo to after it (and the model with them).
# Read-only tools (mutated false in the engine, no undo step): the call leaves the timeline and the pipeline panels
# byte-identical and pushes nothing on the undo stack. Two honest exceptions, from the engine itself:
#   set_ocio_view_delivery: this build has no native-ocio feature (kimo-core Cargo.toml native-ocio = [], off), every
#     call is refused "requires native-ocio feature and OCIO2.5.1"; the window shows the explicit engine-disabled state
#     (the view row struck and dimmed, the HUD names it engine-disabled) and nothing else moves, no undo step;
#   cache_clear: no refusal exists (kimo-core commands/tools_4.rs: its run always returns Ok), so (b) has no case.
PROOF = json.loads(Path(__file__).with_name('ev_cov_c.json').read_text())['proof']
CJS = r"""
window.__c = {
  views: () => { const v = __kimo.state.views || {}, g = (m, n) => (v[m] || {})[n] || null;
    return { pipe: g('color', 'pipe'), insp: g('edit', 'insp'), diff: g('batch', 'diff'), media: g('batch', 'media'), hist: g('batch', 'hist') }; },
  model: () => { const tl = __kimo.state.liveTL || {}, s = __kimo.state;
    return JSON.stringify({ tracks: tl.tracks || [], markers: tl.markers || [], pipe: s._pipe || null, cps: Object.keys(s._cps || {}), saved: Object.keys(s._saved || {}), views: __c.views() }); },
  canon: () => { const tl = __kimo.state.liveTL || {}, s = __kimo.state, v = __c.views();
    return JSON.stringify({ tracks: tl.tracks || [], markers: (tl.markers || []).filter(m => !m.gone), pipe: s._pipe || null, views: [v.pipe, v.insp, v.diff, v.media].map(o => (!o || Object.keys(o).every(k => k === '_flash' || k === '_footFlash' || o[k] == null || (Array.isArray(o[k]) && !o[k].length)) ? null : o)) },
      (k, x) => (k === '_flash' || k === '_footFlash' || k === '_ch' ? undefined : x)); },   /* flash stamps, faded markers and the History log (undo / redo rows) left out */
  clip: id => { for (const t of (__kimo.state.liveTL || {}).tracks || []) for (const c of t.clips || []) if (c.id === id) return { track: t.label, s: c.s, e: c.e, label: c.label, fx: c.fx || [], ids: c.ids || null }; return null; },
  pipe: () => { const p = __kimo.state._pipe; return p ? JSON.parse(JSON.stringify(p, (k, x) => (k === '_ch' ? undefined : x))) : null; },
  last: () => { const c = __kimo.state.calls[__kimo.state.calls.length - 1] || {}, r = c.id != null ? document.querySelector(`[data-call="${CSS.escape(String(c.id))}"]`) : null;
    return { name: c.name, status: c.status, error: c.error, rowErr: !!r && r.classList.contains('err') }; },
  hud: () => { const h = document.querySelector('[data-panel=viewer] .hud'); return h ? [h.className, h.textContent] : ['', '']; },
  depth: () => ((__kimo.state._covHist || {}).undo || []).length,
};
"""
PIPE_ = [('color', 'pipe')]
PSCR = {'set_color_pipeline': PIPE_, 'set_clip_input_color': PIPE_, 'set_hdr_mastering': PIPE_, 'set_ocio_view_delivery': PIPE_ + [('edit', 'tl')],
        'set_meta': [('edit', 'tl'), ('edit', 'insp')], 'undo': [('edit', 'tl'), ('edit', 'insp')], 'redo': [('edit', 'tl'), ('edit', 'insp')],
        'restore': [('edit', 'tl'), ('color', 'pipe')], 'load_project': [('edit', 'tl'), ('batch', 'media')],
        'batch': [('batch', 'diff'), ('edit', 'tl')], 'recipe_apply': [('batch', 'diff'), ('edit', 'tl')], 'template_apply': [('batch', 'diff'), ('edit', 'tl')],
        'template_define': [('batch', 'diff')], 'template_delete': [('batch', 'diff')],
        'save_project': [('batch', 'hist'), ('batch', 'media')], 'checkpoint': [('batch', 'hist')], 'checkpoint_delete': [('batch', 'hist')], 'project_diff': [('batch', 'diff')],
        'cache_clear': [], 'cache_set_budget': [], 'diff_frames': [], 'lint_frame': []}
RO_C = {'save_project', 'checkpoint', 'checkpoint_delete', 'project_diff', 'cache_clear', 'cache_set_budget', 'diff_frames', 'lint_frame'}
STILL = [('edit', 'tl'), ('color', 'pipe')]   # what a read-only call must leave byte-identical
J = json.dumps
J2 = lambda x: json.dumps(x, separators=(',', ':'), ensure_ascii=False)
def ROWS(mode, panel, L): return (mode, panel, L)
def PROOF_SUCCESS(tool, R, A, ids):
    CR = R.get('created'); rc = (CR.get('clip_ids') if isinstance(CR, dict) else None) or [None]; tc = CR if isinstance(CR, list) and CR else [None]
    lab = lambda i, s: f"__c.clip({J(ids[i])}) && __c.clip({J(ids[i])}).label === {J(s)}"
    span = lambda i, s, e: f"__c.clip({J(ids[i])}) && __c.clip({J(ids[i])}).s === {s} && __c.clip({J(ids[i])}).e === {e}"
    P709 = ['inputRec.709 sRGB', 'workingACEScg', 'outputRec.709 sRGB']
    return {
      'set_color_pipeline': ([("pipeline output Rec.709 sRGB -> Rec.2100 PQ (engine settings output_gamut 1, transfer 6), input / working kept", "JSON.stringify(__c.pipe()) === " + J(J2({'input': 'Rec.709 sRGB', 'working': 'ACEScg', 'view': None, 'output': 'Rec.2100 PQ'})))],
                             [ROWS('color', 'pipe', ['inputRec.709 sRGB', 'workingACEScg', 'outputRec.2100 PQ'])], P709),
      'set_clip_input_color': ([(f"{ids[0]} (shot_0009.mp4) gets its own input row AP0 ACEScct (engine gamut 4, transfer 3), {ids[1]} none", "JSON.stringify(__c.pipe().clips) === " + J(J2({'shot_0009': 'AP0 ACEScct'})))],
                               [ROWS('color', 'pipe', ['inputRec.709 sRGB', 'shot_0009AP0 ACEScct', 'workingACEScg', 'outputRec.709 sRGB'])], P709),
      'set_hdr_mastering': ([("HDR mastering 1000 / 400 / 1000 / 0.005 nits in the model", "JSON.stringify(__c.pipe().hdr) === " + J(J2({'max_cll': '1000 nits', 'max_fall': '400 nits', 'max_nits': '1000 nits', 'min_nits': '0.005 nits'})))],
                            [ROWS('color', 'pipe', ['inputRec.709 sRGB', 'workingACEScg', 'outputRec.2100 PQ', 'MaxCLL1000 nits', 'MaxFALL400 nits', 'Peak1000 nits', 'Min0.005 nits'])],
                            ['inputRec.709 sRGB', 'workingACEScg', 'outputRec.2100 PQ']),
      'set_meta': ([(f"{ids[0]} shot_0009.mp4 -> hero_wide, {ids[1]} untouched", lab(0, 'hero_wide') + ' && ' + lab(1, 'shot_0015.mp4'))],
                   [ROWS('edit', 'insp', ['Nameshot_0009.mp4→hero_wide', 'Rolea-roll', 'Tagshero, day'])], None),
      'load_project': ([(f"proj.kimo as saved: {ids[0]} after_save -> shot_0009.mp4", lab(0, 'shot_0009.mp4') + ' && ' + lab(1, 'shot_0015.mp4')),
                        ("History: proj.kimo loaded (the list starts over)", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['proj.kimo', 'loaded']]))),
                        ("Media footer: load_project: proj.kimo, 2 tracks", "__c.views().media.foot === 'load_project: proj.kimo, 2 tracks'")], [], None),
      'restore': ([(f"back to checkpoint rough: {ids[0]} hero_wide -> shot_0009.mp4", lab(0, 'shot_0009.mp4') + ' && ' + lab(1, 'shot_0015.mp4')),
                   ("the pipeline set after the checkpoint is gone (the checkpoint had none)", "__c.pipe() === null"),
                   ("History: restore · rough on top, the checkpoint row kept", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['restore', 'rough'], ['rough', 'checkpoint']])))],
                  [ROWS('color', 'pipe', [])], P709),
      'batch': ([(f"{ids[0]} 0-3 s -> 0-2.5 s, {ids[1]} 0-3 s -> 0-1.5 s (engine result_brief durations)", span(0, 0, 2.5) + ' && ' + span(1, 0, 1.5))],
                [ROWS('batch', 'diff', None)], None),
      'recipe_apply': ([(f"lower_third's 4 engine clips {rc} layered on V1 0.5-5.5 s; {ids[0]} not cut", f"JSON.stringify(__c.clip({J(rc[0])})) === " + J(J2({'track': 'V1', 's': 0.5, 'e': 5.5, 'label': 'lower_third · Jane Doe', 'fx': ['4 layers'], 'ids': rc})) + ' && ' + span(0, 0, 3))], [], None),
      'template_apply': ([(f"engine clip {tc[0]} 'Jane Doe' layered on V1 1-3 s; {ids[0]} not cut", f"JSON.stringify(__c.clip({J(tc[0])})) === " + J(J2({'track': 'V1', 's': 1, 'e': 3, 'label': 'Jane Doe', 'fx': [], 'ids': tc})) + ' && ' + span(0, 0, 3))], [], None),
      'template_define': ([("Diff list: + template lower_third (text, at) · 1 op", "JSON.stringify(__c.views().diff.diff) === " + J(J2([['+', 'template lower_third (text, at) · 1 op']])))], [], None),
      'template_delete': ([("Diff list: - template lower_third under its definition", "JSON.stringify(__c.views().diff.diff) === " + J(J2([['+', 'template lower_third (text, at) · 1 op'], ['-', 'template lower_third']])))], [], None),
      'undo': ([(f"{ids[0]} hero_wide -> shot_0009.mp4 (the rename undone)", lab(0, 'shot_0009.mp4')),
                ("History row: undo · 6 left", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['undo', '6 left']])))], [], None),
      'redo': ([(f"{ids[0]} shot_0009.mp4 -> hero_wide (the rename redone)", lab(0, 'hero_wide')),
                ("History row: redo · 7 left (one row is the stack position)", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['redo', '7 left']])))], [], None),
      'save_project': ([("History: proj2.kimo saved; Media footer names it", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['proj2.kimo', 'saved']])) + " && __c.views().media.foot === 'save_project: proj2.kimo, history 5.9 KB'")], [], None),
      'checkpoint': ([("History: before_grade checkpoint", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name])) === " + J(J2([['before_grade', 'checkpoint']])))], [], None),
      'checkpoint_delete': ([("History: old_cut deleted (dimmed), keep stays", "JSON.stringify(__c.views().hist.items.map(x => [x.id, x.name, !!x.dim])) === " + J(J2([['old_cut', 'deleted', True], ['keep', 'checkpoint', False]])))], [], None),
      'project_diff': ([("Diff list: rough to now, ~ hero_wide 1 change, +0 ~1 -0", "JSON.stringify([__c.views().diff.titleSmall, __c.views().diff.meta, __c.views().diff.diff]) === " + J(J2(['rough to now', '+0 ~1 -0', [['~', 'hero_wide  1 change']]])))], [], None),
      'cache_clear': ([("HUD: frame cache cleared · 2 frames, 28.1 MB freed (engine cleared_entries 2, freed_bytes 29491200)", "__c.hud()[1].endsWith('frame cache cleared · 2 frames, 28.1 MB freed')"), ("rail row keeps it", "railQA('cache_clear', '28.1 MB, 2 frames')")], [], None),
      'cache_set_budget': ([("HUD: frame cache 512 MB, 0 B used", "__c.hud()[1].endsWith('frame cache 512 MB, 0 B used')"), ("rail row keeps it", "railQA('cache_set_budget', '512 MB')")], [], None),
      'diff_frames': ([("HUD: now 0.5s vs rough 0.5s: 50% changed, major (PSNR 5.7 dB)", "__c.hud()[1].endsWith('diff_frames · now 0.5s vs rough 0.5s: 50% changed, major (PSNR 5.7 dB)')"), ("rail row keeps it", "railQA('diff_frames', 'changed: 50%, major')")], [], None),
      'lint_frame': ([("HUD: 1s · low-res upscale ×2, out of bounds ×1 (shot_0015.mp4 moved 900 px right)", "__c.hud()[1].endsWith('lint_frame 1s · low-res upscale ×2, out of bounds ×1')"), ("rail row keeps it", "railQA('lint_frame', 'low-res upscale ×2, out of bounds ×1')")], [], None),
    }[tool]

async def proof(b, url, fails):
    ok, done = 0, []
    for tool, events in PROOF.items():
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); await ctx.add_init_script(CJS)
        pg = await ctx.new_page(); errs = []
        pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.goto(url); await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0'); await pg.evaluate(JS)
        scr = PSCR[tool]
        async def drained():
            # The page's own drain signal: app.js frame() bumps __kimo.metrics.raf and drains the WHOLE event queue in
            # the same synchronous call, so once raf has moved past its value at the last apply (window.__drainAt, set by
            # ap() in JS at every apply) that event is drained and every flash stamp it set (router.js now(): the frame's
            # clock) is in the past. If raf never moves, wait_for_function raises at Playwright's own default timeout and
            # the run fails; nothing falls through.
            await pg.wait_for_function('() => __kimo.metrics.raf > (window.__drainAt ?? -1)', polling=10)
        async def panels(which=None):
            # the 720 ms count from the frame that landed the last event, not from this call: on a loaded box that frame
            # runs late, so a count started at the apply could end <700 ms after the stamp and read a stale "flash" class.
            await drained()
            await pg.wait_for_timeout(720)   # flash stamps run out (FLASH_MS 700)
            out, m0 = [], await pg.evaluate('__kimo.state.mode')
            for m, name in (which or scr):
                await pg.evaluate('m => ap({ type: "mode", mode: m })', m); await pg.wait_for_timeout(200)
                out.append(await pg.evaluate('p => { const e = document.querySelector(`[data-panel=${p}]`); return e ? e.innerHTML : "<missing>" }', name))
            await pg.evaluate('m => ap({ type: "mode", mode: m })', m0); await pg.wait_for_timeout(120)
            await pg.evaluate('() => { __kimo.state.explicitMode = false }')
            return out
        async def rows(m, name):
            await pg.evaluate('m => ap({ type: "mode", mode: m })', m); await pg.wait_for_timeout(200)
            r = await pg.evaluate('p => [...document.querySelectorAll(`[data-panel=${p}] .row`)].map(r => r.textContent.replace(/\\s+/g, " ").trim())', name)
            await pg.evaluate('() => { __kimo.state.explicitMode = false }'); return r
        async def st(): await drained(); return await pg.evaluate("() => ({ model: __c.model(), canon: __c.canon(), depth: __c.depth() })")
        def check(desc, res):
            nonlocal ok
            if res is True: ok += 1; print(f'PASS  proof {tool}: {desc}')
            else: fails.append(f'proof {tool}: {desc}'); print(f'FAIL  proof {tool}: {desc} -> {str(res)[:900]}')
        ids, phase, pre_rows = [], None, None
        fail_cid = next((e.get('call_id') for e in events if e.get('tool_name') == tool and e.get('phase') == 'error'), None)
        k0 = next((n for n, e in enumerate(events) if e.get('call_id') == fail_cid), 0) if fail_cid else 0
        succ_cid = next((e.get('call_id') for e in events[k0:] if e.get('tool_name') == tool and e.get('phase') == 'complete'), None)
        D0 = S0 = Dpre = Spre = D2 = S2 = Dst = None
        steps = []   # the two undo / redo events after the success
        for i, ev in enumerate(events):
            tn, ph, cid = ev.get('tool_name'), ev.get('phase'), ev.get('call_id')
            if ph == 'start' and cid is not None and cid == fail_cid:
                D0 = await panels(); S0 = await st()
            if ph == 'start' and cid is not None and cid == succ_cid:
                Dpre = await panels(); Spre = await st(); pre_rows = await rows('color', 'pipe')
                if tool in RO_C: Dst = await panels(STILL)
            await pg.evaluate('e => ap(e)', ev)
            if ph != 'complete' and ph != 'error': continue
            P_ = ev.get('payload') or {}; R = P_.get('result') if isinstance(P_.get('result'), dict) else {}
            if tn == 'add_clip' and ph == 'complete': ids.append(R.get('clip_id'))
            if cid == fail_cid:
                await pg.wait_for_timeout(60); await drained()
                L = await pg.evaluate('__c.last()'); H = await pg.evaluate('__c.hud()'); S1 = await st()
                check(f"real refused call shows as failed: its rail row in the error state with the engine's words ({P_.get('error')})", L['name'] == tool and L['status'] == 'error' and L['rowErr'] and L['error'] == P_.get('error') or L)
                if tool == 'set_ocio_view_delivery':
                    check("explicit engine-disabled state, never a pass: HUD in the error state names native OCIO off", ('err' in H[0] and H[1].endswith('set_ocio_view_delivery · engine-disabled: native OCIO is off in this build')) or H)
                    got = await rows('color', 'pipe')
                    check("the view row reads struck and dimmed, the rest of the pipeline exactly as before", got == ['inputRec.709 sRGB', 'workingACEScg', 'view✗ ACES 1.0 SDR', 'outputRec.709 sRGB'] and await pg.evaluate("!!document.querySelector('[data-panel=pipe] .row.dim') && !document.querySelector('[data-panel=pipe] .row.emb')") or got)
                    m0, m1 = json.loads(S0['model']), json.loads(S1['model'])
                    check("no engine state: timeline model identical, no undo step pushed", (m0['tracks'], m0['markers'], S0['depth']) == (m1['tracks'], m1['markers'], S1['depth']))
                    D1 = await panels([('edit', 'tl')]); check("timeline panel byte-identical", D1 == D0[1:])
                    phase = 'unsupported'; steps = [('undo', 'pre'), ('redo', 'post')]
                    continue
                check("HUD says it failed", ('err' in H[0] and f'{tool} failed' in H[1]) or H)
                check("no state mutation: model identical, flash stamps included", S1['model'] == S0['model'] or [S0['model'][:400], S1['model'][:400]])
                check("undo stack untouched", S1['depth'] == S0['depth'])
                if scr:
                    D1 = await panels(); check(f"no fake visual: panels byte-identical after the failure ({', '.join(f'{m}:{n}' for m, n in scr)})", D1 == D0)
                phase = 'failed'
            elif cid == succ_cid and ph == 'complete':
                await pg.wait_for_timeout(60); await drained()
                A_ = next((e['payload'].get('args') for e in events[:i] if e.get('call_id') == cid and e.get('phase') == 'start'), {}) or {}
                vals, rws, before_rows = PROOF_SUCCESS(tool, R, A_, ids)
                if before_rows is not None: check(f"before the call the Color pipeline list read {before_rows}", pre_rows == before_rows or pre_rows)
                for desc, js in vals:
                    try: res = await pg.evaluate(js)
                    except Exception as e: res = f'error {e}'
                    if res is not True: res = [res, await pg.evaluate('() => ({ pipe: __c.pipe(), tracks: (__kimo.state.liveTL || {}).tracks, views: __c.views(), hud: __c.hud() })')]
                    check(desc, res)
                for m, name, L in rws:
                    got = await rows(m, name)
                    if L is not None: check(f"on screen ({m} · {name}): {L}", got == L or got)
                S2 = await st()
                if tool in RO_C:
                    check("read-only in the engine (mutated false): no undo step in the window", P_.get('mutated') is False and S2['depth'] == Spre['depth'])
                    check("timeline and pipeline panels byte-identical (timeline untouched)", (await panels(STILL)) == Dst)
                    m0, m2 = json.loads(Spre['model']), json.loads(S2['model'])
                    check("  ... and the timeline model and the pipeline", (m0['tracks'], m0['markers'], m0['pipe']) == (m2['tracks'], m2['markers'], m2['pipe']))
                    if scr: D2 = await panels(); check("its visual is on its panel (it differs from before the call)", D2 != Dpre)
                    phase = 'ro'
                else:
                    D2 = await panels(); check("its visual is on the panels (they differ from before the call)", D2 != Dpre)
                    phase = 'done'
                    steps = [('redo', 'pre'), ('undo', 'post')] if tool == 'undo' else [('undo', 'pre'), ('redo', 'post')]
            elif steps and tn == steps[0][0] and ph == 'complete':
                want = steps.pop(0)[1]
                D = await panels(); S = await st()
                if phase == 'unsupported':
                    if want == 'pre': check("engine undo undoes the step before the refusal (set_color_pipeline; the refusal left no step, undo_left 6): the pipeline list empties, timeline byte-identical", (await rows('color', 'pipe')) == [] and D[1] == D0[1] and json.loads(S['model'])['pipe'] is None and R.get('undo_left') == 6)
                    else: check("redo brings set_color_pipeline back, without the struck view row (no OCIO view was ever applied)", (await rows('color', 'pipe')) == ['inputRec.709 sRGB', 'workingACEScg', 'outputRec.709 sRGB'])
                    continue
                nm = 'undo' if want == 'pre' else 'redo'
                if tool in ('undo', 'redo'): nm = f"{tn} (reversing the {tool})" if want == 'pre' else f"{tn} (repeating the {tool})"
                if want == 'pre': check(f"{nm} restores the panels byte-for-byte to before the call", D == Dpre or [x[:300] for x in D]); check("  ... and the model", S['canon'] == Spre['canon'] or [Spre['canon'][:300], S['canon'][:300]])
                else: check(f"{nm} restores the panels byte-for-byte to after the call", D == D2); check("  ... and the model", S['canon'] == S2['canon'])
        if tool == 'cache_clear': print('NOTE  proof cache_clear: (b) no case, the engine has no refusal path for this tool')
        if steps: fails.append(f'proof {tool}: undo / redo steps never ran {steps}')
        if phase is None or (fail_cid is None and tool != 'cache_clear'): fails.append(f'proof {tool}: the call never completed or no real refusal in the capture')
        if errs: fails.append(f'proof {tool}: page errors {errs[:3]}')
        done.append(tool); await ctx.close()
    return ok, done


async def main():
    from playwright.async_api import async_playwright
    fails = []
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); await ctx.add_init_script(PROBE)
        pg = await ctx.new_page(); errs = []
        pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
        await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
        await pg.evaluate(JS)
        fam = await pg.evaluate("async names => { const r = await import('./router.js'); return names.map(n => [n, r.family(n)]); }", TOOLS)
        for n, f in fam:
            ok = f not in ('other',)
            print(f"{'PASS' if ok else 'FAIL'}  family  {n:<24} {f}")
            if not ok: fails.append('family:' + n)
        seen, start = {}, {}
        async def frame(): await pg.wait_for_timeout(50)
        for e in EV + SYNTH:
            name = e.get('tool_name')
            if name in TOOLS and e.get('phase') in ('complete', 'error'):
                k = seen[name] = seen.get(name, 0) + 1
                key = f'{name}#{k}' if f'{name}#{k}' in CHECK else (name if k == 1 and name in CHECK else None)
                if key is None: print(f'FAIL  no check for {name} #{k}'); fails.append(f'{name}#{k}'); continue
                await frame(); before = await pg.evaluate(CHECK[key]); m0 = await pg.evaluate('__kimo.state.mode'); v0 = await pg.evaluate("[...document.querySelectorAll('[data-panel=viewer] .frame > :not(.hud)')].map(e => e.outerHTML).join('')")
                await pg.evaluate('e => ap(e)', e); await frame()
                after = await pg.evaluate(CHECK[key])
                bad = await pg.evaluate(f"""() => [...document.querySelectorAll('{PANELS}')].map(x => x.textContent).filter(s => {RAW}.test(s) || /\\b(undefined|NaN|null)\\b/.test(s)).map(s => s.slice(0, 80))""")
                mode = await pg.evaluate('__kimo.state.mode')
                still = True
                if name in QA:   # read / tune only: the workspace does not hop and the canvas is untouched
                    v1 = await pg.evaluate("[...document.querySelectorAll('[data-panel=viewer] .frame > :not(.hud)')].map(e => e.outerHTML).join('')")
                    still = mode == m0 and v1 == v0
                    if not still: print(f'      {key}: mode {m0} -> {mode} or the canvas changed')
                ok = (not before) and after and not bad and still
                print(f"{'PASS' if ok else 'FAIL'}  {key:<26} [{mode}] before={before!s:<5} after={after!s:<5}" + (f' raw/bad text: {bad[:2]}' if bad else ''), flush=True)
                if not ok: fails.append(key)
            else:
                await pg.evaluate('e => ap(e)', e)
        missing = [t for t in TOOLS if t not in seen]
        if missing: print('FAIL  never exercised:', missing); fails += missing
        await pg.wait_for_timeout(2000)
        r0 = await pg.evaluate('__raf'); await pg.wait_for_timeout(3000); r1 = await pg.evaluate('__raf')
        print(f"{'PASS' if r1 == r0 else 'FAIL'}  idle after the replay: {r1 - r0} rAF in 3 s")
        if r1 != r0: fails.append('idle')
        if errs: print('page errors:', errs); fails.append('pageerror')
        pfails = []
        pok, pdone = await proof(b, f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off', pfails)
        if sorted(pdone) != sorted(TOOLS): pfails.append(f'proof sessions missing for {sorted(set(TOOLS) - set(pdone))}')
        print(f"cov_c proof: {pok} checks passed, {len(pdone)}/{len(TOOLS)} real engine sessions, {len(pfails)} failures")
        for f in pfails: print('  -', f)
        fails += pfails
        await b.close()
    srv.shutdown()
    print(f"cov_c: {len(TOOLS) - len({f.split('#')[0].split(':')[-1] for f in fails if not f in ('idle', 'pageerror')})}/{len(TOOLS)} tools reactive · {'FAIL ' + ', '.join(fails) if fails else 'all green'}")
    sys.exit(1 if fails else 0)

if __name__ == '__main__':
    asyncio.run(main())
