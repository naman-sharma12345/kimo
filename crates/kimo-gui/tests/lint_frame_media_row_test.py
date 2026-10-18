#!/usr/bin/env python3
"""lint_frame read card with decode_media: true. ev_engine_lint_frame_media.json is a real kimo-mcp spectator stream
(files/jaja/jaja-ad.mp4 at 1920x1080 24 fps, decode only, white transcript captions on top): lint_frame text_contrast at
the middle of three cues (5.7583, 11.1583, 14.4 s) with the default thresholds, first without decode_media (the v5
background, media drawn black: no finding, the card says clean) and then with decode_media: true (the media decoded:
three text_contrast findings, the card says "text contrast ×3", min 1.02 / 1.00 / 1.27 : 1). The card is the existing lint_frame card (Frame, Findings);
this checks that it shows the findings the engine returns, with no clipped or junk text. Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/lint_frame_media_row_test.py [screenshot.png]
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_lint_frame_media.json').read_text())
SESSION = EV[0]; V5, DEC = EV[1:3], EV[3:5]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
assert res(V5)['summary']['counts_by_code'] == {} and res(DEC)['summary']['counts_by_code'] == {'text_contrast': 3}, 'recorded fixture changed'
assert 'decode_media' not in V5[0]['payload']['args'] and DEC[0]['payload']['args'].get('decode_media') is True, 'recorded fixture changed'
CASES = [
    ('default: v5 background, clean', V5, lambda r: clean(r) and [k for k, _ in r] == ['Frame', 'Findings'] and dict(r)['Findings'] == 'clean'),
    ('decode_media: 3 text_contrast findings', DEC, lambda r: clean(r) and [k for k, _ in r] == ['Frame', 'Findings'] and dict(r)['Findings'] == 'text contrast ×3'),
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
            if len(sys.argv) > 1 and name.startswith('decode_media'): await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"lint_frame_media_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
