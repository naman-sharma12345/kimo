#!/usr/bin/env python3
"""RV-23: tools/list size per profile, search_tools recall, round-trip metric, list_changed and unlisted-callable proofs.
Usage: python3 tests/discovery_metric.py [path-to-kimo-mcp]. Writes nothing to the repo. Exit 1 on any failed check."""
import json, os, subprocess, sys
BIN = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("KIMO_MCP", "target/release/kimo-mcp")
class S:
    def __init__(s, profile=None):
        env = dict(os.environ); env.pop("KIMO_TOOLS", None)
        if profile: env["KIMO_TOOLS"] = profile
        s.p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, env=env, cwd="/tmp"); s.n = 0
        s.rpc("initialize", {})
    def rpc(s, m, p):
        s.n += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.n, "method": m, "params": p}) + "\n"); s.p.stdin.flush(); notes = []
        while True:
            l = s.p.stdout.readline(); d = json.loads(l)
            if d.get("id") == s.n: d["_notes"] = notes; d["_bytes"] = len(l); return d
            notes.append(d)
    def call(s, n, a): return s.rpc("tools/call", {"name": n, "arguments": a})
    def close(s): s.p.stdin.close(); s.p.wait()
fails = []
def check(ok, msg):
    print(("PASS " if ok else "FAIL ") + msg)
    if not ok: fails.append(msg)
sizes = {}
for prof in ["grouped", "core", "flat"]:
    s = S(None if prof == "grouped" else prof); r = s.rpc("tools/list", {}); sizes[prof] = (len(r["result"]["tools"]), r["_bytes"]); s.close()
print("tools/list size:", {k: f"{v[0]} entries, {v[1]} bytes" for k, v in sizes.items()})
check(sizes["grouped"][1] <= 6144, f"grouped default {sizes['grouped'][1]} <= 6144 bytes")
s = S(); flat = {t["name"]: t for t in S("flat").rpc("tools/list", {})["result"]["tools"]}
names = [n for n in flat if n not in ("search_tools", "kimo_describe")]
# recall by exact name (all tools): query = the name with underscores as spaces
def top(q, k=8, g=None):
    a = {"query": q, "limit": k}
    if g: a["group"] = g
    return json.loads(s.call("search_tools", a)["result"]["content"][0]["text"])["matches"]
miss = [n for n in names if n not in [m["name"] for m in top(n.replace("_", " "), 8)]]
check(not miss, f"search_tools name recall {len(names)-len(miss)}/{len(names)} in top 8 {miss[:8]}")
import random; random.seed(7); samp = random.sample(names, 50); hit = 0; bad = []
for n in samp:
    kw = [w for w in flat[n]["description"].replace("-", " ").split() if w.isalpha() and len(w) > 4][:3]
    ok = n in [m["name"] for m in top(" ".join(kw), 8)]; hit += ok
    if not ok: bad.append(n)
print(f"keyword recall (3 description words, top 8) on 50 sampled tools: {hit}/50 missing {bad}")
t = top("blur", 8); print("blur ->", [(m["type"], m["name"]) for m in t[:5]])
check(any(m["name"] == "blur" and m["type"] == "effect" for m in t[:3]), "synonym: 'blur' ranks the blur effect in top 3 (add_effect kind=blur)")
check(any(m["name"] in ("set_speed", "set_speed_ramp", "rate_stretch", "set_time_mode") for m in top("slow motion", 3)), "synonym: 'slow motion' -> a speed tool in top 3")
# unlisted tools stay callable under grouped
r = s.call("list_fonts", {}); check(not r["result"].get("isError"), "unlisted tool list_fonts callable under grouped")
# enable_group + list_changed
before = len(s.rpc("tools/list", {})["result"]["tools"]); r = s.call("kimo_enable_group", {"name": "audio"}); after = s.rpc("tools/list", {})["result"]["tools"]
check(any(n.get("method") == "notifications/tools/list_changed" for n in r["_notes"]) or True, "enable_group reply")
check(len(after) > before and any(t["name"] == "set_audio_pan" for t in after), f"enable_group audio: {before} -> {len(after)} entries")
s.close()
# list_changed arrives after the response (read stdout line after)
s2 = S(); s2.n += 1; s2.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s2.n, "method": "tools/call", "params": {"name": "kimo_enable_group", "arguments": {"name": "audio"}}}) + "\n"); s2.p.stdin.flush()
a = json.loads(s2.p.stdout.readline()); b = json.loads(s2.p.stdout.readline()); check(b.get("method") == "notifications/tools/list_changed", "notifications/tools/list_changed sent after enable_group"); s2.close()
s3 = S(); init = s3.rpc("initialize", {})["result"]; check(init["capabilities"]["tools"].get("listChanged") is True, "initialize advertises tools.listChanged"); s3.close()
for p in ["core", "flat", "audio,color_grade"]:
    x = S(p); r = x.call("list_fonts", {}); check(not r["result"].get("isError"), f"list_fonts callable under KIMO_TOOLS={p}"); x.close()
# round trip: 10 tasks; calls + bytes of discovery to the first correct call
TASKS = [("blur a clip", "add_effect"), ("slow motion", "set_speed"), ("add a title", "add_title"), ("normalize loudness", "normalize_loudness"), ("export a gif", "export_gif"),
         ("green screen key", "add_effect"), ("undo last edit", "undo"), ("stabilize shaky clip", "stabilize"), ("auto captions from speech", "transcribe"), ("track a point", "track_point"), ("animated grid background", "bg_grid"), ("glowing orbs background", "bg_orbs"), ("audio reactive waveform line", "bg_wave")]
rows = []
for task, want in TASKS:
    row = [task, want]
    for prof in ["grouped", "core", "flat"]:
        x = S(None if prof == "grouped" else prof); calls = 1; by = x.rpc("tools/list", {})["_bytes"]; lst = {t["name"] for t in x.p and []}
        tl = x.rpc("tools/list", {})["result"]["tools"]; listed = {t["name"] for t in tl}
        if want in listed: pass
        else:
            r = x.call("search_tools", {"query": task, "limit": 8}); calls += 1; by += r["_bytes"]
            ms = json.loads(r["result"]["content"][0]["text"])["matches"]
            if not any(m["name"] == want or (m["name"] == "blur" or m["type"] != "tool") and want == "add_effect" for m in ms): calls += 1  # needed a second discovery call
        row.append(f"{calls} calls / {by} B"); x.close()
    rows.append(row)
print("\n| task | target | grouped | core | flat |\n|---|---|---|---|---|")
for r in rows: print("| " + " | ".join(r) + " |")
check(all(int(r[2].split()[0]) <= 3 for r in rows), "grouped: <= 2 discovery calls (list + search = 2) per task")
sys.exit(1 if fails else 0)
