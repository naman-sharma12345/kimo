"""kimo main window (ui/studio.html) in headless Chromium against the dev mock bridge, served from 127.0.0.1 only.
Checks every panel's behaviour; with KIMO_SHOTS=<dir> it also writes the 1280x800 screenshots.
Run: python3 crates/kimo-gui/tests/studio_ui_test.py"""
import asyncio, functools, http.server, os, sys, threading
from playwright.async_api import async_playwright

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHOTS = os.environ.get('KIMO_SHOTS')

def serve():
    h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=ROOT)
    h.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(('127.0.0.1', 0), h)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv

async def shot(pg, name):
    if SHOTS:
        os.makedirs(SHOTS, exist_ok=True)
        await pg.evaluate('document.getAnimations().forEach(a => { try { a.finish() } catch (e) {} })')
        await pg.wait_for_timeout(60)
        await pg.screenshot(path=os.path.join(SHOTS, name))

async def main():
    srv = serve(); base = f'http://127.0.0.1:{srv.server_address[1]}/ui/studio.html'
    fails, errors = [], []
    def check(cond, what):
        print(('ok   ' if cond else 'FAIL ') + what)
        if not cond: fails.append(what)
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1280, 'height': 800}, device_scale_factor=1)
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
        await pg.click('nav a[data-tab=chat]')
        await pg.fill('#chat-in', 'make the captions bigger'); await pg.click('#chat-send')
        await pg.wait_for_function('document.querySelectorAll(".ccard").length === 1 && !document.querySelector("#chat-send").disabled')
        await pg.click('.composer .sugg button:nth-child(2)')
        await pg.wait_for_function('document.querySelectorAll(".ccard").length === 2 && !document.querySelector("#chat-send").disabled')
        check('Caption size 54' in await pg.inner_text('#changes'), 'chat: change card for the caption edit')
        check('also undoes 1 later change' in await pg.inner_text('#changes'), 'chat: linear undo is explained on the older card')
        await shot(pg, 'chat_panel.png')
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