#!/usr/bin/env python3
"""Spring-eased splits: frame times while panels spring between two screens, and 0 px for everything else.

For each mode hop below (an engine 'mode' event, the way a live session changes screen), a panel that exists on
both screens springs from its old box to its new one (transform only). Measured in the page with a passive
observer loop (test-only; the app itself runs rAF only while a spring is moving):
  - frame intervals while any panel is springing: p50 / p95 / max (60 fps = 16.7 ms) and the app's own work per frame
  - settle time: hop event to the last frame any panel is still springing           assert <= 200 ms
  - overshoot: how far a springing panel's box (x, y, w, h) passes its final box     assert <= 0.5 px
  - frame interval while springing (60 fps = 16.7 ms)                               assert every interval <= 16.7 ms
                                                                                     (+0.5 ms vsync clock tolerance)
  - 0 px: every node that is NOT inside a springing panel (each panel box, every element inside a still panel,
    the header #topbar and everything in it, every playhead) must not move by 0.5 px on any frame of the hop.
    The header is compared between frames whose header text is the same: on a hop the centred live pill takes the
    new screen's activity words and re-centres for them, which is the data, not the spring.
  - a hop where no shared panel changes box springs nothing (reported as '-'), which is correct
  - rAF cancelled at settle: 0 app rAF pending and 0 fired over 1 s after the last spring frame
  - timers: 0 setTimeout/setInterval created by the hop (the header's 1 s session clock, armTicker, is the only timer
    a live session owns and is not counted); pending timers after settle <= before the hop
Exit 0 only when every assertion holds on every hop.

    python3 crates/kimo-gui/tests/spring_test.py
"""
import asyncio, functools, http.server, json, os, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
HOPS = [('edit', 'effects'), ('effects', 'roto'), ('roto', 'graph'), ('graph', 'scene3d'), ('scene3d', 'lighting'), ('lighting', 'export'), ('export', 'edit'), ('edit', 'grading')]
PROBE = """(() => { const rAF = window.requestAnimationFrame.bind(window), cAF = window.cancelAnimationFrame.bind(window);
  window.__app = []; window.__appRaf = 0; const pend = window.__rafPend = new Set();
  window.requestAnimationFrame = cb => { window.__appRaf++; const id = rAF(t => { pend.delete(id); const a = performance.now(); cb(t); window.__app.push([t, performance.now() - a]); }); pend.add(id); return id; };
  window.cancelAnimationFrame = id => { pend.delete(id); cAF(id); };
  const sT = window.setTimeout.bind(window), cT = window.clearTimeout.bind(window), sI = window.setInterval.bind(window), cI = window.clearInterval.bind(window);
  const tp = window.__tPend = new Set(); window.__tMade = 0;
  window.setTimeout = (f, ms, ...a) => { if (!/armTicker/.test(new Error().stack)) window.__tMade++; const id = sT((...b) => { tp.delete(id); f(...b); }, ms, ...a); tp.add(id); return id; };
  window.clearTimeout = id => { tp.delete(id); cT(id); };
  window.setInterval = (f, ms, ...a) => { window.__tMade++; const id = sI(f, ms, ...a); tp.add(id); return id; };
  window.clearInterval = id => { tp.delete(id); cI(id); };
  const ids = new WeakMap(); let idn = 0; const gid = e => { if (!ids.has(e)) ids.set(e, ++idn); return ids.get(e); };
  const box = e => { const r = e.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; };
  window.__obs = null; window.__watch = n => { const S = window.__obs = []; let k = 0; const loop = t => {
    const o = {}, inner = {};
    document.querySelectorAll('[data-panel]').forEach(e => { const p = e.dataset.panel, a = e.dataset.anim ? 1 : 0; o[p] = [...box(e), a];
      if (!a) e.querySelectorAll('*').forEach((c, i) => { inner[p + ' ' + i + ' ' + c.tagName + '.' + (c.className.baseVal ?? c.className)] = box(c); }); });
    const hd = document.getElementById('topbar'); if (hd) { const tx = '|' + hd.textContent; inner['#topbar' + tx] = box(hd); hd.querySelectorAll('*').forEach((c, i) => { inner['#topbar ' + i + ' ' + c.tagName + (c.id ? '#' + c.id : '') + tx] = box(c); }); }
    document.querySelectorAll('.playhead').forEach((c, i) => { if (!c.closest('[data-anim]')) inner['playhead ' + i + ' ' + c.closest('[data-panel]')?.dataset.panel] = box(c); });
    const gen = (window.__kimo ? window.__kimo.state.mode + '|' : '') + [...document.querySelectorAll('[data-panel]')].map(gid).join(','); S.push([t, o, inner, gen]); if (++k < n) rAF(loop); }; rAF(loop); }; })()"""
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
def pct(xs, p): xs = sorted(xs); return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))] if xs else 0

async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    ok = True; allint = []; allwork = []; settles = []; overs = []; traces = {}
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.add_init_script(PROBE)
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await pg.wait_for_timeout(500)
        await pg.evaluate("() => __kimo.apply({type: 'session', project: 'dune_teaser', agent: 'claude'})")
        for a, z in HOPS:
            await pg.evaluate(f"() => __kimo.apply({{type: 'mode', mode: '{a}'}})"); await pg.wait_for_timeout(900)
            await pg.evaluate('() => { window.__app = []; }')
            tm0, tp0 = await pg.evaluate('[window.__tMade, window.__tPend.size]')
            await pg.evaluate('n => __watch(n)', 50)
            await pg.evaluate(f"() => __kimo.apply({{type: 'mode', mode: '{z}'}})")
            await pg.wait_for_timeout(1100)
            obs = await pg.evaluate('window.__obs'); app = await pg.evaluate('window.__app')
            # frames while springing: observer frames where any panel carries data-anim
            sp = [t for t, o, _, _g in obs if any(v[4] for v in o.values())]
            ints = [y - x for x, y in zip(sp, sp[1:])]
            work = [w for t, w in app]
            # 0 px outside the springs: panel boxes, everything inside a still panel, the header, the playheads
            jit = 0.0; jw = ''; moved = set(); nodes = 0
            for (t0, A, IA, g0), (t1, B, IB, g1) in zip(obs, obs[1:]):
                if g0 != g1: continue                  # the hop frame itself: the new screen is built (the mode changes; panels the two screens share keep their element)
                for k, v in B.items():
                    if v[4] or A[k][4]: moved.add(k); continue
                    d = max(abs(x - y) for x, y in zip(v[:4], A[k][:4]))
                    if d > jit: jit, jw = d, k
                same = set(IA) & set(IB); nodes = max(nodes, len(same))
                for k in same:
                    if k.split(' ')[0] in moved: continue
                    d = max(abs(x - y) for x, y in zip(IA[k], IB[k]))
                    if d > jit: jit, jw = d, k
            # overshoot: a springing panel's box passing its final (settled) box, per edge
            over = 0.0; ow = ''
            fin = obs[-1][1]
            for k in moved:
                if k not in fin: continue
                tr = [o[k] for _, o, _, _g in obs if k in o and o[k][4]]
                if not tr: continue
                F = fin[k][:4]; S0 = tr[0][:4]
                for v in tr + [fin[k]]:
                    for c in range(4):
                        if abs(S0[c] - F[c]) < .5: continue
                        past = (v[c] - F[c]) * (-1 if S0[c] > F[c] else 1)   # > 0 means beyond the target
                        if past > over: over, ow = past, k
            settle = (sp[-1] - obs[0][0]) if sp else 0
            for k in moved:   # residual (px from the final box) per observed frame, for SPRING_TRACE
                if k in fin: traces[f'{a}->{z} {k}'] = [[t - obs[0][0], max(abs(o[k][c] - fin[k][c]) for c in range(4))] for t, o, _, _g in obs if k in o and t >= sp[0]]
            pend0 = await pg.evaluate('window.__rafPend.size')
            r0 = await pg.evaluate('window.__appRaf'); await pg.wait_for_timeout(1000); r1 = await pg.evaluate('window.__appRaf')
            tm1, tp1 = await pg.evaluate('[window.__tMade, window.__tPend.size]')
            hop_timers = tm1 - tm0   # every timer made in the hop window except the header's session clock (armTicker)
            allint += ints; allwork += work; settles.append(settle); overs.append(over)
            fail = []
            if settle > 200: fail.append(f'settle {settle:.0f} > 200 ms')
            if over > .5: fail.append(f'overshoot {over:.2f} px ({ow})')
            if ints and max(ints) > 16.7 + .5: fail.append(f'frame interval {max(ints):.1f} > 16.7 ms')
            if jit > .5: fail.append(f'still-node move {jit:.1f} px ({jw})')
            if pend0 or r1 != r0: fail.append(f'rAF pending {pend0}, fired after settle {r1 - r0}')
            if tp1 > tp0 or hop_timers > 0: fail.append(f'timers pending {tp0}->{tp1}, made by the hop {hop_timers}')
            ok &= not fail
            print(f"{'PASS' if not fail else 'FAIL'}  {a:>8} -> {z:<9} springing {sorted(moved) or '-'}  frames {len(sp)}  settle {settle:.0f} ms  overshoot {over:.2f}px  interval p50 {pct(ints, 50):.1f} p95 {pct(ints, 95):.1f} max {max(ints or [0]):.1f} ms  app work p95 {pct(work, 95):.2f} ms  still nodes {nodes} max move {jit:.1f}px  rAF pending {pend0} / after {r1 - r0}  timers {tp0}->{tp1} (hop made {hop_timers})" + (f"  <- {'; '.join(fail)}" if fail else ''))
        await b.close()
    srv.shutdown()
    if os.environ.get('SPRING_TRACE'): json.dump(traces, open(os.environ['SPRING_TRACE'], 'w'))
    print(f"\nall hops: settle max {max(settles or [0]):.0f} ms (each {[round(x) for x in settles]}), overshoot max {max(overs or [0]):.2f} px; {len(allint)} spring frame intervals, p50 {pct(allint, 50):.1f} / p95 {pct(allint, 95):.1f} / max {max(allint or [0]):.1f} ms ({1000 / max(pct(allint, 50), 1e-6):.0f} fps median); app work per frame p95 {pct(allwork, 95):.2f} ms, max {max(allwork or [0]):.2f} ms; page errors {errs[:2]}")
    ok &= not errs
    print('RESULT:', 'springs settle <= 200 ms, no overshoot, frame rate held, 0 px outside them, rAF + timers clean after' if ok else 'FAIL'); return 0 if ok else 1
if __name__ == '__main__':
    sys.exit(asyncio.run(main()))
