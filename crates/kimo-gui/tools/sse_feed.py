#!/usr/bin/env python3
"""Zero-dependency localhost SSE feed for plain-browser spectator development.

Serves http://127.0.0.1:7422/events (text/event-stream). One producer plays the
16 demo scenarios (demo/scenarios.json, one per mockup) in a loop, or with
--events FILE a recorded engine event list (JSON array, e.g. a KIMO_SPECTATOR
file: capture) once at --rate events/s. Every event carries an `id:`; the feed
keeps the last --ring events, so a viewer that drops and comes back (the
browser's EventSource sends Last-Event-ID; the GUI's own reopen passes ?last=)
gets exactly what it missed. A fresh viewer starts at the current scenario.
--drop-at N closes the first viewer's stream once after N events, and --refuse-after-drop K
answers the next K reconnects 503 (reconnect tests).
Open http://127.0.0.1:8123/ui/index.html?feed=sse with the crate root served
on :8123 (python3 -m http.server 8123 in crates/kimo-gui).

    python3 tools/sse_feed.py [--scenario m05] [--hold 4] [--port 7422] [--events f.json --rate 40] [--drop-at N]

The Tauri shell never uses this transport (the engine has no replay buffer).
"""
import argparse, json, threading, time
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse
import scenarios

ap = argparse.ArgumentParser()
ap.add_argument("--port", type=int, default=7422)
ap.add_argument("--scenario")
ap.add_argument("--hold", type=float, default=4.0)
ap.add_argument("--events")
ap.add_argument("--rate", type=float, default=40.0)
ap.add_argument("--ring", type=int, default=4096)
ap.add_argument("--drop-at", type=int, default=0)
ap.add_argument("--retry-ms", type=int, default=300)
ap.add_argument("--no-replay", action="store_true")   # behave like the engine: a resumed viewer gets only what comes next
ap.add_argument("--refuse-after-drop", type=int, default=0)   # answer the next N reconnects 503 (the browser gives up; the GUI reopens)
A = ap.parse_args()

RING = deque(maxlen=A.ring)       # (seq, json line, starts_scenario)
COND = threading.Condition(); SEQ = [0]; STARTED = threading.Event(); DROPPED = [False]; REFUSE = [A.refuse_after_drop]

def put(ev, start=False):
    with COND:
        SEQ[0] += 1; RING.append((SEQ[0], json.dumps(ev), start)); COND.notify_all()

def produce():
    STARTED.wait()
    if A.events:
        for ev in json.load(open(A.events)):
            if "sleep" in ev: time.sleep(ev["sleep"]); continue
            put(ev); time.sleep(1 / A.rate)
        return
    while True:
        for s in scenarios.load():
            if A.scenario and s["id"] != A.scenario: continue
            first = True
            for ev, wait in scenarios.events(s):
                if wait: time.sleep(wait)
                put(ev, first); first = False
            time.sleep(A.hold)

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        u = urlparse(self.path)
        if u.path != "/events":
            self.send_response(404); self.end_headers(); return
        last = self.headers.get("Last-Event-ID") or (parse_qs(u.query).get("last") or [None])[0]
        if DROPPED[0] and REFUSE[0] > 0:
            REFUSE[0] -= 1; self.send_response(503); self.end_headers(); return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        with COND:
            if last is not None and last.isdigit(): nxt = SEQ[0] + 1 if A.no_replay else int(last) + 1   # resume: what it missed (engine-like: only what comes next)
            else:                                                             # fresh: the current scenario's start
                starts = [q for q, _, st in RING if st]; nxt = starts[-1] if starts else (RING[0][0] if RING else SEQ[0] + 1)
        STARTED.set()
        sent = 0; drop = A.drop_at and not DROPPED[0]
        try:
            self.wfile.write(f"retry: {A.retry_ms}\n\n".encode()); self.wfile.flush()
            while True:
                with COND:
                    batch = [(q, line) for q, line, _ in RING if q >= nxt]
                    if not batch: COND.wait(1.0)
                if not batch:
                    self.wfile.write(b": keep-alive\n\n"); self.wfile.flush(); continue
                for q, line in batch:
                    self.wfile.write(f"id: {q}\ndata: {line}\n\n".encode()); nxt = q + 1; sent += 1
                    if drop and sent >= A.drop_at:
                        DROPPED[0] = True; self.wfile.flush(); self.close_connection = True; return   # simulated mid-session drop
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

threading.Thread(target=produce, daemon=True).start()
print(f"kimo spectator SSE feed on http://127.0.0.1:{A.port}/events", flush=True)
ThreadingHTTPServer(("127.0.0.1", A.port), H).serve_forever()
