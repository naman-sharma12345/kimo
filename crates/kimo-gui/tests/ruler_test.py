#!/usr/bin/env python3
"""Timeline rulers: every label reads as time (mm:ss with a colon, or a plain frame / seconds count) and no ruler tick
crosses a label's digits. Ink boxes come from the label's own font metrics (canvas measureText: actual glyph bounds on
the label's baseline), ticks from their boxes; any overlap of a tick with a label's ink is a FAIL (this is what made
"00:02" read as "00;02": the 6 px major tick ran up through the colon).
Screens: the 16 sim screens (still) plus the edit screen live on a REAL k5 session (tests/ev_engine_k5_preview.json).
Exit 0 when all hold."""
import asyncio, functools, http.server, socketserver, threading, json, sys
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests' / 'ev_engine_k5_preview.json').read_text())['events']
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
fails = []; checks = [0]
def ok(c, m):
    checks[0] += 1
    if not c: fails.append(m); print('FAIL', m)
JS = """() => {
  const cv = document.createElement('canvas').getContext('2d'); const out = [];
  for (const R of document.querySelectorAll('.ruler')) {
    const panel = R.closest('[data-panel]');
    const ticks = [...R.querySelectorAll('i'), ...(panel ? panel.querySelectorAll('i.rtk') : [])].map(i => i.getBoundingClientRect()).filter(r => r.width > 0 && r.height > 0);
    for (const sp of R.querySelectorAll('span')) {
      const cs = getComputedStyle(sp); cv.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const t = sp.textContent, m = cv.measureText(t), r = sp.getBoundingClientRect();
      const fA = m.fontBoundingBoxAscent, fD = m.fontBoundingBoxDescent;
      const lh = cs.lineHeight === 'normal' ? r.height : parseFloat(cs.lineHeight);
      const base = r.top + (lh - (fA + fD)) / 2 + fA;
      const pl = parseFloat(cs.paddingLeft) || 0;
      const ink = { l: r.left + pl - m.actualBoundingBoxLeft, r: r.left + pl + m.actualBoundingBoxRight, t: base - m.actualBoundingBoxAscent, b: base + m.actualBoundingBoxDescent };
      for (const k of ticks) {
        const ox = Math.min(ink.r, k.right) - Math.max(ink.l, k.left), oy = Math.min(ink.b, k.bottom) - Math.max(ink.t, k.top);
        if (ox > 0 && oy > 0) out.push({ panel: panel && panel.dataset.panel, text: t, ox: +ox.toFixed(2), oy: +oy.toFixed(2), tick: [k.left, k.top, k.bottom].map(v => +v.toFixed(1)), ink: [ink.l, ink.t, ink.b].map(v => +v.toFixed(1)) });
      }
      if (!/^\\d\\d:\\d\\d$|^\\d+s?$/.test(t)) out.push({ panel: panel && panel.dataset.panel, text: t, bad: 'format' });
    }
  }
  return out; }"""

async def check(pg, name):
    await pg.evaluate('document.fonts.ready')
    o = await pg.evaluate(JS); n = await pg.evaluate("document.querySelectorAll('.ruler span').length")
    for x in o: ok(False, f'{name}: {x}')
    ok(True, f'{name}: {n} labels'); return n
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html'; total = 0
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        msgs = []; pg.on('console', lambda m: msgs.append(m.text) if m.type in ('warning', 'error') else None); pg.on('pageerror', lambda e: msgs.append(str(e)))
        for i in [f'm{k:02d}' for k in range(1, 17)]:
            await pg.goto(f'{url}?feed=sim&scenario={i}&still=1'); await pg.wait_for_timeout(900)
            total += await check(pg, i)
        await pg.goto(f'{url}?feed=off'); await pg.wait_for_function('window.__kimo')
        for e in EV: await pg.evaluate('e => window.__kimo.apply(e)', e)
        await pg.wait_for_timeout(600)
        n = await check(pg, 'k5 live'); total += n
        ok(n > 0, 'k5 live: no ruler labels on screen')
        ok(not msgs, f'console: {msgs[:3]}')
        await b.close()
    srv.shutdown()
    ok(total >= 60, f'only {total} ruler labels checked')
    print(f'ruler: {total} labels on 17 screens, {checks[0]} checks, {len(fails)} fail')
    sys.exit(1 if fails else 0)
asyncio.run(main())
