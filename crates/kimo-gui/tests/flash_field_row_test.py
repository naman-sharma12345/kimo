#!/usr/bin/env python3
"""flash_series info panel with the v6 flash pin (area_basis field): the Worst 1 s flashes row ends with the worst 10 degree
field, " · field x,y w×h", drawn only from the top-level worst_field_rect when area_basis is "field" (the card keeps its 4 rows).
Any other call keeps its rows unchanged.

ev_engine_flash_field.json is a real kimo-mcp spectator stream recorded on a 640x360 24 fps timeline: jaja-ad.mp4 for 0..6 s,
then the 3 s strobe clip made from the jaja 12 s frame (a 320x180 box toggling every 3 frames, white/black then red/black),
flash_series over 4..9 s twice: flash_limit 3 with area_basis field, and flash_limit 3 with area_basis frame.
  - field: rows Range / Transitions / Stamps / Worst 1 s flashes, the last ending " · field x,y w×h" equal to the recorded
    worst_field_rect
  - frame: no field on the row
  - junk worst_field_rect (a string inside): no field on the row, never "NaN"
  - no value cell is clipped by the card, no "…+" marker
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/flash_field_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_flash_field.json').read_text())
SESSION, FIELD, FRAME = EV[0], EV[1:3], EV[3:5]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
RF = FIELD[1]['payload']['result']; FR = RF['worst_field_rect']
WANT = f" · field {FR[0]},{FR[1]} {FR[2]}×{FR[3]}"
def junk():
    pair = copy.deepcopy(FIELD); pair[1]['payload']['result']['worst_field_rect'] = [1, 'x', 3, 4]; return pair
CASES = [
    ('field', FIELD, lambda r: clean(r) and [k for k, _ in r] == ['Range', 'Transitions', 'Stamps', 'Worst 1 s flashes'] and dict(r)['Worst 1 s flashes'].endswith(WANT) and dict(r)['Worst 1 s flashes'].startswith(f"{RF['worst_window_flashes']} @ ")),
    ('frame basis: no field on the row', FRAME, lambda r: clean(r) and 'Worst 1 s flashes' in dict(r) and 'field' not in dict(r)['Worst 1 s flashes']),
    ('junk rect dropped', junk(), lambda r: clean(r) and 'Worst 1 s flashes' in dict(r) and 'field' not in dict(r)['Worst 1 s flashes']),
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
            if len(sys.argv) > 1 and name == 'field': await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"flash_field_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
