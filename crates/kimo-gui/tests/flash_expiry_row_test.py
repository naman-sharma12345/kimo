#!/usr/bin/env python3
"""Flash expiry repaint (v6 eyes (e)): a flash stamp that runs out leaves no .flash in the DOM, with no event after it.

A call's flash is a stamp (performance.now()) the panels read through router.js flashing(): it is on for FLASH_MS (700 ms).
Before this fix nothing repainted an idle screen when a stamp ran out, so the last frame's "hrow flash" (or a READ card's
.flash) stayed in the DOM until the next event, and cov_c's "panels byte-identical after the failure" re-read at 720 ms
could see a stale flash. Now app.js holds ONE timeout at the earliest live stamp + FLASH_MS (no polling, no rAF loop); its
callback paints directly, with no animation frame (router_test's idle transform check counts every rAF), drops the run-out flash and re-arms for the next earliest stamp still drawn; hiding, leaving the page or tearing
the panel down clears it.
  - one stamp (checkpoint_delete, History row): drawn with .flash; the timer is armed at stamp + 700; at stamp + 700 ms +
    one frame (a timeout to the expiry, then two animation frames) the DOM holds no .flash, the History panel matches a
    fresh full render of the same state (the no-flash render) byte for byte, one expiry repaint ran, as one render with
    0 app animation frames (metrics.raf unchanged from before the expiry to the read), no timer is left
  - boundary-before (the History row of a checkpoint_delete stamp): read inside a timeout at stamp + FLASH_MS - 20 ms
    (680 ms; the read time must fall before the expiry) the row still has .flash, so the flash is shown for its full time
  - boundary-after, the BOUNDARY check (same page): read at stamp + FLASH_MS + one frame (a timeout to the expiry, then two
    animation frames, then the read) there is no .flash anywhere and every History row's class is exactly "hrow" (no stale row)
  - stale-735a, the MARGIN check (fresh page; 735 ms is the moment the parent build's screenshot showed a stale History row):
    read inside a timeout at stamp + 735 ms (never before it), with no frame and no event in between
  - stale-735b, the MARGIN-after-frame check (same page): read again after two animation frames have run (the frame has
    rendered, so a late paint cannot hide)
    In both reads no element has .flash and every History row's class is exactly "hrow"; both read times are printed
  - three overlapping stamps (checkpoints c1, c2, c3 about 250 ms apart): at each expiry + one frame exactly the rows
    whose stamp ran out have lost their flash, the others keep it, and the timer is re-armed at the next stamp + 700;
    after the last: no .flash, panel == full render, three expiry repaints with 0 app animation frames across all three
    expiries, no timer, no animation frame in the next 1 s
  - teardown: with a stamp live, pagehide (the window closing) clears the timer; switching to a screen without the
    History panel (panel teardown) leaves no timer; hiding the window clears it. In each case no expiry callback runs
    after the teardown (metrics.flashEnds unchanged, and no animation frame from 50 ms before the expiry to 200 ms after)
No event is applied between a stamp and its read: the only thing that can drop the flash is the expiry repaint.
Exit 0 only when every case holds.

    python3 crates/kimo-gui/tests/flash_expiry_row_test.py
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
SESSION = {'agent': 'kimo-mcp 6', 'elapsed_s': 0, 'height': 720, 'read_only': True, 'reset': True, 'type': 'session', 'width': 1280}
TG = {'track_id': None, 'clip_id': None, 'param': None}
def call(name, args, result, cid):
    return [{'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': TG, 'payload': {'args': args}},
            {'tool_name': name, 'call_id': cid, 'phase': 'complete', 'target': TG, 'payload': {'dur_ms': 20, 'args': args, 'result': result}}]
def cp(name, cid): return call('checkpoint', {'name': name}, {'name': name}, cid)
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
FLASH_MS = 700
READ = "() => ({ t: performance.now(), n: document.querySelectorAll('.flash').length, rows: [...document.querySelectorAll('[data-panel=hist] .hrow')].map(e => e.className) })"
HIST = "() => { const p = document.querySelector('[data-panel=hist]'); return p ? p.innerHTML : null; }"
ROWS = "() => Object.fromEntries([...document.querySelectorAll('[data-panel=hist] .hrow')].map(r => [r.textContent.replace(/\\s+/g, '').slice(0, 2), r.classList.contains('flash')]))"   # {row id (c1 / c2 / c3): has .flash}
NFLASH = "() => document.querySelectorAll('.flash').length"
STAMPS = """() => { const out = {}; const walk = (o, d) => { if (!o || typeof o !== 'object' || d > 6) return;
  if (Array.isArray(o.items)) for (const it of o.items) if (it && it._flash != null && it.id != null) out[it.id] = it._flash; for (const k in o) if (k[0] !== '_') walk(o[k], d + 1); };
  walk(__kimo.state, 0); return out; }"""
TIMER = "() => __kimo.flashTimer ? __kimo.flashTimer() : 'no flashTimer'"
AT = """t => new Promise(r => setTimeout(() => { while (performance.now() < t) {}   // a timer can fire up to ~1 ms early (whole-ms rounding): spin to t
  r({ t: performance.now(), n: document.querySelectorAll('.flash').length,
      rows: [...document.querySelectorAll('[data-panel=hist] .hrow')].map(e => e.className) }); }, Math.max(0, t - performance.now())))"""   # one read at t (never before it), inside the timeout: no frame, no event between
FRAMES2 = "() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))"
async def until(pg, t):   # a timeout to t, then one frame (two rAFs: the repaint's frame, then the next)
    await pg.evaluate("t => new Promise(r => setTimeout(r, Math.max(0, t - performance.now()) + 1))", t)
    await pg.evaluate(FRAMES2); return await pg.evaluate('() => performance.now()')
async def full_render(pg):   # the no-flash render: the same state painted from scratch (hide + show = one full render)
    await pg.evaluate("() => { __kimo.setHidden(true); __kimo.setHidden(false); }"); await pg.evaluate(FRAMES2)
    return await pg.evaluate(HIST)
def near(a, b): return isinstance(a, (int, float)) and isinstance(b, (int, float)) and abs(a - b) < 0.01
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    res = []
    def report(good, text, errs):
        good = bool(good) and not errs; res.append(good); print(f"{'ok  ' if good else 'FAIL'} {text}" + (f' errors={errs}' if errs else ''), flush=True)
    async with async_playwright() as p:
        b = await p.chromium.launch()
        async def page():
            pg = await b.new_page(viewport={'width': 1920, 'height': 1080}); errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
            await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0'); await pg.wait_for_timeout(400)
            await pg.evaluate('e => __kimo.apply(e)', SESSION); await pg.wait_for_timeout(200)
            return pg, errs
        async def apply(pg, evs):
            for e in evs: await pg.evaluate('e => __kimo.apply(e)', e)
            await pg.evaluate(FRAMES2)
        M = lambda pg, k: pg.evaluate(f'() => __kimo.metrics.{k} || 0')
        # ---- one stamp ----
        pg, errs = await page()
        await apply(pg, cp('c1', 'k1')); await pg.wait_for_timeout(1200)
        await apply(pg, call('checkpoint_delete', {'name': 'c1'}, {'deleted': 'c1'}, 'k2'))
        s1 = (await pg.evaluate(STAMPS)).get('c1'); on = await pg.evaluate(ROWS); tm = await pg.evaluate(TIMER); fe0 = await M(pg, 'flashEnds')
        r0 = await M(pg, 'raf'); n0 = await M(pg, 'renders')
        t = await until(pg, s1 + FLASH_MS)
        n = await pg.evaluate(NFLASH); off = await pg.evaluate(ROWS); h1 = await pg.evaluate(HIST); fe1 = await M(pg, 'flashEnds'); tm2 = await pg.evaluate(TIMER)
        r1 = await M(pg, 'raf'); n1 = await M(pg, 'renders')
        h2 = await full_render(pg)
        report(s1 is not None and on == {'c1': True} and near(tm, s1 + FLASH_MS) and n == 0 and off == {'c1': False} and h1 == h2 and 'flash' not in (h1 or 'flash') and fe1 - fe0 == 1 and tm2 is None and r1 == r0 and n1 - n0 == 1,
               f"one stamp: drawn {on}, timer at stamp+{(tm - s1) if isinstance(tm, (int, float)) else tm}; read at stamp+{t - s1:.0f} ms: .flash in DOM {n}, rows {off}, panel == full render {h1 == h2}, expiry repaints {fe1 - fe0} ({n1 - n0} render, {r1 - r0} rAF), timer after {tm2}", errs)
        await pg.close()
        # ---- boundary: the stamp's last moments vs its expiry (one stamp, History row) ----
        pg, errs = await page()
        await apply(pg, cp('c1', 'k1')); await pg.wait_for_timeout(1200)
        await apply(pg, call('checkpoint_delete', {'name': 'c1'}, {'deleted': 'c1'}, 'k2'))
        s1 = (await pg.evaluate(STAMPS)).get('c1'); on = await pg.evaluate(ROWS)
        pre = await pg.evaluate(AT, s1 + FLASH_MS - 20) if s1 is not None else {'t': float('nan'), 'n': -1, 'rows': []}
        report(s1 is not None and on == {'c1': True} and pre['t'] < s1 + FLASH_MS and pre['n'] >= 1 and len(pre['rows']) == 1 and 'flash' in pre['rows'][0].split(),
               f"boundary-before (flash shown for its full time): read at stamp+{pre['t'] - (s1 or 0):.1f} ms (must be < {FLASH_MS}): History rows {pre['rows']}, .flash in DOM {pre['n']} (the flash is still shown)", errs)
        t = await until(pg, (s1 or 0) + FLASH_MS)
        post = await pg.evaluate(READ)   # after until(): the expiry timeout, two animation frames, then this read
        report(s1 is not None and post['n'] == 0 and len(post['rows']) == 1 and all(c == 'hrow' for c in post['rows']),
               f"boundary-after, BOUNDARY check: read at stamp+{post['t'] - (s1 or 0):.1f} ms (expiry {FLASH_MS} + one frame): History rows {post['rows']}, .flash in DOM {post['n']} (no stale row)", errs)
        await pg.close()
        # ---- 735 ms: the moment the parent build's screenshot showed a stale History row ----
        pg, errs = await page()
        await apply(pg, cp('c1', 'k1')); await pg.wait_for_timeout(1200)
        await apply(pg, call('checkpoint_delete', {'name': 'c1'}, {'deleted': 'c1'}, 'k2'))
        s1 = (await pg.evaluate(STAMPS)).get('c1'); on = await pg.evaluate(ROWS)
        r735 = await pg.evaluate(AT, s1 + 735) if s1 is not None else {'t': float('nan'), 'n': -1, 'rows': []}
        await pg.evaluate(FRAMES2); r735f = await pg.evaluate(READ)   # (b) the same state after two animation frames: the frame has rendered
        clean = lambda r: r['n'] == 0 and len(r['rows']) == 1 and all(c == 'hrow' for c in r['rows'])
        report(s1 is not None and on == {'c1': True} and r735['t'] >= s1 + 735 and clean(r735),
               f"stale-735a, MARGIN check (exact read): drawn {on}; read at stamp+{r735['t'] - (s1 or 0):.1f} ms (must be >= 735): History rows {r735['rows']}, .flash in DOM {r735['n']}", errs)
        report(s1 is not None and on == {'c1': True} and r735f['t'] > r735['t'] and clean(r735f),
               f"stale-735b, MARGIN-after-frame check (735 + two animation frames): read at stamp+{r735f['t'] - (s1 or 0):.1f} ms: History rows {r735f['rows']}, .flash in DOM {r735f['n']}", errs)
        await pg.close()
        # ---- three overlapping stamps, then idle ----
        pg, errs = await page()
        await apply(pg, cp('c1', 'k1')); await pg.wait_for_timeout(250)
        await apply(pg, cp('c2', 'k2')); await pg.wait_for_timeout(250)
        await apply(pg, cp('c3', 'k3'))
        st = await pg.evaluate(STAMPS); s = [st.get('c1'), st.get('c2'), st.get('c3')]; on = await pg.evaluate(ROWS); tm0 = await pg.evaluate(TIMER); fe0 = await M(pg, 'flashEnds')
        steps, good = [], None not in s and on == {'c1': True, 'c2': True, 'c3': True} and near(tm0, s[0] + FLASH_MS)
        ra0 = await M(pg, 'raf')
        for i in range(3):
            t = await until(pg, s[i] + FLASH_MS); rows = await pg.evaluate(ROWS); tm = await pg.evaluate(TIMER)
            judged = i == 2 or t < s[i + 1] + FLASH_MS   # read before the next stamp ran out too
            want = {f'c{k + 1}': k > i for k in range(3)}; nxt = s[i + 1] + FLASH_MS if i < 2 else None
            ok_i = (rows == want and (near(tm, nxt) if nxt else tm is None)) if judged else True
            good = good and ok_i; steps.append(f"c{i + 1}+{t - s[i]:.0f} ms rows {rows} timer {'+%.0f' % (tm - s[0]) if isinstance(tm, (int, float)) else tm}{'' if judged else ' (late read, not judged)'}")
        n = await pg.evaluate(NFLASH); h1 = await pg.evaluate(HIST); fe1 = await M(pg, 'flashEnds'); ra1 = await M(pg, 'raf'); h2 = await full_render(pg)
        report(good and n == 0 and h1 == h2 and fe1 - fe0 == 3 and ra1 == ra0, f"three stamps at +0/+{s[1] - s[0]:.0f}/+{s[2] - s[0]:.0f} ms: drawn {on}; " + '; '.join(steps) + f"; after: .flash in DOM {n}, panel == full render {h1 == h2}, expiry repaints {fe1 - fe0}, app rAF across the 3 expiries {ra1 - ra0}", errs)
        r0 = await M(pg, 'raf'); await pg.wait_for_timeout(1000); r1 = await M(pg, 'raf')
        report(r1 == r0, f"idle after the last expiry: {r1 - r0} animation frames in 1 s", errs)
        await pg.close()
        # ---- teardown: pagehide, panel teardown (screen switch), hide ----
        for kind in ('pagehide', 'panel teardown', 'hidden'):
            pg, errs = await page()
            await apply(pg, cp('c1', 'k1'))
            s1 = (await pg.evaluate(STAMPS)).get('c1'); tm = await pg.evaluate(TIMER)
            if kind == 'pagehide': await pg.evaluate("() => window.dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true }))")
            elif kind == 'panel teardown': await pg.evaluate("() => __kimo.apply({ type: 'mode', mode: 'grading' })"); await pg.evaluate(FRAMES2)
            else: await pg.evaluate("() => __kimo.setHidden(true)")
            tm2 = await pg.evaluate(TIMER); hist = await pg.evaluate("() => !!document.querySelector('[data-panel=hist]')"); fe0 = await M(pg, 'flashEnds')
            await pg.evaluate("t => new Promise(r => setTimeout(r, Math.max(0, t - performance.now()) - 50))", s1 + FLASH_MS)   # frames are counted from 50 ms before the expiry
            r0 = await M(pg, 'raf')                                                                                   # (a screen switch springs its splits for ~25 frames before that)
            await pg.evaluate("t => new Promise(r => setTimeout(r, Math.max(0, t - performance.now()) + 200))", s1 + FLASH_MS)
            fe1 = await M(pg, 'flashEnds'); r1 = await M(pg, 'raf'); tm3 = await pg.evaluate(TIMER)
            report(near(tm, s1 + FLASH_MS) and tm2 is None and tm3 is None and fe1 == fe0 and r1 == r0 and (kind != 'panel teardown' or not hist),
                   f"teardown by {kind}: timer before {'+%.0f' % (tm - s1) if isinstance(tm, (int, float)) else tm}, after {tm2}" + (f", History panel on screen {hist}" if kind == 'panel teardown' else '') + f"; past the expiry: expiry callbacks {fe1 - fe0}, animation frames from expiry-50 to expiry+200 ms {r1 - r0}, timer {tm3}", errs)
            await pg.close()
        await b.close()
    srv.shutdown()
    print(f"flash_expiry_row_test: {sum(res)}/{len(res)} passed"); sys.exit(0 if all(res) else 1)
asyncio.run(main())
