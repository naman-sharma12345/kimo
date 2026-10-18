#!/usr/bin/env python3
"""Router test: every tool call must be visible.

Loads the m16 edit screen (still snapshot), then injects unified tool events
(the engine contract in README "Unified tool events"): one per tool family
plus an unknown tool and an error. For each call it asserts:
  - the agent rail gained an entry with the tool's name (and an error state
    for the failing call),
  - the DOM changed on start and again on completion,
  - the family's own effect landed (clip split / added, fx badge, track
    header state, inspector diff, caption chip, HUD pulse...).
Finally it checks the render loop goes idle again (0 rAF over 3 s).

    python3 crates/kimo-gui/tests/router_test.py      # exit 1 on failure
"""
import asyncio, functools, http.server, json, re, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]

PROBE = r"""
(() => {
  window.__muts = 0; window.__raf = 0;
  const rAF = window.requestAnimationFrame.bind(window);
  window.requestAnimationFrame = cb => rAF(t => { window.__raf++; cb(t); });
  addEventListener('DOMContentLoaded', () => new MutationObserver(m => { window.__muts += m.length; })
    .observe(document.getElementById('main'), { subtree: true, childList: true, attributes: true, characterData: true }));
})();
"""

EVENTS = [
  ('ingest',    'import_media',  {'track_id': 'V2'}, {'track': 'V2', 's': 18.5, 'e': 21.5, 'label': 'new_clip', 'clip': 'new_clip'},
               "[...document.querySelectorAll('[data-panel=tl] .clip .cl')].some(e => e.textContent === 'new_clip')"),
  ('edit',      'split_clip',    {'track_id': 'V1', 'clip_id': 'dune_wide'}, {'at': 11},
               "document.querySelectorAll('[data-panel=tl] .trk')[1].querySelectorAll('.clip').length === 4"),
  ('effects',   'apply_effect',  {'track_id': 'V1', 'clip_id': 'dune_ridge'}, {'effect': 'glow'},
               "[...document.querySelectorAll('[data-panel=tl] .fxchip')].some(e => e.textContent === 'glow')"),
  ('transform', 'set_transform', {'param': 'Scale'}, {'from': '104%', 'to': '110%'},
               "[...document.querySelectorAll('[data-panel=insp] .row')].some(r => r.textContent.includes('110%') && r.textContent.includes('104%'))"),
  ('transform', 'set_property',  {'clip_id': 'clip4', 'param': 'scale'}, {'from': 1.04, 'to': 1.12},
               "[...document.querySelectorAll('[data-panel=agent] .call')].some(r => r.textContent.includes('scale: 104% → 112%') || r.textContent.includes('scale 104→112%'))"),
  ('color',     'grade',         {'clip_id': 'clip_04', 'param': 'Temperature'}, {'from': '6050K', 'to': '6200K'},
               "[...document.querySelectorAll('[data-panel=insp] .row')].some(r => r.textContent.includes('6200K'))"),
  ('audio',     'mute_track',    {'track_id': 'A2'}, {'on': True},
               "!!document.querySelector('[data-panel=tl] .trk.muted .ms .m')"),
  ('text',      'add_caption',   {'track_id': 'CAP'}, {'text': 'Again.', 's': 19.2, 'e': 21.5},
               "[...document.querySelectorAll('[data-panel=tl] .clip.cap .cl')].some(e => e.textContent === 'Again.')"),
  ('3d',        'add_light',     {}, {'intensity': 2.4},
               "[...document.querySelectorAll('[data-panel=insp] .row')].some(r => r.textContent.includes('2.4'))"),
  ('export',    'export_render', {}, {'preset': 'h264_master'},
               "[...document.querySelectorAll('[data-panel=insp] .row')].some(r => r.textContent.includes('h264_master'))"),
  ('other',     'frobnicate_widget', {}, {'knob': 7},
               "[...document.querySelectorAll('[data-panel=insp] .row')].some(r => r.textContent.includes('Knob'))"),
]

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=sim&scenario=m16&still=1'
    fails, rows = [], []
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); await ctx.add_init_script(PROBE)
        pg = await ctx.new_page(); errs = []
        pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.goto(url); await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0'); await pg.wait_for_timeout(400)
        frame = lambda: pg.wait_for_timeout(60)
        BAD = r"""() => { const re = /\b(undefined|NaN|null)\b/; return [...document.querySelectorAll('[data-panel=agent] .call, [data-panel=insp] .row, [data-panel=insp] .sec')].map(e => e.textContent).filter(s => re.test(s)); }"""
        rail = "() => [...document.querySelectorAll('[data-panel=agent] .call .fn')].map(e => e.textContent)"
        for k, (fam, name, target, payload, check) in enumerate(EVENTS + [('error', 'blade_cut', {'track_id': 'V9', 'clip_id': 'nope'}, {'error': 'no such clip'}, None)]):
            cid = f't{k}'
            m0 = await pg.evaluate('__muts')
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': target, 'payload': {}, 'ts': k})
            await frame()
            on_start = (await pg.evaluate('__muts')) > m0 and name in (await pg.evaluate(rail))
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'progress', 'target': target, 'payload': {'pct': 50}, 'ts': k})
            m1 = await pg.evaluate('__muts')
            phase = 'error' if fam == 'error' else 'complete'
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': phase, 'target': target, 'payload': {**payload, 'dur_ms': 120 + k}, 'ts': k})
            await frame()
            on_end = (await pg.evaluate('__muts')) > m1
            hud = await pg.evaluate("document.querySelector('[data-panel=viewer] .hud') ? document.querySelector('[data-panel=viewer] .hud').textContent : ''")
            if fam == 'error':
                effect = await pg.evaluate("!!document.querySelector('[data-panel=agent] .call.err')")
            else:
                effect = await pg.evaluate(check)
            bad = await pg.evaluate(BAD)
            if bad: print('   bad text on rail/inspector:', bad[:3])
            ok = on_start and on_end and effect and name in hud and not bad
            rows.append((fam, name, on_start, on_end, effect, hud))
            print(f"{'PASS' if ok else 'FAIL'}  {fam:<9} {name:<18} rail+DOM on start={on_start!s:<5} DOM on end={on_end!s:<5} effect={effect!s:<5} hud='{hud}'", flush=True)
            if not ok: fails.append(name)
        await pg.wait_for_timeout(2000)
        r0 = await pg.evaluate('__raf'); await pg.wait_for_timeout(3000); r1 = await pg.evaluate('__raf')
        idle = r1 == r0
        print(f"{'PASS' if idle else 'FAIL'}  idle after the burst: {r1 - r0} rAF in 3 s")
        if not idle: fails.append('idle')
        if errs: print('page errors:', errs); fails.append('pageerror')

        # ---- read-only family: quiet HUD read state + result pill + peek ring, nothing mutated, no mode hop ----
        snap = "() => JSON.stringify([__kimo.state.mode, [...document.querySelectorAll('[data-panel=tl] .clip')].length, [...document.querySelectorAll('[data-panel=insp] .row, [data-panel=insp] .sec')].map(e => e.textContent)])"
        s0 = await pg.evaluate(snap)
        await pg.evaluate('e => __kimo.apply(e)', {'tool_name': 'get_transform', 'call_id': 'r1', 'phase': 'start', 'target': {'track_id': 'V1', 'clip_id': 'dune_ridge', 'param': 'Scale'}, 'payload': {'args': {'clip_id': 'dune_ridge'}}})
        await frame()
        peek = await pg.evaluate("[document.querySelectorAll('[data-panel=tl] .clip.peek').length, document.querySelectorAll('[data-panel=insp] .row.peek').length]")
        await pg.evaluate('e => __kimo.apply(e)', {'tool_name': 'get_transform', 'call_id': 'r1', 'phase': 'complete', 'target': {'track_id': 'V1', 'clip_id': 'dune_ridge'}, 'payload': {'dur_ms': 3.2, 'result': {'x': 40, 'y': 0, 'scale': 1.04}}, 'duration_ms': 3.2})
        await frame()
        h1 = await pg.evaluate("(() => { const h = document.querySelector('[data-panel=viewer] .hud'); return h ? [h.className, h.textContent] : ['', ''] })()")
        for k, (nm, res) in enumerate([('list_masks', [{'id': 'm1'}, {'id': 'm2'}]), ('sample_pbr_raw', [0.1, 0.2, 0.3]), ('inspect_color_pipeline', {'working': 'ACEScct', 'view': 'sRGB'})]):
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': nm, 'call_id': f'r{k + 2}', 'phase': 'start', 'target': {}, 'payload': {}})
            await pg.evaluate('e => __kimo.apply(e)', {'tool_name': nm, 'call_id': f'r{k + 2}', 'phase': 'complete', 'target': {}, 'payload': {'dur_ms': 1.0, 'result': res}, 'duration_ms': 1.0})
        await frame()
        h2 = await pg.evaluate("document.querySelector('[data-panel=viewer] .hud') ? document.querySelector('[data-panel=viewer] .hud').textContent : ''")
        s1 = await pg.evaluate(snap); bad = await pg.evaluate(BAD); railn = await pg.evaluate(rail)
        ok_r = (peek[0] >= 1 and 'read' in h1[0] and 'get_transform · x 40, y 0' in h1[1] and s0 == s1 and not bad
                and all(n in railn for n in ['get_transform', 'list_masks', 'sample_pbr_raw', 'inspect_color_pipeline']) and 'working ACEScct, view sRGB' in h2)
        print(f"{'PASS' if ok_r else 'FAIL'}  read-only inspect state: peek clip={peek[0]} row={peek[1]} hud '{h1[1]}' ({h1[0]}) last '{h2}' unchanged mode/clips/inspector={s0 == s1}")
        if not ok_r: fails.append('inspect')

        # ---- transform family: viewer gizmo (box, handles, anchor, previous pose, arrow, readout) + keyframe diamonds ----
        await pg.evaluate('e => __kimo.apply(e)', {'type': 'project_changed', 'rev': 1, 'width': 1920, 'height': 1080})
        X = lambda name, cid, ph, tg, pl: pg.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': ph, 'target': tg, 'payload': pl})
        tgt = {'track_id': 'V1', 'clip_id': 'dune_ridge', 'param': 'scale'}
        await X('set_property', 'g1', 'start', tgt, {'args': {'clip_id': 'dune_ridge', 'property': 'scale', 'value': 1.25}})
        await X('set_property', 'g1', 'complete', tgt, {'dur_ms': 2, 'from': 1, 'to': 1.25, 'param': 'scale'}); await frame()
        g1 = await pg.evaluate("(() => { const v = document.querySelector('[data-panel=viewer]'); const r = v.querySelector('.gzread'); return [v.querySelectorAll('svg.gz rect').length, r ? r.textContent : '', v.querySelectorAll('svg.gz rect[stroke-dasharray]').length]; })()")
        tgx = {'track_id': 'V1', 'clip_id': 'dune_ridge', 'param': 'x'}
        await X('set_property', 'g2', 'start', tgx, {'args': {'clip_id': 'dune_ridge', 'property': 'x', 'value': 480}})
        await X('set_property', 'g2', 'complete', tgx, {'dur_ms': 2, 'from': 0, 'to': 480, 'param': 'x'}); await frame()
        g2 = await pg.evaluate("(() => { const v = document.querySelector('[data-panel=viewer]'); const r = v.querySelector('.gzread'); return [v.querySelectorAll('svg.gz rect[stroke-dasharray]').length, v.querySelectorAll('svg.gz path').length, r ? r.textContent : '', [...document.querySelectorAll('[data-panel=insp] .row')].map(e => e.textContent).find(s => s.startsWith('Position')) || '']; })()")
        tk = {'track_id': 'V1', 'clip_id': 'dune_ridge', 'param': 'opacity'}
        await X('set_keyframes', 'g3', 'start', tk, {'args': {'clip_id': 'dune_ridge', 'property': 'opacity', 'keys': [{'time': 0.5, 'value': 0}, {'time': 1.5, 'value': 1}]}})
        await X('set_keyframes', 'g3', 'complete', tk, {'dur_ms': 1})
        await X('set_keyframe_bezier', 'g4', 'start', tk, {'args': {'clip_id': 'dune_ridge', 'property': 'opacity', 'time': 0.5, 'bezier': [0.34, 1.56, 0.64, 1]}})
        await X('set_keyframe_bezier', 'g4', 'complete', tk, {'dur_ms': 1}); await frame()
        g3 = await pg.evaluate("(() => { const keys = document.querySelectorAll('[data-panel=tl] .clip .ckey'); const r = document.querySelector('[data-panel=viewer] .gzread'); return [keys.length, document.querySelectorAll('[data-panel=tl] .clip .ckey.em').length, r ? r.textContent : '', r ? r.querySelectorAll('svg path').length : 0]; })()")
        await X('set_motion_path', 'g5', 'start', {'clip_id': 'dune_ridge'}, {'args': {'clip_id': 'dune_ridge', 'points': [[-400, 100], [0, -150], [400, 100]]}})
        await X('set_motion_path', 'g5', 'complete', {'clip_id': 'dune_ridge'}, {'dur_ms': 1}); await frame()
        g5 = await pg.evaluate("(() => { const r = document.querySelector('[data-panel=viewer] .gzread'); return [r ? r.textContent : '', document.querySelectorAll('[data-panel=viewer] svg.gz path[fill=\"var(--cream)\"]').length]; })()")
        bad = await pg.evaluate(BAD)
        rr0 = await pg.evaluate('__raf'); await pg.wait_for_timeout(3000); rr1 = await pg.evaluate('__raf')
        ok_g = (g1[0] >= 9 and 'scale 100% → 125%' in g1[1] and g1[2] == 1 and g2[0] == 1 and g2[1] >= 3 and 'x 0 → 480' in g2[2] and '480, 0' in g2[3] and '0, 0' in g2[3]
                and g3[0] >= 2 and g3[1] >= 1 and 'ease [0.34, 1.56, 0.64, 1]' in g3[2] and g3[3] >= 3 and 'motion path · 3 points' in g5[0] and g5[1] >= 3 and not bad and rr1 == rr0)
        print(f"{'PASS' if ok_g else 'FAIL'}  transform gizmo: scale {g1} move {g2} keys {g3} path {g5} rAF after {rr1 - rr0}")
        if not ok_g: fails.append('transform-gizmo')

        # ---- 3D / tracking / lighting / mesh: drawn from the call's own args on the screen it opens ----
        pg5 = await ctx.new_page(); errs5 = []; pg5.on('pageerror', lambda e: errs5.append(str(e)))
        await pg5.goto(url.replace('feed=sim&scenario=m16&still=1', 'feed=off')); await pg5.wait_for_timeout(300)
        async def run3(name, cid, args, result=None):
            await pg5.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': {}, 'payload': {'args': args}})
            await pg5.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'complete', 'target': {}, 'payload': {'dur_ms': 5, 'result': result or {}}})
            await pg5.wait_for_timeout(60)
            return await pg5.evaluate("(() => { const v = document.querySelector('[data-panel=viewer]'); return [__kimo.state.mode, v.querySelectorAll('svg.ov polygon').length, v.querySelectorAll('svg.ov line').length, v.querySelectorAll('svg.ov circle').length, v.textContent]; })()")
        t1 = await run3('track_planar', 'd1', {'clip_id': 'clip_02', 'corners': [.3, .3, .7, .32, .68, .7, .32, .66]}, {'rms_px': 0.7})
        t2 = await run3('track_mesh', 'd2', {'clip_id': 'clip_02', 'corners': [.2, .2, .8, .2, .8, .8, .2, .8], 'nx': 4, 'ny': 3})
        l1 = await run3('set_light', 'd3', {'id': 'key', 'kind': 'point', 'pos': [.3, .25, 1], 'color': [1, .8, .6], 'intensity': 2.4})
        lrows = await pg5.evaluate("[...document.querySelectorAll('[data-panel=lights] *')].map(e => e.textContent).filter(s => s.includes('key')).length")
        m1 = await run3('mesh_lathe', 'd4', {'profile': [[0, 0], [1, 1]], 'segments': 96})
        nodes = await pg5.evaluate("document.querySelector('[data-panel=stack]') ? document.querySelector('[data-panel=stack]').textContent : ''")
        c1 = await run3('set_camera', 'd5', {'cam_pan': 0, 'cam_fov': 40})
        c2 = await run3('set_camera_keyframe', 'd6', {'property': 'cam_pan', 'time': 2, 'value': 35})
        cam = await pg5.evaluate("(() => { const v = document.querySelector('[data-panel=viewer]'); return [v.querySelectorAll('svg.ov ellipse').length, v.textContent.includes('fov 40°'), [...document.querySelectorAll('[data-panel=insp] .row')].map(r => r.textContent).join('|')]; })()")
        bad = await pg.evaluate(BAD)
        ok_3 = (t1[0] == 'tracking' and t1[1] >= 1 and 'solve' not in t1[4] and 'track 0.70 px RMS' in t1[4] and t2[2] >= 5 and l1[0] == 'lighting' and l1[3] >= 1 and lrows >= 1
                and m1[0] == 'modelling' and 'lathe' in nodes and not bad and not errs5 and c1[0] == 'scene3d' and c2[0] == 'scene3d' and cam[0] >= 1 and cam[1] and 'Rotation' in cam[2] and '35' in cam[2])
        print(f"{'PASS' if ok_3 else 'FAIL'}  3d/tracking: planar {t1[:4]} '{t1[4][-30:]}' mesh grid lines {t2[2]} light {l1[:4]} rows {lrows} stack '{nodes[-40:]}' m1 {m1[0]} cam {c1[0]} {c2[0]} {cam} bad {bad[:2]} errs {errs5[:2]}")
        if not ok_3: fails.append('3d')

        # ---- cold real-engine session (?feed=off): events shaped like kimo-mcp's (no `type`) ----
        pg2 = await ctx.new_page(); errs2 = []; pg2.on('pageerror', lambda e: errs2.append(str(e)))
        await pg2.goto(url.replace('feed=sim&scenario=m16&still=1', 'feed=off')); await pg2.wait_for_timeout(300)
        async def call(k, name, target, done, start_args=None):
            cid = f'c{k}'
            await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': target, 'payload': {'args': start_args or {}}, 'ts': k})
            ph = 'error' if 'error' in done else 'complete'
            await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': ph, 'target': target, 'payload': {'dur_ms': 10 + k, **done}, 'duration_ms': 10 + k, 'ts': k})
            await pg2.wait_for_timeout(60)
        T = lambda tr=None, cl=None, pa=None: {'track_id': tr, 'clip_id': cl, 'param': pa}
        tracks_js = "[...document.querySelectorAll('.tl .trk')].map(e => { const r = e.getBoundingClientRect(); return [e.querySelector('.lab') ? e.querySelector('.lab').textContent : '', Math.round(r.y), e.querySelectorAll('.clip').length]; })"
        await call(1, 'add_track', T(), {'changed': {'tracks_added': ['track1']}, 'mutated': True})
        await call(2, 'add_clip', T('track1'), {'clip': 'clip2', 'track': 'track1', 's': 0, 'e': 6, 'label': 'gradient', 'mutated': True})
        before = await pg2.evaluate(tracks_js)
        await call(3, 'add_track', T(), {'changed': {'tracks_added': ['track3']}})
        after = await pg2.evaluate(tracks_js)
        await call(4, 'add_text', T('track3'), {'clip': 'clip4', 'track': 'track3', 's': 0.5, 'e': 5.5, 'label': 'KIMO', 'text': 'KIMO'})
        await call(5, 'add_effect', T('track1', 'clip2'), {'effect': 'temperature', 's': 0, 'e': 6})
        await call(6, 'set_effect_param', T('track1', 'clip2', 'amount'), {'param': 'amount', 'from': 0.0, 'to': 0.35, 's': 0, 'e': 6})
        await call(7, 'set_property', T('track3', 'clip4', 'scale'), {'from': 1, 'to': 1.25, 's': 0.5, 'e': 5.5})
        await call(8, 'split_clip', T('track1', 'clip2'), {'clip': 'clip7', 'track': 'track1', 's': 3, 'e': 6, 'at': 3})
        await call(9, 'frobnicate_widget', T(), {'error': 'unknown tool'})
        final = await pg2.evaluate("__kimo.state.liveTL.tracks.map(t => [t.label, t.clips.length])")
        fx = await pg2.evaluate("__kimo.state.liveTL.tracks.flatMap(t => t.clips.flatMap(c => c.fx || []))")
        labels = await pg2.evaluate("__kimo.state.liveTL.tracks.flatMap(t => t.clips.map(c => c.label))")
        insp = await pg2.evaluate("[...document.querySelectorAll('[data-panel=insp] .row')].map(e => e.textContent)")
        bad2 = await pg2.evaluate(BAD)
        rail2 = await pg2.evaluate("__kimo.state.calls.map(c => [c.name, c.status, c.dur_ms, Object.entries(c.args || {}).map(([k, v]) => k + ': ' + v).join(', ')])")
        ok_cold = (len(before) == 1 and before[0][2] == 1 and len(after) == 2 and after[0][1] == before[0][1]   # V1 did not move when V2 arrived
                   and final[0][1] == 2 and final[1][1] == 1 and 'temperature' in fx and not bad2 and not errs2
                   and not any(re.match(r'^clip\d+$', l or '') for l in labels) and 'KIMO' in labels
                   and any(c[0] == 'set_property' and 'scale: 100% → 125%' in str(c[3]) for c in rail2)
                   and [c[1] for c in rail2] == ['ok'] * 8 + ['error'] and [c[2] for c in rail2] == [10 + k for k in range(1, 10)])
        print(f"{'PASS' if ok_cold else 'FAIL'}  cold engine session: tracks {final} labels {labels} fx {fx} rail {[c[1] for c in rail2]} V1 y {before[0][1] if before else None}->{after[0][1] if after else None}")
        if bad2: print('   bad text:', bad2[:3])
        if not ok_cold: fails.append('cold-session')

        # export job card: progress (frame / total, fps, elapsed, ETA) then a settled history row
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x1', 'phase': 'start', 'target': {}, 'payload': {'args': {'path': '/tmp/k/out.mp4', 'audio': False, 'codec': 'h264'}}})
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x1', 'phase': 'progress', 'target': {}, 'payload': {'current_frame': 72, 'total_frames': 144, 'fps': 41.7, 'pct': 50, 'elapsed_ms': 1700, 'eta_ms': 1700}})
        await pg2.wait_for_timeout(80)
        mid_txt = await pg2.evaluate("document.querySelector('[data-panel=viewer]').textContent + ' | ' + document.querySelector('[data-panel=renders]').textContent")
        TINY = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=='
        await pg2.evaluate('e => __kimo.apply(e)', {'type': 'frame_rendered', 'png_b64': TINY})
        await pg2.wait_for_timeout(80)
        h_mid = await pg2.evaluate("document.querySelector('[data-panel=renders] .tr:not(.h)').getBoundingClientRect().height")
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x1', 'phase': 'complete', 'target': {}, 'payload': {'dur_ms': 3400, 'result': {'path': '/tmp/k/out.mp4', 'duration': 6.0, 'bytes': 4200000}}, 'duration_ms': 3400})
        # wait for the history row to show the job done (cap 3 s) instead of a fixed 80 ms; on a timeout the checks below report what is on screen
        try: await pg2.wait_for_function("() => { const r = document.querySelector('[data-panel=renders] .tr:not(.h)'); return !!r && r.textContent.includes('/tmp/k/out.mp4') && r.textContent.includes('✓ done'); }", timeout=3000)
        except Exception: pass
        end_txt = await pg2.evaluate("document.querySelector('[data-panel=renders]').textContent + ' | ' + document.querySelector('[data-panel=exp]').textContent")
        row1 = await pg2.evaluate("(() => { const r = document.querySelector('[data-panel=renders] .tr:not(.h)'); const i = r.querySelector('img.rthumb'); return [r.getBoundingClientRect().height, i ? i.getAttribute('src').slice(0, 22) : null, r.textContent]; })()")
        # a second job whose engine result carries no size: no size text, no placeholder
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x2', 'phase': 'start', 'target': {}, 'payload': {'args': {'path': '/tmp/k/out2.mp4'}}})
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x2', 'phase': 'complete', 'target': {}, 'payload': {'dur_ms': 900, 'result': {'path': '/tmp/k/out2.mp4', 'duration': 2.5}}, 'duration_ms': 900})
        # wait for the history row to show the job done (cap 3 s) instead of a fixed 80 ms; on a timeout the checks below report what is on screen
        try: await pg2.wait_for_function("() => { const r = document.querySelector('[data-panel=renders] .tr:not(.h)'); return !!r && r.textContent.includes('/tmp/k/out2.mp4') && r.textContent.includes('✓ done'); }", timeout=3000)
        except Exception: pass
        row2 = await pg2.evaluate("document.querySelector('[data-panel=renders] .tr:not(.h)').textContent")
        await pg2.evaluate('e => __kimo.apply(e)', {'tool_name': 'export', 'call_id': 'x3', 'phase': 'start', 'target': {}, 'payload': {'args': {'path': '/home/naman/films/deliveries/kimo_live_square_master.mp4', 'width': 1080, 'height': 1080}}})
        await pg2.wait_for_timeout(80)
        kvfit = await pg2.evaluate("[...document.querySelectorAll('[data-panel=exp] .row')].map(r => { const k = r.querySelector('.k').getBoundingClientRect(), v = r.querySelector('.v').getBoundingClientRect(), p = r.getBoundingClientRect(); return [r.querySelector('.k').textContent, Math.round(v.left - k.right), v.right <= p.right + 0.5]; })")
        ok_x = (all(s in mid_txt for s in ['frame 72 / 144', '42 fps', 'eta 00:02', 'out.mp4', 'rendering 50%']) and '✓ done' in end_txt and 'MP4' in end_txt and 'out.mp4' in end_txt
                and row1[1] == 'data:image/png;base64,' and '/tmp/k/out.mp4' in row1[2] and '00:06' in row1[2] and '4.2 MB' in row1[2] and abs(row1[0] - h_mid) < 0.5
                and all(g >= 12 and inside for _, g, inside in kvfit) and 'out2.mp4' in row2 and '00:03' in row2 and '✓ done' in row2 and ' · ' not in row2 and not re.search(r'undefined|NaN|null|-- ?MB|\? ?MB', row2))
        if not ok_x: print('   checks:', [s in mid_txt for s in ['frame 72 / 144', '42 fps', 'eta 00:02', 'out.mp4', 'rendering 50%']], ['✓ done' in end_txt, 'MP4' in end_txt, 'out.mp4' in end_txt], end_txt[-200:])
        print(f"{'PASS' if ok_x else 'FAIL'}  export job card + history row: mid '{mid_txt[-120:]}' row1 {row1[0]}px (mid {h_mid}) thumb {row1[1]} '{row1[2][:90]}' row2 '{row2[:80]}' kv gaps {[(k, g) for k, g, _ in kvfit]}")
        if not ok_x: fails.append('export-card')
        # ---- burst: 50 calls (start, 3 progress, complete) inside 100 ms ----
        # already on the grading screen, so the count is the burst's own frames (a screen hop springs its splits for
        # ~25 frames; that is measured by tests/spring_test.py)
        await pg2.evaluate("() => __kimo.apply({type: 'mode', mode: 'grading'})"); await pg2.wait_for_timeout(800)
        r0 = await pg2.evaluate('__kimo.metrics.raf')
        await pg2.evaluate("""() => { const t0 = performance.now(); for (let k = 0; k < 50; k++) { const id = 'b' + k, tg = {track_id: 'track1', clip_id: 'clip2', param: 'temperature'};
            __kimo.apply({tool_name: 'grade', call_id: id, phase: 'start', target: tg, payload: {args: {k}}, ts: k});
            for (const pct of [25, 50, 75]) __kimo.apply({tool_name: 'grade', call_id: id, phase: 'progress', target: tg, payload: {pct}, ts: k});
            __kimo.apply({tool_name: 'grade', call_id: id, phase: 'complete', target: tg, payload: {dur_ms: 100 + k, from: 5600 + k, to: 5601 + k}, duration_ms: 100 + k, ts: k}); }
            window.__burst_ms = performance.now() - t0; }""")
        await pg2.wait_for_timeout(500)
        m = await pg2.evaluate('({...__kimo.metrics, burst: window.__burst_ms, qmax_cap: 2048})')
        last = await pg2.evaluate("__kimo.state.calls.slice(-50).map(c => [c.status, c.dur_ms])")
        ok_burst = m['qmax'] <= m['qmax_cap'] and m['raf'] - r0 <= 6 and all(s == 'ok' for s, _ in last) and [d for _, d in last] == [100 + k for k in range(50)]
        print(f"{'PASS' if ok_burst else 'FAIL'}  burst 250 events in {m['burst']:.1f} ms: {m['raf'] - r0} rAF, max queue {m['qmax']} (cap {m['qmax_cap']}), coalesced {m['coalesced']}, dropped {m['dropped']}, 50/50 settled with exact durations")
        if not ok_burst: fails.append('burst')

        # ---- m04 audio family: EQ node, compressor + live gain reduction, Pan, loudness, meter ballistics ----
        pg3 = await ctx.new_page(); errs3 = []; pg3.on('pageerror', lambda e: errs3.append(str(e)))
        await pg3.goto(url.replace('scenario=m16', 'scenario=m04')); await pg3.wait_for_function('window.__kimo && __kimo.metrics.renders > 0'); await pg3.wait_for_timeout(400)
        async def acall(k, name, args, done, mid=None):
            cid = f'a{k}'; tg = {'track_id': 'Dialogue', 'clip_id': None, 'param': None}
            await pg3.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'start', 'target': tg, 'payload': {'args': args}})
            await pg3.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'progress', 'target': tg, 'payload': {'pct': 50, 'args': args}})
            await pg3.wait_for_timeout(50)
            if mid is not None: mid.append(await pg3.evaluate("[...document.querySelectorAll('[data-panel=audio] svg line')].some(l => l.getAttribute('stroke') === 'var(--ember)')"))
            await pg3.evaluate('e => __kimo.apply(e)', {'tool_name': name, 'call_id': cid, 'phase': 'complete', 'target': tg, 'payload': {'dur_ms': 40 + k, 'args': args, **done}})
            await pg3.wait_for_timeout(60)
        snap = "() => ({eq: [...document.querySelectorAll('[data-panel=audio] svg circle')].map(c => c.getAttribute('cx') + ',' + c.getAttribute('cy')).join(' '), audio: document.querySelector('[data-panel=audio]').textContent, loud: document.querySelector('[data-panel=loud]').textContent, meter: [...document.querySelectorAll('[data-panel=loud] .meter > div')].map(d => d.style.height + d.style.bottom).join('|')})"
        s0 = await pg3.evaluate(snap)
        await acall(1, 'set_eq_band', {'band': 2, 'freq': 4200, 'gain': 6.5}, {})
        s1 = await pg3.evaluate(snap)
        gr = []
        await acall(2, 'compress', {'threshold': -22, 'ratio': 4}, {}, gr)
        s2 = await pg3.evaluate(snap)
        await acall(3, 'set_pan', {'pan': -0.3}, {})
        s3 = await pg3.evaluate(snap)
        await acall(4, 'normalize_loudness', {'target_lufs': -16}, {'lufs': -16.0})
        s4 = await pg3.evaluate(snap)
        mid_meter = s4['meter']
        await pg3.wait_for_timeout(2200)
        s5 = await pg3.evaluate(snap)
        ra = await pg3.evaluate('__kimo.metrics.raf'); await pg3.wait_for_timeout(1500); rb = await pg3.evaluate('__kimo.metrics.raf')
        bad3 = await pg3.evaluate(BAD.replace('[data-panel=insp]', '[data-panel=audio]'))
        checks = {'eq node moved': s1['eq'] != s0['eq'], 'gain reduction drawn mid-call': bool(gr and gr[0]),
                  'threshold -22 / ratio 4:1': '-22 dB' in s2['audio'] and '4:1' in s2['audio'], 'pan C → L30': 'L30' in s3['audio'],
                  'LUFS -16.0': '-16.0' in s4['loud'], 'meters moved then settled': mid_meter != s5['meter'], 'idle after decay': ra == rb,
                  'no bad text': not bad3, 'no page errors': not errs3}
        ok_a = all(checks.values())
        print(f"{'PASS' if ok_a else 'FAIL'}  m04 audio: " + ', '.join(f"{k}={'ok' if v else 'NO'}" for k, v in checks.items()))
        if not ok_a: fails.append('m04-audio')
        # ---- frame latch: one engine frame, then 30 calls hopping modes; the viewer never blanks or changes ----
        pg4 = await ctx.new_page(); await pg4.goto(url.replace('feed=sim&scenario=m16&still=1', 'feed=off')); await pg4.wait_for_timeout(300)
        png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
        await pg4.evaluate('e => __kimo.apply(e)', {'type': 'frame_rendered', 'png_b64': png, 'call_id': 'f0'}); await pg4.wait_for_timeout(80)
        src0 = await pg4.evaluate("(document.querySelector('[data-panel=viewer] .frame img') || {}).src || ''")
        hop = ['add_clip', 'add_effect', 'grade', 'compress', 'set_property', 'add_caption', 'export_render', 'add_mesh', 'split_clip', 'normalize_loudness']
        latched = bool(src0)
        for k in range(30):
            nm = hop[k % len(hop)]
            await pg4.evaluate('e => __kimo.apply(e)', {'tool_name': nm, 'call_id': f'h{k}', 'phase': 'start', 'target': {}, 'payload': {'args': {}}})
            await pg4.evaluate('e => __kimo.apply(e)', {'tool_name': nm, 'call_id': f'h{k}', 'phase': 'complete', 'target': {}, 'payload': {'dur_ms': 5}})
            await pg4.wait_for_timeout(40)
            s = await pg4.evaluate("(document.querySelector('[data-panel=viewer] .frame img') || {}).src || ''")
            if s != src0: latched = False; print('   frame changed/blank after', nm, 'mode', await pg4.evaluate('__kimo.state.mode')); break
        print(f"{'PASS' if latched else 'FAIL'}  frame latch: 1 engine frame, 30 calls across modes, viewer src unchanged after every frame")
        if not latched: fails.append('frame-latch')
        await b.close()
    srv.shutdown()
    print('\nRESULT:', 'every tool call visible' if not fails else 'FAILED: ' + ', '.join(fails))
    sys.exit(1 if fails else 0)

asyncio.run(main())
