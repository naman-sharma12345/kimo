"""Robustness runner: calls every MCP tool with hostile args. Fails on crash, hang, or non-JSON reply.
Usage (from repo root): python3 tests/tool_fuzz.py   (KIMO_FUZZ_SMOKE=1 for the 4-calls-per-tool smoke, KIMO_BIN to pick the binary)"""
import json, os, subprocess, sys, select, time
EXE = os.environ.get("KIMO_BIN", "target/release/kimo-mcp")
SMOKE = os.environ.get("KIMO_FUZZ_SMOKE") == "1"   # fast tier: every tool still gets 4 hostile calls (fixed, no randomness), full tier runs everything
class S:
    def __init__(s):
        s.p = subprocess.Popen([EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True); s.i = 0; s.rpc("initialize", {})
    def rpc(s, m, params, to=60):
        s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": params}) + "\n"); s.p.stdin.flush()
        r, _, _ = select.select([s.p.stdout], [], [], to)
        if not r: raise TimeoutError("hang")
        line = s.p.stdout.readline()
        if not line: raise EOFError("server died")
        return json.loads(line)
SKIP = {"export", "export_gif", "project_set", "render_frame", "export_gif", "export_audio", "transcribe", "download_stock"}   # slow / network
def vals(sc):
    t = sc.get("type")
    if isinstance(t, list): t = next((x for x in t if x != "null"), None)
    if "enum" in sc: return [sc["enum"][0]]
    return {"number": [0, -1, 1e12, 0.5], "integer": [0, -1, 2**40], "string": ["", "nope", "x" * 5000], "boolean": [True, False], "array": [[], [1, 2, 3], ["a"]], "object": [{}, {"a": 1}]}.get(t, [None, 0, "x"])
def main():
    s = S(); tools = s.rpc("tools/list", {"mode": "flat"})["result"]["tools"]; bad = []; n = 0
    def ok(n, **a):
        r = s.rpc("tools/call", {"name": n, "arguments": a}); return json.loads(r["result"]["content"][0]["text"])
    ids = {}
    try:
        ok("project_set", width=320, height=180, fps=24)
        ids["asset_id"] = ok("import_media", path="/tmp/bgB.png")["asset_id"]
        ids["track_id"] = ok("add_track")["track_id"]
        c = ok("add_clip", track_id=ids["track_id"], source_type="media", asset_id=ids["asset_id"], start=0, duration=2)
        ids["clip_id"] = c.get("clip_id") or c.get("id")
    except Exception as e:
        print("setup failed (running without real ids):", e)
    def call(name, args):
        nonlocal s, n; n += 1
        try:
            r = s.rpc("tools/call", {"name": name, "arguments": args})
            if "result" not in r and "error" not in r: bad.append((name, args, "malformed"))
        except Exception as e:
            bad.append((name, str(args)[:120], repr(e))); s = S()
    for t in tools:
        name = t["name"]
        if name in SKIP: continue
        props = t.get("inputSchema", {}).get("properties", {})
        call(name, {})
        call(name, {k: None for k in props})
        call(name, {k: "nope" for k in props})
        call(name, {k: -1 for k in props})
        if SMOKE: continue
        for k, sc in props.items():
            for v in vals(sc): call(name, {k: v})
        real = {k: ids[k] for k in props if k in ids}
        if real:
            call(name, real)
            for k, sc in props.items():
                for v in vals(sc): call(name, {**real, k: v})
        for v in (1e12, -1e12, 0):
            call(name, {**real, **{k: v for k, sc in props.items() if sc.get("type") in ("number", "integer")}})
        for v in (1e12, -1e12, 0):
            call(name, {k: v for k, sc in props.items() if sc.get("type") in ("number", "integer")})
    # RV-11 batch: random op sequences. Invariant: a failed batch leaves the project byte-identical; a successful one is ONE undo step.
    import random
    rng = random.Random(11); pool = ["add_track", "add_shape", "set_blend", "delete_clip", "move_clip", "undo", "batch", "nope_tool", "set_property"]
    def proj():
        try: return json.dumps(ok("get_project"), sort_keys=True)
        except Exception: return None
    for k in range(40 if SMOKE else 200):
        ops = []
        for j in range(rng.randint(1, 12)):
            t = rng.choice(pool); ref = f"$o{rng.randint(0, 14)}.clip_id"
            a = {"add_shape": {"track_id": "$o0.track_id", "shape": "rect", "width": 9, "height": 9, "start": 0.0, "duration": 1.0}, "set_blend": {"clip_id": ref, "mode": rng.choice(["add", "bogus"])}, "delete_clip": {"clip_id": ref}, "move_clip": {"clip_id": ref, "start": rng.choice([0.5, -3, "x"])}, "set_property": {"clip_id": ref, "property": "opacity", "value": rng.random()}}.get(t, {})
            ops.append({"id": f"o{j}", "tool": rng.choice(["add_track", t]) if j == 0 else t, "args": a})
        before = proj(); atomic = rng.random() < 0.8
        try:
            r = s.rpc("tools/call", {"name": "batch", "arguments": {"ops": ops, "atomic": atomic, "dry_run": rng.random() < 0.2}}); n += 1
            txt = r.get("result", {}).get("content", [{}])[0].get("text", ""); failed = r.get("result", {}).get("isError") or "error" in r
            if failed and atomic and proj() != before: bad.append(("batch", "random ops", "failed batch changed the project"))
        except Exception as e:
            bad.append(("batch", "random ops", repr(e))); s = S()
    print(f"{len(tools)} tools, {n} calls, {len(bad)} failures")
    for b in bad[:60]: print(" FAIL", b)
    sys.exit(1 if bad else 0)
main()
