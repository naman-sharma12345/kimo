#!/usr/bin/env python3
"""Layout jitter: does anything that is already on screen move or resize while calls stream in?

Plays every demo scenario live (feed=sim, the scenario's calls arriving on their own clock) and the
cold-engine event files given on the command line, sampling the layout every 50 ms from outside the
page (no in-page timers). Jitter = an element that is already there changing box between two samples where no call started,
progressed or settled and the mode did not change (a mode hop lays out another screen; a call landing
moves the log — both reported apart, not counted):
  - panels:   any change to a panel's box                                  (must be 0 px)
  - rail:     a row's height, or its args / duration column x, changing   (rows scrolling up by exactly
              one row pitch when a call lands is the log moving, not jitter)
  - inspector rows / timeline tracks: height changes                         (must be 0 px)
Prints per-scenario maxima and exits 1 when any jitter is over 0.5 px.

    python3 crates/kimo-gui/tests/jitter_test.py [events.json ...]
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
SAMPLE_JS = """() => {
  const R = e => { const r = e.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; };
  const o = { panels: {}, calls: [], rows: [], trks: [], mode: window.__kimo ? __kimo.state.mode : '', sig: window.__kimo ? __kimo.state.calls.map(c => c.id + ':' + c.status).join(',') : '' };
  document.querySelectorAll('[data-panel]').forEach(e => { if (!e.dataset.anim) o.panels[e.dataset.panel] = R(e); });   /* a springing split is the animation itself (tests/spring_test.py) */
  document.querySelectorAll('.call').forEach(e => { if (e.closest('[data-anim]')) return; const s = e.querySelector('.args'), d = e.querySelector('.dur');
    o.calls.push({ key: (e.dataset.call != null ? '#' + e.dataset.call + ' ' : '') + (e.querySelector('.fn') || {}).textContent + '|' + (s ? s.textContent : ''), box: R(e), argsX: s ? s.getBoundingClientRect().x : null, durR: d ? d.getBoundingClientRect().right : null, durX: d ? d.getBoundingClientRect().x : null, argsW: s ? s.getBoundingClientRect().width : null }); });
  document.querySelectorAll('.kv .row').forEach((e, i) => e.closest('[data-anim]') || o.rows.push({ key: (e.querySelector('.k') || {}).textContent + '#' + i, h: e.getBoundingClientRect().height }));
  document.querySelectorAll('.trk').forEach((e, i) => e.closest('[data-anim]') || o.trks.push({ key: i, h: e.getBoundingClientRect().height }));
  o.cells = []; document.querySelectorAll('[data-panel=renders] .tr:not(.h)').forEach((e, i) => { const nm = (e.querySelector('.cream') || {}).textContent || i; o.cells.push({ key: nm, h: e.getBoundingClientRect().height, xs: [...e.children].map(c => c.getBoundingClientRect().x), th: (() => { const t = e.querySelector('.rthumb'); return t ? R(t) : null; })() }); });
  o.meta = [...document.querySelectorAll('.vmeta')].map(e => [...e.children].map(c => c.getBoundingClientRect().x)[0] ?? null);
  o.bar = (() => { const v = document.querySelector('[data-panel=viewer]'); const b = v && [...v.children].find(c => c.style && c.style.height && c.style.height.endsWith('px') && c.firstElementChild && c.style.borderRadius); return b ? R(b) : null; })();
  return o; }"""
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

def diff(samples):
    J = {'panel': 0.0, 'row_h': 0.0, 'args_x': 0.0, 'args_w': 0.0, 'dur_left': 0.0, 'dur_right': 0.0, 'kv_h': 0.0, 'track_h': 0.0, 'renders_cells': 0.0, 'thumb_box': 0.0, 'meta_x': 0.0, 'bar': 0.0}; where = {}
    def bump(k, v, w):
        if v > J[k]: J[k] = v; where[k] = w
    for a, b in zip(samples, samples[1:]):
        if a['mode'] != b['mode']: continue                 # a mode hop lays out a different screen: not jitter
        if a['sig'] != b['sig']:                            # a call landed / settled: the log moving, reported apart
            for p, box in b['panels'].items():
                if p in a['panels']: J['_panel_on_event'] = max(J.get('_panel_on_event', 0.0), max(abs(x - y) for x, y in zip(box, a['panels'][p])))
            continue
        for p, box in b['panels'].items():
            if p in a['panels']: bump('panel', max(abs(x - y) for x, y in zip(box, a['panels'][p])), p)
        prev = {c['key']: c for c in a['calls']}
        for c in b['calls']:
            q = prev.get(c['key'])
            if not q: continue
            bump('row_h', abs(c['box'][3] - q['box'][3]), c['key'])
            if c['argsX'] is not None and q['argsX'] is not None: bump('args_x', abs(c['argsX'] - q['argsX']), c['key'])
            if c['durR'] is not None and q['durR'] is not None: bump('dur_right', abs(c['durR'] - q['durR']), c['key'])
            if c['durX'] is not None and q['durX'] is not None: bump('dur_left', abs(c['durX'] - q['durX']), c['key'])
            if c['argsW'] is not None and q['argsW'] is not None: bump('args_w', abs(c['argsW'] - q['argsW']), c['key'])
        pc = {r['key']: r for r in a.get('cells', [])}
        for r in b.get('cells', []):
            q = pc.get(r['key'])
            if not q: continue
            bump('row_h', abs(r['h'] - q['h']), 'renders ' + str(r['key']))
            if len(r['xs']) == len(q['xs']): bump('renders_cells', max([abs(x - y) for x, y in zip(r['xs'], q['xs'])] or [0]), str(r['key']))
            if r['th'] and q['th']: bump('thumb_box', max(abs(x - y) for x, y in zip(r['th'], q['th'])), str(r['key']))
        if len(a.get('meta', [])) == len(b.get('meta', [])): bump('meta_x', max([abs(x - y) for x, y in zip(a['meta'], b['meta']) if x is not None and y is not None] or [0]), 'vmeta')
        if a.get('bar') and b.get('bar'): bump('bar', max(abs(x - y) for x, y in zip(a['bar'], b['bar'])), 'progress bar')
        pk = {r['key']: r['h'] for r in a['rows']}
        for r in b['rows']:
            if r['key'] in pk: bump('kv_h', abs(r['h'] - pk[r['key']]), r['key'])
        pt = {t['key']: t['h'] for t in a['trks']}
        for t in b['trks']:
            if t['key'] in pt: bump('track_h', abs(t['h'] - pt[t['key']]), t['key'])
    return J, where

async def run(evfiles):
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html'
    scen = [s['id'] for s in json.loads((CRATE / 'demo' / 'scenarios.json').read_text())]
    worst = 0.0; out = {}
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
        async def one(sid):
            await pg.goto(f'{base}?feed=sim&scenario={sid}', wait_until='domcontentloaded', timeout=8000); await pg.wait_for_timeout(300)
            S = []
            for _ in range(40):
                S.append(await pg.evaluate(SAMPLE_JS)); await pg.wait_for_timeout(50)
            return S
        for sid in scen:
            try: out[sid] = diff(await asyncio.wait_for(one(sid), 25)); print('.', end='', flush=True)
            except Exception as e:
                print(f'\n{sid}: no measurement ({type(e).__name__})', flush=True); pg = await b.new_page(viewport={'width': 1376, 'height': 768}); pg.on('pageerror', lambda e: errs.append(str(e)))
        for f in evfiles:
            evs = json.loads(Path(f).read_text())
            await pg.goto(f'{base}?feed=off'); await pg.wait_for_timeout(400)
            S = [await pg.evaluate(SAMPLE_JS)]
            for e in evs:
                if 'sleep' in e: continue
                await pg.evaluate('e => __kimo.apply(e)', e); await pg.wait_for_timeout(60); S.append(await pg.evaluate(SAMPLE_JS))
            out[Path(f).name] = diff(S)
        # live text stream: an export job reporting at 20/s (frame / fps / eta / pct changing every event), its first
        # engine frame landing mid-stream (thumb replaces the glyph in place), and a running call whose ticking
        # duration crosses 9.9 s -> 10.0 s; every sample here is measured (no call starts or settles inside it)
        import random; rnd = random.Random(7)
        await pg.goto(f'{base}?feed=off'); await pg.wait_for_timeout(400)
        for e in [{'type': 'session', 'width': 1920, 'height': 1080},
                  {'tool_name': 'export', 'call_id': 'j1', 'phase': 'start', 'target': {}, 'payload': {'args': {'path': '/home/n/films/deliveries/final/dune_master_v3.mp4', 'width': 1920, 'height': 1080, 'codec': 'h264', 'crf': 18}}},
                  {'tool_name': 'export', 'call_id': 'j1', 'phase': 'progress', 'target': {}, 'payload': {'current_frame': 1, 'total_frames': 2160, 'fps': 9, 'pct': 0, 'eta_ms': 240000}}]:
            await pg.evaluate('e => __kimo.apply(e)', e)
        await pg.evaluate("() => __kimo.apply({type: 'tool_start', id: 'long', name: 'render_proxy', args: {clip: 'street_b.mp4'}, elapsed_ms: 9300})")
        await pg.wait_for_timeout(150)
        S = [await pg.evaluate(SAMPLE_JS)]; t_start = asyncio.get_event_loop().time(); f = 1
        TINY = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=='
        for k in range(60):
            f = min(2160, f + rnd.randint(5, 90)); fps = rnd.choice([3, 9, 12, 41.7, 118, 240.5]); eta = int((2160 - f) / max(fps, 1) * 1000)
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'j1', 'phase': 'progress', 'target': {}, 'payload': {'current_frame': f, 'total_frames': 2160, 'fps': fps, 'pct': 100 * f / 2160, 'elapsed_ms': k * 50, 'eta_ms': eta}})
            if k == 20: await pg.evaluate('e => __kimo.apply(e)', {'type': 'frame_rendered', 'png_b64': TINY})
            await pg.wait_for_timeout(50); S.append(await pg.evaluate(SAMPLE_JS))
        dt = asyncio.get_event_loop().time() - t_start
        mode_sw = [x['mode'] for x in S]
        J, where = diff(S); out[f'live text stream ({len(S)} samples / {dt:.1f} s)'] = (J, where)
        # rail pick while calls stream: a call lands every ~25 ms (start, then complete with the engine's `changed`
        # diff), and every 4th call a rail row is clicked (pick -> the Inspector shows that call) or Esc returns.
        # Sampled right before and right after each click/Esc with no event between them: the click must not move
        # any panel or rail row (box, args x/width, duration edges) — the Inspector's own rows are the content swap.
        await pg.goto(f'{base}?feed=off'); await pg.wait_for_timeout(400)
        await pg.evaluate('e => __kimo.apply(e)', {'type': 'session', 'width': 1920, 'height': 1080})
        RAF2 = '() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))'
        PK = {'panel': 0.0, 'row_box': 0.0, 'args_x': 0.0, 'args_w': 0.0, 'dur_left': 0.0, 'dur_right': 0.0}; pw = {}; picks = 0
        def pbump(k, v, w):
            if v > PK[k]: PK[k] = v; pw[k] = w
        t_start = asyncio.get_event_loop().time()
        for k in range(120):
            cid = f'p{k}'; v0, v1 = round(k * .01, 2), round(k * .01 + .05, 2)
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': 'set_effect_param', 'call_id': cid, 'phase': 'start', 'target': {'clip_id': 'c1', 'param': 'amount'}, 'payload': {'args': {'clip_id': 'c1', 'effect_id': 'fx1', 'param': 'amount', 'value': v1}}})
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': 'set_effect_param', 'call_id': cid, 'phase': 'complete', 'target': {'clip_id': 'c1', 'param': 'amount'}, 'payload': {'dur_ms': 0.8 + k % 7, 'result': {'ok': True}, 'changed': {'clip': {'effects[fx1].params.amount.base': [v0, v1]}}, 'from': v0, 'to': v1, 'param': 'amount'}})
            await pg.wait_for_timeout(25)
            if k % 4 != 3: continue
            await pg.evaluate(RAF2); a = await pg.evaluate(SAMPLE_JS)
            if (k // 4) % 2 == 0: await pg.evaluate("() => { const r = document.querySelectorAll('.call[data-call]'); r[Math.max(0, r.length - 3)].click(); }")
            else: await pg.keyboard.press('Escape')
            await pg.evaluate(RAF2); b2 = await pg.evaluate(SAMPLE_JS); picks += 1
            if a['sig'] != b2['sig'] or a['mode'] != b2['mode']: continue
            for p, box in b2['panels'].items():
                if p in a['panels']: pbump('panel', max(abs(x - y) for x, y in zip(box, a['panels'][p])), p)
            prev = {c['key']: c for c in a['calls']}
            for c in b2['calls']:
                q = prev.get(c['key'])
                if not q: continue
                pbump('row_box', max(abs(x - y) for x, y in zip(c['box'], q['box'])), c['key'])
                for n, f2 in (('args_x', 'argsX'), ('args_w', 'argsW'), ('dur_left', 'durX'), ('dur_right', 'durR')):
                    if c[f2] is not None and q[f2] is not None: pbump(n, abs(c[f2] - q[f2]), c['key'])
        npick = await pg.evaluate('__kimo.state.calls.filter(c => c.diff).length')
        out[f'rail pick/Esc while streaming ({picks} clicks, {npick} calls w/ diff, {asyncio.get_event_loop().time() - t_start:.1f} s)'] = (PK, pw)
        await b.close()
    srv.shutdown()
    print()
    for k, (J, where) in out.items():
        m = max(v for n, v in J.items() if not n.startswith('_')); worst = max(worst, m)
        print(f"{'PASS' if m <= 0.5 else 'JITTER'}  {k:<16} " + '  '.join(f'{n} {v:.1f}' for n, v in J.items() if not n.startswith('_')) + (f"   (panel move on a call event {J['_panel_on_event']:.1f})" if J.get('_panel_on_event') else '') + (f"   worst: {max(where.items(), key=lambda kv: J[kv[0]])}" if m > 0.5 else ''))
    if errs: print('page errors:', errs[:3])
    print(f"\nRESULT: {'no layout jitter' if worst <= 0.5 else f'jitter up to {worst:.1f} px'}")
    return 0 if worst <= 0.5 and not errs else 1

if __name__ == '__main__':
    sys.exit(asyncio.run(run(sys.argv[1:])))
