"""kimo main window (ui/studio.html) in headless Chromium against the dev mock bridge, served from 127.0.0.1 only.
Checks every panel's behaviour; with KIMO_SHOTS=<dir> it also writes the 1280x800 screenshots.
Run: python3 crates/kimo-gui/tests/studio_ui_test.py   (KIMO_SCHEME=dark for the dark theme)"""
import asyncio, functools, http.server, os, sys, threading
from playwright.async_api import async_playwright

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHOTS = os.environ.get('KIMO_SHOTS')
SCHEME = os.environ.get('KIMO_SCHEME', 'light')  # 'dark' renders the dark theme; screenshots get a _dark suffix

def serve():
    h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=ROOT)
    h.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), h)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv

async def shot(pg, name):
    if SHOTS:
        os.makedirs(SHOTS, exist_ok=True)
        await pg.mouse.move(1279, 799)  # no stray hover state in the picture
        await pg.evaluate("document.getElementById('toast')?.classList.remove('on')")
        await pg.evaluate('document.getAnimations().forEach(a => { try { a.finish() } catch (e) {} })')
        await pg.wait_for_timeout(60)
        await pg.screenshot(path=os.path.join(SHOTS, name if SCHEME == 'light' else name.replace('.png', '_dark.png')))

async def main():
    srv = serve(); base = f'http://127.0.0.1:{srv.server_address[1]}/ui/studio.html'
    fails, errors = [], []
    def check(cond, what):
        print(('ok   ' if cond else 'FAIL ') + what)
        if not cond: fails.append(what)
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1280, 'height': 800}, device_scale_factor=1, color_scheme=SCHEME)
        pg = await ctx.new_page()
        pg.on('pageerror', lambda e: errors.append(str(e)))
        await pg.route('**/*', lambda r: r.continue_() if r.request.url.startswith('http://127.0.0.1') else r.abort())
        await pg.goto(base); await pg.wait_for_selector('body[data-ready="1"]')
        await pg.evaluate('document.fonts.ready')
        # HOME
        check('Running' in await pg.inner_text('#h-engine'), 'home: engine shows running')
        check((await pg.inner_text('#h-mcp')) == 'Off', 'home: MCP is off by default')
        check(not await pg.is_visible('#setup'), 'setup window hidden when parts are installed')
        await shot(pg, 'home.png')
        # CONNECTIONS + MCP
        await pg.click('nav a[data-tab=connections]')
        check(not await pg.is_visible('#mcp-on'), 'mcp: no config shown while off')
        check(await pg.is_disabled('[data-add=claude]'), 'agents: Add for me disabled while MCP is off')
        await pg.click('#mcp-switch'); await pg.wait_for_selector('#mcp-on:not([hidden])')
        v = await pg.inner_text('#mcp-verified')
        check('17 tools' in v and '540 actions' in v and '8.12.1' in v, f'mcp: verified line shows the probe ({v})')
        cfg = await pg.inner_text('#cfg-box')
        check('claude mcp add --env KIMO_FONTS=' in cfg and '--transport stdio --scope user kimo --' in cfg, 'mcp: Claude Code command shown')
        await pg.click('#cfg-seg button[data-c=codex_toml]')
        check('[mcp_servers.kimo]' in await pg.inner_text('#cfg-box'), 'mcp: Codex TOML shown')
        await pg.click('#cfg-seg button[data-c=claude_add]')
        await pg.evaluate('document.querySelector("main").scrollTop = 0')
        await shot(pg, 'mcp_panel.png')
        await pg.wait_for_selector('#agent-claude')
        check('not found on PATH' in await pg.inner_text('#agent-codex'), 'agents: Codex not on PATH says so')
        check(not await pg.is_disabled('[data-add=claude]'), 'agents: Add for me enabled once MCP is on')
        # BYO key
        await pg.select_option('#key-provider', 'anthropic')
        check((await pg.input_value('#key-model')) == 'claude-sonnet-4-5', 'keys: provider default model filled')
        await pg.select_option('#key-provider', 'openai')
        await pg.fill('#key-input', 'sk-test-0000000000001234')
        await pg.check('#key-remember')
        await pg.click('#key-save'); await pg.wait_for_function('document.querySelector("#key-msg").textContent.length > 0')
        check('Saved in the' in await pg.inner_text('#key-msg'), 'keys: remembered key saved to the keychain')
        check(await pg.input_value('#key-input') == '', 'keys: key field cleared after save (never shown again)')
        check('1234' in await pg.get_attribute('#key-input', 'placeholder'), 'keys: only a hint is shown')
        await pg.click('#key-test'); await pg.wait_for_function('document.querySelector("#key-msg").textContent.startsWith("Connected")')
        check(True, 'keys: Test connection reports success')
        await pg.evaluate('document.querySelector("#agent-claude").scrollIntoView({block:"start"})')
        await pg.evaluate('window.scrollBy(0,-20)')
        await shot(pg, 'connections.png')
        # Add for me asks first
        await pg.click('[data-add=claude]')
        check(await pg.is_visible('#confirm') and 'add-json' in await pg.inner_text('#cf-code'), 'agents: Add for me shows the exact command before running')
        await pg.click('#cf-yes'); await pg.wait_for_function('document.querySelector("#agent-claude").innerText.includes("kimo linked")')
        check(True, 'agents: linked after confirm')
        # CHAT
        # VIEWER (chat tab, right column): the engine's frame at the playhead, draft clip, before/after
        await pg.click('nav a[data-tab=chat]')
        await pg.wait_for_selector('#viewer[data-state=ready]')
        check(await pg.is_visible('#vw-img') and (await pg.get_attribute('#vw-img', 'src') or '').startswith(('data:', 'kimo:', 'http://kimo.localhost')), 'viewer: ready shows the engine frame at the playhead')
        check(await pg.is_disabled('#vw-ab button[data-v=before]'), 'viewer: Before is off until something changed')
        rev0 = await pg.get_attribute('#viewer', 'data-rev')
        calls0 = await pg.evaluate('window.__mock.M.viewerCalls.frame')
        for i in range(20):
            await pg.evaluate(f"(() => {{ const s = document.querySelector('#vw-scrub'); s.value = {0.3 * i}; s.dispatchEvent(new Event('input')); }})()")
        check(await pg.evaluate('window.__mock.M.viewerCalls.frame') == calls0, 'viewer: scrubbing ticks never ask the engine for a frame')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 4.5; s.dispatchEvent(new Event('change')); })()")
        check((await pg.get_attribute('#viewer', 'data-state')) == 'rendering', 'viewer: releasing the scrub bar renders, with a rendering state')
        await pg.wait_for_selector('#viewer[data-state=ready]')
        check(await pg.evaluate('window.__mock.M.viewerCalls.frame') == calls0 + 1, 'viewer: one engine frame per release')
        check('0:04.5' in await pg.inner_text('#vw-time'), 'viewer: time shows the playhead')
        await pg.fill('#chat-in', 'make the captions bigger'); await pg.click('#chat-send')
        await pg.wait_for_function('document.querySelectorAll(".ccard").length === 1 && !document.querySelector("#chat-send").disabled')
        await pg.click('.composer .sugg button:nth-child(2)')
        await pg.wait_for_function('document.querySelectorAll(".ccard").length === 2 && !document.querySelector("#chat-send").disabled')
        check('Caption size 54' in await pg.inner_text('#changes'), 'chat: change card for the caption edit')
        check('also undoes 1 later change' in await pg.inner_text('#changes'), 'chat: linear undo is explained on the older card')
        await pg.wait_for_function(f"document.querySelector('#viewer').dataset.rev !== '{rev0}' && document.querySelector('#viewer').dataset.state === 'ready'")
        check(True, 'viewer: a chat edit refreshes the frame (new revision)')
        check(not await pg.is_disabled('#vw-ab button[data-v=before]'), 'viewer: Before is on once something changed')
        await shot(pg, 'chat_panel.png')
        await pg.evaluate('document.querySelector("#chat-hello")?.scrollIntoView()')
        await shot(pg, 'viewer.png')
        await pg.click('#vw-ab button[data-v=before]')
        brev, crev = await pg.get_attribute('#vw-before', 'data-rev'), await pg.get_attribute('#viewer', 'data-rev')
        check(await pg.is_visible('#vw-before') and brev and brev != crev and await pg.get_attribute('#vw-before', 'src'), f'viewer: Before shows the frame of the revision before the last change ({brev} vs {crev})')
        check('Before' in await pg.inner_text('#vw-tag'), 'viewer: Before is labelled on the frame')
        await shot(pg, 'viewer_before.png')
        await pg.click('#vw-ab button[data-v=after]')
        check(not await pg.is_visible('#vw-before') and await pg.is_visible('#vw-img'), 'viewer: After returns to the current frame')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 2; s.dispatchEvent(new Event('change')); })()")
        await pg.wait_for_selector('#viewer[data-state=ready]')
        d0 = await pg.evaluate('window.__mock.M.viewerCalls.draft')
        await pg.click('#vw-play')
        await pg.wait_for_selector('#viewer[data-playing="1"]', timeout=15000)
        check(await pg.is_visible('#vw-video') and await pg.evaluate('window.__mock.M.viewerCalls.draft') == d0 + 1, 'viewer: Play renders one draft clip and plays it')
        await pg.click('#vw-play')
        check((await pg.get_attribute('#viewer', 'data-playing')) == '0', 'viewer: Pause stops the draft')
        f0 = await pg.evaluate('window.__mock.M.viewerCalls.frame')
        await pg.evaluate("(() => { const s = document.querySelector('#vw-scrub'); s.value = 2.4; s.dispatchEvent(new Event('input')); s.dispatchEvent(new Event('change')); })()")
        await pg.wait_for_timeout(150)
        check(await pg.evaluate('window.__mock.M.viewerCalls.frame') == f0 and await pg.is_visible('#vw-video'), 'viewer: scrubbing inside the draft seeks its cached frames, no engine render')
        rev1 = await pg.get_attribute('#viewer', 'data-rev')
        await pg.evaluate("window.__mock.engineCall({kind:'external', conn:2}, 'color_grade', 'set_saturation', {value:1.3}, {target:{clip_id:'v1_c1'}, payload:{}}, 60).then(() => window.__mock.M.edit.sat = 1.3)")
        await pg.wait_for_function(f"document.querySelector('#viewer').dataset.rev !== '{rev1}' && document.querySelector('#viewer').dataset.state === 'ready'", timeout=8000)
        check(not await pg.is_visible('#vw-video'), 'viewer: an outside agent\'s edit refreshes the frame and drops the stale draft')
        await pg.click('.ccard:not(.undone) [data-undo]')  # newest card first in the list
        await pg.wait_for_function('document.querySelectorAll(".ccard.undone").length === 1')
        check(True, 'chat: Undo marks the card undone')
        # LIVE
        await pg.click('nav a[data-tab=live]')
        rows = await pg.locator('#live-rows tr').count()
        check(rows >= 4, f'live: chat tool calls appear in the feed ({rows} rows)')
        await pg.evaluate('''async () => {
            const ext = {kind:'external', conn:1};
            await window.__mock.engineCall(ext, 'timeline', 'split_clip', {clip_id:'v1_c2', at:4.2}, {target:{track_id:'V1', clip_id:'v1_c2'}, payload:{at:4.2}});
            window.__mock.emit('live-event', {source: ext, t: Date.now(), ev:{tool_name:'add_caption', call_id:'x9', phase:'start', target:{track_id:'T1'}, payload:{args:{text:'Welcome back', start:0, end:2.5}}}});
        }''')
        check('External #1' in await pg.inner_text('#live-rows'), 'live: an external MCP client call is tagged External')
        check(await pg.locator('#live-rows tr.running').count() == 1, 'live: a running call is highlighted while it runs')
        await shot(pg, 'live_tab.png')
        await pg.click('#live-filter button[data-f=gui]')
        check('External' not in await pg.inner_text('#live-rows'), 'live: filter to kimo chat only')
        await pg.click('#live-filter button[data-f=all]')
        # VIEWER empty + error states
        pg3 = await ctx.new_page()
        pg3.on('pageerror', lambda e: errors.append(str(e)))
        await pg3.route('**/*', lambda r: r.continue_() if r.request.url.startswith('http://127.0.0.1') else r.abort())
        await pg3.goto(base + '?project=empty'); await pg3.wait_for_selector('body[data-ready="1"]')
        await pg3.click('nav a[data-tab=chat]')
        await pg3.wait_for_selector('#viewer[data-state=empty]')
        check(await pg3.is_visible('#vw-empty') and not await pg3.is_visible('#vw-img') and await pg3.is_disabled('#vw-play'), 'viewer: empty project shows the empty state, Play off')
        await pg3.evaluate("window.__mock.M.failViewer = 'render_frame: font not found: Inter Display'; window.__mock.M.empty = false; window.__kimo.viewerRefresh()")
        await pg3.wait_for_selector('#viewer[data-state=error]')
        check('font not found' in await pg3.inner_text('#vw-err'), 'viewer: error state shows the engine\'s own words')
        await pg3.evaluate("window.__mock.M.failViewer = null"); await pg3.click('#vw-retry')
        await pg3.wait_for_selector('#viewer[data-state=ready]')
        check(True, 'viewer: Try again recovers to ready')
        await pg3.close()
        # FIRST-RUN POPUP
        pg2 = await ctx.new_page()
        await pg2.route('**/*', lambda r: r.continue_() if r.request.url.startswith('http://127.0.0.1') else r.abort())
        await pg2.goto(base + '?setup=show'); await pg2.wait_for_selector('body[data-ready="1"]')
        await pg2.evaluate('document.fonts.ready')
        check(await pg2.is_visible('#setup'), 'setup: first-run window opens when tools are missing')
        await pg2.evaluate('''() => { const k = window.__kimo; 
            const ev = (id, done, total, bps, phase) => window.__mock.emit('setup-progress', {id, done, total, bytes_per_s: bps, eta_s: (total-done)/bps, phase, mirror:0});
            ev('ffmpeg', 79000000, 79000000, 1, 'done'); ev('ffprobe', 3100000, 3100000, 1, 'done'); ev('whisper', 4600000, 4600000, 1, 'done');
            ev('model', 71400000, 190085487, 11400000, 'downloading'); }''')
        t = await pg2.inner_text('#tot-t')
        check('MB/s' in t and 'left' in t, f'setup: total shows speed and time left ({t})')
        await shot(pg2, 'popup.png')
        await pg2.click('#setup-go'); await pg2.wait_for_selector('#setup', state='hidden', timeout=15000)
        check(True, 'setup: window closes when downloads finish')
        await b.close()
    srv.shutdown()
    check(not errors, f'no page errors {errors}')
    print(f'\n{len(fails)} failed' if fails else '\nall passed')
    sys.exit(1 if fails else 0)

asyncio.run(main())