#!/usr/bin/env python3
"""1,000-clip timeline benchmark: mount, pan/scrub frame times, JS heap, GUI-side edit handling, DOM size.

Builds a live edit timeline from engine-shaped add_clip events (--clips spread over 6 tracks, back to back) and
measures, in the page (headless Chromium, 1376x768):
  (a) mount: all 2,000 add_clip start/complete events handed to the app in one go -> the frame that drew them
      (events in -> DOM committed), and -> the next frame (painted). Target < 100 ms.
  (b) pan / scrub: the timeline zoomed in with ctrl+wheel (~19 s visible, like the mockups' ruler), then real wheel
      input panning continuously across all 391 s and back (~120 Hz trackpad cadence). Frame intervals from a passive
      observer loop on every vsync while panning (p50 / p95 / max), plus the app's own work per frame. Target <= 16.6 ms.
  (c) JS heap (usedJSHeapSize after a forced GC, Chromium only): before loading, after loading 1k clips, after the
      edits, and after a fast scrub (also the raw, pre-GC peak). No runaway growth: after-scrub <= after-load + 2 MB.
  (d) GUI processing per edit under load: one split_clip or move_clip (start + complete) per frame on the 1k
      timeline; the app's frame callback that drains it (reducer + render + DOM write). This is GUI-side time,
      not engine latency. Asserted: p95 AND max < 10 ms for split and for move in every one of --runs runs (default
      3; run 1 on this page, each further run on a fresh page with the same 1k timeline), misses listed. Every DOM
      change is attributed to its panel and frame (MutationObserver): an edit's frame may touch only the timeline and
      the rail (the call's row); the Inspector, viewer HUD and top bar catch up on the next frame with no edit.
  (e) DOM while panning: whole-page element count and timeline .clip / .crun elements sampled on every vsync of the
      full-timeline pan and of the fast scrub (min / p50 / max, every sample with --dom-log); the page must stay at
      <= 400 elements on every sample and the timeline bounded by the visible window, never one element per clip.
  Also: the vertical wheel brings track V6 (below the panel) into view.
Also: zoomed in, back-to-back clips keep a >= 1 px seam and no clip name runs past its clip.
Exit 0 when (d) holds in every run, (e) is bounded, seams hold, V6 is reachable, no page errors and the heap does not run
away; target misses for (a)/(b)/(d) are printed as MISS beside the number.

    python3 crates/kimo-gui/tests/timeline_perf_test.py [--clips 1000] [--edits 240] [--shot out.png]
"""
import argparse, asyncio, functools, http.server, json, random, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
ap = argparse.ArgumentParser(); ap.add_argument('--clips', type=int, default=1000); ap.add_argument('--edits', type=int, default=240); ap.add_argument('--shot'); ap.add_argument('--dom-log'); ap.add_argument('--runs', type=int, default=3); A = ap.parse_args()
DOM_MAX = 400

# every DOM mutation while edits stream, attributed to the panel it lands in (a text-node change in place is
# 'name:text'; header elements by id) and to the frame it happened in: the edit's own frame (the first app frame
# after the edit was handed over, which drains it) or a later one (the catch-up frame that brings the Inspector,
# the viewer HUD and the top bar up to date once no edit is pending). EDIT_SCOPE is all an edit frame may touch:
# the timeline and the rail (the call's row).
MUT_ON = """() => { window.__mut = { edit: {}, after: {} }; window.__editFr = -1; window.__mh = rs => { for (const r of rs) {
  const n = r.target.nodeType === 1 ? r.target : r.target.parentElement, p = n && n.closest('[data-panel]');
  const k = p ? p.dataset.panel + (r.type === 'characterData' ? ':text' : '') : (n && (n.closest('[id]') || {}).id) || '?';
  const b = window.__mut[window.__fr === window.__editFr ? 'edit' : 'after']; b[k] = (b[k] || 0) + 1; } }; window.__mo = new MutationObserver(window.__mh);
  window.__mo.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true }); }"""
MUT_OFF = "() => { window.__mh(window.__mo.takeRecords()); window.__mo.disconnect(); return window.__mut; }"
EDIT_SCOPE = {'tl', 'agent'}
TICKS = {'clock'}   # the header's session clock ticks once a second on its own timer, edits or not

# zoomed in, back-to-back clips: every touching pair keeps a 1 px seam (>= .95 px in layout units, which are 1/64 px), and no clip name runs past its own clip
# (a name that does not fit ends in an ellipsis inside the clip)
SEAMS = """() => { const out = { pairs: 0, overlap: 0, minGap: null, past: 0, ellipsis: 0, labels: 0 };
  for (const t of document.querySelectorAll('[data-panel=tl] .trk')) {
    const cs = [...t.querySelectorAll('.clip:not(.crun)')].map(c => [c, c.getBoundingClientRect()]).sort((a, b) => a[1].left - b[1].left);
    for (let i = 0; i + 1 < cs.length; i++) { const g = cs[i + 1][1].left - cs[i][1].right; if (g < -.5) out.overlap++; else if (g < 3) { out.pairs++; out.minGap = out.minGap == null ? g : Math.min(out.minGap, g); } }
    for (const [c, r] of cs) { const l = c.querySelector('.cl'); if (!l) continue; out.labels++; const q = l.getBoundingClientRect();
      if (q.right > r.right + .5 || q.left < r.left - .5) out.past++;
      if (l.scrollWidth > l.clientWidth && getComputedStyle(l).textOverflow === 'ellipsis') out.ellipsis++; } }
  return out; }"""
PROBE = """(() => { const rAF = window.requestAnimationFrame.bind(window); window.__app = [];
  window.__wheel = []; addEventListener('wheel', e => window.__wheel.push(e.timeStamp), { capture: true, passive: true });
  window.__fr = 0; window.requestAnimationFrame = cb => rAF(t => { window.__fr++; const a = performance.now(); cb(t); const b = performance.now(); window.__app.push([t, b - a, b]); });
  window.__obs = null; window.__watch = () => { const S = window.__obs = []; window.__stop = false; const loop = t => {
    const tl = document.querySelector('[data-panel=tl]');
    S.push([t, document.getElementsByTagName('*').length, tl ? tl.querySelectorAll('.clip:not(.crun)').length : -1, tl ? tl.querySelectorAll('.crun').length : -1]);
    if (!window.__stop) rAF(loop); }; rAF(loop); }; })()"""
class Q(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
def pct(xs, p): xs = sorted(xs); return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))] if xs else 0
def tri(xs): return f"p50 {pct(xs, 50):.1f} / p95 {pct(xs, 95):.1f} / max {max(xs or [0]):.1f} ms"
mb = lambda x: f'{x / 1048576:.2f} MB' if x else 'n/a'
def mark(ok): return 'ok' if ok else 'MISS'

def build():
    evs = [{'type': 'session', 'width': 1920, 'height': 1080}]
    rnd = random.Random(3); ends = [0.0] * 6; clips = []
    for k in range(A.clips):
        t = k % 6; tid = f'V{t + 1}'; s = ends[t]; d = round(rnd.uniform(.6, 4.0), 2); ends[t] = round(s + d, 2); cid = f'c{k + 1}'
        name = f'shot_{k + 1:04d}.mp4'; clips.append((cid, tid, s, s + d))
        evs.append({'tool_name': 'add_clip', 'call_id': f'a{k}', 'phase': 'start', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'args': {'track_id': tid, 'path': f'/m/{name}', 'start': s}}})
        evs.append({'tool_name': 'add_clip', 'call_id': f'a{k}', 'phase': 'complete', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'dur_ms': 1.2, 'result': {'clip_id': cid}, 'clip': cid, 'track': tid, 's': s, 'e': s + d, 'path': f'/m/{name}', 'changed': {'clips_added': [{'clip_id': cid, 'track_id': tid}]}}})
    return evs, ends, clips

SNAP = """() => { const tl = document.querySelector('[data-panel=tl]'); const r = tl ? tl.getBoundingClientRect() : null;
  const labs = tl ? [...tl.querySelectorAll('.trk .lab, .trk .labchip')].filter(e => { const b = e.getBoundingClientRect(); return b.height > 0 && b.top >= r.top && b.bottom <= r.bottom + .5; }).map(e => e.textContent) : [];
  return { heap: performance.memory ? performance.memory.usedJSHeapSize : null, dom: document.getElementsByTagName('*').length,
           clips: tl ? tl.querySelectorAll('.clip:not(.crun)').length : -1, runs: tl ? tl.querySelectorAll('.crun').length : -1, w: tl ? tl.clientWidth : 0,
           box: r ? [r.x, r.y, r.width, r.height] : null, mode: __kimo.state.mode, labels: labs,
           inState: (__kimo.state.liveTL ? __kimo.state.liveTL.tracks : []).reduce((n, t) => n + (t.clips || []).length, 0) }; }"""

async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Q, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    evs, ends, clips = build(); rnd = random.Random(9); ok = True
    async with async_playwright() as p:
        b = await p.chromium.launch(args=['--enable-precise-memory-info']); pg = await b.new_page(viewport={'width': 1376, 'height': 768})
        errs = []; pg.on('pageerror', lambda e: errs.append(str(e)))
        cdp = await pg.context.new_cdp_session(pg)
        async def heap():
            await cdp.send('HeapProfiler.collectGarbage'); await pg.wait_for_timeout(50)
            return await pg.evaluate('performance.memory.usedJSHeapSize')
        await pg.add_init_script(PROBE)
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await pg.wait_for_timeout(600)
        await pg.evaluate('() => __kimo.apply({type: "mode", mode: "edit"})'); await pg.wait_for_timeout(300)
        h0 = await heap()

        # (a) mount: every add_clip event in one go, then the frame that drew them
        await pg.evaluate('() => { window.__app = []; }')
        m = await pg.evaluate("""es => { const t0 = performance.now(); es.forEach(e => __kimo.apply(e)); const t1 = performance.now();
            return new Promise(res => requestAnimationFrame(() => { const drawn = performance.now(); requestAnimationFrame(t => res({ t0, t1, drawn, painted: performance.now() })); })); }""", evs)
        app = await pg.evaluate('window.__app'); fr = [x for x in app if x[2] >= m['t1']]
        mount_work = fr[0][1] if fr else 0
        s0 = await pg.evaluate(SNAP)
        mount_dom = (m['drawn'] - m['t0']); mount_paint = (m['painted'] - m['t0'])
        await pg.wait_for_timeout(1500); s1 = await pg.evaluate(SNAP); h1 = await heap()
        print(f"timeline: {s1['inState']} clips in state on 6 tracks ({max(ends):.0f} s), mode {s1['mode']}, panel {s1['w']} px wide")
        print(f"(a) mount 1k clips: events in -> DOM committed {mount_dom:.1f} ms [{mark(mount_dom < 100)}] (hand-off {m['t1'] - m['t0']:.1f} ms, app frame {mount_work:.1f} ms), -> next frame painted {mount_paint:.1f} ms; "
              f"DOM at mount {s0['dom']} elements ({s0['clips']} .clip + {s0['runs']} .crun), after the add flash {s1['dom']} ({s1['clips']} + {s1['runs']})")

        # (d) GUI processing per split_clip / move_clip, one edit per frame on the 1k timeline, over --runs runs (the
        # first on this page, the rest each on a fresh page that loads the same 1k timeline first). Every DOM change
        # an edit frame makes is recorded by a MutationObserver and attributed to the panel (or header element) it
        # landed in: the edit's repaint must stay in the timeline; outside it only the call's own words may change
        # (its rail row, the Inspector's call card, the viewer HUD caption as text, the header's call count).
        async def edit_run(pg, rnd):
            per = {'split_clip': [], 'move_clip': []}; catch = []
            await pg.evaluate(MUT_ON); await pg.evaluate('() => __watch()')
            for k in range(A.edits):
                cid, tid, s, e = clips[rnd.randrange(len(clips))]; n = 'split_clip' if k % 2 == 0 else 'move_clip'
                if n == 'split_clip':
                    at = round((s + e) / 2, 3); ev = {'tool_name': n, 'call_id': f'e{k}', 'phase': 'complete', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'dur_ms': 1.0, 'result': {'ok': True}, 'at': at, 'clip': f'{cid}_s{k}', 'changed': {'clips_added': [{'clip_id': f'{cid}_s{k}', 'track_id': tid}]}}}
                    st = {'tool_name': n, 'call_id': f'e{k}', 'phase': 'start', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'args': {'clip_id': cid, 'at': at}}}
                else:
                    to = round(s + .25, 3); ev = {'tool_name': n, 'call_id': f'e{k}', 'phase': 'complete', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'dur_ms': 1.0, 'result': {'ok': True}, 'param': 'start', 'from': s, 'to': to, 's': to, 'e': to + (e - s)}}
                    st = {'tool_name': n, 'call_id': f'e{k}', 'phase': 'start', 'target': {'track_id': tid, 'clip_id': cid}, 'payload': {'args': {'clip_id': cid, 'start': to}}}
                await pg.evaluate('() => { window.__app = []; }')
                await pg.evaluate('es => { window.__editFr = window.__fr + 1; es.forEach(e => __kimo.apply(e)); }', [st, ev]); await pg.wait_for_timeout(34)
                app = await pg.evaluate('window.__app')
                if app: per[n].append(app[0][1]); catch.extend(x[1] for x in app[1:])
            await pg.evaluate('() => { window.__stop = true; }'); await pg.wait_for_timeout(50)
            doms = [o[1] for o in await pg.evaluate('window.__obs')]
            return per, await pg.evaluate(MUT_OFF), catch, doms
        runs = [await edit_run(pg, rnd)]
        # names after the splits: every clip on the timeline and the Inspector read as names, never a raw engine id
        # (c475, c475_s0, clip7), and a split's right-hand piece reads "<source name> · split N"
        nm = await pg.evaluate(r"""() => { const RAW = /^(c|clip|t|track|fx|effect|node|n|layer|l|asset|media)[_-]?\d+(?:[_-][a-z]{0,5}\d+|[_-][a-z])*$/i;
          const labs = (__kimo.state.liveTL.tracks || []).flatMap(t => t.clips.map(c => String(c.label)));
          const insp = [...document.querySelectorAll('[data-panel=insp] .v, [data-panel=insp] .sub')].map(e => e.textContent.trim());
          return { n: labs.length, raw: labs.filter(l => RAW.test(l)).slice(0, 5), split: labs.filter(l => / · split \d+$/.test(l)).length,
            ex: labs.filter(l => / · split \d+$/.test(l)).slice(0, 2), inspRaw: insp.filter(l => RAW.test(l)) }; }""")
        names_ok = not nm['raw'] and not nm['inspRaw'] and nm['split'] > 0
        print(f"clip names after the edits: {nm['n']} clips, {len(nm['raw'])} raw ids {nm['raw']}, {nm['split']} split pieces named like {nm['ex']}; Inspector raw ids: {nm['inspRaw']} [{'ok' if names_ok else 'FAIL'}]")
        ok &= names_ok
        await pg.wait_for_timeout(1300); h2 = await heap(); s2 = await pg.evaluate(SNAP)
        for r in range(1, A.runs):
            q = await b.new_page(viewport={'width': 1376, 'height': 768}); q.on('pageerror', lambda e: errs.append(str(e)))
            await q.add_init_script(PROBE)
            await q.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await q.wait_for_timeout(600)
            await q.evaluate('() => __kimo.apply({type: "mode", mode: "edit"})'); await q.wait_for_timeout(300)
            await q.evaluate('es => es.forEach(e => __kimo.apply(e))', evs); await q.wait_for_timeout(1500)
            runs.append(await edit_run(q, random.Random(9 + r))); await q.close()
        for r, (per, mut, catch, doms) in enumerate(runs):
            for n, xs in per.items():
                miss = [round(x, 1) for x in xs if x >= 10]
                good = pct(xs, 95) < 10 and max(xs or [99]) < 10; ok &= good
                print(f"(d) run {r + 1}/{len(runs)}: GUI processing per {n} on the 1k timeline ({len(xs)} events, reducer + render + DOM write, not engine latency): {tri(xs)} "
                      f"[{'ok' if good else 'MISS'} p95 and max < 10]; frames >= 10 ms: {len(miss)}{' ' + str(miss) if miss else ''}")
            stray = {k: v for k, v in mut['edit'].items() if k not in EDIT_SCOPE | TICKS}
            print(f"    DOM changes in the {sum(len(x) for x in per.values())} edit frames, by region: {mut['edit']} [{'ok: timeline + rail only' if not stray else 'FAIL outside the timeline + rail: ' + str(stray)}]")
            print(f"    catch-up frames (Inspector / viewer HUD / top bar, no edit in them): {len(catch)}, work {tri(catch)}; their DOM changes: {mut['after']}")
            print(f"    page DOM on every vsync during the edits ({len(doms)} samples): min {min(doms)} / max {max(doms)} elements [{mark(max(doms) <= DOM_MAX)} <= {DOM_MAX}]")
            ok &= not stray and mut['edit'].get('tl', 0) > 0 and max(doms) <= DOM_MAX
        # (b) + (e) pan: zoom in with ctrl+wheel, then continuous wheel panning across the whole timeline and back
        bx = s2['box']; await pg.mouse.move(bx[0] + bx[2] * .5, bx[1] + bx[3] * .6)
        await pg.keyboard.down('Control')
        for _ in range(16): await pg.mouse.wheel(0, -50); await pg.wait_for_timeout(20)
        await pg.keyboard.up('Control'); await pg.wait_for_timeout(300)
        RUL = '() => [...document.querySelectorAll("[data-panel=tl] .ruler span")].map(e => e.textContent)'
        await pg.mouse.wheel(-100000, 0); await pg.wait_for_timeout(200)    # to the start of the timeline
        z = await pg.evaluate(RUL); ends_seen = []
        sm = await pg.evaluate(SEAMS); seams_ok = sm['pairs'] > 0 and sm['minGap'] >= .95 and sm['past'] == 0
        print(f"clip seams zoomed to {z[0]}..{z[-1]}: {sm['pairs']} back-to-back pairs on screen, smallest gap {sm['minGap'] if sm['minGap'] is None else round(sm['minGap'], 2)} px ({sm['overlap']} pairs overlap after the moves, not counted); "
              f"{sm['labels']} clip names, {sm['past']} running past their clip, {sm['ellipsis']} cut with an ellipsis [{'ok' if seams_ok else 'FAIL'}]")
        ok &= seams_ok
        await pg.evaluate('() => { window.__app = []; window.__wheel = []; __watch(); }')
        STEP = 130
        for direction in (1, -1):                                          # start -> end of the timeline -> start
            for _ in range(int(31000 / STEP)):
                await pg.mouse.wheel(direction * STEP, 0); await pg.wait_for_timeout(8)
            ends_seen.append((await pg.evaluate(RUL))[-1])
        await pg.evaluate('() => { window.__stop = true; }'); await pg.wait_for_timeout(100)
        obs = await pg.evaluate('window.__obs'); app = await pg.evaluate('window.__app')
        # wheel input -> the app frame that drew it: from the event's timestamp to the end of the first app frame
        # that started after it (DOM updated; the paint follows in the same frame)
        lat = await pg.evaluate("""() => { const A = window.__app.map(([t, d, b]) => [b - d, b]); let j = 0; const out = [];
          for (const ts of window.__wheel) { while (j < A.length && A[j][0] < ts) j++; if (j < A.length) out.push(A[j][1] - ts); } return out; }""")
        pint = [y[0] - x[0] for x, y in zip(obs, obs[1:])]; pwork = [w for _, w, _ in app]
        print(f"(b) wheel input -> frame drawn ({len(lat)} wheel events while panning): {tri(lat)}")
        print(f"(b) pan, ruler zoomed to {z[0]}..{z[-1]} visible, panned start -> {ends_seen[0]} (end) -> {ends_seen[1]}; {len(app)} app frames over {(obs[-1][0] - obs[0][0]) / 1000:.1f} s of continuous wheel panning: frame interval {tri(pint)} [{mark(pct(pint, 95) <= 16.7 + .5)}], "
              f"frames over 16.7 ms {sum(1 for x in pint if x > 17.2)} of {len(pint)}; app work per frame {tri(pwork)}")
        doms = [o[1] for o in obs]; tc = [o[2] + max(o[3], 0) for o in obs]; samples = [['pan'] + o for o in obs]
        bounded = max(tc or [0]) <= 6 * max(1, s2['w']) // 2 and max(doms) <= DOM_MAX
        print(f"(e) DOM while panning, {len(obs)} samples (one per vsync): whole page min {min(doms)} / p50 {pct(doms, 50)} / max {max(doms)} elements [{mark(max(doms) <= DOM_MAX)} <= {DOM_MAX}]; "
              f"timeline {min(o[2] for o in obs)}..{max(o[2] for o in obs)} .clip + {min(o[3] for o in obs)}..{max(o[3] for o in obs)} .crun for {s2['inState']} clips in state [{mark(bounded)}]")
        ok &= bounded

        # (c) fast scrub: big wheel deltas back and forth, then the heap
        await pg.evaluate('() => { window.__app = []; __watch(); }')
        for _ in range(6):
            for direction in (1, -1):
                for _ in range(40): await pg.mouse.wheel(direction * 900, 0); await pg.wait_for_timeout(4)
        await pg.evaluate('() => { window.__stop = true; }'); await pg.wait_for_timeout(200)
        raw = await pg.evaluate('performance.memory.usedJSHeapSize'); fwork = [w for _, w, _ in await pg.evaluate('window.__app')]
        obs2 = await pg.evaluate('window.__obs'); d2 = [o[1] for o in obs2]; samples += [['scrub'] + o for o in obs2]
        print(f"(e) DOM during the fast scrub, {len(obs2)} samples: whole page min {min(d2)} / p50 {pct(d2, 50)} / max {max(d2)} elements [{mark(max(d2) <= DOM_MAX)} <= {DOM_MAX}]")
        ok &= max(d2) <= DOM_MAX
        if A.dom_log: json.dump({'cols': ['phase', 'raf_t_ms', 'page_elements', 'tl_clip', 'tl_crun'], 'samples': samples}, open(A.dom_log, 'w'))
        h3 = await heap()
        runaway = h3 > h1 + 2 * 1048576
        print(f"(c) JS heap after GC: before loading {mb(h0)}, after loading 1k clips {mb(h1)}, after {A.edits} edits {mb(h2)}, after fast scrub {mb(h3)} (raw before GC {mb(raw)}) [{mark(not runaway)}]; fast-scrub app work {tri(fwork)}")
        ok &= not runaway

        # vertical: plain wheel steps the rows so V6 comes into view
        before = (await pg.evaluate(SNAP))['labels']
        for _ in range(3): await pg.mouse.wheel(0, 120); await pg.wait_for_timeout(40)
        await pg.wait_for_timeout(200); after = (await pg.evaluate(SNAP))['labels']
        v6 = 'V6' in after and 'V6' not in before
        print(f"vertical wheel: track labels fully in the panel {before} -> {after} [{'V6 reachable' if v6 else 'FAIL'}]")
        ok &= v6
        if A.shot: await pg.screenshot(path=A.shot)
        print('page errors', errs[:2]); await b.close()
    srv.shutdown()
    ok &= not errs
    print('RESULT:', f'page DOM <= {DOM_MAX} on every pan/scrub sample, V6 reachable, heap flat, edits within frame budget' if ok else 'FAIL'); return 0 if ok else 1
if __name__ == '__main__':
    sys.exit(asyncio.run(main()))
