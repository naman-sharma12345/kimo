#!/usr/bin/env python3
"""Scope dock + export progress, from two REAL engine sessions (kimo-mcp over stdio, unedited):
  - tests/ev_engine_scope_export.json: the 8:35 demo dry run. render_scope(vectorscope) sends its image as a
    frame_rendered event, and export sends its own progress events (frames 0..300 of 300, fps, eta).
  - tests/ev_engine_k5_scope.json: k5 with auto-preview (rev-tagged frames). A scope with the viewer up to date, a real
    refused scope, set_property + render_scope written back to back (the engine sends no frame for that rev, so two
    scope frames land while the viewer reads "out of date"), waveform then set_property x (its rev 6 frame), export.

Asserts:
  - the vectorscope image lands in the scope dock (the colour screen's Scopes panel, over its vectorscope half, labelled
    with the scope) and NOT on any viewer: every screen's viewer markup and state are unchanged by it, the preview rev
    state is untouched
  - zero cross-talk with preview safety (k5): a scope frame never changes the viewer's shown rev, the rev it waits for,
    its "out of date · rev N" mark or its image; while out of date the mark stays exactly as the engine's revs say
    (independently tracked here from project_changed / frame rev), and the next real frame still lands
  - a refused render_scope changes nothing (dock, viewers, preview)
  - export: from start until the engine's first progress event the bar is indeterminate (a sweep, no percentage on screen,
    the Renders row says "rendering" with no number); each progress event then shows the engine's own pct, frame n / N and
    eta; complete shows 100 and done
Exit 0 when all hold.
"""
import asyncio, functools, http.server, json, re, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
SESSIONS = [(n, json.loads((CRATE / 'tests' / n).read_text())['events']) for n in ('ev_engine_scope_export.json', 'ev_engine_k5_scope.json')]
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
fails = []; checks = [0]
def ok(c, m):
    checks[0] += 1
    if not c: fails.append(m); print('FAIL', m)
SNAP = """() => { const st = window.__kimo.state, out = {};
  for (const [m, V] of Object.entries(st.views)) for (const [n, d] of Object.entries(V)) if (d && ('src' in d || '_engine' in d)) out[m + '.' + n] = d.src || null;
  const vf = [...document.querySelectorAll('[data-panel=viewer] .frame')].map(f => { const c = f.cloneNode(true); c.querySelectorAll('.hud, .gz').forEach(x => x.remove()); return c.outerHTML; });
  const sd = document.querySelector('.scopedock img'), lab = document.querySelector('.scopedock .lab');
  const pb = document.querySelector('[data-panel=viewer] .pbar'), vm = pb ? pb.nextElementSibling : null;
  const rows = [...document.querySelectorAll('[data-panel] tr, [data-panel] .rrow')].map(r => r.textContent).filter(t => /out\\.mp4/.test(t));
  return { mode: st.mode, views: out, vf, dock: sd ? sd.getAttribute('src') : null, lab: lab ? lab.textContent : null,
    pbar: pb ? pb.className : null, pbw: pb ? pb.firstElementChild.style.width : null, meta: vm ? vm.textContent : null, rows,
    preview: JSON.stringify(st.preview || {}), stale: [...document.querySelectorAll('[data-panel=viewer] .tag.stale')].map(t => t.textContent),
    vimg: [...document.querySelectorAll('[data-panel=viewer] .frame img')].map(i => i.getAttribute('src')), anim: pb ? getComputedStyle(pb.firstElementChild).animationName : null }; }"""
async def settle(pg): await pg.evaluate('() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))')
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        msgs = []; pg.on('console', lambda m: msgs.append(m.text) if m.type in ('warning', 'error') else None); pg.on('pageerror', lambda e: msgs.append(str(e)))
        for name, EV in SESSIONS:
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await pg.wait_for_function('window.__kimo'); await settle(pg)
            scope_ids = {e['call_id'] for e in EV if e.get('tool_name') == 'render_scope' and e.get('phase') == 'start'}
            bad_ids = {e['call_id'] for e in EV if e.get('tool_name') == 'render_scope' and e.get('phase') == 'error'}
            nprog = 0; nscope = 0; nstale_scope = 0; want = 0; shown = 0; last_src = None; dock = None; revs = False   # revs: the engine tags its frames (k5); a rev-less engine raises no mark
            for e in EV:
                before = await pg.evaluate(SNAP)
                await pg.evaluate('e => window.__kimo.apply(e)', e); await settle(pg)
                s = await pg.evaluate(SNAP)
                if e.get('type') == 'project_changed' and isinstance(e.get('rev'), int): want = max(want, e['rev'])
                if e.get('type') == 'frame_rendered' and e.get('call_id') in scope_ids:
                    nscope += 1; src = 'data:image/png;base64,' + e['png_b64']; kind = next(x['payload']['args']['scope'] for x in EV if x.get('call_id') == e['call_id'] and x.get('phase') == 'start')
                    ok(s['dock'] == src, f'{name}: scope image not in the scope dock'); dock = src
                    ok(s['lab'] and s['lab'].startswith(kind), f'{name}: dock label {s["lab"]!r}, want {kind}')
                    ok(s['mode'] == 'color', f'{name}: screen {s["mode"]}, want color (the screen with the Scopes dock)')
                    ok(s['views'] == before['views'], f'{name}: a viewer changed: {[k for k in s["views"] if s["views"][k] != before["views"].get(k)]}')
                    ok(s['vf'] == before['vf'], f'{name}: viewer markup changed by the scope image')
                    ok(src not in json.dumps(s['views']) and src not in s['vimg'], f'{name}: scope image reached a viewer')
                    ok(s['preview'] == before['preview'], f'{name}: scope frame moved the preview rev state {before["preview"]} -> {s["preview"]}')
                    ok(s['stale'] == before['stale'], f'{name}: scope frame changed the out-of-date mark {before["stale"]} -> {s["stale"]}')
                    if revs and want > shown:
                        nstale_scope += 1
                        ok(s['stale'] and all(t == f'out of date · rev {want}' for t in s['stale']), f'{name}: viewer out of date at rev {want} (shown {shown}) but mark reads {s["stale"]}')
                    if revs: ok(json.loads(s['preview']).get('shown') == shown, f'{name}: shown rev {s["preview"]}, want {shown}')
                    else: ok(not s['stale'], f'{name}: a rev-less engine but the viewer reads {s["stale"]}')
                elif e.get('type') == 'frame_rendered' and isinstance(e.get('rev'), int):
                    revs = True
                    if e['rev'] >= shown: shown = e['rev']; last_src = 'data:image/png;base64,' + e['png_b64']
                    ok(json.loads(s['preview']).get('shown') == shown, f'{name}: after frame rev {e["rev"]} shown {s["preview"]}')
                    if dock: ok(s['dock'] == dock, f'{name}: a preview frame changed the scope dock')
                if shown and want: ok(bool(s['stale']) == (want > shown), f'{name}: after {e.get("type") or e.get("tool_name")}/{e.get("phase")}: mark {s["stale"]} with want {want} shown {shown}')
                if last_src and s['mode'] != 'color': ok(last_src in s['vimg'] or not s['vimg'], f'{name}: viewer lost the rev {shown} frame')
                if e.get('call_id') in bad_ids:
                    ok(s['dock'] == before['dock'] and s['views'] == before['views'] and s['preview'] == before['preview'] and s['stale'] == before['stale'], f'{name}: refused render_scope changed dock/viewer/preview')
                if e.get('tool_name') == 'export':
                    ph = e['phase']; P = e['payload']
                    ok(s['mode'] == 'export', f'{name}: export on screen {s["mode"]}')
                    if ph == 'start':
                        ok(s['pbar'] and 'indet' in s['pbar'], f'{name}: export start: bar {s["pbar"]!r}, want indeterminate')
                        ok(s['anim'] == 'kindet', f'{name}: export start: no sweep ({s["anim"]})')
                        ok(s['meta'] is not None and '%' not in s['meta'], f'{name}: export start: a percentage on screen before any progress: {s["meta"]!r}')
                        ok(all('%' not in r for r in s['rows']), f'{name}: Renders row shows a number before any progress: {s["rows"]}')
                    elif ph == 'progress':
                        nprog += 1; pct = round(P['pct'])
                        ok(s['pbar'] and 'indet' not in s['pbar'] and s['pbw'] == f'{pct}%', f'{name}: progress {P["pct"]}: bar {s["pbar"]} width {s["pbw"]}')
                        ok(f'{pct}%' in (s['meta'] or '') and f'frame {P["current_frame"]} / {P["total_frames"]}' in (s['meta'] or ''), f'{name}: progress {P["pct"]}: meta {s["meta"]!r}')
                    elif ph == 'complete':
                        ok(s['pbw'] == '100%' and 'done' in (s['meta'] or ''), f'{name}: export complete: {s["pbw"]} {s["meta"]!r}')
                        ok(s['anim'] in (None, 'none'), f'{name}: export complete: the sweep still runs ({s["anim"]})')
            nfr = sum(1 for e in EV if e.get('type') == 'frame_rendered' and e.get('call_id') in scope_ids)
            ok(nscope == nfr and nscope >= 1, f'{name}: {nscope} scope frames seen of {nfr}')
            want_prog = sum(1 for e in EV if e.get('tool_name') == 'export' and e.get('phase') == 'progress')
            ok(nprog == want_prog and nprog > 0, f'{name}: {nprog} export progress events, want {want_prog}')
            if name == 'ev_engine_k5_scope.json':
                ok(nstale_scope >= 2, f'{name}: only {nstale_scope} scope frames landed while out of date (the race should give 2)')
                ok(shown == want, f'{name}: session ends with shown {shown} != latest rev {want}')
                ok(len(bad_ids) == 1, f'{name}: no refused render_scope in the session')
            print(f'{name}: {nscope} scope frames ({nstale_scope} while out of date), {nprog} progress events, final rev {shown}/{want}')
        ok(not msgs, f'console: {msgs[:3]}')
        await b.close()
    srv.shutdown()
    print(f'scope_export: {checks[0]} checks, {len(fails)} fail')
    sys.exit(1 if fails else 0)
asyncio.run(main())
