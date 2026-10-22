#!/usr/bin/env python3
"""flash_series Stamps row: drawn only from the payload's lum_stamps, never from the capped frames arrays.

ev_engine_flash_capped.json is a real kimo-mcp spectator stream (session, flash_series start, complete) recorded on
jaja-ad.mp4 before spectator.rs carried lum_stamps: its result.frames arrays are cut by summarize() to eight entries
plus a "…+856" marker. The GUI used to read that marker as a downward transition at 0 s ("Stamps ↓0s").
  - capped shape, no lum_stamps (an older engine): no ↑/↓ stamp and no "0s" on the Stamps row
  - the same event with the six real stamps in lum_stamps (lum_stamps_total 6): exactly those six, in order
  - lum_stamps carrying a "…+N" string, a NaN/None time and a dir of 0: all skipped, never read as 0
  - an empty lum_stamps list: "none"
  - a capped red_transition [0×8, "…+856"] with empty red_stamps / lum_stamps: "none" (the ui draws no red stamps)
  - the Stamps value is never clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/flash_stamps_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_flash_capped.json').read_text())
REAL = [{'t': 4.75, 'dir': 1}, {'t': 9.666667, 'dir': -1}, {'t': 14.75, 'dir': 1}, {'t': 24.75, 'dir': -1}, {'t': 29.708333, 'dir': 1}, {'t': 30.541667, 'dir': -1}]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROW = "() => { const k = [...document.querySelectorAll('.rk')].find(e => e.textContent.trim() === 'Stamps'); const v = k && k.nextElementSibling; if (v && v.scrollWidth > v.clientWidth + 1) return 'CLIPPED ' + v.textContent; return k ? [...k.parentElement.children].map(c => c.textContent.replace(/\\s+/g, ' ').trim()).join(' ') : null; }"
def red_capped():   # the ui never draws red stamps: a capped red_transition and empty lists must read "none", no red value leaks
    ev = variant({'lum_stamps': [], 'lum_stamps_total': 0, 'red_stamps': [], 'red_stamps_total': 0})
    ev[2]['payload']['result']['frames']['red_transition'] = [0] * 8 + ['…+856']; return ev
def variant(extra):
    ev = copy.deepcopy(EV); ev[2]['payload'].update(extra); return ev
CASES = [
    ('capped, no lum_stamps', variant({}), lambda r: r is not None and '↑' not in r and '↓' not in r and '0s' not in r),
    ('six real stamps', variant({'lum_stamps': REAL, 'lum_stamps_total': 6}), lambda r: r == 'Stamps ↑4.75s ↓9.67s ↑14.75s ↓24.75s ↑29.71s ↓30.54s'),
    ('junk entries skipped', variant({'lum_stamps': ['…+3', {'t': None, 'dir': 1}, {'t': 'NaN', 'dir': -1}, {'t': 2.5, 'dir': 0}, {'t': 4.75, 'dir': 1}], 'lum_stamps_total': 1}), lambda r: r == 'Stamps ↑4.75s'),
    ('empty list', variant({'lum_stamps': [], 'lum_stamps_total': 0}), lambda r: r == 'Stamps none'),
    ('capped red_transition, empty lists', red_capped(), lambda r: r == 'Stamps none'),
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
            for e in evs: await pg.evaluate('e => __kimo.apply(e)', e)
            await pg.wait_for_timeout(300)
            row = await pg.evaluate(ROW); good = bool(ok(row)); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {row!r}")
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"flash_stamps_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
