#!/usr/bin/env python3
"""render_flow read card with stats. From flow_stats (canvas px per frame): "Motion" is the mean vector's length and direction
("21.8 px/f · 180°", 0° = right, 90° = down), "Speed" the p50 / p90 / max speeds (label on its own line, value under it),
"Moving" the share of the field faster than moving_px. Without stats there is no card (the v5 screen).

ev_engine_flow_stats.json is a real kimo-mcp spectator stream on files/kino-car-edit.mp4 (1080x1920 30 fps, decode only):
render_flow at 20 s without stats, the same with stats (a leftward pan: mean (-21.8, 0.03) px/f), and 8 s with
{grid 2, moving_px 2}. The flow-picture frame_rendered events are left out of the fixture (750 KB each; the card does not
read them). render_flow routes as an export call: the card goes on the info panel of the screen it lands on (the Edit Inspector
here), and without flow_stats (v5) there is no read card at all, only the render row. Junk: a broken speed_max drops the Speed row; flow_stats that is not an object
gives the v5 screen (no card).
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/flow_stats_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_flow_stats.json').read_text())
SESSION = EV[0]; V5, PAN, G2 = EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5)"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def r1(x): s = f"{round(x * 10) / 10:g}"; return s
def want(pair):
    F = res(pair)['flow_stats']; import math
    pm = F['moving_frac'] * 100; pc = (f"{round(pm * 100) / 100:g}" if pm < 10 else f"{round(pm * 10) / 10:g}") + '%'
    return [['Motion', f"{r1(math.hypot(F['mean_u'], F['mean_v']))} px/f · {round(F['mean_dir_deg']) if F['mean_dir_deg'] >= 0 else -round(-F['mean_dir_deg'])}°"],
            ['Speed', f"p50 {r1(F['speed_p50'])} · p90 {r1(F['speed_p90'])} · max {r1(F['speed_max'])} px/f"], ['Moving', pc]]
def junk(edit):
    pair = copy.deepcopy(PAN); edit(pair[1]['payload']['result']); return pair
assert 'flow_stats' not in res(V5) and 'flow_stats' in res(PAN) and res(G2)['flow_stats']['params'] == {'grid': 2, 'moving_px': 2.0}, 'recorded fixture changed'
CASES = [
    ('no stats: the v5 render row, no read card', V5, lambda r: r == [], 0),
    ('stats, pan at 20 s', PAN, lambda r: clean(r) and r == want(PAN), 1),
    ('stats grid 2 moving_px 2 at 8 s', G2, lambda r: clean(r) and r == want(G2), 1),
    ('junk speed_max: Speed dropped', junk(lambda R: R['flow_stats'].__setitem__('speed_max', 'x')), lambda r: clean(r) and [k for k, _ in r] == ['Motion', 'Moving'], 0),
    ('flow_stats not an object: no read card (v5)', junk(lambda R: R.__setitem__('flow_stats', 'x')), lambda r: r == [], 0),
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
            if len(sys.argv) > 1 and name == 'stats, pan at 20 s': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"flow_stats_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
