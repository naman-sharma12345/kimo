#!/usr/bin/env python3
"""Keyframes from a REAL k5 session (tests/ev_engine_keys.json): set_keyframe scale 0s / 2s, x 0s / 2s, x 2s again
(180 -> 240), set_keyframe_bezier x 0s [.42 0 .58 1] and scale 0s [.1 .9 .2 1], a real refusal (clip999), undo, redo.
The engine's payload carries the whole key list before and after ({t, v, ease, bez}); the rail used to print it as
"x: → ," and the Graph drew nothing after a bezier key.

Asserts, per call:
  - the rail row's title reads the one key that changed, "<prop> @<t>s: <before> → <after>", values computed here from
    the engine's own key lists (before = the curve's value there, or "none" when the curve had no key); the visible
    text shows both values and never an empty side of an arrow ("x: →", "→ ,", "→)")
  - the Inspector carries the same change (value row "from → to", or the ease row for a bezier key)
  - set_keyframe_bezier: the Graph (m03) draws the curve the engine now holds, evaluated as a CSS cubic-bezier between
    the keys: every drawn sample matches an independent evaluation here (|dv| < 1e-6 of the value range), the curve is
    not the straight line it was before, the two handles sit at the control points, and the old curve is dashed under it
  - the refused call: an error row, no key row, Graph and Inspector byte-identical
Exit 0 when all hold.
"""
import asyncio, functools, http.server, json, re, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads((CRATE / 'tests' / 'ev_engine_keys.json').read_text())['events']
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
fails = []; checks = [0]
def ok(c, m):
    checks[0] += 1
    if not c: fails.append(m); print('FAIL', m)
def fmt(v, p):
    if p in ('scale', 'opacity'): return f"{round(v * 100, 6):g}%"
    return f"{round(v, 3):g}"
def at(keys, t):
    ks = sorted(keys, key=lambda k: k['t'])
    if not ks: return None
    if t <= ks[0]['t']: return ks[0]['v']
    for a, b in zip(ks, ks[1:]):
        if t <= b['t']: return a['v'] + (b['v'] - a['v']) * (t - a['t']) / (b['t'] - a['t']) if b['t'] > a['t'] else b['v']
    return ks[-1]['v']
def cubic(b, x):   # CSS cubic-bezier y(x), by bisection only (independent of the GUI's Newton)
    x1, y1, x2, y2 = b
    B = lambda p, q, s: 3 * p * s * (1 - s) ** 2 + 3 * q * s * s * (1 - s) + s ** 3
    lo, hi = 0.0, 1.0
    for _ in range(80):
        m = (lo + hi) / 2
        if B(x1, x2, m) < x: lo = m
        else: hi = m
    return B(y1, y2, (lo + hi) / 2)
EASE = {'0.25 0.1 0.25 1': 'ease', '0.42 0 1 1': 'ease-in', '0 0 0.58 1': 'ease-out', '0.42 0 0.58 1': 'ease-in-out'}
def ease_name(k):
    if not k.get('bez'): return k.get('ease') or 'linear'
    s = ' '.join(f'{v:g}' for v in k['bez'])
    return EASE.get(s) or 'bezier ' + re.sub(r'(^| )0\.', r'\1.', s)
def expect(e):
    P, p = e['payload'], e['target']['param']; F, T = P['from'], P['to']
    for t in sorted({k['t'] for k in F + T}):
        a = next((k for k in F if abs(k['t'] - t) < 1e-9), None); b = next((k for k in T if abs(k['t'] - t) < 1e-9), None)
        if a and b and a['v'] == b['v'] and ease_name(a) == ease_name(b): continue
        if a and b and a['v'] == b['v']: return f"{p} @{t:g}s ease", ease_name(a), ease_name(b), t
        was = a['v'] if a else at(F, t); now = b['v'] if b else at(T, t)
        return f"{p} @{t:g}s", 'none' if was is None else fmt(was, p), 'none' if now is None else fmt(now, p), t
SNAP = """() => { const row = [...document.querySelectorAll('[data-panel=agent] .call')].pop();
  const P = n => { const el = document.querySelector(`[data-panel=${n}]`); return el ? el.innerHTML : null; };
  const st = window.__kimo.state, gv = (st.views.graph || {}).graph || {};
  return { mode: st.mode, title: row ? (row.querySelector('.sig') || {}).title || '' : '', text: row ? row.textContent : '', cls: row ? row.className : '',
    insp: (document.querySelector('[data-panel=insp]') || {}).innerText || '', inspHTML: P('insp'), graphHTML: P('graph'),
    cur: (gv.curves || []).filter(c => c.cls === 'kg-cur').map(c => ({ raw: c._raw, handles: c._handles, n: c.pts.length })),
    prev: (gv.curves || []).filter(c => c.cls === 'kg-prev').map(c => ({ dash: c.dash, n: c.pts.length })),
    paths: [...document.querySelectorAll('[data-panel=graph] path.kg-cur')].map(p => (p.getAttribute('d').match(/L/g) || []).length) }; }"""
async def settle(pg): await pg.evaluate('() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))')
async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    async with async_playwright() as p:
        b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        msgs = []; pg.on('console', lambda m: msgs.append(m.text) if m.type in ('warning', 'error') else None); pg.on('pageerror', lambda e: msgs.append(str(e)))
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await pg.wait_for_function('window.__kimo'); await settle(pg)
        seen = 0
        for e in EV:
            before = await pg.evaluate(SNAP)
            await pg.evaluate('e => window.__kimo.apply(e)', e); await settle(pg)
            if not str(e.get('tool_name', '')).startswith('set_keyframe') or e.get('phase') not in ('complete', 'error'): continue
            s = await pg.evaluate(SNAP); tn = e['tool_name']
            vis = s['text']
            ok(not re.search(r':\s*→|→\s*[,)]|→\s*$', vis), f'{tn}: blank side of an arrow on the rail: {vis!r}')
            if e['phase'] == 'error':
                ok('error' in s['cls'] or 'err' in s['cls'], f'{tn} refused: rail row not an error ({s["cls"]})')
                ok(s['inspHTML'] == before['inspHTML'] and s['graphHTML'] == before['graphHTML'], f'{tn} refused: Inspector / Graph changed')
                continue
            seen += 1
            lab, fr, to, t = expect(e)
            want = f'{lab}: {fr} → {to}'
            ok(want in s['title'], f'{tn}: rail title {s["title"]!r} lacks {want!r}')
            u = re.search(r'[^\d.]*$', to).group(0); fr2 = fr[:-len(u)] if u and fr.endswith(u) else fr   # the tight form keeps one unit: "100→125%"
            ok(any(f'{x}→{to}' in vis.replace(' → ', '→') for x in (fr, fr2)) or '(…)' in vis, f'{tn}: rail shows {vis!r}, not both values of {want!r}')
            if '(…)' in vis: ok(s['mode'] == 'graph', f'{tn}: only the Graph screen\'s long tool name may fold the args ({vis!r})')
            ok(fr in s['insp'] and to in s['insp'], f'{tn}: Inspector lacks {fr} / {to}: {s["insp"][-160:]!r}')
            if tn == 'set_keyframe_bezier':
                ok(s['mode'] == 'graph', f'{tn}: screen {s["mode"]}, want graph')
                ok(len(s['cur']) == 1 and len(s['prev']) == 1 and s['prev'][0]['dash'], f'{tn}: curves {s["cur"]} / prev {s["prev"]}')
                ok(s['paths'] and s['paths'][0] >= 20, f'{tn}: drawn path has {s["paths"]} segments (a straight line has 1)')
                T_ = sorted(e['payload']['to'], key=lambda k: k['t']); a, bk = T_[0], T_[1]
                rng = abs(bk['v'] - a['v']) or 1
                raw = s['cur'][0]['raw'] if s['cur'] else []
                worst = 0; dev = 0
                for tt, vv in raw:
                    if a['t'] <= tt <= bk['t']:
                        x = (tt - a['t']) / (bk['t'] - a['t']); ev_ = a['v'] + cubic(a['bez'], x) * (bk['v'] - a['v'])
                        worst = max(worst, abs(vv - ev_) / rng); dev = max(dev, abs(vv - (a['v'] + x * (bk['v'] - a['v']))) / rng)
                ok(len(raw) >= 20 and worst < 1e-6, f'{tn}: drawn samples vs cubic-bezier: {len(raw)} samples, worst {worst:.2e}')
                ok(dev > 0.02, f'{tn}: curve is still the straight line (max deviation {dev:.3f})')
                h = s['cur'][0]['handles'] if s['cur'] else []
                x1, y1, x2, y2 = a['bez']; dt, dv = bk['t'] - a['t'], bk['v'] - a['v']
                hw = [[[a['t'], a['v']], [a['t'] + x1 * dt, a['v'] + y1 * dv]], [[a['t'] + x2 * dt, a['v'] + y2 * dv], [bk['t'], bk['v']]]]
                ok(len(h) == 2 and all(abs(h[i][j][k] - hw[i][j][k]) < 1e-9 for i in range(2) for j in range(2) for k in range(2)), f'{tn}: handles {h} != {hw}')
        ok(seen == 7, f'{seen} key calls checked, want 7')
        ok(not msgs, f'console: {msgs[:3]}')
        await b.close()
    srv.shutdown()
    print(f'keyframe: {checks[0]} checks, {len(fails)} fail')
    sys.exit(1 if fails else 0)
asyncio.run(main())
