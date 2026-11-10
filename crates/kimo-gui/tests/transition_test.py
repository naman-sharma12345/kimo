#!/usr/bin/env python3
"""Transition test: switching between the sixteen screens is one cohesive workspace.

Feed off; the sixteen demo scenarios' end states are applied as views (demo/replay.js eventsFor), then the GUI is
switched through every mode (edit first, then each other screen and back through the cycle) with real `mode` events
on the production ingestion path (window.__kimo.apply). For every switch the compositor's own frames are captured
(CDP screencast, every frame) from just before the event until 450 ms after it, and asserted:
  (a) no blank or near-empty frame: every captured frame keeps at least 60 % of the content energy (mean distance
      from the canvas colour) of the quieter of the two settled screens it moves between, and the viewer's frame
      area is never blacked out where both screens show a picture;
  (b) element identity: #app, #topbar, #main and #footer, the Agent rail, the viewer and the timeline panel are the
      SAME nodes across every switch (a JS marker set on them at the start survives, and === holds);
  (c) no navigation or reload: no framenavigated after load, and a window marker survives;
  (d) idle afterwards: 0 requestAnimationFrame in 2 s once the last switch has settled; no page errors or console
      warnings.
Exit 1 on any miss.

    python3 crates/kimo-gui/tests/transition_test.py
"""
import asyncio, base64, os, functools, http.server, io, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
INIT = """(() => { window.__raf = 0; const r = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = cb => { window.__raf++; return r(cb); }; })()"""
PRELOAD = """async () => {
  const { eventsFor } = await import('../demo/replay.js');
  const all = await (await fetch('../demo/scenarios.json')).json();
  const K = window.__kimo;
  K.apply({ type: 'session', reset: true, project: 'dune_film.kimo', agent: 'codex', elapsed_s: 768, still: true });
  for (const s of all) for (const e of eventsFor(s, { still: true, live: false })) if (e.type === 'view' || e.type === 'stats') K.apply(e);
  const m16 = all.find(s => s.id === 'm16');
  K.apply({ type: 'mode', mode: 'edit' });
  for (const e of eventsFor(m16, { still: true, live: false })) if (e.type === 'tool_start' || e.type === 'tool_end') K.apply(e);
  return all.map(s => [s.id, s.mode]);
}"""
MARK = """() => { const g = s => document.querySelector(s);
  window.__ids = { app: g('#app'), topbar: g('#topbar'), main: g('#main'), footer: g('#footer'), agent: g('[data-panel=agent]'), viewer: g('[data-panel=viewer]') };
  for (const [k, e] of Object.entries(window.__ids)) e.__mark = k;
  window.__navMark = Math.random(); window.__tl = null;
  /* every DOM batch, at its microtask checkpoint: a named panel with no content at that moment is an unpopulated
     panel the compositor could show */
  window.__empty = [];
  new MutationObserver(() => { for (const s of document.querySelectorAll('#main > section[data-panel]')) if (!s.childElementCount) window.__empty.push(s.dataset.panel); })
    .observe(document.getElementById('main'), { childList: true, subtree: true, attributes: true, attributeFilter: ['data-panel'] });
  return Object.keys(window.__ids).length; }"""
CHECK_IDS = """() => { const g = s => document.querySelector(s), bad = [];
  const now = { app: g('#app'), topbar: g('#topbar'), main: g('#main'), footer: g('#footer'), agent: g('[data-panel=agent]'), viewer: g('[data-panel=viewer]') };
  for (const [k, e] of Object.entries(now)) if (!e || e !== window.__ids[k] || e.__mark !== k) bad.push(k);
  const tl = g('.panel.k-timeline');
  if (tl) { if (window.__tl && tl !== window.__tl) bad.push('timeline'); window.__tl = window.__tl || tl; tl.__mark = 'timeline'; }
  return { bad, nav: window.__navMark, frame: !!g('[data-panel=viewer] .frame img'), tl: !!tl }; }"""
# every panel of the screen, by name, with its box (leaving panels have given up their name and are not listed)
PANELS = """(() => [...document.querySelectorAll('#main > section[data-panel]')].map(e => { const r = e.getBoundingClientRect();
  return [e.dataset.panel, [r.left, r.top, r.right, r.bottom].map(Math.round)]; }).filter(([, b]) => b[2] - b[0] > 8 && b[3] - b[1] > 8))()"""
def energy(img, box=None):
    from PIL import Image, ImageStat
    im = img.convert('RGB')
    if box: im = im.crop(box)
    st = ImageStat.Stat(im)
    # mean distance from the canvas colour (#050506): a blank or half-painted frame has little of it
    return sum(abs(m - b) for m, b in zip(st.mean, (5, 5, 6))) / 3
async def main():
    from playwright.async_api import async_playwright
    from PIL import Image
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'
    misses, errs, navs = [], [], []
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        pg.on('pageerror', lambda e: errs.append(str(e)))
        pg.on('console', lambda m: errs.append(m.type + ': ' + m.text) if m.type in ('error', 'warning') else None)
        await pg.add_init_script(INIT)
        await pg.goto(url); await pg.wait_for_function('window.__kimo'); await pg.wait_for_timeout(300)
        pg.on('framenavigated', lambda f: navs.append(f.url) if f == pg.main_frame else None)
        modes = await pg.evaluate(PRELOAD); await pg.wait_for_timeout(500)
        await pg.evaluate(MARK)
        cdp = await pg.context.new_cdp_session(pg)
        frames = []
        async def on_frame(ev):
            frames.append((ev['metadata']['timestamp'], ev['data']))
            try: await cdp.send('Page.screencastFrameAck', {'sessionId': ev['sessionId']})
            except Exception: pass
        cdp.on('Page.screencastFrame', lambda ev: asyncio.ensure_future(on_frame(ev)))
        order = ['edit'] + [m for _, m in modes if m != 'edit'] + ['edit', 'captions', 'mixing', 'edit']
        settled = {}
        async def still(mode):
            png = await pg.screenshot(); im = Image.open(io.BytesIO(png)).convert('RGB')
            vb = await pg.evaluate("(() => { const f = document.querySelector('[data-panel=viewer] .frame'); if (!f) return null; const r = f.getBoundingClientRect(); return [r.left, r.top, r.right, r.bottom].map(Math.round); })()")
            boxes = await pg.evaluate(PANELS)
            return energy(im), (energy(im, tuple(vb)) if vb else None), (tuple(vb) if vb else None), im, boxes
        settled['edit'] = await still('edit')
        nsw = 0; nfr = 0
        for prev, mode in zip(order, order[1:]):
            frames.clear()
            await cdp.send('Page.startScreencast', {'format': 'jpeg', 'quality': 60, 'everyNthFrame': 1})
            await pg.wait_for_timeout(120)
            await pg.evaluate("() => { window.__empty = []; }")
            await pg.evaluate("m => window.__kimo.apply({ type: 'mode', mode: m })", mode)
            await pg.wait_for_timeout(450)
            await cdp.send('Page.stopScreencast'); await pg.wait_for_timeout(60)
            ids = await pg.evaluate(CHECK_IDS)
            # a panel that went empty during the switch and is not empty on the settled screen was shown unpopulated
            emp = await pg.evaluate("() => { const now = new Set([...document.querySelectorAll('#main > section[data-panel]')].filter(s => !s.childElementCount).map(s => s.dataset.panel)); return [...new Set(window.__empty)].filter(n => !now.has(n)); }")
            if emp: misses.append(f'{prev} -> {mode}: panel(s) {emp} were empty mid-switch')
            if mode not in settled: settled[mode] = await still(mode)
            lo = min(settled[prev][0], settled[mode][0])
            vlo = min(x for x in (settled[prev][1], settled[mode][1]) if x is not None) if settled[prev][1] is not None and settled[mode][1] is not None else None
            worst = None; vbox = settled[mode][2]
            for ts, data in frames:
                im = Image.open(io.BytesIO(base64.b64decode(data)))
                if im.size != (1376, 768): im = im.resize((1376, 768))
                e = energy(im)
                if worst is None or e < worst: worst = e
                if e < .6 * lo: misses.append(f'{prev} -> {mode}: near-empty frame (energy {e:.1f} vs settled {lo:.1f})'); break
                # RETAIN-UNTIL-READY (Naman 11:46): no panel of either screen is ever near-empty mid-flight. Each panel's box
                # (the outgoing screen's panels in their old boxes, the incoming screen's in their new ones) must keep at
                # least half the content energy of the dimmer of the two settled screens over that same box: a panel may
                # crossfade from what was there to what will be, never through blank.
                bad = None
                ob, nb = dict(settled[prev][4]), dict(settled[mode][4])
                for side, boxes in (('outgoing', settled[prev][4]), ('incoming', settled[mode][4])):
                    for name, bx in boxes:
                        # a panel on both screens in a different box is the same node springing between them (identity is
                        # asserted above, and the empty-panel probe covers its contents); its endpoint boxes are not its box mid-flight
                        if name in ob and name in nb and max(abs(x - y) for x, y in zip(ob[name], nb[name])) > 1: continue
                        ref = min(energy(settled[prev][3], tuple(bx)), energy(settled[mode][3], tuple(bx)))
                        if ref < 6: continue      # a box that is (near) canvas on one screen has nothing to lose
                        ef = energy(im, tuple(bx))
                        if ef < .5 * ref: bad = f'{side} panel {name} near-empty (energy {ef:.1f} vs {ref:.1f})'; break
                    if bad: break
                if bad:
                    misses.append(f'{prev} -> {mode}: {bad}')
                    if os.environ.get('TRANSITION_DUMP'): im.save(f"{os.environ['TRANSITION_DUMP']}/miss_{prev}_{mode}.jpg")
                    break
                # the picture: where both screens show one, the new screen's frame box never goes dark
                if vlo is not None and vbox and energy(im, vbox) < .5 * vlo: misses.append(f'{prev} -> {mode}: viewer frame blacked out (energy {energy(im, vbox):.1f} vs settled {vlo:.1f})'); break
            nsw += 1; nfr += len(frames)
            if not frames: misses.append(f'{prev} -> {mode}: no compositor frame captured')
            if ids['bad']: misses.append(f'{prev} -> {mode}: re-created {ids["bad"]}')
            print(f"{'PASS' if not ids['bad'] and frames and (worst is None or worst >= .6 * lo) else 'FAIL'}  {prev:>10} -> {mode:<10} frames {len(frames):3d}  min energy {worst if worst is not None else 0:5.1f} / settled {lo:5.1f}  same nodes {not ids['bad']}")
        await pg.wait_for_timeout(600)
        r0 = await pg.evaluate('window.__raf'); await pg.wait_for_timeout(2000); r1 = await pg.evaluate('window.__raf')
        nav = await pg.evaluate('window.__navMark')
        await b.close()
    srv.shutdown()
    if navs: misses.append(f'navigation after load: {navs}')
    if not nav: misses.append('window marker lost (reload)')
    if r1 - r0: misses.append(f'idle after the switches: {r1 - r0} rAF in 2 s')
    if errs: misses.append(f'page errors / warnings: {errs[:3]}')
    print(f'{nsw} switches, {nfr} compositor frames captured; navigations {len(navs)}; idle rAF {r1 - r0}; errors {errs[:3]}')
    for m in misses: print('MISS ', m)
    print('RESULT:', 'every switch seamless: no near-empty frame, same shell / rail / viewer / timeline nodes, no reload, idle 0 rAF' if not misses else f'FAIL ({len(misses)} misses)')
    sys.exit(1 if misses else 0)
asyncio.run(main())
