#!/usr/bin/env python3
"""audio_spectrum info panel rows: drawn only from the result's top-level numbers, never from the capped arrays.

ev_engine_spectrum_capped.json is a real kimo-mcp spectator stream (session, audio_spectrum start, complete) recorded on
jaja-ad.mp4 (final timeline mix, 0-36 s, octave bands): its result.bands array is cut by summarize() to eight entries
plus a "…+2" marker, and the spectrum arrays likewise. The rows read peak_hz / peak_dbfs / total_dbfs / loudest_band.
  - the real event: Range "mix 0s–36s", Peak "70 Hz, −19.9 dBFS", Loudest band "45–89 Hz, −20.5 dBFS", Total "−17.2 dBFS"
  - silence (every level null): Peak "silent", Loudest band "none", Total "silent"; no "0 Hz" and no "0.0 dBFS"
  - junk scalars (a "…+3" string, "NaN", Infinity-as-string): read as missing, never as 0
  - no "…+" marker reaches any row, and no value cell is clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/spectrum_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_spectrum_capped.json').read_text())
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def variant(res):
    ev = copy.deepcopy(EV); ev[2]['payload']['result'].update(res); return ev
SILENT = {'peak_hz': None, 'peak_dbfs': None, 'total_dbfs': None, 'loudest_band': None}
JUNK = {'peak_hz': '…+3', 'peak_dbfs': 'NaN', 'total_dbfs': 'Infinity', 'loudest_band': {'lo_hz': '…+1', 'hi_hz': 89.125, 'dbfs': None}}
def clean(rows): return rows and not any('…+' in v or v.startswith('CLIPPED') for _, v in rows)
CASES = [
    ('real capped event', variant({}), lambda r: clean(r) and r == [['Range', 'mix 0s–36s'], ['Peak', '70 Hz, −19.9 dBFS'], ['Loudest band', '45–89 Hz, −20.5 dBFS'], ['Total', '−17.2 dBFS']]),
    ('silence, every level null', variant(SILENT), lambda r: clean(r) and dict(r).get('Peak') == 'silent' and dict(r).get('Loudest band') == 'none' and dict(r).get('Total') == 'silent' and not any(' 0 Hz' in ' ' + v or '0.0 dBFS' in v for _, v in r)),
    ('junk scalars', variant(JUNK), lambda r: clean(r) and dict(r).get('Peak') == 'silent' and dict(r).get('Loudest band') == 'none' and dict(r).get('Total') == 'silent'),
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
            await pg.wait_for_timeout(900)
            rows = await pg.evaluate(ROWS); good = bool(ok(rows)); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {rows!r}")
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"spectrum_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
