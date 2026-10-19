#!/usr/bin/env python3
"""Loudness panel label (v6 eyes (d)): the line under the big LUFS number names what the number is.

measure_loudness lands a MEASURED integrated loudness (EBU R128) in the big number. Before this fix the line under it read a
bare "target" with no value, so a measurement looked like "-15.8 LUFS target". Now:
  - measure_loudness on a live spectator stream (feed=off, no target anywhere): the line reads "Integrated", the panel text
    carries no "target", the big number is the measured value to 1 decimal
  - normalize_loudness with target_lufs -16: a real target exists, so the line keeps "target  -16.0"
  - the m04 mixing mock (its own seeded target -14): unchanged, "target  -14"
  - at 1920 x 1080 and 640 x 360: "Integrated" is whole (not clipped, inside the panel, left of the target line) and its
    effective font size is >= 9 px
The measure_loudness event is synthetic: the result keys are the engine's (editorial_tools.rs measure_loudness output:
integrated_lufs, sample_peak_dbfs, true_peak_dbfs, method); no recorded stream in tests/ carries a measure_loudness call.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/loudness_label_row_test.py
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
SESSION = {'agent': 'kimo-mcp 6', 'elapsed_s': 0, 'height': 720, 'read_only': True, 'reset': True, 'type': 'session', 'width': 1280}
TG = {'track_id': 'Dialogue', 'clip_id': None, 'param': None}
def call(name, args, result, cid):
    return [{'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': TG, 'payload': {'args': args}},
            {'tool_name': name, 'call_id': cid, 'phase': 'complete', 'target': TG, 'payload': {'dur_ms': 40, 'args': args, 'result': result}}]
MEASURE = call('measure_loudness', {}, {'integrated_lufs': -15.796, 'sample_peak_dbfs': -1.9, 'true_peak_dbfs': -1.62, 'method': 'EBU R128 stereo 48k'}, 'L1')
NORMALIZE = call('normalize_loudness', {'target_lufs': -16}, {'measured_lufs': -19.2, 'gain_db': 3.2, 'target_lufs': -16}, 'L2')
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
READ = """() => { const p = document.querySelector('[data-panel=loud]'); if (!p) return null; const pb = p.getBoundingClientRect();
  const big = p.querySelector('.cream span'); const i = p.querySelector('.lufs-int');
  const line = [...p.children].find(d => d.style.height === '1px' && d.style.background.includes('141, 136, 126'));
  const under = [...p.children].find(d => d.style.top && d.style.height === '20px' && d.style.lineHeight === '20px' && !d.style.width);
  const scale = pb.width / p.offsetWidth, ib = i && i.getBoundingClientRect(), lb = line && line.getBoundingClientRect();
  return { text: p.textContent.replace(/\\u00a0/g, ' '), big: big ? big.textContent : null, under: under ? under.textContent.replace(/\\u00a0/g, ' ').replace(/\\s+/g, ' ').trim() : null,
    int: i ? { whole: i.scrollWidth <= i.clientWidth + 1 || getComputedStyle(i).overflow === 'visible', inside: ib.left >= pb.left && ib.right <= pb.right && ib.top >= pb.top && ib.bottom <= pb.bottom,
               left_of_line: lb ? ib.right <= lb.left : true, eff_px: parseFloat(getComputedStyle(i).fontSize) * scale } : null }; }"""
def ok_measure(r):
    i = r and r['int']
    return bool(r and r['under'] == 'Integrated' and 'target' not in r['text'].lower() and r['big'] == '-15.8' and i and i['whole'] and i['inside'] and i['left_of_line'] and i['eff_px'] >= 9)
CASES = [
    ('measure_loudness 1920', 'feed=off', MEASURE, (1920, 1080), ok_measure),
    ('measure_loudness 640x360', 'feed=off', MEASURE, (640, 360), ok_measure),
    ('normalize_loudness keeps its target', 'feed=off', NORMALIZE, (1920, 1080), lambda r: bool(r and r['under'] == 'target -16.0' and r['int'] is None and r['big'] == '-16.0')),
    ('m04 mock target unchanged', 'feed=sim&scenario=m04&still=1', [], (1920, 1080), lambda r: bool(r and r['under'] == 'target -14' and r['int'] is None and r['big'] == '-14.2')),
]
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    bad = 0
    async with async_playwright() as p:
        b = await p.chromium.launch()
        for name, q, evs, (w, h), ok in CASES:
            pg = await b.new_page(viewport={'width': w, 'height': h}); errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?{q}')
            await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0'); await pg.wait_for_timeout(400)
            for e in ([SESSION] if q == 'feed=off' else []) + evs: await pg.evaluate('e => __kimo.apply(e)', e)
            await pg.wait_for_timeout(900)
            r = await pg.evaluate(READ); good = bool(ok(r)) and not errs; bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: under={r and r['under']!r} big={r and r['big']!r} int={r and r['int']!r}" + (f" errors={errs}" if errs else ''))
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"loudness_label_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
