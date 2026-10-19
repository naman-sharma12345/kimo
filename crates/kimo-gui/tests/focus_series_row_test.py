#!/usr/bin/env python3
"""focus_series info panel: 4 rows (the read card keeps 4). Sharpest and Softest "frame f · t s · lap x · ten y" (label on its own
line, value under it, checked: value top >= label bottom); then ONE row: Per rect "n rects · lap best x · worst y" when rects were
given ("none inside the frame" when no rect has an interior pixel), else Soft "r runs · n frames at lap ≤ x" or "none at lap ≤ x" when
soft_max was given, else Lap var "min–max · median m"; then Frames read "n · start–end s" plus "· every k" when step > 1.
Numbers: 1000 and up as whole numbers, 10 and up with one decimal, under 10 with three. No verdict anywhere on the card.

ev_engine_focus_series.json is a real kimo-mcp spectator stream (KIMO_SPECTATOR=file:, 160x90 24 fps): the pre-registered blur ramp
(8 px checker, blur radius keyed 0 at 0.5 s to 10 at 2.5 s) read every frame at the defaults, the same ramp every 2nd frame with
soft_max at the softest value, and a frame that is half checker, half flat grey read with two rects and soft_max 0.
Junk cases are the ramp recording with fields broken: a NaN sharpest value drops the Sharpest row, a broken lap_var summary drops
the Lap var row, a missing start drops Frames read, a non-list soft_runs drops Soft (Lap var takes its place), rects that are not
objects read "none inside the frame". Never 0 for a missing number. Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/focus_series_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_focus_series.json').read_text())
SESSION = EV[0]; PAIRS = [EV[1:3], EV[3:5], EV[5:7]]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => [e.querySelector('.rk').textContent.trim(), e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5])"
HEAD = "() => { const h = document.querySelector('.readcard .rh'); return h ? h.lastChild.textContent.trim() : null; }"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v or 'null' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def js(x): x = float(x); return str(int(x)) if x.is_integer() else repr(x)
def r3(x): return js(round(x * 1000) / 1000)
def fv(x): return js(round(x)) if abs(x) >= 1000 else js(round(x * 10) / 10) if abs(x) >= 10 else js(round(x * 1000) / 1000)
def plural(n, one, many): return one if n == 1 else many
def at(S): return f"frame {S['frame']} · {r3(S['t'])} s · lap {fv(S['lap_var'])} · ten {fv(S['tenengrad'])}"
def spread(S): return f"{fv(S['min'])}–{fv(S['max'])} · median {fv(S['median'])}"
def want(R):
    d = {'Sharpest': at(R['sharpest']), 'Softest': at(R['softest'])}
    rc = R['rects']; sm = R['params'].get('soft_max')
    if rc:
        V = [x['lap_var'] for x in rc if isinstance(x, dict) and isinstance(x.get('lap_var'), dict)]
        d['Per rect'] = f"{len(rc)} {plural(len(rc), 'rect', 'rects')} · " + (f"lap best {fv(max(s['max'] for s in V))} · worst {fv(min(s['min'] for s in V))}" if V else 'none inside the frame')
    elif sm is not None:
        SR = R['soft_runs']; sn = sum(x['samples'] for x in SR)
        d['Soft'] = f"none at lap ≤ {fv(sm)}" if not SR else f"{len(SR)} {plural(len(SR), 'run', 'runs')} · {sn} {plural(sn, 'frame', 'frames')} at lap ≤ {fv(sm)}"
    else: d['Lap var'] = spread(R['lap_var'])
    st = R['params']['step']; d['Frames read'] = f"{R['frames']} · {r3(R['start'])}–{r3(R['end'])} s" + (f" · every {st}" if st > 1 else ''); return d
def full(pair, check=None):
    R = res(pair)
    return lambda r, h: clean(r) and h == 'focus_series' and [k for k, _ in r] == list(want(R)) and dict(r) == want(R) and (check is None or check(R))
def junk(edit):
    pair = copy.deepcopy(PAIRS[0]); edit(pair[1]['payload']['result']); return pair
def jr(R): R['rects'] = ['x', None]
CASES = [
    ('blur ramp, every frame', PAIRS[0], full(PAIRS[0], lambda R: R['frames'] == 72 and R['sharpest']['frame'] == 0 and R['softest']['frame'] == 60 and R['soft_runs'] is None)),
    ('blur ramp, every 2nd frame, soft line', PAIRS[1], full(PAIRS[1], lambda R: R['frames'] == 36 and R['params']['step'] == 2 and [(x['start_frame'], x['end_frame']) for x in R['soft_runs']] == [(60, 70)])),
    ('split frame, two rects', PAIRS[2], full(PAIRS[2], lambda R: R['rects'][0]['lap_var']['min'] > 0 and R['rects'][1]['lap_var']['max'] == 0 and R['soft_runs'] == [])),
    ('junk sharpest: row dropped', junk(lambda R: R['sharpest'].__setitem__('lap_var', 'NaN')), lambda r, h: clean(r) and 'Sharpest' not in dict(r) and 'Softest' in dict(r)),
    ('junk lap_var summary: row dropped', junk(lambda R: R.__setitem__('lap_var', {'min': 1})), lambda r, h: clean(r) and 'Lap var' not in dict(r) and [k for k, _ in r] == ['Sharpest', 'Softest', 'Frames read']),
    ('junk range: row dropped', junk(lambda R: R.__setitem__('start', None)), lambda r, h: clean(r) and 'Frames read' not in dict(r) and 'Sharpest' in dict(r)),
    ('junk soft runs: row dropped', junk(lambda R: (R['params'].__setitem__('soft_max', 5.0), R.__setitem__('soft_runs', 'x'))), lambda r, h: clean(r) and 'Soft' not in dict(r) and 'Lap var' in dict(r) and len(r) == 4),
    ('junk rects: none inside the frame', junk(jr), lambda r, h: clean(r) and dict(r).get('Per rect') == '2 rects · none inside the frame'),
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
            good = bool(ok(rows, h)) and all(x[1] for x in st) and sorted(x[0] for x in st) == sorted(k for k, _ in rows if k in ('Sharpest', 'Softest')); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {h!r} {rows!r}")
            if len(sys.argv) > 1 and name.startswith('blur ramp, every 2nd'): await pg.screenshot(path=sys.argv[1])
            if len(sys.argv) > 2 and name.startswith('split'): await pg.screenshot(path=sys.argv[2])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"focus_series_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
