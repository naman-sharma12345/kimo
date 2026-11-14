"""Media-bin proxy badge (ui/studio.js, engine: kimo_core::auto_proxy) in headless Chromium, served from 127.0.0.1 only.
The engine's `project.proxy_jobs` read and `{type:'proxy'}` engine events are stood in for inside the page: the badge goes
queued -> building NN% (with a progress bar) -> ready, ignores another project's asset with the same id, and shows
nothing for light media.
Run: python3 crates/kimo-gui/tests/proxy_badge_test.py"""
import asyncio, functools, http.server, json, os, sys, threading
from playwright.async_api import async_playwright

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = 'C:/media/uhd.mp4'
PROJECT = {'fps': 30, 'width': 1920, 'height': 1080, 'tracks': [], 'markers': [], 'assets': {
    'asset1': {'id': 'asset1', 'path': SRC, 'width': 3840, 'height': 2160, 'duration': 12.0, 'fps': 30},
    'asset2': {'id': 'asset2', 'path': 'C:/media/small.mp4', 'width': 640, 'height': 360, 'duration': 3.0, 'fps': 30}}}
JOBS = {'enabled': True, 'jobs': [{'asset_ids': ['asset1'], 'path': SRC, 'state': 'queued', 'progress': 0, 'stage': '', 'reasons': ['4k']}]}

STUB = """([project, jobs]) => {
  const B = window.__kimoBridge, orig = B.invoke.bind(B);
  B.invoke = async (c, a) => {
    if (c === 'engine_read' || c === 'engine_call') {
      const act = a?.args?.action;
      if (act === 'get_project') return { ok: true, data: project };
      if (act === 'proxy_jobs') return { ok: true, data: jobs };
      return { ok: true, data: null };
    }
    if (c === 'media_thumb') return null;
    return orig(c, a);
  };
}"""

def serve():
    h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=ROOT)
    h.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), h)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv

async def main():
    srv = serve(); base = f'http://127.0.0.1:{srv.server_address[1]}/ui/studio.html'
    fails, errors = [], []
    def check(cond, what):
        print(('ok   ' if cond else 'FAIL ') + what)
        if not cond: fails.append(what)
    async with async_playwright() as p:
        b = await p.chromium.launch()
        pg = await (await b.new_context(viewport={'width': 1280, 'height': 800})).new_page()
        pg.on('pageerror', lambda e: errors.append(str(e)))
        await pg.route('**/*', lambda r: r.continue_() if r.request.url.startswith('http://127.0.0.1') else r.abort())
        await pg.goto(base); await pg.wait_for_selector('body[data-ready="1"]')
        await pg.evaluate(STUB, [PROJECT, JOBS])
        await pg.evaluate("window.__mock.emit('project-changed', {actor: 'you', action: 'load_project'})")
        await pg.wait_for_selector('.asset[data-id="asset1"]', state='attached')
        badge = '.asset[data-id="asset1"] .px'
        await pg.wait_for_selector(badge, state='attached')
        check((await pg.text_content(badge)).strip() == 'Proxy queued', 'proxy_jobs seeds the badge: queued')
        check(await pg.query_selector('.asset[data-id="asset2"] .px') is None, 'an asset with no job has no badge')

        ev = lambda **k: json.dumps({'type': 'proxy', 'asset_ids': ['asset1'], 'path': SRC.replace('/', '\\'), 'reasons': ['4k'], 'stage': 'edit', **k})
        await pg.evaluate(f"window.__mock.emit('engine-event', {ev(state='building', progress=0.42)})")
        check((await pg.text_content(badge)).strip() == 'Proxy 42%', 'building event: percentage (Windows-style path still matches)')
        w = await pg.eval_on_selector('.asset[data-id="asset1"] .px-bar', 'e => e.style.width')
        check(w == '42%', f'progress bar follows ({w})')
        title = await pg.get_attribute(badge, 'title')
        check('export always uses the original' in title.lower(), 'tooltip says export uses the original')

        # same asset id from another project (different file): ignored
        await pg.evaluate("window.__mock.emit('engine-event', {type: 'proxy', asset_ids: ['asset1'], path: 'D:/other/clip.mp4', state: 'failed', progress: 0})")
        check((await pg.text_content(badge)).strip() == 'Proxy 42%', "another project's asset1 does not change this badge")

        await pg.evaluate(f"window.__mock.emit('engine-event', {ev(state='ready', progress=1, stage='', edit_proxy='C:/cache/0123456789abcdef.mp4')})")
        check(await pg.get_attribute(badge, 'data-state') == 'ready', 'ready event: badge state ready')
        check((await pg.text_content(badge)).strip() == 'Proxy', 'ready badge text')
        check(await pg.query_selector('.asset[data-id="asset1"] .px-bar') is None, 'no progress bar once ready')
        check('Smooth previews ready' in (await pg.text_content('#toast')), 'toast when previews become smooth')
        check(await pg.get_attribute(badge, 'aria-label') == 'Proxy ready', 'badge has an accessible label')

        # re-render of the bin (search) keeps the badge
        await pg.evaluate("document.getElementById('bin-q').dispatchEvent(new Event('input'))")
        check(await pg.get_attribute(badge, 'data-state') == 'ready', 'badge survives a bin re-render')

        await pg.evaluate("window.__mock.emit('engine-event', {type: 'proxy', asset_ids: ['asset2'], path: 'C:/media/small.mp4', state: 'skipped', progress: 1, reasons: []})")
        check(await pg.query_selector('.asset[data-id="asset2"] .px') is None, 'light media (skipped) shows no badge')
        await pg.evaluate("window.__mock.emit('engine-event', {type: 'proxy', asset_ids: ['asset2'], path: 'C:/media/small.mp4', state: 'unsupported', progress: 0, reasons: ['long_gop'], note: 'no edit proxy: variable frame rate'})")
        check((await pg.text_content('.asset[data-id="asset2"] .px')).strip() == 'Original', 'unsupported shows Original')
        await b.close()
    srv.shutdown()
    check(not errors, f'no page errors {errors}')
    print(f'{len(fails)} failure(s)')
    sys.exit(1 if fails else 0)

asyncio.run(main())
