#!/usr/bin/env python3
"""saliency_map read card. From the result: "Peak" is where the map is highest (canvas px), "Half the mass in" the share of the
frame area that holds half the saliency mass, "In rect 1" the first rect's share of the mass and of the area (label on its
own line, value under it), "Motion" the motion weight when one was given. A flat frame reads "Saliency: flat frame, none".

ev_engine_saliency_map.json is a real kimo-mcp spectator stream on files/jaja/jaja-ad.mp4 (1920x1080 24 fps, decode only):
saliency_map at 8 s without rects, the same with a lower-third rect and a top-centre rect, and 30 s with the lower third and
motion 0.5. Junk: a broken peak drops the Peak row; rects that are not an array drop In rect 1; empty: true reads flat.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/saliency_map_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_saliency_map.json').read_text())
SESSION = EV[0]; PLAIN, RECTS, MOT = EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5)"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def pc(x):
    x = x * 100; return (f"{round(x * 100) / 100:g}" if x < 10 else f"{round(x * 10) / 10:g}") + '%'
def jround(x): import math; return math.floor(x + 0.5)
def want(pair):
    R = res(pair); out = [['Peak', f"{jround(R['peak']['x'])}, {jround(R['peak']['y'])} px"], ['Half the mass in', f"{pc(R['half_mass_area_frac'])} of the frame"]]
    if R.get('rects'): r0 = R['rects'][0]; out.append(['In rect 1', f"{pc(r0['mass_frac'])} of the mass · {pc(r0['area_frac'])} of the area"])
    if R.get('motion'): out.append(['Motion', f"weight {round(R['motion']['weight'] * 100) / 100:g}"])
    return out
def junk(pair, edit):
    p = copy.deepcopy(pair); edit(p[1]['payload']['result']); return p
assert 'rects' in res(RECTS) and len(res(RECTS)['rects']) == 2 and res(MOT)['motion']['weight'] == 0.5 and res(PLAIN)['rects'] == [], 'recorded fixture changed'
CASES = [
    ('no rects, 8 s', PLAIN, lambda r: clean(r) and r == want(PLAIN), 2),
    ('two rects, 8 s', RECTS, lambda r: clean(r) and r == want(RECTS), 3),
    ('lower third + motion 0.5, 30 s', MOT, lambda r: clean(r) and r == want(MOT), 3),
    ('junk peak: Peak dropped', junk(RECTS, lambda R: R.__setitem__('peak', 'x')), lambda r: clean(r) and [k for k, _ in r] == ['Half the mass in', 'In rect 1'], 2),
    ('junk rects: In rect 1 dropped', junk(RECTS, lambda R: R.__setitem__('rects', 'x')), lambda r: clean(r) and [k for k, _ in r] == ['Peak', 'Half the mass in'], 2),
    ('empty: flat', junk(PLAIN, lambda R: R.__setitem__('empty', True)), lambda r: r == [['Saliency', 'flat frame, none']], 0),
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
            if len(sys.argv) > 1 and name == 'two rects, 8 s': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"saliency_map_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
