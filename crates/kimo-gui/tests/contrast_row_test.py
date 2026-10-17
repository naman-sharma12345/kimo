#!/usr/bin/env python3
"""contrast_ratio info panel rows: drawn only from top-level scalars (ratio, luminance_a / _b, ratio_protan / _deutan /
_tritan, severity), never from the nested a / b / cvd objects.

ev_engine_contrast.json is a real kimo-mcp spectator stream recorded on jaja-ad.mp4 (640x360 timeline, the 12 s title
card): #777 on white, a 2x2 rect inside a title letter against a 13x13 rect of the background with all three CVD types,
and two sampled colours with deutan at severity 0.5.
  - colours: Ratio "4.48:1" and Luminance a / b equal to the recorded numbers; no CVD rows
  - rects + cvd: Ratio / Luminance a / b / Under CVD "P x:1 · D y:1 · T z:1" / Severity "1", equal to the recorded numbers
  - one cvd at severity 0.5: Under CVD "D x:1" and Severity "0.5"
  - junk scalars ("NaN", "…+1") are dropped, never shown as 0
  - no "…+" marker reaches any row, and no value cell is clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/contrast_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_contrast.json').read_text())
SESSION, COL, RECT, SEV = EV[0], EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def r3(x): return f"{round(x * 1000) / 1000:g}"
def lum(R): return f"{r3(R['luminance_a'])} / {r3(R['luminance_b'])}"
RC, RR, RS = COL[1]['payload']['result'], RECT[1]['payload']['result'], SEV[1]['payload']['result']
def junk():
    pair = copy.deepcopy(RECT); pair[1]['payload']['result'].update({'ratio': 'NaN', 'ratio_deutan': '…+1'}); return pair
CASES = [
    ('colours', COL, lambda r: clean(r) and [k for k, _ in r] == ['Ratio', 'Luminance a / b'] and dict(r)['Ratio'] == '4.48:1' and dict(r)['Luminance a / b'] == lum(RC)),
    ('rects + cvd', RECT, lambda r: clean(r) and [k for k, _ in r] == ['Ratio', 'Luminance a / b', 'Under CVD', 'Severity'] and dict(r)['Ratio'] == f"{r2(RR['ratio'])}:1" and dict(r)['Luminance a / b'] == lum(RR)
        and dict(r)['Under CVD'] == f"P {r2(RR['ratio_protan'])}:1 · D {r2(RR['ratio_deutan'])}:1 · T {r2(RR['ratio_tritan'])}:1" and dict(r)['Severity'] == '1'),
    ('one cvd at severity 0.5', SEV, lambda r: clean(r) and dict(r).get('Under CVD') == f"D {r2(RS['ratio_deutan'])}:1" and dict(r).get('Severity') == '0.5'),
    ('junk scalars dropped', junk(), lambda r: clean(r) and 'Ratio' not in dict(r) and 'D ' not in dict(r).get('Under CVD', '') and not any(v == '0:1' for _, v in r)),
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
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"contrast_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
