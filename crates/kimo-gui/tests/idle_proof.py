#!/usr/bin/env python3
"""Idle proof for the Kimo spectator render loop.

Proves, in headless Chromium, that the UI does no frame work when nothing
is happening:
  1. visible, live session, no events (after settle): 0 rAF callbacks.
     The only timer allowed is the 1 Hz elapsed clock (text-only update).
  2. visible, finished/frozen session, no events: 0 rAF, 0 timers.
  3. hidden while engine events keep arriving: 0 rAF, 0 timers
     (events are folded into state, nothing painted).
  4. hidden, no events: 0 rAF, 0 timers.
  5. visible again: exactly one catch-up paint (showing the newest hidden
     event), then back to 0.
Each phase also logs the page's main-thread task time and the renderer
process CPU time over the window.

    python3 crates/kimo-gui/tests/idle_proof.py            # 10 s windows
    python3 crates/kimo-gui/tests/idle_proof.py --secs 5   # quicker

Needs: pip install playwright && python3 -m playwright install chromium
Exit code 0 = all assertions hold, 1 = a phase did frame work.

How "hidden" is produced: headless Chromium never reports a page hidden, so
by default the test overrides document.visibilityState and fires the real
visibilitychange event; the browser keeps running rAF normally, so any frame
the app requested would still fire and be counted (a stricter check than a
real background tab, where Chromium would throttle rAF for us).
"""
import argparse, asyncio, functools, http.server, json, socketserver, sys, threading, time
from pathlib import Path

CRATE = Path(__file__).resolve().parents[1]

PROBE = r"""
(() => {
  const P = window.__probe = { rafReq: 0, rafCb: 0, toSet: 0, toCb: 0, ivSet: 0, ivCb: 0, stack: {} };
  const note = k => { if (!P.trace) return; const s = (new Error().stack || '').split('\n')[3] || ''; P.stack[k + ' ' + s.trim()] = (P.stack[k + ' ' + s.trim()] || 0) + 1; };
  const rAF = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = cb => { P.rafReq++; note('raf'); return rAF(t => { P.rafCb++; cb(t); }); };
  const sT = window.setTimeout.bind(window);
  window.setTimeout = (cb, ms, ...a) => { P.toSet++; note('timeout'); return sT((...x) => { P.toCb++; typeof cb === 'function' ? cb(...x) : 0; }, ms, ...a); };
  const sI = window.setInterval.bind(window);
  window.setInterval = (cb, ms, ...a) => { P.ivSet++; note('interval'); return sI((...x) => { P.ivCb++; cb(...x); }, ms, ...a); };
  let fake = null;
  Object.defineProperty(Document.prototype, 'visibilityState', { configurable: true, get() { return fake ?? 'visible'; } });
  Object.defineProperty(Document.prototype, 'hidden', { configurable: true, get() { return (fake ?? 'visible') === 'hidden'; } });
  window.__setVis = v => { fake = v; document.dispatchEvent(new Event('visibilitychange')); };
  window.__probeSnap = () => ({ rafReq: P.rafReq, rafCb: P.rafCb, toSet: P.toSet, toCb: P.toCb, ivSet: P.ivSet, ivCb: P.ivCb,
    renders: window.__kimo ? window.__kimo.metrics.renders : -1, events: window.__kimo ? window.__kimo.metrics.events : -1 });
})();
"""

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

def serve():
    h = functools.partial(Quiet, directory=str(CRATE))
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), h); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv

async def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--secs', type=float, default=10.0)
    ap.add_argument('--scenario', default='m16')
    ap.add_argument('--json', help='write results here')
    o = ap.parse_args()
    from playwright.async_api import async_playwright
    srv = serve(); port = srv.server_address[1]
    base = f'http://127.0.0.1:{port}/ui/index.html?feed=sim&scenario={o.scenario}'
    res, fails = {}, []
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768})
        await ctx.add_init_script(PROBE)
        bcdp = await b.new_browser_cdp_session()
        pg = await ctx.new_page()
        cdp = await ctx.new_cdp_session(pg)
        await cdp.send('Performance.enable')

        async def cpu():
            m = {x['name']: x['value'] for x in (await cdp.send('Performance.getMetrics'))['metrics']}
            pi = (await bcdp.send('SystemInfo.getProcessInfo'))['processInfo']
            rend = sum(x['cpuTime'] for x in pi if x['type'] == 'renderer')
            gpu = sum(x['cpuTime'] for x in pi if x['type'] == 'GPU')
            return m.get('TaskDuration', 0), m.get('ScriptDuration', 0), rend, gpu

        async def window(name, secs, during=None, expect_raf=0, max_timers=0):
            s0 = await pg.evaluate('__probeSnap()'); c0 = await cpu(); t0 = time.monotonic()
            if during: await during(secs)
            else: await asyncio.sleep(secs)
            s1 = await pg.evaluate('__probeSnap()'); c1 = await cpu(); dt = time.monotonic() - t0
            d = {k: s1[k] - s0[k] for k in s0}
            timers = d['toCb'] + d['ivCb']
            r = {'secs': round(dt, 2), 'raf_callbacks': d['rafCb'], 'raf_requests': d['rafReq'],
                 'timer_callbacks': timers, 'renders': d['renders'], 'events_in': d['events'],
                 'main_thread_task_ms': round((c1[0] - c0[0]) * 1000, 2), 'script_ms': round((c1[1] - c0[1]) * 1000, 2),
                 'renderer_cpu_ms': round((c1[2] - c0[2]) * 1000, 1), 'gpu_proc_cpu_ms': round((c1[3] - c0[3]) * 1000, 1)}
            r['main_thread_busy_pct'] = round(100 * r['main_thread_task_ms'] / 1000 / dt, 3)
            ok_raf = (d['rafCb'] == expect_raf) if isinstance(expect_raf, int) else expect_raf(d['rafCb'])
            ok = ok_raf and timers <= max_timers
            r['pass'] = ok; res[name] = r
            print(f"{'PASS' if ok else 'FAIL'}  {name:<34} rAF={d['rafCb']:<3} timers={timers:<3} renders={d['renders']:<3} events={d['events']:<4} "
                  f"main-thread {r['main_thread_task_ms']:>7.2f} ms ({r['main_thread_busy_pct']}%)  renderer cpu {r['renderer_cpu_ms']} ms", flush=True)
            if not ok: fails.append(name)

        async def settle(quiet=1.5, limit=60):
            last, t, t0 = None, time.monotonic(), time.monotonic()
            while time.monotonic() - t0 < limit:
                s = await pg.evaluate('__probeSnap()')
                k = (s['events'], s['rafCb'])
                if k != last: last, t = k, time.monotonic()
                elif time.monotonic() - t >= quiet: return
                await asyncio.sleep(0.25)
            raise SystemExit('page never settled')

        # live session: replay the scenario once, quickly, then go quiet
        await pg.goto(base + '&loop=0&gap=40&hold=0')
        await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
        await settle()
        clock_ticks = max(2, int(o.secs) + 2)
        await window('visible live, no events', o.secs, max_timers=clock_ticks)

        async def feed(secs):
            # engine keeps talking while hidden: 20 events/s from outside the page
            n = int(secs * 20); t0 = time.monotonic()
            for i in range(n):
                ev = {'type': 'frame_rendered', 'tc': f'01:00:{i // 24 % 60:02d}:{i % 24:02d}', 'res': '1920 x 1080 24p'}
                await pg.evaluate('e => __kimo.apply(e)', ev)
                if i % 5 == 4: await pg.evaluate('e => __kimo.apply(e)', {'type': 'tool_progress', 'id': 'probe', 'progress': {'done': i, 'total': n}})
                await asyncio.sleep(max(0, t0 + (i + 1) / 20 - time.monotonic()))

        await pg.evaluate("__setVis('hidden')"); await asyncio.sleep(0.3)
        await window('hidden, events arriving (20/s)', o.secs, during=feed)
        await window('hidden, no events', o.secs)
        async def show(secs):
            await pg.evaluate("__setVis('visible')"); await asyncio.sleep(secs)
        await window('visible again: catch-up', 1.0, during=show, expect_raf=lambda n: n == 1, max_timers=2)
        tc = await pg.evaluate("document.querySelector('.tc, .timecode') ? document.querySelector('.tc, .timecode').textContent : ''")
        res['catch_up_timecode'] = tc
        await window('visible after catch-up, no events', o.secs, max_timers=clock_ticks)

        # frozen session (finished run, clock stopped): nothing at all
        await pg.goto(base + '&still=1')
        await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
        await settle()
        await window('visible, frozen session', o.secs)

        await b.close()
    srv.shutdown()
    if o.json: Path(o.json).write_text(json.dumps(res, indent=2))
    print('\nRESULT:', 'all phases idle' if not fails else 'FAILED: ' + ', '.join(fails))
    sys.exit(1 if fails else 0)

if __name__ == '__main__':
    asyncio.run(main())
