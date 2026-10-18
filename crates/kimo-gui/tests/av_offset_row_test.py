#!/usr/bin/env python3
"""av_offset info panel: 4 rows from the top-level numbers only (never the curve): Best lag "n ms · audio late|early"
("0 ms" alone at zero; "none · no audio onsets" when the audio is silent), r there / at 0 "0.383 / -0.004" (3 decimals),
Runner-up "n ms · r x · margin m", Biggest picture change "t s · change". Best lag, Runner-up and Biggest picture change
put the label on its own line and the value under it (checked: value top >= label bottom).

ev_engine_av_offset.json is a real kimo-mcp spectator stream recorded on files/jaja/jaja-ad.mp4 (1920x1080 24 fps,
decode only): the clip against its own audio, the same picture against the same audio placed 0.12 s late as an audio clip,
and a 20-30 s window with a 2 s search. Junk cases are the first recording with scalars broken: a NaN lag drops the row,
a silent result reads "none · no audio onsets", a broken top change drops that row. Never 0 for a missing number.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/av_offset_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_av_offset.json').read_text())
SESSION = EV[0]; PAIRS = [EV[1:3], EV[3:5], EV[5:7]]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
STACK = "() => [...document.querySelectorAll('.readcard .rr.stack')].map(e => [e.querySelector('.rk').textContent.trim(), e.querySelector('.rv').getBoundingClientRect().top >= e.querySelector('.rk').getBoundingClientRect().bottom - 0.5])"
HEAD = "() => { const h = document.querySelector('.readcard .rh'); return h ? h.lastChild.textContent.trim() : null; }"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v or 'undefined' in v for _, v in rows)
def res(pair): return pair[1]['payload']['result']
def lag(R): L = R['best_lag_ms']; return f"{L} ms" + (' · audio late' if L > 0 else ' · audio early' if L < 0 else '')
def rr(R): return f"{R['best_r']:.3f} / {R['r_at_zero']:.3f}"
def ru(R): return f"{R['runner_up_lag_ms']} ms · r {R['runner_up_r']:.3f} · margin {R['peak_margin']:.3f}"
def big(R): T = R['top_picture_changes'][0]; return f"{T['t']:.3f} s · {T['change']:.3f}"
K4 = ['Best lag', 'r there / at 0', 'Runner-up', 'Biggest picture change']
def full(pair): R = res(pair); return lambda r, h: clean(r) and h == 'av_offset' and [k for k, _ in r] == K4 and dict(r) == {'Best lag': lag(R), 'r there / at 0': rr(R), 'Runner-up': ru(R), 'Biggest picture change': big(R)}
def junk(edit):
    pair = copy.deepcopy(PAIRS[0]); edit(pair[1]['payload']['result']); return pair
def silent(R):
    for k in ('best_lag_ms', 'best_r', 'r_at_zero', 'runner_up_lag_ms', 'runner_up_r', 'peak_margin'): R[k] = None
    R['onset_max'] = 0.0
CASES = [
    ('clip vs its own audio', PAIRS[0], full(PAIRS[0])),
    ('audio placed 0.12 s late', PAIRS[1], full(PAIRS[1])),
    ('20-30 s window, 2 s search', PAIRS[2], full(PAIRS[2])),
    ('junk lag: row dropped', junk(lambda R: R.__setitem__('best_lag_ms', 'NaN')), lambda r, h: clean(r) and 'Best lag' not in dict(r) and 'Runner-up' in dict(r)),
    ('silent: none, no numbers', junk(silent), lambda r, h: clean(r) and dict(r).get('Best lag') == 'none · no audio onsets' and 'r there / at 0' not in dict(r) and 'Runner-up' not in dict(r)),
    ('junk top change: row dropped', junk(lambda R: R.__setitem__('top_picture_changes', [{'t': 'x', 'change': 1}])), lambda r, h: clean(r) and 'Biggest picture change' not in dict(r)),
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
            good = bool(ok(rows, h)) and all(x[1] for x in st) and sorted(x[0] for x in st) == sorted(k for k, _ in rows if k in ('Best lag', 'Runner-up', 'Biggest picture change')); bad += not good
            print(f"{'ok  ' if good else 'FAIL'} {name}: {h!r} {rows!r}")
            if len(sys.argv) > 1 and name.startswith('audio placed'): await pg.screenshot(path=sys.argv[1])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"av_offset_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
