#!/usr/bin/env python3
"""edit_check info panel rows: drawn only from the top-level counts (bit_identical, changed_pixels / changed_percent,
allowed_count, outside.changed_pixels / max_delta, inside.changed_pixels / percent_of_allowed, region_count) and the largest
region's bbox, never from the whole regions list. The card keeps 4 rows.

ev_engine_edit_check.json is a real kimo-mcp spectator stream recorded on the jaja end-card edit (1920x1080 24 fps timeline:
jaja-ad_backup_pre-endcard.mp4 at 0 s, the trimmed jaja-ad.mp4 at 36 s, same frame 36 s apart):
  - band allowed (33.375 s vs 69.375 s, allowed [[640,810,640,60]], threshold 0.2): Changed "n px · p%", Outside allowed
    "0 px · max Δ d", Inside allowed "n px · p% of area", Regions "k · largest w×h at x,y", all equal to the recorded numbers
  - identical (12 s vs 48 s, nothing allowed): Changed "identical", Outside "0 px · max Δ 0 · none allowed", Regions "none",
    no Inside row
  - nothing allowed (33.375 s, threshold 0.1, max_regions 8): Outside ends " · none allowed", no Inside row
  - junk scalars ("NaN" changed_pixels, a junk largest bbox): the row is dropped or the bbox left out, never 0
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
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def pct(x): return (r2(x) if x < 10 else f"{round(x * 10) / 10:g}") + '%'
RB, RS, RN = BAND[1]['payload']['result'], SAME[1]['payload']['result'], NONE[1]['payload']['result']
assert RB['allowed_count'] == 1 and RB['outside']['changed_pixels'] == 0 and RS['bit_identical'] is True and RN['allowed_count'] == 0, 'recorded fixture changed'
def changed(R): return 'identical' if R['bit_identical'] else f"{R['changed_pixels']} px · {pct(R['changed_percent'])}"
def outside(R): return f"{R['outside']['changed_pixels']} px · max Δ {r2(R['outside']['max_delta'])}" + (' · none allowed' if R['allowed_count'] == 0 else '')
def inside(R): return f"{R['inside']['changed_pixels']} px · {pct(R['inside']['percent_of_allowed'])} of area"
def regions(R):
    if R['region_count'] == 0: return 'none'
    x, y, w, h = R['regions'][0]['bbox']; return f"{R['region_count']} · largest {w}×{h} at {x},{y}"
def junk_count():
    pair = copy.deepcopy(BAND); pair[1]['payload']['result']['changed_pixels'] = 'NaN'; pair[1]['payload']['result']['bit_identical'] = False; return pair
def junk_bbox():
    pair = copy.deepcopy(BAND); pair[1]['payload']['result']['regions'][0]['bbox'] = ['x', 0, 1, 1]; return pair
CASES = [
    ('band allowed', BAND, lambda r: clean(r) and [k for k, _ in r] == ['Changed', 'Outside allowed', 'Inside allowed', 'Regions'] and dict(r)['Changed'] == changed(RB)
        and dict(r)['Outside allowed'] == outside(RB) and dict(r)['Inside allowed'] == inside(RB) and dict(r)['Regions'] == regions(RB)),
    ('identical', SAME, lambda r: clean(r) and [k for k, _ in r] == ['Changed', 'Outside allowed', 'Regions'] and dict(r)['Changed'] == 'identical' and dict(r)['Outside allowed'] == '0 px · max Δ 0 · none allowed' and dict(r)['Regions'] == 'none'),
    ('nothing allowed', NONE, lambda r: clean(r) and [k for k, _ in r] == ['Changed', 'Outside allowed', 'Regions'] and dict(r)['Changed'] == changed(RN) and dict(r)['Outside allowed'] == outside(RN) and dict(r)['Regions'] == regions(RN)),
    ('junk count dropped', junk_count(), lambda r: clean(r) and 'Changed' not in dict(r) and 'Outside allowed' in dict(r)),
    ('junk bbox: count only', junk_bbox(), lambda r: clean(r) and dict(r)['Regions'] == str(RB['region_count'])),
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
            rows = await pg.evaluate(ROWS); good = bool(ok(rows)); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {rows!r}")
            if len(sys.argv) > 1 and name == 'band allowed': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"edit_check_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
