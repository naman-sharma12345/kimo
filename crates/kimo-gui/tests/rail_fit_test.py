#!/usr/bin/env python3
"""Rail fit: every rail row reads whole, on one baseline.

Loads each of the 16 demo screens (sim feed, frozen) and real kimo-mcp captures (tests/ev_engine_*.json, plus any
event files given on the command line) and checks every visible rail row:
  - no text runs past its box: the call (.sig), its args and its duration are not cut (no hidden overflow, no
    mid-word ellipsis); a call too long for the row drops whole args from the end and reads "name(a, b, …)"
  - one baseline per row: the check mark, name, parens, args and duration sit on the same baseline (+-0.5 px),
    measured with a zero-size inline-block probe at each text's baseline
  - rows of the same kind (current / earlier) put that baseline at the same offset from the row's top (+-0.5 px)
Exit 1 on any miss; every miss is printed with its row text.

    python3 crates/kimo-gui/tests/rail_fit_test.py [--dump] [ev files...]
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
CHECK = """() => { const out = [];
  for (const row of document.querySelectorAll('[data-panel] .call')) {
    const r = row.getBoundingClientRect(); if (r.height === 0 || r.bottom < 0) continue;
    const sig = row.querySelector('.sig'), args = row.querySelector('.args'), dur = row.querySelector('.dur');
    const text = row.textContent.replace(/\\s+/g, ' ').trim(), miss = []; let note = null;
    const over = el => el && el.textContent.trim() && el.scrollWidth > el.clientWidth + .5;
    /* a name-only row (not even "(…)" fits) whose tool name alone is wider than the rail: a name-length limit, not an
       args cut; it is reported as a NOTE, and only arises at the narrowed-rail stress width */
    const bareOver = !!row.querySelector('.sig.bare, .sig.noargs') && over(sig);
    if (over(sig) && !bareOver) miss.push('call cut');
    if (bareOver && !row.querySelector('.sig.bare')) note = 'tool name alone wider than the rail: ' + JSON.stringify(row.querySelector('.fn').textContent + '()'); if (over(args)) miss.push('args cut'); if (over(dur)) miss.push('duration cut');
    /* no mid-word cut: a shortened arg list shows whole args of the full call (the row's title, "name(full args)")
       in their order, with "…" where args were dropped — never part of an arg */
    const at = args ? args.textContent : '';
    const fullT = sig ? (sig.getAttribute('title') || '').replace(/^[^(]*\\(/, '').replace(/\\)$/, '') : '';
    const sgb = row.querySelector('.sig.bare');
    if (sgb) {   /* the name alone: only when not even "(…)" fits */
      sgb.classList.remove('bare'); args.textContent = '…'; const fitsEll = sgb.scrollWidth <= sgb.clientWidth - 1 && args.scrollWidth <= args.clientWidth - 1;   /* clearly fits (1px margin; the fit itself measures sub-pixel) */ sgb.classList.add('bare'); args.textContent = '';
      if (fitsEll) miss.push('name only though "(…)" fits'); else note = 'name only, not even "(…)" fits: ' + JSON.stringify(sig.getAttribute('title') || '');
      if (bareOver) note = 'tool name alone wider than the rail: ' + JSON.stringify(sig.getAttribute('title') || '');
    }
    if (at && fullT && at !== fullT) {
      const top = t => { const o = []; let d = 0, q = '', f = 0; for (let i = 0; i < t.length; i++) { const ch = t[i];
        if (q) { if (ch === q) q = ''; continue; } if (ch === '"' || ch === "'") q = ch; else if ('[{('.includes(ch)) d++; else if (']})'.includes(ch)) d = Math.max(0, d - 1);
        else if (ch === ',' && t[i + 1] === ' ' && !d) { o.push(t.slice(f, i)); f = i + 2; i++; } } o.push(t.slice(f)); return o; };
      const full = top((sig.getAttribute('title') || '').replace(/^[^(]*\\(/, '').replace(/\\)$/, '')), shown = top(at);
      let j = 0, ok = true;
      /* a change may show tight, one unit and no spaces round the arrow ("scale 104→112%" for "scale: 104% → 112%"),
         or — only when even the tight form cannot fit — its values alone ("104→112%"): old and new always both show */
      const dm = f => /^([^:]+): (.+?) → (.+)$/.exec(f);
      const tightOf = f => { const m = dm(f); if (!m) return null; const [, k, a, b] = m, ua = /[^\\d.]*$/.exec(a)[0], ub = /[^\\d.]*$/.exec(b)[0];
        const one = ua && ua === ub && /\\d$/.test(a.slice(0, a.length - ua.length)); return `${k} ${one ? a.slice(0, a.length - ua.length) : a}→${b}`; };
      const compactOf = f => { const t = tightOf(f); return t ? t.slice(dm(f)[1].length + 1) : null; };   /* values only, "104→112%": the tight form less its name (a name may hold a space, "x @2s") */
      const same = (f, q) => f === q || tightOf(f) === q || compactOf(f) === q;
      const compacts = [];
      /* last resort, when no whole arg fits: the first arg's leading whole words and " …" ("1s: low-res …"); never a
         change, never inside a word, and only when one more word would not fit */
      const wp = shown.length === 1 && /^(.*\S) …$/.exec(shown[0]);
      if (wp && full[0] !== shown[0] && full[0].startsWith(wp[1] + ' ') && !full[0].includes('→')) {
        const nx = full[0].indexOf(' ', wp[1].length + 1), more = nx > 0 ? full[0].slice(0, nx) + ' …' : full[0] + (full.length > 1 ? ', …' : '');
        const was = args.textContent; args.textContent = more; const fitsMore = args.scrollWidth <= args.clientWidth - 1; args.textContent = was;   /* clearly fits (1px margin) */
        if (fitsMore) miss.push('word-prefix arg though one more word fits: ' + JSON.stringify(more));
        else note = 'no whole arg fits, leading words shown: ' + JSON.stringify(at);
        shown.length = 0;
      }
      for (const q of shown) { if (q === '…') continue; while (j < full.length && !same(full[j], q)) j++; if (j >= full.length) { ok = false; break; } if (q === compactOf(full[j]) && q !== full[j]) compacts.push([shown.indexOf(q), tightOf(full[j])]); j++; }
      for (const [si, tq] of compacts) {
        const was = args.textContent, hadT = row.classList.contains('tight'), alt = shown.slice(); alt[si] = tq;
        args.textContent = alt.join(', '); if (!row.querySelector('.dur[data-run]')) row.classList.add('tight');
        const fitsTight = args.scrollWidth <= args.clientWidth + .5;
        args.textContent = was; if (!hadT) row.classList.remove('tight');
        if (fitsTight) miss.push('values-only change though the tight form fits: ' + JSON.stringify(alt.join(', '))); else note = 'values-only change, tight form cannot fit: ' + JSON.stringify(alt.join(', '));
      }
      /* one form for every from→to change on the rail: tight ("scale 104→112%") or values only, never the spaced
         "scale: 104% → 112%" beside a tight one */
      if (/ → /.test(at)) miss.push('change not in the one rail form (tight): ' + JSON.stringify(at));
      /* every ellipsis stands after whitespace or the open paren: a cut never lands inside a word */
      if (/[^\s(]…/.test(at)) miss.push('cut not on whitespace: ' + JSON.stringify(at));
      if (!ok) miss.push('mid-word cut: ' + JSON.stringify(at) + ' is not whole args of ' + JSON.stringify(full.join(', ')));
      if (full.some(q => q.includes('→')) && !shown.some(q => q.includes('→'))) {
        /* the change was dropped: a miss unless even the changes alone ("…, start: 3 → 6") overflow the row with all the
           room a settled row may borrow; then it is reported as a note with the row's visible text */
        const only = []; for (const q of full) { if (q.includes('→')) only.push(tightOf(q) || q); else if (only[only.length - 1] !== '…') only.push('…'); }
        const was = args.textContent, hadT = row.classList.contains('tight'), hadW = row.classList.contains('wide'), settled = !row.querySelector('.dur[data-run]');
        args.textContent = only.join(', '); if (settled) row.classList.add('tight'); row.classList.add('wide');   /* wide: a current row's empty check column */
        const fitsAlone = args.scrollWidth <= args.clientWidth + .5;
        args.textContent = was; if (!hadT) row.classList.remove('tight'); if (!hadW) row.classList.remove('wide');
        if (fitsAlone) miss.push('from→to change dropped though ' + JSON.stringify(only.join(', ')) + ' fits'); else note = 'change cannot fit: ' + JSON.stringify(only.join(', '));
      }
    }
    if (row.textContent.includes('…') && !at.includes('…') && row.querySelector('.fn') && row.querySelector('.fn').textContent.includes('…')) miss.push('name cut');
    const base = [];
    for (const el of row.querySelectorAll('.ck, .fn, .pp, .args, .dur')) {
      if (getComputedStyle(el).display === 'none') continue;
      const w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT); let tn = null; while (w.nextNode()) if (w.currentNode.nodeValue.trim()) { tn = w.currentNode; break; } if (!tn) continue;
      /* the text's baseline, read with a zero-size inline-block probe beside it in a plain inline wrapper (a probe
         dropped straight into a flex box would become a flex item and sit at the top); the wrapper is undone after */
      const wrap = document.createElement('span'), pr = document.createElement('span');
      pr.style.cssText = 'display:inline-block;width:0;height:0;vertical-align:baseline';
      tn.parentNode.insertBefore(wrap, tn); wrap.appendChild(tn); wrap.appendChild(pr);
      base.push([el.className, pr.getBoundingClientRect().top - r.top]);
      wrap.parentNode.insertBefore(tn, wrap); wrap.remove();
    }
    const ys = base.map(b => b[1]), spread = ys.length ? Math.max(...ys) - Math.min(...ys) : 0;
    if (spread > .5) miss.push('baselines ' + base.map(([c, y]) => c.split(' ')[0] + ' ' + y.toFixed(2)).join(', '));
    out.push({ note, panel: row.closest('[data-panel]').dataset.panel, cur: row.classList.contains('cur'), h: Math.round(r.height), text, miss, base: ys.length ? ys[ys.length > 1 ? 1 : 0] : null });
  }
  return out; }"""
# calls whose point is a from→to change, with more args than the rail has room for: the change must stay visible
def _ev(name, cid, args, res):
    return [{'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': {'clip_id': args.get('clip_id', 'clip4')}, 'payload': {'args': args}},
            {'tool_name': name, 'call_id': cid, 'phase': 'complete', 'target': {'clip_id': args.get('clip_id', 'clip4')}, 'payload': {'dur_ms': 2, 'result': {'ok': True}, **res}}]
DELTA = (_ev('set_property', 'd1', {'clip_id': 'clip4', 'property': 'scale', 'value': 1.12}, {'from': 1.04, 'to': 1.12, 'param': 'scale'})
         + _ev('set_property', 'd2', {'clip_id': 'dune_ridge_wide_aerial', 'property': 'position_x', 'value': 480, 'ease': 'ease_in_out', 'duration': 1.5}, {'from': 0, 'to': 480, 'param': 'position_x'})
         + _ev('set_property', 'd5', {'clip_id': 'clip4', 'property': 'scale'}, {'from': '104%', 'to': '112%', 'param': 'scale'})
         + _ev('move_clip', 'd3', {'clip_id': 'shot_0015.mp4', 'track_id': 'V1', 'start': 6, 'ripple': True}, {'changed': {'clip': {'start': [3, 6]}}})
         + _ev('set_effect_param', 'd4', {'clip_id': 'c1', 'effect_id': 'fx1', 'param': 'amount', 'value': 0.42}, {'changed': {'clip': {'effects[fx1].params.amount.base': [0.35, 0.42]}}, 'from': 0.35, 'to': 0.42, 'param': 'amount'}))
EXPECT = {
    'ev_engine_split.json': ['add_clip(V1, shot_0009.mp4, …)1ms', 'add_clip(V1, shot_0015.mp4, …)1ms'],
    'm14': ['export(1920x1080, h264, …)38400ms'],
    'm13': ['keyframe(camera, pos, …)300ms'],
}
async def main(extra):
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html'
    files = sorted(str(p) for p in Path(__file__).parent.glob('ev_engine_*.json')) + extra
    bad = 0; total = 0
    async with async_playwright() as p:
        b = await p.chromium.launch(); errs = []
        cases = [(f'm{i:02d}', None) for i in range(1, 17)] + [(Path(f).name, f) for f in files] + [('delta rows', DELTA)]
        # every case at several window widths (Naman 8:06): the rail's room changes with the window, so the drop / the
        # word-boundary cut must hold at each; Naman's named rows are pinned at the reference 1376
        for vw in WIDTHS:
          pg = await b.new_page(viewport={'width': vw, 'height': 768}); pg.on('pageerror', lambda e: errs.append(str(e)))
          print(f'--- {vw}x768')
          for name, f in cases:
              if isinstance(f, list):
                  await pg.goto(f'{url}?feed=off'); await pg.wait_for_timeout(400)
                  await pg.evaluate('es => es.forEach(e => __kimo.apply(e))', f); await pg.wait_for_timeout(900)
              elif f is None:
                  await pg.goto(f'{url}?feed=sim&scenario={name}&still=1'); await pg.wait_for_timeout(700)
              else:
                  evs = json.load(open(f)) if f.endswith('.json') and open(f).read(1) == '[' else [json.loads(l) for l in open(f) if l.strip()]
                  await pg.goto(f'{url}?feed=off'); await pg.wait_for_timeout(400)
                  await pg.evaluate('es => es.forEach(e => __kimo.apply(e))', evs); await pg.wait_for_timeout(900)
              if vw != 1376:
                  # the stage is drawn in 1376-wide design units, so the window alone leaves the rail's room unchanged;
                  # scale the rail container itself by the same factor so the fit really runs at another width
                  await pg.evaluate(RAILW, vw / 1376); await pg.wait_for_timeout(400)
              rows = await pg.evaluate(CHECK); total += len(rows)
              if vw != 1376 and not await pg.evaluate("k => { const el = document.querySelector('[data-panel=agent]'); return !el || (k < 1 ? el.clientWidth < window.__railW0 * .95 : el.clientWidth > window.__railW0 * 1.05);   /* the container really changed width */ }", vw / 1376):
                  rows.append({'text': 'rail container', 'miss': ['container width not applied'], 'note': None, 'base': None, 'panel': 'agent', 'cur': False, 'h': 0})
              miss = [r for r in rows if r['miss']]
              # same kind of row (panel, current or not, height): one baseline offset
              groups = {}
              for r in rows:
                  if r['base'] is not None: groups.setdefault((r['panel'], r['cur'], r['h']), []).append(r)
              for k, g in groups.items():
                  ys = [r['base'] for r in g]
                  if max(ys) - min(ys) > .5: miss.append({'text': f'{len(g)} rows {k}', 'miss': [f'baseline offsets {min(ys):.2f}..{max(ys):.2f}']})
              # the rows Naman named (6:2x Oct 16): whole args kept, the trailing one dropped whole, integer ms kept
              for want in (EXPECT.get(name, []) if vw == 1376 else []):
                  if not any(r['text'].lstrip('✓○✕? ').startswith(want) for r in rows): miss.append({'text': want, 'miss': ['expected row not found']})
              bad += len(miss)
              if DUMP: print('\n'.join('      | ' + r['text'] for r in rows))
              for r in rows:
                  if r['note']: NOTES.append(f"{vw} {name}: {r['text']}  ({r['note']})")
              print(f"{'PASS' if not miss else 'FAIL'}  {vw} {name:22s} {len(rows)} rail rows" + ''.join(f"\n      {m['miss']}  {m['text'][:90]}" for m in miss))
          await pg.close()
        await b.close()
    srv.shutdown()
    for n in NOTES: print('NOTE ', n)
    print('page errors', errs[:3])
    print('RESULT:', f'{total} rail rows at {len(WIDTHS)} widths ({", ".join(map(str, WIDTHS))}), every one whole and on one baseline' if not bad and not errs else f'FAIL ({bad} misses)')
    return 0 if not bad and not errs else 1
DUMP = '--dump' in sys.argv
WIDTHS = (1376, 1100, 1600)
RAILW = '''k => {
  const el = document.querySelector('[data-panel=agent]'); if (!el) return;
  window.__railW0 = window.__railW0 || el.clientWidth;
  let st = document.getElementById('__railw'); if (!st) { st = document.createElement('style'); st.id = '__railw'; document.head.appendChild(st); }
  st.textContent = `[data-panel=agent] { width: ${Math.round(window.__railW0 * k)}px !important; max-width: none !important; justify-self: start !important }`;
  el._sz = null; window.dispatchEvent(new Event('resize'));
}'''
NOTES = []
if __name__ == '__main__':
    sys.exit(asyncio.run(main([a for a in sys.argv[1:] if a != '--dump'])))
