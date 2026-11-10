#!/usr/bin/env python3
"""Preview safety on a REAL k5 auto-preview session (tests/ev_engine_k5_preview.json: k5-kimo-mcp-bin over stdio,
KIMO_SPECTATOR=file:). The engine sends, after each mutating call, project_changed {rev} and then its own frame
frame_rendered {auto, rev, png_b64}; read-only, cache, save and failed calls send neither. The session carries a grade
(add_effect color_wheels), a cut (split_clip), a transform (set_property scale), a real failed grade (clip999),
read-only / cache / save calls, and a RACE: set_color_curve and add_effect written back to back (rev 7 and rev 8; the
engine renders once, for rev 8).

Asserts, replaying the engine's events in its own order:
  - between a mutation's project_changed and its frame, every engine viewer reads "out of date · rev N" (N = the new
    rev) over the previous frame; the frame for rev N (or a newer one) clears it and is what the viewer shows
  - read-only / cache / save / failed calls: the viewer's markup (minus the transient call HUD and gizmo) is byte-identical before
    and after, no mark, no rev move
  - the race: the mark reads rev 8 until the rev 8 frame lands; viewer image swaps <= mutations (7 frames, 8 mutations)
  - a stale frame never overwrites a newer one: the session's real rev 5 frame delivered after its rev 6 frame is dropped
    (viewer keeps rev 6, metrics.staleFrames == 1), and a k4-style frame with no rev still shows (older engines)
  - the GUI never asks for a frame: no "render_frame" in ui/*.js or src/
Exit 0 when all hold.
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests' / 'ev_engine_k5_preview.json').read_text())['events']
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
fails = []; checks = [0]
def ok(cond, msg):
    checks[0] += 1
    if not cond: fails.append(msg); print('FAIL', msg)
SNAP = """() => { const vs = [...document.querySelectorAll('[data-panel] .frame')].filter(f => f.closest('[data-panel]').dataset.panel === 'viewer');
  const f = vs[0]; const img = f && f.querySelector('img'); const st = f && f.querySelector('.tag.stale');
  const cl = f && f.cloneNode(true); if (cl) cl.querySelectorAll('.hud, .gz').forEach(x => x.remove());   /* the call HUD and the fading transform gizmo are not the preview */
  return { html: cl ? cl.outerHTML : null, src: img ? img.getAttribute('src') : null, stale: st ? st.textContent : null, n: vs.length,
    prev: JSON.parse(JSON.stringify(window.__kimo.state.preview || {})), staleFrames: window.__kimo.metrics.staleFrames ?? 0,
    /* element identity (Naman 11:38): each viewer node gets a serial once; a frame swap must keep the same nodes */
    ids: [f && f.closest('[data-panel]'), f, img].map(x => x ? (x.__kid || (x.__kid = (window.__kidn = (window.__kidn || 0) + 1))) : null), mode: window.__kimo.state.mode }; }"""
async def settle(pg): await pg.evaluate('() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))')
async def main():
    for p in list((CRATE / 'ui').glob('*.js')) + list((CRATE / 'src').rglob('*.rs')):
        ok('render_frame' not in p.read_text(), f'{p.name} mentions render_frame: the GUI must never ask for a frame')
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'
    frames = {e['rev']: 'data:image/png;base64,' + e['png_b64'] for e in EV if e.get('type') == 'frame_rendered'}
    async with async_playwright() as p:
        b = await p.chromium.launch()
        # ---- 1. the engine's own order ----
        pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        msgs = []; pg.on('console', lambda m: msgs.append(m.type + ': ' + m.text) if m.type in ('warning', 'error') else None); pg.on('pageerror', lambda e: msgs.append('PAGEERR ' + str(e)))
        await pg.goto(url); await pg.wait_for_function('window.__kimo'); await settle(pg)
        swaps, last_src, mutations, pending = 0, None, 0, None
        idswaps = 0
        byid = {}
        for e in EV:
            if e.get('tool_name'): byid.setdefault(e['call_id'], []).append(e)
        for i, e in enumerate(EV):
            before = await pg.evaluate(SNAP)
            await pg.evaluate('e => window.__kimo.apply(e)', e); await settle(pg)
            now = await pg.evaluate(SNAP)
            ty, tn = e.get('type'), e.get('tool_name')
            if ty == 'project_changed':
                mutations += 1; pending = e['rev']
                if now['prev'].get('revs'):
                    ok(now['stale'] == f'out of date · rev {e["rev"]}', f'after project_changed rev {e["rev"]}: mark {now["stale"]!r}')
                    ok(now['src'] == before['src'], f'rev {e["rev"]}: the old frame stays up until the new one lands')
            elif ty == 'frame_rendered':
                ok(now['src'] == frames[e['rev']], f'frame rev {e["rev"]} is what the viewer shows')
                if before['src'] and now['mode'] == before['mode']:   # a swap on the same screen: the buffer changes, the viewer stays
                    ok(now['ids'] == before['ids'], f'frame rev {e["rev"]}: the viewer was re-mounted (panel, frame, img ids {before["ids"]} -> {now["ids"]})'); idswaps += 1
                ok(now['stale'] is None, f'frame rev {e["rev"]} clears the mark (still {now["stale"]!r})')
                ok(now['prev'].get('shown') == e['rev'], f'shown rev {now["prev"].get("shown")} != {e["rev"]}')
            elif tn and e.get('phase') in ('complete', 'error'):
                mut = (e.get('payload') or {}).get('mutated')
                if e.get('phase') == 'error' or mut is False:
                    nxt = EV[i + 1] if i + 1 < len(EV) else {}
                    ok(nxt.get('type') != 'project_changed', f'{tn}: engine sent no rev change (real capture)')
                    ok(now['html'] == before['html'], f'{tn} ({e["phase"]}, mutated {mut}): the viewer changed')
                    ok(now['prev'] == before['prev'], f'{tn}: preview state moved {before["prev"]} -> {now["prev"]}')
                    ok(now['stale'] is None, f'{tn}: a read-only / save / cache / failed call marked the preview')
            if now['src'] != last_src: swaps += 1 if last_src is not None or now['src'] else 0; last_src = now['src']
            if tn == 'add_effect' and e.get('call_id') == 'kc581ce-13' and e.get('phase') == 'complete':
                ok(now['stale'] == 'out of date · rev 7' or now['stale'] == 'out of date · rev 8', f'race: mark during the race {now["stale"]!r}')
            if ty == 'project_changed' and e['rev'] == 8:
                ok(now['stale'] == 'out of date · rev 8', f'race: the newer rev is the one marked ({now["stale"]!r})')
        nfr = sum(1 for e in EV if e.get('type') == 'frame_rendered')
        ok(mutations == 8 and nfr == 7, f'capture shape: {mutations} mutations, {nfr} frames (expect 8, 7: the race renders once)')
        ok(swaps <= mutations, f'viewer image swaps {swaps} > mutations {mutations}')
        distinct = 0; prev_ = None
        for e in EV:
            if e.get('type') == 'frame_rendered':
                if e['png_b64'] != prev_: distinct += 1
                prev_ = e['png_b64']
        ok(swaps == distinct, f'viewer image swaps {swaps} != engine frames with new pixels {distinct}')
        ok(idswaps >= 3, f'only {idswaps} same-screen frame swaps checked for viewer identity')
        end = await pg.evaluate(SNAP)
        ok(end['stale'] is None and end['prev'].get('shown') == 8 and end['prev'].get('want') == 8, f'end state {end["prev"]} mark {end["stale"]!r}')
        ok(end['n'] >= 1, 'a viewer is on screen')
        ok(not msgs, f'console: {msgs[:3]}')
        # ---- 2. out of order: the real rev 5 frame arrives after the real rev 6 frame ----
        pg2 = await b.new_page(viewport={'width': 1376, 'height': 768})
        await pg2.goto(url); await pg2.wait_for_function('window.__kimo'); await settle(pg2)
        fr = {e['rev']: e for e in EV if e.get('type') == 'frame_rendered'}
        pc = {e['rev']: e for e in EV if e.get('type') == 'project_changed'}
        for r in (5, 6): await pg2.evaluate('e => window.__kimo.apply(e)', pc[r])
        await pg2.evaluate('e => window.__kimo.apply(e)', fr[6]); await settle(pg2)
        s6 = await pg2.evaluate(SNAP)
        await pg2.evaluate('e => window.__kimo.apply(e)', fr[5]); await settle(pg2)
        s5 = await pg2.evaluate(SNAP)
        ok(s5['src'] == frames[6] and s5['html'] == s6['html'], 'a late rev 5 frame overwrote the rev 6 frame')
        ok(s5['staleFrames'] == 1 and s5['prev'].get('shown') == 6, f'late frame not counted as dropped: {s5["staleFrames"]}, shown {s5["prev"].get("shown")}')
        # a newer project rev, then the late frames again: the mark stays on the newest rev
        await pg2.evaluate('e => window.__kimo.apply(e)', pc[8]); await pg2.evaluate('e => window.__kimo.apply(e)', fr[5]); await settle(pg2)
        s8 = await pg2.evaluate(SNAP)
        ok(s8['stale'] == 'out of date · rev 8' and s8['src'] == frames[6], f'after rev 8 + late rev 5 frame: mark {s8["stale"]!r}')
        await pg2.evaluate('e => window.__kimo.apply(e)', fr[8]); await settle(pg2)
        s9 = await pg2.evaluate(SNAP)
        ok(s9['stale'] is None and s9['src'] == frames[8], 'rev 8 frame clears the mark')
        # ---- 3. an engine without auto-preview (no rev on its frames): frames still show, no mark ever ----
        pg3 = await b.new_page(viewport={'width': 1376, 'height': 768})
        await pg3.goto(url); await pg3.wait_for_function('window.__kimo'); await settle(pg3)
        await pg3.evaluate('e => window.__kimo.apply(e)', pc[5]); await settle(pg3)
        k4 = {k: v for k, v in fr[5].items() if k not in ('rev', 'gen', 'auto')}
        await pg3.evaluate('e => window.__kimo.apply(e)', k4); await settle(pg3)
        await pg3.evaluate('e => window.__kimo.apply(e)', pc[6]); await settle(pg3)
        s3 = await pg3.evaluate(SNAP)
        ok(s3['src'] == frames[5] and s3['stale'] is None, f'rev-less frame: src shown {s3["src"] == frames[5]}, mark {s3["stale"]!r}')
        await b.close()
    srv.shutdown()
    print(f'preview_rev: {checks[0]} checks, {len(fails)} fail')
    sys.exit(1 if fails else 0)
asyncio.run(main())
