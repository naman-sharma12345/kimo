"""examples/mcp_client.py behaves: structured errors survive, ids are checked, JSON-RPC errors / exits / timeouts are raised, notifications skipped.
Uses tiny fake servers (python scripts) so it needs no kimo binary; one extra check runs against the real server when target/release/kimo-mcp exists."""
import os, sys, tempfile, textwrap, json
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "examples"))
import mcp_client as M

def fake(body):
    f = tempfile.NamedTemporaryFile("w", suffix=".py", delete=False); f.write(textwrap.dedent('''
        import sys, json
        def send(o): sys.stdout.write(json.dumps(o) + "\\n"); sys.stdout.flush()
        for line in sys.stdin:
            req = json.loads(line); m = req.get("method"); rid = req.get("id")
            if m == "initialize": send({"jsonrpc": "2.0", "id": rid, "result": {"serverInfo": {"name": "fake", "version": "9"}}}); continue
    ''') + textwrap.indent(textwrap.dedent(body), "    ")); f.close()
    sh = tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False); sh.write(f"#!/bin/sh\nexec {sys.executable} {f.name}\n"); sh.close(); os.chmod(sh.name, 0o755); return sh.name

def expect(exc, fn, label):
    try: fn()
    except exc as e: print("ok", label, "->", type(e).__name__); return e
    except Exception as e: raise SystemExit(f"FAIL {label}: raised {type(e).__name__}: {e}")
    raise SystemExit(f"FAIL {label}: nothing raised")

# 1 structured error fields survive and old except RuntimeError still works
k = M.Kimo(fake('''
    if m == "tools/call": send({"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": "no such clip 'x'"}], "isError": True, "structuredContent": {"code": "not_found", "param": "clip_id", "hint": "valid clip ids: clip3"}}})
'''), timeout=5)
e = expect(RuntimeError, lambda: k.call("delete_clip", clip_id="x"), "structured error is a RuntimeError")
assert isinstance(e, M.KimoError) and (e.code, e.param, e.hint) == ("not_found", "clip_id", "valid clip ids: clip3") and "no such clip" in str(e) and e.tool == "delete_clip", vars(e)
assert k.server_info.get("name") == "fake"; k.close()
# 2 notifications without an id are skipped, the right response is still found
k = M.Kimo(fake('''
    if m == "tools/call":
        send({"jsonrpc": "2.0", "method": "notifications/progress", "params": {"p": 1}})
        send({"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": "{\\"ok\\": true}"}], "isError": False}})
'''), timeout=5)
assert k.call("x") == {"ok": True} and len(k.notifications) == 1; print("ok notification skipped"); k.close()
# 3 mismatched id
k = M.Kimo(fake('''
    if m == "tools/call": send({"jsonrpc": "2.0", "id": rid + 100, "result": {"content": [], "isError": False}})
'''), timeout=5); expect(M.KimoProtocolError, lambda: k.call("x"), "mismatched response id"); k.close()
# 4 JSON-RPC error
k = M.Kimo(fake('''
    if m == "tools/call": send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "method not found"}})
'''), timeout=5); e = expect(M.KimoRpcError, lambda: k.call("x"), "json-rpc error"); assert e.code == -32601; k.close()
# 5 process exits mid-request
k = M.Kimo(fake('''
    if m == "tools/call": sys.exit(3)
'''), timeout=5); e = expect(M.KimoProcessExit, lambda: k.call("x"), "process exit"); assert e.returncode == 3, e.returncode
expect(M.KimoProcessExit, lambda: k.call("y"), "call after exit")
# 6 malformed line
k = M.Kimo(fake('''
    if m == "tools/call": sys.stdout.write("not json\\n"); sys.stdout.flush()
'''), timeout=5); expect(M.KimoProtocolError, lambda: k.call("x"), "malformed line"); k.close()
# 7 timeout + cancellation notification sent
log = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False); log.close()
k = M.Kimo(fake(f'''
    if m == "notifications/cancelled": open({log.name!r}, "a").write(json.dumps(req) + "\\n")
'''), timeout=0.5)
import time; t0 = time.monotonic(); expect(M.KimoTimeout, lambda: k.call("slow"), "timeout"); assert time.monotonic() - t0 < 4
time.sleep(0.5); assert k.closed; cancelled = [json.loads(l) for l in open(log.name)]; assert cancelled and cancelled[0]["params"]["requestId"] == 2, cancelled; print("ok cancellation notification reached the server")
# 8 per-call timeout override
k = M.Kimo(fake('''
    pass
'''), timeout=30); t0 = time.monotonic(); expect(M.KimoTimeout, lambda: k.call("slow", timeout=0.3), "per-call timeout"); assert time.monotonic() - t0 < 5
# 9 real server, when built: structured code/param/hint reach the caller
exe = os.path.join(os.path.dirname(__file__), "..", "target", "release", "kimo-mcp")
if os.path.exists(exe):
    k = M.Kimo(exe, timeout=60); e = expect(M.KimoError, lambda: k.call("delete_clip", clip_id="nope"), "real server structured error")
    assert e.code == "not_found" and e.param == "clip_id", vars(e); k.close()
print("mcp_client_proof: all checks passed")
