#!/usr/bin/env python3
"""lint_frame read card with quality: true. Frame and Findings rows as before, then from summary.quality "Crushed / clipped"
("max 12.3% / 0.4%": the largest crushed and clipped fractions of any sampled frame) and "Banding / frozen" ("plateau 9.1 px
· 41 levels · 3 same as prev": the longest mean flat plateau, the fewest 8-bit luma levels, the samples identical to the
frame before), both with the label on its own line and the value under it. Without quality the card is the v5 card (two rows).

ev_engine_frame_quality.json is a real kimo-mcp spectator stream on files/jaja/jaja-ad.mp4 (1920x1080 24 fps, decode only):
lint_frame over 0-36 s every 1 s with quality, the same without it, and the 32-36 s end card every 0.5 s with band_step 1.
Junk: a broken max_clipped_frac drops the Crushed / clipped row; a quality summary that is not an object gives the v5 card.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/frame_quality_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_frame_quality.json').read_text())
SESSION = EV[0]; FULL, V5, END = EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5)"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def pc(x): return f"{round(x * 1000) / 10:g}%"
def qrows(pair):
    Qs = res(pair)['summary']['quality']
    cc = f"max {pc(Qs['max_crushed_frac'])} / {pc(Qs['max_clipped_frac'])}"
    bf = ' · '.join([f"plateau {round(Qs['max_band_plateau_px'] * 10) / 10:g} px", f"{Qs['min_luma_levels']} levels", f"{Qs['identical_to_prev']} same as prev"])
    return lambda r: clean(r) and [k for k, _ in r] == ['Frame', 'Findings', 'Crushed / clipped', 'Banding / frozen'] and dict(r)['Crushed / clipped'] == cc and dict(r)['Banding / frozen'] == bf
def junk(edit):
    pair = copy.deepcopy(FULL); edit(pair[1]['payload']['result']['summary']); return pair
assert 'quality' in res(FULL)['summary'] and 'quality' not in res(V5)['summary'], 'recorded fixture changed'
CASES = [
    ('quality, 0-36 s', FULL, qrows(FULL), 2),
    ('no quality: the v5 card', V5, lambda r: clean(r) and [k for k, _ in r] == ['Frame', 'Findings'], 0),
    ('quality, end card band_step 1', END, qrows(END), 2),
    ('junk clipped: row dropped', junk(lambda S: S['quality'].__setitem__('max_clipped_frac', 'x')), lambda r: clean(r) and 'Crushed / clipped' not in dict(r) and 'Banding / frozen' in dict(r), 1),
    ('quality not an object: v5 card', junk(lambda S: S.__setitem__('quality', 'x')), lambda r: clean(r) and [k for k, _ in r] == ['Frame', 'Findings'], 0),
]
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    bad = 0
    async with async_playwright() as p:
        b = await p.chromium.launch()
        for name, evs, ok, nstack in CASES:
            pg = await b.new_page(viewport={'width': 1600, 'height': 900})
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
            await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
            for e in [SESSION] + list(evs): await pg.evaluate('e => __kimo.apply(e)', e)
            await pg.wait_for_timeout(900)
            rows = await pg.evaluate(ROWS); st = await pg.evaluate(STACK); good = bool(ok(rows)) and all(st) and len(st) == nstack; bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {rows!r} stack={st}")
            if len(sys.argv) > 1 and name == 'quality, 0-36 s': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"frame_quality_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
