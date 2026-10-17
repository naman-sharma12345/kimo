#!/usr/bin/env python3
"""flash_series info panel with the v6 flash rule numbers (optional flash_limit): a fourth row, Worst 1 s flashes, drawn only
from top-level scalars (worst_window_flashes / _start, windows_over_limit, flash_area_pct_max and the red_ twins), never from
the windows or frames arrays (they arrive cut to 8 entries). Without flash_limit the card keeps its three rows.

ev_engine_flash_rule.json is a real kimo-mcp spectator stream recorded on a 640x360 24 fps timeline: jaja-ad.mp4 for 0..6 s,
then a 3 s strobe clip made from the jaja 12 s frame (a 320x180 box toggling every 3 frames, white/black then red/black),
flash_series over 4..9 s three times: no flash_limit, flash_limit 3, flash_limit 3 with area_min_pct 30.
  - no flash_limit: Range / Transitions / Stamps only
  - flash_limit 3: "N @ Ts, W > 3, area A% · red N @ Ts, W > 3, area A%" from the recorded scalars
  - area_min_pct 30: the strobe box (25 % of the frame) is left out on both sides; one luminance flash stays, the cut from
    the jaja shot into the strobe clip (98 % of the frame), so "1 @ 4s, 0 > 3, area 98% · red 0, 0 > 3"
  - junk scalars ("NaN" count): that side reads "–", never 0
  - no value cell is clipped by the card, no "…+" marker
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/flash_rule_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_flash_rule.json').read_text())
SESSION, PLAIN, RULE, AREA = EV[0], EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def side(c, s, o, a, lim):
    if not isinstance(c, (int, float)) or isinstance(c, bool): return '–'
    out = f"{c:g}" + (f" @ {r2(s)}s" if isinstance(s, (int, float)) else '') + (f", {o:g} > {r2(lim)}" if isinstance(o, (int, float)) else '')
    return out + (f", area {round(a):g}%" if isinstance(a, (int, float)) and c > 0 else '')
def want(R):
    lim = R['flash_limit']
    return side(R['worst_window_flashes'], R['worst_window_start'], R['windows_over_limit'], R['flash_area_pct_max'], lim) + ' · red ' + side(R['worst_red_window_flashes'], R['worst_red_window_start'], R['red_windows_over_limit'], R['red_flash_area_pct_max'], lim)
RR, RA = RULE[1]['payload']['result'], AREA[1]['payload']['result']
def junk():
    pair = copy.deepcopy(RULE); pair[1]['payload']['result'].update({'worst_red_window_flashes': 'NaN'}); return pair
JR = junk()[1]['payload']['result']
CASES = [
    ('no flash_limit', PLAIN, lambda r: clean(r) and [k for k, _ in r] == ['Range', 'Transitions', 'Stamps']),
    ('flash_limit 3', RULE, lambda r: clean(r) and [k for k, _ in r] == ['Range', 'Transitions', 'Stamps', 'Worst 1 s flashes'] and dict(r).get('Worst 1 s flashes', '') == want(RR) and RR['worst_window_flashes'] > 3),
    ('area_min_pct 30', AREA, lambda r: clean(r) and dict(r).get('Worst 1 s flashes', '') == want(RA) and RA['flashes_counted'] == 1 and RA['red_flashes_counted'] == 0 and want(RA).endswith('· red 0, 0 > 3')),
    ('junk scalars', junk(), lambda r: clean(r) and dict(r).get('Worst 1 s flashes', '').endswith('· red –') and dict(r).get('Worst 1 s flashes', '') == want(JR)),
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
    print(f"flash_rule_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
