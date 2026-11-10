#!/usr/bin/env python3
"""Stream the 16 demo scenarios to the Tauri shell over loopback TCP/JSONL
(127.0.0.1:7421), exactly as an engine would. Frames are inlined as data:
URLs because the shell serves only ui/.

    python3 tools/feed_demo.py [--scenario m16] [--hold 4] [--loop]
"""
import argparse, json, socket, time
import scenarios

ap = argparse.ArgumentParser()
ap.add_argument("--host", default="127.0.0.1")
ap.add_argument("--port", type=int, default=7421)
ap.add_argument("--scenario")
ap.add_argument("--hold", type=float, default=4.0)
ap.add_argument("--loop", action="store_true")
A = ap.parse_args()

with socket.create_connection((A.host, A.port)) as sock:
    while True:
        for s in scenarios.load():
            if A.scenario and s["id"] != A.scenario: continue
            for ev, wait in scenarios.events(s, inline=True):
                if wait: time.sleep(wait)
                sock.sendall((json.dumps(ev) + "\n").encode())
            time.sleep(A.hold)
        if not A.loop: break
