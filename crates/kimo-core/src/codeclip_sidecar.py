#!/usr/bin/env python3
# Kimo code clip renderer (sidecar). Embedded in kimo-core (src/codeclip_sidecar.py) and written to a temp dir per job.
# usage: python3 codeclip_sidecar.py job.json
# job: {"dir","entry","width","height","frame_timeout_ms","load_timeout_ms","chromium","lock",
#       "steps":[{"t":float,"params":{name:value},"out":path} | {"reload":true}]}
# Each frame step writes raw straight-alpha RGBA8 (width*height*4 bytes, top row first) to "out".
# stdout: one JSON line per frame ({"frame":i,"ms":..}), then {"ok":true,...} or {"error":"..."} (exit 3).
# Sandbox: the page is served from a virtual origin that maps only to files inside "dir"; every other request is aborted,
# DNS resolves nothing, WebSocket/RTCPeerConnection are removed, Date/performance.now are frozen and Math.random is a
# seeded PRNG restarted on every page load.
import sys, os, json, time, mimetypes, io, fcntl, urllib.parse

ORIGIN = 'http://kimo-clip.invalid/'
INIT = r"""(() => {
  const T0 = 1767225600000; const D = Date;
  class FixedDate extends D { constructor(...a) { if (a.length === 0) super(T0); else super(...a); } static now() { return T0; } }
  window.Date = FixedDate;
  try { Object.defineProperty(performance, 'now', { value: () => 0 }); } catch (e) {}
  let s = 0x2545F491;
  Math.random = () => { s = (s + 0x6D2B79F5) | 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
  const off = function () { throw new Error('network is disabled inside a kimo code clip'); };
  window.WebSocket = off; window.RTCPeerConnection = off; window.EventSource = off;
})();"""

def out(o):
    sys.stdout.write(json.dumps(o) + '\n'); sys.stdout.flush()

def main():
    job = json.load(open(sys.argv[1]))
    root = os.path.realpath(job['dir']); W = int(job['width']); H = int(job['height'])
    entry = job.get('entry') or 'index.html'
    fms = int(job.get('frame_timeout_ms') or 20000); lms = int(job.get('load_timeout_ms') or 60000)
    lock = job.get('lock')
    if lock:
        lf = open(lock, 'a'); fcntl.flock(lf, fcntl.LOCK_EX)
    try:
        from playwright.sync_api import sync_playwright
        from PIL import Image
    except Exception as e:
        out({'error': 'renderer_failed: the code clip renderer needs python3 with playwright and Pillow (%s)' % e}); sys.exit(3)
    exe = job.get('chromium') or ''
    if not exe:
        for c in sorted(__import__('glob').glob(os.path.expanduser('~/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell')), reverse=True):
            exe = c; break
    args = ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--hide-scrollbars',
            '--mute-audio', '--force-device-scale-factor=1', '--disable-background-timer-throttling', '--disable-renderer-backgrounding',
            '--disable-background-networking', '--host-resolver-rules=MAP * ~NOTFOUND', '--no-first-run', '--disable-extensions', '--disable-sync']
    blocked = []; errors = []
    def handler(route):
        u = route.request.url
        if not u.startswith(ORIGIN):
            if len(blocked) < 20: blocked.append(u[:200])
            return route.abort()
        rel = urllib.parse.unquote(u[len(ORIGIN):].split('?')[0].split('#')[0]) or entry
        f = os.path.realpath(os.path.join(root, rel))
        if not (f == root or f.startswith(root + os.sep)) or not os.path.isfile(f):
            if len(blocked) < 20: blocked.append(u[:200])
            return route.fulfill(status=404, body='not found')
        ct = mimetypes.guess_type(f)[0] or 'application/octet-stream'
        if f.endswith('.js') or f.endswith('.mjs'): ct = 'text/javascript'
        route.fulfill(status=200, body=open(f, 'rb').read(), headers={'Content-Type': ct})
    with sync_playwright() as p:
        b = p.chromium.launch(executable_path=exe if exe and os.path.exists(exe) else None, args=args)
        ctx = b.new_context(viewport={'width': W, 'height': H}, device_scale_factor=1, service_workers='block', java_script_enabled=True)
        ctx.add_init_script(INIT)
        ctx.route('**/*', handler)
        pg = ctx.new_page()
        pg.on('pageerror', lambda e: errors.append(str(e)[:300]))
        pg.on('console', lambda m: errors.append('console.error: ' + m.text[:300]) if m.type == 'error' else None)
        def load():
            pg.goto(ORIGIN + urllib.parse.quote(entry), timeout=lms)
            try:
                pg.wait_for_function('window.__ready === true || (window.__ready === undefined && typeof window.renderFrame === "function")', timeout=lms)
            except Exception:
                raise RuntimeError('%s did not define window.renderFrame (or set window.__ready) within %d ms%s' % (entry, lms, ('; page errors: ' + ' | '.join(errors[-3:])) if errors else ''))
            if pg.evaluate('typeof window.renderFrame') != 'function':
                raise RuntimeError('%s set __ready but window.renderFrame is not a function' % entry)
            pg.evaluate('document.fonts ? document.fonts.ready.then(() => 1) : 1')
        load()
        n = 0
        for i, st in enumerate(job['steps']):
            if st.get('reload'):
                load(); continue
            t0 = time.time(); errors.clear()
            pg.evaluate("""async ([t, p, ms]) => { const r = window.renderFrame(t, p); if (r && typeof r.then === 'function') {
                await Promise.race([r, new Promise((_, j) => setTimeout(() => j(new Error('renderFrame(' + t + ') took longer than ' + ms + ' ms')), ms))]); } }""",
                [float(st['t']), st.get('params') or {}, fms])
            if errors:
                raise RuntimeError('renderFrame(%s) raised: %s' % (st['t'], ' | '.join(errors[-3:])))
            png = pg.screenshot(type='png', omit_background=True, timeout=fms)
            img = Image.open(io.BytesIO(png)).convert('RGBA')
            if img.size != (W, H):
                raise RuntimeError('frame came back %dx%d, expected %dx%d' % (img.size[0], img.size[1], W, H))
            tmp = st['out'] + '.part'
            with open(tmp, 'wb') as fh: fh.write(img.tobytes())
            os.replace(tmp, st['out'])
            out({'frame': n, 't': st['t'], 'ms': round((time.time() - t0) * 1000, 1)}); n += 1
        ua = pg.evaluate("(()=>{const c=document.createElement('canvas');const g=c.getContext('webgl2')||c.getContext('webgl');if(!g)return 'no webgl';const e=g.getExtension('WEBGL_debug_renderer_info');return e?g.getParameter(e.UNMASKED_RENDERER_WEBGL):g.getParameter(g.RENDERER)})()")
        b.close()
    out({'ok': True, 'frames': n, 'blocked': blocked, 'gl': ua})

if __name__ == '__main__':
    try:
        main()
    except SystemExit:
        raise
    except Exception as e:
        msg = str(e).strip().splitlines()
        out({'error': ' '.join(msg[:4])[:900]}); sys.exit(3)
