"""Report-only: for each tool, call it with required args only, then with one optional arg set to a non-default value.
If neither the result nor the project JSON changes, the arg is a candidate dead arg. Tools whose base call errors are 'not exercised'.
Usage: python3 tests/dead_arg_report.py [tool ...]   (writes nothing in the repo)"""
import json, subprocess, sys, select
EXE = "target/release/kimo-mcp"
class S:
    def __init__(s):
        s.p = subprocess.Popen([EXE], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True); s.i = 0; s.rpc("initialize", {})
    def rpc(s, m, params, to=30):
        s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": params}) + "\n"); s.p.stdin.flush()
        r, _, _ = select.select([s.p.stdout], [], [], to)
        if not r: raise TimeoutError("hang")
        return json.loads(s.p.stdout.readline())
    def call(s, n, **a):
        r = s.rpc("tools/call", {"name": n, "arguments": a})["result"]; t = r["content"][0]["text"]
        try: v = json.loads(t)
        except Exception: v = t
        return r.get("isError", False), v
    def close(s): s.p.kill()
SKIP = {"export", "export_gif", "project_set", "render_frame", "export_audio", "transcribe", "download_stock", "render_scope", "undo", "redo"}
def setup(s):
    ids = {}
    s.call("project_set", width=320, height=180, fps=24)
    ids["asset_id"] = s.call("import_media", path="/tmp/bgB.png")[1]["asset_id"]
    ids["track_id"] = s.call("add_track")[1]["track_id"]
    c = s.call("add_clip", track_id=ids["track_id"], source_type="media", asset_id=ids["asset_id"], start=0, duration=2)[1]
    ids["clip_id"] = c.get("clip_id")
    return ids
def val(k, sc, ids):
    if k in ids: return ids[k]
    if "path" in k or k.endswith("_file") or k == "file": return "/tmp/dead_arg_out"
    if "enum" in sc: return sc["enum"][0]
    t = sc.get("type"); t = next((x for x in t if x != "null"), None) if isinstance(t, list) else t
    return {"number": 0.5, "integer": 1, "string": "x", "boolean": True, "array": [], "object": {}}.get(t, 0.5)
def alt(sc, base):
    if "enum" in sc: return [e for e in sc["enum"] if e != base][:1] or None
    t = sc.get("type")
    if t == "number" or t == "integer": return [base * 2 + 1 if isinstance(base, (int, float)) else 3]
    if t == "boolean": return [not base] if isinstance(base, bool) else [True, False]
    if t == "string": return ["alt"]
    return None
def main():
    only = set(sys.argv[1:]); s = S(); tools = s.rpc("tools/list", {"mode": "flat"})["result"]["tools"]; s.close()
    exer = dead = 0; rows = []
    for t in tools:
        n = t["name"]
        if n in SKIP or (only and n not in only): continue
        sc = t.get("inputSchema", {}); props = sc.get("properties", {}); req = sc.get("required", [])
        def run(extra):
            s = S()
            try:
                ids = setup(s); a = {k: val(k, props[k], ids) for k in req}; a.update({k: v for k, v in extra.items()})
                err, res = s.call(n, **a); proj = s.call("get_project")[1]; return err, res, proj
            except Exception as e: return True, repr(e), None
            finally: s.close()
        base = run({})
        if base[0]: rows.append((n, "(not exercised)", "base call errors")); continue
        exer += 1
        for k, p in props.items():
            if k in req: continue
            vs = alt(p, None if "default" not in p else p["default"]) or []
            outs = [run({k: v}) for v in vs]; outs = [r for r in outs if not r[0]]
            if outs and all(r[1] == base[1] and r[2] == base[2] for r in outs): rows.append((n, k, f"no change with {vs!r}")); dead += 1
    print(f"exercised {exer} tools, {dead} candidate dead args")
    for r in rows:
        if r[1] != "(not exercised)": print(*r, sep="\t")
    print(f"not exercised: {sum(1 for r in rows if r[1]=='(not exercised)')}")
main()
