#!/usr/bin/env python3
"""get_scopes info panel rows with the v6 numeric arguments: drawn only from top-level scalars and the first / last entries
of the top-level percentile arrays, never from the capped histograms, waveforms or zone lists.

ev_engine_scopes.json is a real kimo-mcp spectator stream recorded on jaja-ad.mp4 (640x360 timeline): get_scopes at 12 s with
v5 arguments, with percentiles [1, 50, 99] + zones [3, 3], with percentiles [5, 95] only, and at 20 s with zones [4, 2] +
crush_below 0.1 + clip_above 0.9.
  - v5 arguments: exactly the four v5 rows (Luma mean / Avg RGB / Clipped whites / Crushed blacks)
  - percentiles + zones: Luma mean / Luma p1–p99 / Chroma mean / Zone max clip/crush, equal to the recorded numbers
  - percentiles only: Luma mean / Luma p5–p95 / Chroma mean / Clipped whites
  - zones only: Luma mean / Avg RGB / Clipped whites / Zone max clip/crush
  - 12 percentiles capped by the spectator to 8 entries plus "…+4": the spread pairs p[0] with p[7] (p60), the last entry both arrays hold
  - junk scalars ("NaN", "…+3") are dropped, never shown as 0
  - no "…+" marker reaches any row, and no value cell is clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/scopes_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_scopes.json').read_text())
SESSION, V5, PZ, PO, ZO = EV[0], EV[1:3], EV[3:5], EV[5:7], EV[7:9]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def pct(x): return (r2(x) if x < 10 else f"{round(x * 10) / 10:g}") + '%'
R_PZ, R_PO, R_ZO = PZ[1]['payload']['result'], PO[1]['payload']['result'], ZO[1]['payload']['result']
def spread(R, k=None): lp = R['luma_percentiles']; k = len(lp) - 1 if k is None else k; return f"{r2(lp[0])} – {r2(lp[k])}"
def chroma(R): return f"{round(R['chroma_mean'] * 1000) / 1000:g}"
def zone(R): return f"{pct(R['zone_clipped_whites_max_pct'])} / {pct(R['zone_crushed_blacks_max_pct'])}"
def capped():
    pair = copy.deepcopy(PZ); res = pair[1]['payload']['result']; P = [1, 2, 5, 10, 25, 40, 50, 60, 75, 90, 95, 99]
    res['percentiles'] = P[:8] + ['…+4']; res['luma_percentiles'] = [0.1 + 0.05 * i for i in range(8)] + ['…+4']; pair[0]['payload']['args']['percentiles'] = P; return pair
def junk():
    pair = copy.deepcopy(PZ); pair[1]['payload']['result'].update({'chroma_mean': 'NaN', 'zone_clipped_whites_max_pct': '…+3'}); return pair
CASES = [
    ('v5 arguments (v5 rows)', V5, lambda r: clean(r) and [k for k, _ in r] == ['Luma mean', 'Avg RGB', 'Clipped whites', 'Crushed blacks']),
    ('percentiles + zones', PZ, lambda r: clean(r) and [k for k, _ in r] == ['Luma mean', 'Luma p1–p99', 'Chroma mean', 'Zone max clip/crush'] and dict(r)['Luma p1–p99'] == spread(R_PZ) and dict(r)['Chroma mean'] == chroma(R_PZ) and dict(r)['Zone max clip/crush'] == zone(R_PZ)),
    ('percentiles only', PO, lambda r: clean(r) and [k for k, _ in r] == ['Luma mean', 'Luma p5–p95', 'Chroma mean', 'Clipped whites'] and dict(r)['Luma p5–p95'] == spread(R_PO)),
    ('zones only, thresholds as args', ZO, lambda r: clean(r) and [k for k, _ in r] == ['Luma mean', 'Avg RGB', 'Clipped whites', 'Zone max clip/crush'] and dict(r)['Zone max clip/crush'] == zone(R_ZO)),
    ('capped percentile arrays', capped(), lambda r: clean(r) and dict(r).get('Luma p1–p60') == '0.1 – 0.45'),
    ('junk scalars dropped', junk(), lambda r: clean(r) and 'Chroma mean' not in dict(r) and not any(v.startswith('0 ') or v == '0' for _, v in r)),
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
    print(f"scopes_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
