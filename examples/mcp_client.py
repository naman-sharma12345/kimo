"""Small MCP stdio client for kimo, used by the examples and proofs.

- Tool errors raise KimoError (a RuntimeError, so old `except RuntimeError` still works) carrying the structured fields the server sends:
  .code, .param, .hint, .tool, .text. str(err) keeps the old "tool: message" form.
- Every request waits for the response with the SAME id; lines without an id (notifications, progress) are collected in .notifications and skipped.
- JSON-RPC errors raise KimoRpcError(code, message). A server that exits or closes stdout raises KimoProcessExit with the exit code. Malformed
  lines from the server raise KimoProtocolError.
- Every call has a timeout (default 600 s, per-call `timeout=`); on timeout the request is cancelled (notifications/cancelled) and KimoTimeout is raised.
  After a timeout or process exit the client is closed; make a new Kimo().
"""
import json, queue, subprocess, threading


class KimoError(RuntimeError):
    """A tool call returned isError: true. code/param/hint come from structuredContent when the server sends it."""
    def __init__(self, tool, text, structured=None):
        s = structured or {}
        super().__init__(f"{tool}: {text}")
        self.tool, self.text, self.code, self.param, self.hint = tool, text, s.get("code"), s.get("param"), s.get("hint")
        self.structured = s


class KimoRpcError(RuntimeError):
    def __init__(self, method, code, message):
        super().__init__(f"{method}: JSON-RPC error {code}: {message}"); self.method, self.code, self.message = method, code, message


class KimoProcessExit(RuntimeError):
    def __init__(self, method, returncode):
        super().__init__(f"{method}: kimo-mcp exited (code {returncode}) before answering"); self.method, self.returncode = method, returncode


class KimoProtocolError(RuntimeError):
    pass


class KimoTimeout(TimeoutError):
    pass


class Kimo:
    def __init__(self, exe="target/release/kimo-mcp", timeout=600.0):
        self.timeout = timeout; self.i = 0; self.notifications = []; self.closed = False
        self.p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, stderr=None)
        self._q = queue.Queue(); self._t = threading.Thread(target=self._pump, daemon=True); self._t.start()
        self.server_info = self.rpc("initialize", {}).get("serverInfo", {})

    def _pump(self):
        try:
            for line in self.p.stdout: self._q.put(line)
        finally: self._q.put(None)  # EOF marker

    def close(self):
        if self.closed: return
        self.closed = True
        try: self.p.stdin.close()
        except Exception: pass
        try: self.p.wait(timeout=5)
        except Exception: self.p.kill()

    def __enter__(self): return self
    def __exit__(self, *a): self.close()

    def _send(self, obj):
        try: self.p.stdin.write(json.dumps(obj) + "\n"); self.p.stdin.flush()
        except (BrokenPipeError, OSError, ValueError): pass  # the reader reports the exit

    def rpc(self, m, params, timeout=None):
        if self.closed: raise KimoProcessExit(m, self.p.poll())
        self.i += 1; rid = self.i; wait = self.timeout if timeout is None else timeout
        self._send({"jsonrpc": "2.0", "id": rid, "method": m, "params": params})
        deadline = None if wait is None else __import__("time").monotonic() + wait
        while True:
            left = None if deadline is None else max(0.0, deadline - __import__("time").monotonic())
            try: line = self._q.get(timeout=left)
            except queue.Empty:
                self._send({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": rid, "reason": "client timeout"}})
                self.close(); raise KimoTimeout(f"{m}: no response within {wait} s (request {rid} cancelled, client closed)")
            if line is None:
                self.closed = True
                try: rc = self.p.wait(timeout=5)
                except Exception: rc = None
                raise KimoProcessExit(m, rc)
            if not line.strip(): continue
            try: msg = json.loads(line)
            except ValueError as e: raise KimoProtocolError(f"{m}: server sent a line that is not JSON ({e}): {line[:200]!r}")
            if not isinstance(msg, dict): raise KimoProtocolError(f"{m}: server sent a non-object message: {line[:200]!r}")
            if "id" not in msg or msg["id"] is None:
                self.notifications.append(msg); continue
            if msg["id"] != rid: raise KimoProtocolError(f"{m}: response id {msg['id']!r} does not match request id {rid}")
            if "error" in msg:
                e = msg["error"] or {}; raise KimoRpcError(m, e.get("code"), e.get("message"))
            if "result" not in msg: raise KimoProtocolError(f"{m}: response {rid} has neither result nor error")
            return msg["result"]

    def tools(self, flat=True): return self.rpc("tools/list", {"mode": "flat"} if flat else {})["tools"]
    def group_call(self, group, action, **args): return self.call(group, action=action, args=args)

    def call(self, tool, timeout=None, **args):
        r = self.rpc("tools/call", {"name": tool, "arguments": args}, timeout=timeout)
        content = r.get("content") or []; t = content[0].get("text", "") if content else ""
        if r.get("isError"): raise KimoError(tool, t, r.get("structuredContent"))
        try: return json.loads(t)
        except ValueError as e: raise KimoProtocolError(f"{tool}: result is not JSON ({e}): {t[:200]!r}")
