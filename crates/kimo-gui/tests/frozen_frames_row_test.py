#!/usr/bin/env python3
"""frozen_frames info panel: 4 rows from the top-level numbers and the runs list: Held "n runs · h held frames" ("none" at zero),
Longest run "L frames · start–end · max diff d/255" (dropped when nothing held; label on its own line, value under it, checked:
value top >= label bottom), Tolerance "t/255 · min run m", Frames read "n · start–end s".

ev_engine_frozen_frames.json is a real kimo-mcp spectator stream recorded on files/jaja/jaja-ad.mp4 (1920x1080 24 fps,
decode only): a 6-10 s window as it is, the same window after freeze_frame hold_from_here at 8 s, and that window again at
tolerance 0.05 with min_run 6. Junk cases are the held recording with scalars broken: a NaN run count drops the Held row, a
broken runs list drops the Longest run row, a missing range drops Frames read, missing params drop Tolerance. Never 0 for a
missing number. Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/frozen_frames_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_frozen_frames.json').read_text())
SESSION = EV[0]; PAIRS = [EV[1:3], EV[3:5], EV[5:7]]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => [e.querySelector('.rk').textContent.trim(), e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5])"
HEAD = "() => { const h = document.querySelector('.readcard .rh'); return h ? h.lastChild.textContent.trim() : null; }"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def r3(x): return str(int(x)) if float(x).is_integer() else str(round(x, 3))
def plural(n, one, many): return one if n == 1 else many
def held(R): return 'none' if R['run_count'] == 0 else f"{R['run_count']} {plural(R['run_count'], 'run', 'runs')} · {R['held_frames']} held {plural(R['held_frames'], 'frame', 'frames')}"
def longest(R):
    L = next(r for r in R['runs'] if r['length'] == R['longest_run']); return f"{R['longest_run']} frames · {L['start_frame']}–{L['end_frame']} · max diff {L['max_diff_255']}/255"
def tol(R): return f"{round(R['params']['tolerance'] * 255)}/255 · min run {R['params']['min_run']}"
def read(R): return f"{R['frames']} · {r3(R['start'])}–{r3(R['end'])} s"
def want(R):
    d = {'Held': held(R)}
    if R['run_count']: d['Longest run'] = longest(R)
    d['Tolerance'] = tol(R); d['Frames read'] = read(R); return d
def full(pair, check=None):
    R = res(pair)
    return lambda r, h: clean(r) and h == 'frozen_frames' and [k for k, _ in r] == list(want(R)) and dict(r) == want(R) and (check is None or check(R))
def junk(edit):
    pair = copy.deepcopy(PAIRS[1]); edit(pair[1]['payload']['result']); return pair
CASES = [
    ('jaja 6-10 s as it is', PAIRS[0], full(PAIRS[0])),
    ('freeze_frame hold from 8 s', PAIRS[1], full(PAIRS[1], lambda R: R['longest_run'] == 48 and any(r['start_frame'] == 192 and r['end_frame'] == 239 and r['max_diff_255'] == 0 for r in R['runs']))),
    ('hold at tolerance 0.05, min run 6', PAIRS[2], full(PAIRS[2], lambda R: R['params']['tolerance'] == 0.05 and R['params']['min_run'] == 6 and R['longest_run'] >= 48)),
    ('junk run count: row dropped', junk(lambda R: R.__setitem__('run_count', 'NaN')), lambda r, h: clean(r) and 'Held' not in dict(r) and 'Frames read' in dict(r)),
    ('junk runs: row dropped', junk(lambda R: R.__setitem__('runs', [{'length': 'x'}])), lambda r, h: clean(r) and 'Longest run' not in dict(r) and 'Held' in dict(r)),
    ('junk range: row dropped', junk(lambda R: R.__setitem__('start', None)), lambda r, h: clean(r) and 'Frames read' not in dict(r) and 'Held' in dict(r)),
    ('junk params: row dropped', junk(lambda R: R.__delitem__('params')), lambda r, h: clean(r) and 'Tolerance' not in dict(r) and 'Held' in dict(r)),
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
            rows = await pg.evaluate(ROWS); h = await pg.evaluate(HEAD); st = await pg.evaluate(STACK)
            good = bool(ok(rows, h)) and all(x[1] for x in st) and sorted(x[0] for x in st) == sorted(k for k, _ in rows if k == 'Longest run'); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {h!r} {rows!r}")
            if len(sys.argv) > 1 and name.startswith('freeze_frame'): await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"frozen_frames_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
