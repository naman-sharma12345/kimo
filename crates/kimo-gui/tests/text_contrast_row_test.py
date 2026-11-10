#!/usr/bin/env python3
"""text_contrast_series info panel rows: drawn only from top-level scalars (worst_ratio / _time / _frame, threshold,
size_class, frames_below / frames_sampled / first_below_time, cut_count / cut_times[0]), never from the series array. The card
keeps 4 rows.

ev_engine_text_contrast.json is a real kimo-mcp spectator stream recorded on jaja-ad.mp4 (640x360 24 fps timeline, the
top-right label box, white text): 8..16 s every 2nd frame plus every cut with text_px 5 (the bar from the size class), 10..14 s
with no bar, and 8..16 s every 4th frame with an explicit threshold 7.
  - text_px: Worst ratio "r:1 (bar b:1, normal|large)", Worst at "t s · frame f", Below bar "n of N frames · first t s"
    plus " · scrim ≥ a%" when frames fall below (the fix is a scrim, never a recolour),
    Cuts "k inside · first t s" or "none inside", all equal to the recorded numbers
  - no bar: Frames instead of Below bar, no "(bar"
  - explicit threshold: "(bar 7:1)" with no size class
  - captions batch (same jaja timeline, four named spans of the label, two of them across cuts): Worst ratio "r:1 · name",
    Worst at, Captions "n · k below bar", Cross a cut "name @ t s, ..." (first 3, then "and N more")
  - junk scalars ("NaN" ratio): the row is dropped, never 0; a junk scrim_alpha_min keeps " · scrim" with no number
  - no "…+" marker, no clipped value cell
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/text_contrast_row_test.py [screenshot.png]
"""
import asyncio, copy, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests/ev_engine_text_contrast.json').read_text())
SESSION, PX, NOBAR, EXP, CAPS = EV[0], EV[1:3], EV[3:5], EV[5:7], EV[7:9]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
ROWS = "() => [...document.querySelectorAll('.readcard .rr')].map(r => { const k = r.querySelector('.rk'), v = r.querySelector('.rv'); return [k.textContent.trim(), (v.scrollWidth > v.clientWidth + 1 ? 'CLIPPED ' : '') + v.textContent.replace(/\\s+/g, ' ').trim()]; })"
def clean(rows): return bool(rows) and not any('…+' in v or v.startswith('CLIPPED') or 'NaN' in v for _, v in rows)
def r2(x): return f"{round(x * 100) / 100:g}"
def cuts(R): return 'none inside' if R['cut_count'] == 0 else f"{R['cut_count']} inside · first {r2(R['cut_times'][0])}s"
RP, RN, RE, RC = PX[1]['payload']['result'], NOBAR[1]['payload']['result'], EXP[1]['payload']['result'], CAPS[1]['payload']['result']
def at(R): return f"{r2(R['worst_time'])}s · frame {R['worst_frame']}"
def scrim(R): return (f" · scrim ≥ {round(R['scrim_alpha_min'] * 100)}%" if isinstance(R.get('scrim_alpha_min'), (int, float)) else ' · scrim') if R.get('suggested_fix') == 'scrim' else ''
def below(R): return f"{R['frames_below']} of {R['frames_sampled']} frames" + (f" · first {r2(R['first_below_time'])}s" if R.get('first_below_time') is not None else '') + scrim(R)
def junk():
    pair = copy.deepcopy(PX); pair[1]['payload']['result']['worst_ratio'] = 'NaN'; return pair
def junk_scrim():
    pair = copy.deepcopy(EXP); pair[1]['payload']['result']['scrim_alpha_min'] = 'x'; return pair
assert RE.get('suggested_fix') == 'scrim' and isinstance(RE.get('scrim_alpha_min'), float), 'the recorded explicit-bar call must carry a scrim suggestion'

def crossing(R): return 'none' if R['captions_crossing_cut'] == 0 else ', '.join(f"{c['name'][:24]} @ {r2(c['cut_times'][0])}s" for c in R['crossing_cut'][:3]) + (f" and {R['captions_crossing_cut'] - 3} more" if R['captions_crossing_cut'] > 3 else '')
CASES = [
    ('captions batch', CAPS, lambda r: clean(r) and [k for k, _ in r] == ['Worst ratio', 'Worst at', 'Captions', 'Cross a cut'] and dict(r)['Worst ratio'] == f"{r2(RC['worst_ratio'])}:1 · {RC['worst_caption'][:40]}"
        and dict(r)['Worst at'] == at(RC) and dict(r)['Cross a cut'] == crossing(RC) and dict(r)['Captions'].startswith(f"{RC['caption_count']} · {RC['captions_below']} below bar")),
    ('text_px bar', PX, lambda r: clean(r) and [k for k, _ in r] == ['Worst ratio', 'Worst at', 'Below bar', 'Cuts'] and dict(r)['Worst ratio'] == f"{r2(RP['worst_ratio'])}:1 (bar {r2(RP['threshold'])}:1, {RP['size_class']})"
        and dict(r)['Worst at'] == at(RP) and dict(r)['Below bar'] == below(RP) and dict(r)['Cuts'] == cuts(RP)),
    ('no bar', NOBAR, lambda r: clean(r) and [k for k, _ in r] == ['Worst ratio', 'Worst at', 'Frames', 'Cuts'] and dict(r)['Worst ratio'] == f"{r2(RN['worst_ratio'])}:1" and dict(r)['Frames'] == str(RN['frames_sampled']) and dict(r)['Cuts'] == cuts(RN)),
    ('explicit bar', EXP, lambda r: clean(r) and dict(r)['Worst ratio'] == f"{r2(RE['worst_ratio'])}:1 (bar 7:1)" and dict(r)['Below bar'] == below(RE)),
    ('junk ratio dropped', junk(), lambda r: clean(r) and 'Worst ratio' not in dict(r) and 'Worst at' in dict(r)),
    ('junk scrim alpha: scrim named, no number', junk_scrim(), lambda r: clean(r) and dict(r)['Below bar'].endswith(' · scrim') and '%' not in dict(r)['Below bar']),
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
            if len(sys.argv) > 1 and name == 'text_px bar': await pg.screenshot(path=sys.argv[1])
            if len(sys.argv) > 2 and name == 'captions batch': await pg.screenshot(path=sys.argv[2])
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"text_contrast_row_test: {len(CASES) - bad}/{len(CASES)} passed"); sys.exit(1 if bad else 0)
asyncio.run(main())
