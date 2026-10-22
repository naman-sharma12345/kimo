#!/usr/bin/env python3
"""query_safe_margins info panel rows with the v6 `rects` argument: Profile / Max outside safe / Max in avoid / Worst edge
(the card's 4 rows), drawn only from top-level scalars (rects_outside_safe_max_pct, rects_in_avoid_max_pct, worst_edge,
worst_edge_px, worst_edge_rect), never from the nested per-zone numbers. Without rects: Profile / Zones / Estimated / Source,
the facts the generic rows showed, with Source wrapped (the generic row clipped 'documented-constant').

ev_engine_safe.json is a real kimo-mcp spectator stream recorded on a 640x360 jaja-ad.mp4 timeline: query_safe_margins with
v5 arguments, title_safe with the title card's text box and a rect past the left edge, the tiktok estimate on 1080x1920 with
a centred title and a rect on the action rail, a custom avoid-only profile, and title_safe with one rect fully inside.
  - v5 arguments: Profile, Zones '1 · title_safe', Estimated 'no', Source 'documented-constant' unclipped, no overlap rows
  - title_safe + rects: Max outside safe / Max in avoid equal to the recorded scalars ("no avoid zone" when null), Worst edge
    "<edge> <px> px · <rect>" from worst_edge / worst_edge_px / worst_edge_rect
  - tiktok: both scalars from the recorded result
  - custom avoid-only: Max outside safe and Worst edge read "no safe zone"
  - title_safe, rect inside: Worst edge reads "none crossed"
  - junk scalars ("NaN") fall back to the null wording, never 0
  - no "…+" marker reaches any row, and no value cell is clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/safe_area_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_safe.json').read_text())
SESSION, V5, TS, TT, CU, IN = EV[0], EV[1:3], EV[3:5], EV[5:7], EV[7:9], EV[9:11]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def pct(x): return (r2(x) if x < 10 else f"{round(x * 10) / 10:g}") + '%'
RTS, RTT, RCU, RIN = (x[1]['payload']['result'] for x in (TS, TT, CU, IN))
def rows_for(R, safe_none='no safe zone', avoid_none='no avoid zone'):
    s, a = R.get('rects_outside_safe_max_pct'), R.get('rects_in_avoid_max_pct')
    e, ep, er = R.get('worst_edge'), R.get('worst_edge_px'), R.get('worst_edge_rect')
    we = (f"{e} {r2(ep)} px" + (f" · {er}" if isinstance(er, str) else '')) if isinstance(e, str) and isinstance(ep, (int, float)) and not isinstance(ep, bool) else ('none crossed' if ep == 0 else safe_none)
    return {'Profile': R['profile'], 'Max outside safe': pct(s) if isinstance(s, (int, float)) else safe_none, 'Max in avoid': pct(a) if isinstance(a, (int, float)) else avoid_none, 'Worst edge': we}
def match(r, R): d = dict(r); return all(d.get(k) == v for k, v in rows_for(R).items())
def junk():
    pair = copy.deepcopy(TT); pair[1]['payload']['result'].update({'rects_in_avoid_max_pct': 'NaN', 'worst_edge_px': 'NaN', 'worst_edge_rect': 7}); return pair
CASES = [
    ('v5 arguments', V5, lambda r: clean(r) and r == [['Profile', 'title_safe'], ['Zones', '1 · title_safe'], ['Estimated', 'no'], ['Source', 'documented-constant']]),
    ('title_safe + rects', TS, lambda r: clean(r) and [k for k, _ in r] == ['Profile', 'Max outside safe', 'Max in avoid', 'Worst edge'] and match(r, RTS) and dict(r)['Max in avoid'] == 'no avoid zone' and dict(r)['Worst edge'].startswith(RTS['worst_edge'] + ' ')),
    ('tiktok estimate', TT, lambda r: clean(r) and match(r, RTT) and dict(r)['Profile'] == 'tiktok'),
    ('custom avoid-only', CU, lambda r: clean(r) and dict(r).get('Max outside safe') == 'no safe zone' and dict(r).get('Worst edge') == 'no safe zone' and match(r, RCU)),
    ('title_safe, rect inside', IN, lambda r: clean(r) and dict(r).get('Worst edge') == 'none crossed' and match(r, RIN)),
    ('junk scalars', junk(), lambda r: clean(r) and dict(r).get('Max in avoid') == 'no avoid zone' and dict(r).get('Worst edge') == 'no safe zone'),
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
    print(f"safe_area_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
