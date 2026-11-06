#!/bin/sh
# End-to-end test of kimo-setup.sh against a localhost server (Range-capable): fresh install, `kimo --version`,
# `kimo mcp` answering initialize, resume of a partial download, a tampered package refused, upgrade, uninstall.
#   test-setup.sh <dir with kimo-gui-linux-x86_64.tar.xz, .sha512 and kimo-setup.sh>
set -eu
OUT=$(cd "$1" && pwd); PKG=kimo-gui-linux-x86_64.tar.xz
T=$(mktemp -d); SRV_PID=
cleanup() { [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; rm -rf "$T"; }
trap cleanup EXIT
mkdir -p "$T/www"; cp "$OUT/$PKG" "$OUT/$PKG.sha512" "$T/www/"
cat > "$T/srv.py" <<'EOF'
import http.server, os, re, sys
root = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        p = os.path.join(root, os.path.basename(self.path))
        if not os.path.isfile(p): self.send_error(404); return
        data = open(p, 'rb').read(); start = 0
        m = re.match(r'bytes=(\d+)-', self.headers.get('Range', ''))
        if m and int(m.group(1)) < len(data):
            start = int(m.group(1)); self.send_response(206); open(os.path.join(root, '..', 'ranges'), 'a').write(f'{start}\n')
            self.send_header('Content-Range', f'bytes {start}-{len(data)-1}/{len(data)}')
        else: self.send_response(200)
        self.send_header('Content-Length', str(len(data) - start)); self.end_headers(); self.wfile.write(data[start:])
s = http.server.ThreadingHTTPServer(('127.0.0.1', 0), H)
open(os.path.join(root, '..', 'port'), 'w').write(str(s.server_address[1])); s.serve_forever()
EOF
python3 "$T/srv.py" "$T/www" >"$T/srv.log" 2>&1 & SRV_PID=$!
for i in 1 2 3 4 5 6 7 8 9 10; do [ -s "$T/port" ] && break; sleep 0.3; done
export KIMO_BASE_URL="http://127.0.0.1:$(cat "$T/port")" KIMO_PREFIX="$T/inst/kimo" KIMO_BIN_DIR="$T/bin" KIMO_APPS_DIR="$T/apps" XDG_CACHE_HOME="$T/cache"
ok() { echo "ok   $*"; }
fail() { echo "FAIL $*"; exit 1; }

sh "$OUT/kimo-setup.sh" >"$T/1.log" 2>&1 || { cat "$T/1.log"; fail "fresh install"; }
[ -x "$T/bin/kimo" ] && ok "fresh install: kimo command" || fail "no kimo command"
grep -q "Exec=$KIMO_PREFIX/kimo-gui" "$T/apps/kimo.desktop" && ok "menu entry" || fail "menu entry"
"$T/bin/kimo" --version | grep -q '^kimo-gui ' && ok "kimo --version: $("$T/bin/kimo" --version)" || fail "--version"
R=$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}' | KIMO_SPECTATOR=off timeout 20 "$T/bin/kimo" mcp 2>/dev/null | head -1)
echo "$R" | grep -q '"serverInfo"' && ok "kimo mcp answers initialize" || fail "kimo mcp: $R"

# resume: keep the first half of the package in the cache, the second run must fetch only the rest (HTTP 206)
SIZE=$(wc -c < "$T/www/$PKG"); head -c $((SIZE / 2)) "$T/www/$PKG" > "$T/cache/kimo-setup/$PKG"
sh "$OUT/kimo-setup.sh" >"$T/2.log" 2>&1 || { cat "$T/2.log"; fail "resume"; }
grep -q "SHA-512 verified" "$T/2.log" && grep -qx "$((SIZE / 2))" "$T/ranges" && ok "resumed from byte $((SIZE / 2)) with an HTTP Range request (206), verified, upgraded in place" || fail "resume (no 206 from byte $((SIZE / 2)))"

# tamper: a package that no longer matches the published SHA-512 is refused and removed
rm -f "$T/cache/kimo-setup/$PKG"; printf 'x' >> "$T/www/$PKG"
if sh "$OUT/kimo-setup.sh" >"$T/3.log" 2>&1; then fail "tampered package accepted"; fi
grep -q "does not match its SHA-512" "$T/3.log" && [ ! -f "$T/cache/kimo-setup/$PKG" ] && ok "tampered package refused and removed" || { cat "$T/3.log"; fail "tamper message"; }
[ -x "$KIMO_PREFIX/kimo-gui" ] && ok "previous install untouched after a refused download" || fail "install damaged"

sh "$OUT/kimo-setup.sh" --uninstall >/dev/null
[ ! -e "$T/bin/kimo" ] && [ ! -e "$KIMO_PREFIX" ] && ok "uninstall" || fail "uninstall"
echo "all passed"
