"""Real-time playback in the viewer (ui/playback.js + the viewer in ui/studio.js) in headless Chromium against the dev mock
bridge's preview stream stand-in (studio.html?stream=1), served from 127.0.0.1 only.
Checks: Play draws stream frames on the canvas in step with the clock and schedules the sound, J / K / L shuttle, frame step,
the loop region, scrubbing, full screen, and that a bridge without the preview commands still plays draft clips.
Run: python3 crates/kimo-gui/tests/playback_ui_test.py"""
import asyncio, functools, http.server, os, sys, threading
from playwright.async_api import async_playwright

sys.stdout.reconfigure(encoding='utf-8')
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

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
        b = await p.chromium.launch(args=['--autoplay-policy=no-user-gesture-required'])
        ctx = await b.new_context(viewport={'width': 1280, 'height': 800})
        pg = await ctx.new_page()
        pg.on('pageerror', lambda e: errors.append(str(e)))
        await pg.route('**/*', lambda r: r.continue_() if r.request.url.startswith(('http://127.0.0.1', 'data:', 'blob:')) else r.abort())
        await pg.goto(base + '?stream=1'); await pg.wait_for_selector('body[data-ready="1"]')
        await pg.keyboard.press('Escape'); await pg.evaluate("window.__kimo.show('edit')")
        await pg.wait_for_selector('#viewer[data-state=ready]')
        T = lambda: pg.evaluate('window.__kimo.V.t')
        frame = lambda: pg.evaluate('window.__kimo.PB.shown')
        # PLAY: frames on the canvas, the clock runs, sound is scheduled
        await pg.click('#vw-play')
        await pg.wait_for_selector('#viewer[data-playing="1"]')
        await pg.wait_for_function('window.__kimo.PB.clock !== null', timeout=8000)
        await pg.wait_for_timeout(600)  # past the start: the lead-in and the output latency hold the first frame
        t0 = await T(); await pg.wait_for_timeout(1500); t1 = await T()
        check(await pg.is_visible('#vw-canvas') and not await pg.is_visible('#vw-img'), 'play: the stream canvas is on screen')
        check(1.3 < t1 - t0 < 1.7, f'play: the playhead runs in real time ({t1 - t0:.2f} s in 1.5 s)')
        f, t = await frame(), await T()
        check(abs(f - t * 30) <= 3, f'play: the frame on screen matches the clock (frame {f} at {t:.2f} s)')
        uses = await pg.evaluate('window.__kimo.PB.usesAudio()')
        sched = await pg.evaluate('[...window.__kimo.PB.chunks.values()].filter(c => c.scheduled).length')
        check(uses and sched >= 1, f'play: the sound plays on the audio clock ({sched} chunks scheduled)')
        msg = await pg.inner_text('#vw-msg')
        check('Playing' in msg and 'sound' in msg, f'play: the message says what plays ({msg})')
        # J / K / L
        await pg.keyboard.press('l')
        await pg.wait_for_function('window.__kimo.PB.rate === 2 && window.__kimo.PB.clock !== null', timeout=8000)
        t0 = await T(); await pg.wait_for_timeout(1000); t1 = await T()
        check(1.4 < t1 - t0 < 2.6, f'L: double speed ({t1 - t0:.2f} s in 1 s)')
        check('2×' in await pg.inner_text('#vw-msg') and not await pg.evaluate('window.__kimo.PB.usesAudio()'), 'L: 2× is shown, no sound while fast')
        await pg.keyboard.press('k')
        await pg.wait_for_selector('#viewer[data-playing="0"]')
        t0 = await T(); await pg.wait_for_timeout(300)
        check(await T() == t0, 'K: pauses')
        await pg.keyboard.press('j')
        await pg.wait_for_function('window.__kimo.PB.rate === -1 && window.__kimo.PB.clock !== null', timeout=8000)
        t0 = await T(); await pg.wait_for_timeout(800); t1 = await T()
        check(-1.1 < t1 - t0 < -0.4, f'J: plays backwards ({t1 - t0:.2f} s in 0.8 s)')
        await pg.keyboard.press('k'); await pg.wait_for_selector('#viewer[data-playing="0"]')
        # frame step: arrows and K+L
        t0 = await T()
        await pg.keyboard.press('ArrowRight'); await pg.wait_for_timeout(150)
        check(abs(await T() - t0 - 1 / 30) < 1e-3, 'step: → moves one frame')
        await pg.keyboard.down('k'); await pg.keyboard.press('j'); await pg.keyboard.press('j'); await pg.keyboard.up('k')
        await pg.wait_for_timeout(150)
        t2 = await T()
        check(abs(t2 - (t0 - 1 / 30)) < 1e-3, f'step: K+J twice moves two frames back ({t0:.4f} -> {t2:.4f})')
        check(await pg.get_attribute('#viewer', 'data-playing') == '0', 'step: holding K does not start playback')
        # loop region: I at 1.0 s, O at 2.0 s, play from 1.5 s for 2 s: the playhead stays inside
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 1; s.dispatchEvent(new Event('input')); })()")
        await pg.keyboard.press('i')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 2; s.dispatchEvent(new Event('input')); })()")
        await pg.keyboard.press('o')
        check(await pg.get_attribute('#viewer', 'data-loop') == '1' and 'aria-pressed="true"' in await pg.evaluate("document.querySelector('#vw-loop').outerHTML"), 'loop: I and O turn the loop on')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 1.5; s.dispatchEvent(new Event('input')); })()")
        await pg.keyboard.press('Space')
        await pg.wait_for_function('window.__kimo.PB.clock !== null', timeout=8000)
        seen = []
        for _ in range(20): await pg.wait_for_timeout(100); seen.append(await T())
        check(all(0.95 <= x <= 2.05 for x in seen) and min(seen) < 1.4, f'loop: playback wraps from out to in ({min(seen):.2f}..{max(seen):.2f} s)')
        await pg.keyboard.press('Space'); await pg.wait_for_selector('#viewer[data-playing="0"]')
        await pg.keyboard.press('Control+Shift+X')
        check(await pg.get_attribute('#viewer', 'data-loop') == '0', 'loop: Ctrl+Shift+X clears it')
        # scrubbing: a draft frame from the stream
        calls = await pg.evaluate('window.__mock.M.viewerCalls.frame')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 7.0; s.dispatchEvent(new Event('input')); })()")
        await pg.wait_for_function('window.__kimo.PB.shown === 210', timeout=3000)
        check(await pg.is_visible('#vw-canvas') and 'Draft frame' in await pg.inner_text('#vw-msg'), 'scrub: the draft frame is drawn at once')
        check(await pg.evaluate('window.__mock.M.viewerCalls.frame') == calls, 'scrub: ticks do not ask for a full-quality render')
        # full screen
        await pg.click('#vw-full'); await pg.wait_for_timeout(300)
        check(await pg.evaluate("document.fullscreenElement === document.querySelector('#viewer .vw-screen')"), 'full screen: the viewer fills the screen')
        await pg.keyboard.press('f'); await pg.wait_for_timeout(300)
        check(await pg.evaluate('document.fullscreenElement === null'), 'full screen: F leaves it')
        # an app without the preview commands: Play still plays a draft clip
        pg2 = await ctx.new_page()
        pg2.on('pageerror', lambda e: errors.append(str(e)))
        await pg2.route('**/*', lambda r: r.continue_() if r.request.url.startswith(('http://127.0.0.1', 'data:', 'blob:')) else r.abort())
        await pg2.goto(base); await pg2.wait_for_selector('body[data-ready="1"]')
        await pg2.keyboard.press('Escape'); await pg2.evaluate("window.__kimo.show('edit')"); await pg2.wait_for_selector('#viewer[data-state=ready]')
        await pg2.click('#vw-play'); await pg2.wait_for_selector('#viewer[data-playing="1"]', timeout=15000)
        check(await pg2.evaluate('window.__mock.M.viewerCalls.draft') == 1 and await pg2.evaluate('window.__kimo.PB.supported') is False, 'fallback: without the stream, Play renders a draft clip')
        await b.close()
    check(not errors, f'no page errors {errors}')
    print(f"\n{len(fails)} failed" if fails else "\nall passed")
    sys.exit(1 if fails else 0)

asyncio.run(main())
