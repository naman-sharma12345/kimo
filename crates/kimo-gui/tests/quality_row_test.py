#!/usr/bin/env python3
"""diff_frames quality row and compare_ranges info panel rows: drawn only from scalars, never from the capped curves.

ev_engine_quality.json is a real kimo-mcp spectator stream recorded on jaja-ad.mp4 (640x360 timeline, B = exposure +0.25
stops + contrast 1.15, A = snapshot:checkpoint:orig): compare_ranges over 10 s every 1 s (its t_s / psnr_db / ssim /
ms_ssim curves arrive cut by summarize() to eight entries plus a "…+2" marker), diff_frames at 5 s with quality, and an
identical compare_ranges (A = B, 3 samples, every PSNR null).
  - compare_ranges rows: Range / PSNR / SSIM / MS-SSIM read from summary.{metric}.{mean,min,min_at_s,p5}, equal to the
    numbers in the recorded result (formatted 2 dp for dB, 4 dp for SSIM / MS-SSIM)
  - identical compare_ranges: PSNR reads "identical", SSIM / MS-SSIM mean 1.0000
  - diff_frames: Compared / Changed rows unchanged, plus a Quality row "PSNR x dB · SSIM y · MS-SSIM z" from result.quality
  - a diff_frames call WITHOUT quality keeps exactly the two v5 rows (no Quality row); the Compared row now wraps instead of
    being cut by an ellipsis ("orig 5s vs now 5s" was clipped in the 163 px value column before)
  - junk summary scalars ("…+3", "NaN") read as "none", never 0
  - no "…+" marker reaches any row, and no value cell is clipped by the card (a clipped cell reads as CLIPPED and fails)
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/quality_row_test.py
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_quality.json').read_text())
SESSION, CR, DF, CRI = EV[0], EV[1:3], EV[3:5], EV[5:7]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') for _, v in rows)
S = CR[1]['payload']['result']['summary']
def want_row(k, unit, d):
    m = S[k]; return f"mean {m['mean']:.{d}f}{unit}, min {m['min']:.{d}f}{unit} at {round(m['min_at_s'] * 100) / 100:g}s, p5 {m['p5']:.{d}f}{unit}"
QD = DF[1]['payload']['result']['quality']
WANT_Q = f"PSNR {QD['psnr_db']:.2f} dB · SSIM {QD['ssim']:.4f} · MS-SSIM {QD['ms_ssim']:.4f}"
def no_quality():
    pair = copy.deepcopy(DF); pair[0]['payload']['args'].pop('quality', None); pair[1]['payload']['result'].pop('quality', None); return pair
def junk():
    pair = copy.deepcopy(CR); pair[1]['payload']['result']['summary']['ssim'].update({'mean': '…+3', 'min': 'NaN', 'p5': None}); return pair
CASES = [
    ('compare_ranges real capped event', CR, lambda r: clean(r) and [k for k, _ in r] == ['Range', 'PSNR', 'SSIM', 'MS-SSIM'] and dict(r)['PSNR'] == want_row('psnr_db', ' dB', 2) and dict(r)['SSIM'] == want_row('ssim', '', 4) and dict(r)['MS-SSIM'] == want_row('ms_ssim', '', 4) and '10s' in dict(r)['Range'] and '10 samples' in dict(r)['Range']),
    ('compare_ranges identical', CRI, lambda r: clean(r) and dict(r).get('PSNR') == 'identical' and dict(r).get('SSIM', '').startswith('mean 1.0000') and dict(r).get('MS-SSIM', '').startswith('mean 1.0000')),
    ('diff_frames with quality', DF, lambda r: clean(r) and [k for k, _ in r] == ['Compared', 'Changed', 'Quality'] and dict(r)['Quality'] == WANT_Q),
    ('diff_frames without quality (v5 rows)', no_quality(), lambda r: clean(r) and [k for k, _ in r] == ['Compared', 'Changed']),
    ('junk summary scalars', junk(), lambda r: clean(r) and dict(r).get('SSIM') == 'none' and not any(' 0.0000' in v for _, v in r)),
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
    print(f"quality_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
