#!/usr/bin/env python3
"""edit_check info panel: the card header carries the threshold ("threshold t · edit_check", threshold first so a narrow
header cuts the tool name, not the number; from R.threshold, never a hardcoded number) and 4 rows drawn only from the top-level counts and the largest region's bbox, never from the whole
regions list: Changed outside band "n px (max Δ there d)" (the largest delta over the pixels outside the band, 2 decimals;
" · no band given" when no rects were passed), Changed inside band "n px" (only with rects), Max Δ overall "d" (2 decimals,
" · identical" for bit-identical frames), Regions "k · largest w×h at x,y" or "none".

ev_engine_edit_check.json is a real kimo-mcp spectator stream recorded on the jaja end-card edit (1920x1080 24 fps timeline:
jaja-ad_backup_pre-endcard.mp4 at 0 s, the trimmed jaja-ad.mp4 at 36 s, same frame 36 s apart):
  - band (33.375 s vs 69.375 s, allowed [[640,810,640,60]], threshold 0.2): header "threshold 0.2 · edit_check", outside "0 px (max Δ
    there 0.17)", inside "2345 px", Max Δ overall "0.80", Regions "72 · largest 39×29 at 676,824" (all from the recording)
  - identical (12 s vs 48 s, no rects, default threshold 0.04): header "threshold 0.04", Max Δ overall "0.00 · identical",
    Regions "none", no inside row
  - no rects (33.375 s, threshold 0.1, max_regions 8): header "threshold 0.1", outside ends " · no band given", no inside row
  - junk scalars ("NaN" outside count, a junk largest bbox, a junk threshold): the row is dropped, the bbox left out, the
    header falls back to the tool name; never 0
  - no "…+" marker, no clipped value cell
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/edit_check_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_edit_check.json').read_text())
SESSION, BAND, SAME, NONE = EV[0], EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
HEAD = "() => { const h = document.querySelector('.readcard .rh'); return h ? h.lastChild.textContent.trim() : null; }"
# layout facts: every 'stack' row puts its value under its key (value top >= key bottom), and the header shows the threshold
# in full (the text node's right edge, measured with a Range, sits inside the header box up to the end of the number)
LAYOUT = """() => { const out = {}; const h = document.querySelector('.readcard .rh'); if (h) { const t = h.lastChild; const r = document.createRange();
  const s = t.textContent, i = s.indexOf(' · '); r.setStart(t, 0); r.setEnd(t, i < 0 ? s.length : i); out.head_fits = r.getBoundingClientRect().right <= h.getBoundingClientRect().right + 0.5; }
  out.stack = [...document.querySelectorAll('.readcard .rr.stack')].map(e => e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5);
  out.stack_rows = out.stack.length; return out; }"""
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
RB, RS, RN = BAND[1]['payload']['result'], SAME[1]['payload']['result'], NONE[1]['payload']['result']
assert RB['allowed_count'] == 1 and RB['outside']['changed_pixels'] == 0 and RS['bit_identical'] is True and RN['allowed_count'] == 0, 'recorded fixture changed'
def head(R): return f"threshold {r2(R['threshold'])} · edit_check"
def outside(R): return f"{R['outside']['changed_pixels']} px (max Δ there {R['outside']['max_delta']:.2f})" + (' · no band given' if R['allowed_count'] == 0 else '')
def overall(R): return f"{R['max_delta']:.2f}" + (' · identical' if R['bit_identical'] else '')
def regions(R):
    if R['region_count'] == 0: return 'none'
    x, y, w, h = R['regions'][0]['bbox']; return f"{R['region_count']} · largest {w}×{h} at {x},{y}"
def junk(path, value):
    pair = copy.deepcopy(BAND); o = pair[1]['payload']['result']
    for k in path[:-1]: o = o[k]
    o[path[-1]] = value; return pair
K4 = ['Changed outside band', 'Changed inside band', 'Max Δ overall', 'Regions']; K3 = ['Changed outside band', 'Max Δ overall', 'Regions']
CASES = [
    ('band', BAND, lambda r, h: clean(r) and h == head(RB) == 'threshold 0.2 · edit_check' and [k for k, _ in r] == K4 and dict(r)['Changed outside band'] == outside(RB) == '0 px (max Δ there 0.17)'
        and dict(r)['Changed inside band'] == f"{RB['inside']['changed_pixels']} px" and dict(r)['Max Δ overall'] == overall(RB) == '0.80' and dict(r)['Regions'] == regions(RB)),
    ('identical', SAME, lambda r, h: clean(r) and h == head(RS) and [k for k, _ in r] == K3 and dict(r)['Max Δ overall'] == '0.00 · identical' and dict(r)['Regions'] == 'none' and dict(r)['Changed outside band'] == outside(RS)),
    ('no rects', NONE, lambda r, h: clean(r) and h == head(RN) and [k for k, _ in r] == K3 and dict(r)['Changed outside band'] == outside(RN) and dict(r)['Changed outside band'].endswith(' · no band given') and dict(r)['Regions'] == regions(RN)),
    ('junk outside count dropped', junk(['outside', 'changed_pixels'], 'NaN'), lambda r, h: clean(r) and 'Changed outside band' not in dict(r) and 'Changed inside band' in dict(r)),
    ('junk bbox: count only', junk(['regions', 0, 'bbox'], ['x', 0, 1, 1]), lambda r, h: clean(r) and dict(r)['Regions'] == str(RB['region_count'])),
    ('junk threshold: plain header', junk(['threshold'], 'x'), lambda r, h: clean(r) and h == 'edit_check' and [k for k, _ in r] == K4),
]
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    bad = 0
    async with async_playwright() as p:
        b = await p.chromium.launch()
        for name, evs, ok in CASES:
            pg = await b.new_page(viewport={'width': 1600, 'height': 900})
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
            await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
            for e in [SESSION] + list(evs): await pg.evaluate('e => __kimo.apply(e)', e)
            await pg.wait_for_timeout(900)
            rows = await pg.evaluate(ROWS); h = await pg.evaluate(HEAD); lay = await pg.evaluate(LAYOUT)
            good = bool(ok(rows, h)) and all(lay['stack']) and (lay.get('head_fits', True) or h == 'edit_check') and lay['stack_rows'] == sum(1 for k, _ in rows if k.startswith('Changed ')); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {h!r} {rows!r} {lay}")
            if len(sys.argv) > 1 and name == 'band': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"edit_check_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
