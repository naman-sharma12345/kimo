"""Minimal MCP stdio client for kimo-mcp (newline-delimited JSON-RPC 2.0)."""
import json, os, subprocess, threading, queue, time

class McpError(Exception):
    pass

class Kimo:
    def __init__(self, binary, env=None, cwd=None, log=None):
        e = dict(os.environ); e.setdefault("KIMO_SPECTATOR", "off")
        if env: e.update(env)
        self.p = subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(log or os.devnull, "ab"),
                                  env=e, cwd=cwd, text=True, bufsize=1, start_new_session=True)
        self.q = queue.Queue(); self.nid = 0
        threading.Thread(target=self._read, daemon=True).start()
        self.server = self.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "director_eval", "version": "1"}})
        self.notify("notifications/initialized")

    def _read(self):
        for line in self.p.stdout:
            line = line.strip()
            if not line: continue
            try: self.q.put(json.loads(line))
            except ValueError: pass
        self.q.put(None)

    def notify(self, method, params=None):
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params or {}}) + "\n"); self.p.stdin.flush()

    def request(self, method, params=None, timeout=600):
        self.nid += 1; rid = self.nid
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}}) + "\n"); self.p.stdin.flush()
        end = time.time() + timeout
        while True:
            try: m = self.q.get(timeout=max(0.1, end - time.time()))
            except queue.Empty: raise McpError(f"{method}: no answer in {timeout}s")
            if m is None: raise McpError(f"{method}: server exited")
            if m.get("id") == rid:
                if "error" in m: raise McpError(m["error"].get("message"))
                return m["result"]

    def list_tools(self, mode=None):
        return self.request("tools/list", {"mode": mode} if mode else {})["tools"]

    def call(self, name, args, timeout=600):
        """Returns (is_error, text, images[(mime, b64)])."""
        r = self.request("tools/call", {"name": name, "arguments": args}, timeout=timeout)
        texts, imgs = [], []
        for c in r.get("content", []):
            if c.get("type") == "text": texts.append(c.get("text", ""))
            elif c.get("type") == "image": imgs.append((c.get("mimeType", "image/png"), c.get("data", "")))
        return bool(r.get("isError")), "\n".join(texts), imgs

    def j(self, name, args=None):
        """Call and parse JSON; raise on error (for scoring helpers)."""
        err, t, _ = self.call(name, args or {})
        if err: raise McpError(f"{name}: {t}")
        try: return json.loads(t)
        except ValueError: return t

    def close(self):
        try: self.p.stdin.close()
        except Exception: pass
        try: self.p.wait(timeout=5)
        except Exception: self.p.kill()
