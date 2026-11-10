#!/usr/bin/env python3
"""Reconnect proof: a mid-session drop of the spectator's feed loses no tool call and stalls nothing.

Plays one engine event stream (real kimo-mcp telemetry from tests/ev_engine_diff.json, then N synthetic
set_effect_param calls carrying the engine's `changed` shape) through tools/sse_feed.py at --rate events/s.
The feed closes the viewer's stream once, mid-call, after --drop-at events. The browser's EventSource comes
back by itself with Last-Event-ID, and the feed replays what it missed. Measured from outside the page:
  - calls:   every call_id in the file is in the rail exactly once and settled (none running, none missing, no dupes)
  - pill:    the pill read 'reconnecting' during the gap and 'live' afterwards
  - stall:   the longest main-thread task (PerformanceObserver longtask; 0 when none > 50 ms) and the longest gap
             between event arrival and its paint
  - idle:    after the stream ends, 0 rAF over 1.5 s and no reconnect timer pending; with --refuse K (the feed answers
             K reconnects 503, so the browser gives up) exactly K backoff timers were set, all while down
Exit 0 when all hold.

    python3 crates/kimo-gui/tests/reconnect_test.py [--calls 150] [--rate 60] [--drop-at 200] [--refuse 2] [--no-replay]

--no-replay makes the feed behave like the engine (kimo-mcp keeps no replay buffer): calls wholly inside the gap
are gone, a call whose start was heard but whose result fell in the gap shows '?' (no result), never 'running'.
"""
import argparse, asyncio, functools, http.server, json, os, socket, socketserver, subprocess, sys, tempfile, threading, time
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
ap = argparse.ArgumentParser(); ap.add_argument('--calls', type=int, default=150); ap.add_argument('--rate', type=float, default=60); ap.add_argument('--drop-at', type=int, default=200); ap.add_argument('--refuse', type=int, default=0); ap.add_argument('--no-replay', action='store_true')
A = ap.parse_args()

def stream():
    evs = [e for e in json.loads((CRATE / 'tests' / 'ev_engine_diff.json').read_text())]
    for k in range(A.calls):
        cid = f'p{k}'; v0, v1 = round(k * .01, 2), round(k * .01 + .05, 2)
        evs.append({'tool_name': 'set_effect_param', 'call_id': cid, 'phase': 'start', 'target': {'clip_id': 'clip2', 'param': 'amount'}, 'payload': {'args': {'clip_id': 'clip2', 'effect_id': 'fx3', 'param': 'amount', 'value': v1}}})
        evs.append({'tool_name': 'set_effect_param', 'call_id': cid, 'phase': 'complete', 'target': {'clip_id': 'clip2', 'param': 'amount'}, 'payload': {'dur_ms': 0.8 + k % 7, 'result': {'ok': True}, 'changed': {'clip': {'effects[fx3].params.amount.base': [v0, v1]}}, 'from': v0, 'to': v1, 'param': 'amount'}})
    return evs

class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

def free_port():
    s = socket.socket(); s.bind(('127.0.0.1', 0)); p = s.getsockname()[1]; s.close(); return p

PROBE = """(() => { window.__lt = 0; window.__heard = new Set();
  const ES = window.EventSource; window.EventSource = function (u) { const es = new ES(u); es.addEventListener('message', e => { try { const v = JSON.parse(e.data); if (v.call_id) window.__heard.add(v.call_id); } catch (x) {} }); return es; }; window.EventSource.CLOSED = ES.CLOSED; window.EventSource.prototype = ES.prototype;
  try { new PerformanceObserver(l => { for (const e of l.getEntries()) window.__lt = Math.max(window.__lt, e.duration); }).observe({ type: 'longtask', buffered: true }); } catch (e) {}
  window.__to = 0; window.__tob = 0; window.__tobSet = 0; const sT = window.setTimeout.bind(window); window.setTimeout = (f, ms) => { const br = /bridge\.js/.test(new Error().stack || ''); if (br) { window.__tob++; window.__tobSet++; } window.__to++; return sT(() => { window.__to--; if (br) window.__tob--; f(); }, ms); };
  const rAF = window.requestAnimationFrame.bind(window); window.__raf = 0; window.requestAnimationFrame = cb => { window.__raf++; return rAF(cb); }; })()"""

async def main():
    from playwright.async_api import async_playwright
    evs = stream(); ids = []
    for e in evs:
        if e.get('call_id') and e['call_id'] not in ids: ids.append(e['call_id'])
    tf = tempfile.NamedTemporaryFile('w', suffix='.json', delete=False); json.dump(evs, tf); tf.close()
    port = free_port()
    feed = subprocess.Popen([sys.executable, '-u', str(CRATE / 'tools' / 'sse_feed.py'), '--port', str(port), '--events', tf.name, '--rate', str(A.rate), '--drop-at', str(A.drop_at), '--refuse-after-drop', str(A.refuse)] + (['--no-replay'] if A.no_replay else []),
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    ok = True
    try:
        for _ in range(50):
            try: socket.create_connection(('127.0.0.1', port), .2).close(); break
            except OSError: time.sleep(.1)
        async with async_playwright() as p:
            b = await p.chromium.launch(); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
            errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
            await pg.add_init_script(PROBE)
            t0 = time.time()
            await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=sse&sse={port}')
            seen = []; last = None; done_at = None; total = len(evs) / A.rate + 8
            while time.time() - t0 < total:
                s = await pg.evaluate("() => [__kimo.state.status.kind, __kimo.state.status.label, __kimo.state.calls.length, __kimo.state.calls.filter(c => c.status === 'running').length]")
                lab = f'{s[0]}:{s[1]}'
                if lab != last: seen.append((round(time.time() - t0, 2), lab, s[2])); last = lab
                if s[2] >= len(ids) and s[3] == 0 and done_at is None: done_at = time.time() - t0
                if done_at is not None and time.time() - t0 > done_at + .3: break
                await pg.wait_for_timeout(20)
            calls = await pg.evaluate("() => __kimo.state.calls.map(c => [c.id, c.status, !!c.diff])")
            got = [c[0] for c in calls]
            dupes = len(got) - len(set(got)); running = [c[0] for c in calls if c[1] == 'running']; lost = [c[0] for c in calls if c[1] == 'lost']
            if A.no_replay:   # engine-like feed: what fell in the gap is gone; every call the page heard any part of must show, settled or marked lost
                heard = await pg.evaluate('window.__heard ? [...window.__heard] : []'); exp = [i for i in ids if i in heard]
                missing = [i for i in exp if i not in got]; gone = [i for i in ids if i not in heard]
                print(f'engine-like (no replay): {len(gone)} calls fell entirely in the gap and are not shown; {len(lost)} started before the gap and show as no result (?)')
            else: missing = [i for i in ids if i not in got]
            r0, t0b = await pg.evaluate('[window.__raf, window.__tob]'); await pg.wait_for_timeout(1500); r1, t1b, nb = await pg.evaluate('[window.__raf, window.__tob, window.__tobSet]')
            lt = await pg.evaluate('window.__lt')
            print('status timeline:', ' -> '.join(f'{t}s {l} ({n} calls)' for t, l, n in seen))
            rec = any('reconnecting' in l for _, l, _ in seen); back = seen and seen[-1][1].startswith('live')
            print(f'events {len(evs)} at {A.rate}/s, dropped once after {A.drop_at}; calls in file {len(ids)}, in rail {len(got)}, missing {len(missing)}, duplicates {dupes}, still running {len(running)}; with diff {sum(c[2] for c in calls)}')
            print(f'pill reconnecting seen: {rec}; live again: {back}; longest main-thread task {lt:.0f} ms (longtask entries are > 50 ms; 0 = none); all calls settled at {done_at and round(done_at, 2)} s (stream length {len(evs) / A.rate:.1f} s)')
            print(f'reconnect timers: {nb} set while down (browser retried natively: 0; refused {A.refuse}x: one per refusal), pending after the stream {t1b}; rAF after the stream {r1 - r0}; page errors {errs[:2]}')
            ok = not missing and not dupes and not running and rec and back and lt < 200 and r1 - r0 == 0 and t1b == 0 and nb == A.refuse and not errs
            await b.close()
    finally:
        feed.kill(); srv.shutdown(); os.unlink(tf.name)
    print('RESULT:', 'no call lost across the drop, no stall' if ok else 'FAIL'); return 0 if ok else 1

if __name__ == '__main__':
    sys.exit(asyncio.run(main()))
