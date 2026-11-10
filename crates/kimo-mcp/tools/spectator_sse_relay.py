#!/usr/bin/env python3
"""Watch the real engine in a plain browser: kimo-mcp telemetry (TCP JSON lines) -> SSE for the spectator UI.

The Tauri shell listens on 127.0.0.1:7421 itself (the first free port of 7421-7430). Without the shell, run this relay
in its place and open the UI with ?feed=sse; it greets kimo-mcp with the same hello line, so the default finds it:

    python3 crates/kimo-mcp/tools/spectator_sse_relay.py            # TCP :7421 in, SSE :7422 out
    kimo-mcp                                                         # streams to :7421 by default; any MCP client drives it
    open http://127.0.0.1:8123/ui/index.html?feed=sse                # crates/kimo-gui served on :8123

Stdlib only, loopback only. Events that arrive before a browser connects are kept (last 2000) and replayed to it.
"""
import argparse, collections, socket, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ap = argparse.ArgumentParser()
ap.add_argument("--tcp", type=int, default=7421, help="port kimo-mcp connects to (KIMO_SPECTATOR)")
ap.add_argument("--sse", type=int, default=7422, help="port the UI reads (?sse=PORT)")
A = ap.parse_args()

backlog = collections.deque(maxlen=2000)
total = [0]                      # events ever received; each client resumes from its own count
lock = threading.Condition()

def publish(line):
    with lock:
        backlog.append(line); total[0] += 1; lock.notify_all()

def engine_loop():
    srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", A.tcp)); srv.listen(4)
    while True:
        conn, _ = srv.accept()
        try: conn.sendall(b"KIMO-SPECTATOR/1\n")  # the hello kimo-mcp's default sink waits for (spectator.rs HELLO)
        except OSError: conn.close(); continue
        threading.Thread(target=read_engine, args=(conn,), daemon=True).start()

def read_engine(conn):
    with conn, conn.makefile("r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if line: publish(line)

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path.split("?")[0] != "/events":
            self.send_response(404); self.end_headers(); return
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        sent = 0
        try:
            self.wfile.write(b": kimo relay\n\n"); self.wfile.flush()
            while True:
                with lock:
                    base = total[0] - len(backlog)
                    if sent < base: sent = base
                    if sent >= total[0]: lock.wait(15)
                    pending = list(backlog)[sent - base:] if total[0] > sent else []
                if not pending:
                    self.wfile.write(b": keep-alive\n\n"); self.wfile.flush(); continue
                for line in pending:
                    self.wfile.write(f"data: {line}\n\n".encode())
                sent += len(pending); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


threading.Thread(target=engine_loop, daemon=True).start()
print(f"kimo spectator relay: engine tcp://127.0.0.1:{A.tcp} -> sse http://127.0.0.1:{A.sse}/events", flush=True)
ThreadingHTTPServer(("127.0.0.1", A.sse), H).serve_forever()
