#!/usr/bin/env python3
"""validate_captions Inspector rows with legibility: true: Captions, then from the top-level legibility summary Min caps
("47 px · 4.35%": min_cap_px and min_cap_frac of the frame height), Lowest contrast ("1.00:1") and Max reading ("15.6 cps"),
none clipped; without legibility the rows are the v5 rows one for one (Captions, Profile, Groups, Skipped checks).

ev_engine_caption_legibility.json is a real kimo-mcp spectator stream: white captions from transcript_to_captions over
files/jaja/jaja-ad.mp4 (1920x1080 24 fps, decode only), validate_captions with legibility (web), without it, and with it
under social_9x16. Junk: a broken min_contrast drops that row; a legibility block that is not an object gives the v5 card.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/caption_legibility_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_caption_legibility.json').read_text())
SESSION = EV[0]; LEG, V5, SOC = EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.kv .row')].filter(r => !r.closest('[data-leaving]')).map(r => [...r.children].map(e => (e.scrollWidth > e.clientWidth + 1 ? 'CLIPPED ' : '') + e.textContent.replace(/\\s+/g, ' ').trim()))"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5)"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def pct(x): return (f"{round(x * 100) / 100:g}" if x < 10 else f"{round(x * 10) / 10:g}") + '%'
def caps(R): L = R['legibility']; return f"{round(L['min_cap_px'] * 10) / 10:g} px · {pct(L['min_cap_frac'] * 100)}"
def legrows(pair):
    R = res(pair); L = R['legibility']
    return lambda r: clean(r) and r == [['Captions', 'clean' if R['clean'] else 'issues'], ['Min caps', caps(R)], ['Lowest contrast', f"{L['min_contrast']:.2f}:1"], ['Max reading', f"{L['max_cps']:.1f} cps"]]
def v5rows(pair):
    R = res(pair); return lambda r: clean(r) and r == [['Captions', 'clean' if R['clean'] else 'issues'], ['Profile', R['profile']], ['Groups', str(len(R['groups']))], ['Skipped checks', str(len(R['skipped']))]]
def junk(edit):
    pair = copy.deepcopy(LEG); edit(pair[1]['payload']['result']); return pair
assert 'legibility' in res(LEG) and 'legibility' not in res(V5), 'recorded fixture changed'
CASES = [
    ('legibility, web', LEG, legrows(LEG), 0),
    ('no legibility: the v5 card', V5, v5rows(V5), 0),
    ('legibility, social_9x16', SOC, legrows(SOC), 0),
    ('junk min_contrast: row dropped', junk(lambda R: R['legibility'].__setitem__('min_contrast', 'x')), lambda r: clean(r) and 'Lowest contrast' not in dict(r) and 'Min caps' in dict(r), 0),
    ('legibility not an object: v5 rows', junk(lambda R: R.__setitem__('legibility', 'x')), lambda r: clean(r) and [k for k, _ in r] == ['Captions', 'Profile', 'Groups', 'Skipped checks'], 0),
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
            if len(sys.argv) > 1 and name == 'legibility, web': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"caption_legibility_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
