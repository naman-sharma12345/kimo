#!/usr/bin/env python3
"""COV-A reactive test: the thirteen timeline tools that used to hit the generic fallback.

Replays real kimo-mcp captures (tests/ev_cov_a.json, three engine sessions) into a cold page
(?feed=off) and, after each COV-A tool's completion, asserts the tool's OWN change on the screen
the call lands on: not the rail row or HUD pulse every call gets, but the marker tick on the ruler,
the beat ticks on the clip, the key diamonds and pulse curve, the badge, the moved clip, the
multicam spans, the viewer tag, the Inspector rows with the call's values. Every touched row is
checked for raw ids (clip4, track10, marker7, asset2...) and undefined/NaN/null.

    python3 crates/kimo-gui/tests/cov_a_test.py      # exit 1 on failure
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads(Path(__file__).with_name('ev_cov_a.json').read_text())['sessions']

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

# helpers in the page: rows of the Inspector-kind panel on screen, the tl clips, viewer tags
JS = r"""
window.__q = {
  mode: () => __kimo.state.mode,
  rows: () => [...document.querySelectorAll('.kv .row')].filter(r => !r.closest('[data-leaving]')).map(r => r.textContent.replace(/\s+/g, ' ').trim()),
  secs: () => [...document.querySelectorAll('.kv .sec')].map(r => r.textContent.trim()),
  tags: () => [...document.querySelectorAll('.frame .tag')].map(e => e.textContent),
  clips: () => [...document.querySelectorAll('.trk .clip')].map(c => ({ label: (c.querySelector('.cl') || {}).textContent || '', left: parseFloat(c.style.left), width: parseFloat(c.style.width),
      fx: [...c.querySelectorAll('.fxchip')].map(e => e.textContent), beats: c.querySelectorAll('.cbeat').length, keys: c.querySelectorAll('.ckey.em').length })),
  marks: () => ({ live: document.querySelectorAll('.tlmk:not(.gone)').length, gone: document.querySelectorAll('.tlmk.gone').length, rng: document.querySelectorAll('.tlmk.rng:not(.gone)').length,
      cuts: document.querySelectorAll('.tlcut').length, razors: document.querySelectorAll('.trk ~ .razor, .tl > .razor').length }),
  graph: () => { const g = document.querySelector('[data-panel=graph]'); if (!g) return null;
      return { lanes: [...g.querySelectorAll('.labchip')].map(e => e.textContent), diamonds: g.querySelectorAll('svg path[fill="#d9d4c9"]').length,
               ember: g.querySelectorAll('svg path[stroke="var(--ember)"]').length, keys: g.querySelectorAll('svg rect[fill="var(--ember)"]').length }; },
};
"""
RAW = r"\b(clip|track|marker|asset|multicam|captions)\d+\b"

# tool -> list of (description, JS predicate) checked after its Nth completion (index by occurrence)
CHECKS = {
  0: {  # session 1
    ('add_marker', 0): [("ember tick on the ruler", "__q.marks().live === 1 && __q.marks().rng === 0"),
                        ("Inspector names it", "__q.rows().some(r => r.includes('Marker') && r.includes('drop')) && __q.rows().some(r => r.includes('At') && r.includes('1.5 s'))"),
                        ("tick sits at 1.5 s on the ruler", "(() => { const m = document.querySelector('.tlmk'); const lab = [...document.querySelectorAll('.ruler span')]; return !!m && parseFloat(m.style.left) > 57; })()")],
    ('add_marker', 1): [("range marker draws a band", "__q.marks().live === 2 && __q.marks().rng === 1 && parseFloat(document.querySelector('.tlmk.rng').style.width) > 4"),
                        ("Inspector length", "__q.rows().some(r => r.includes('Length') && r.includes('0.5 s'))")],
    ('remove_marker', 0): [("the drop marker fades out, the range stays", "__q.marks().gone === 1 && __q.marks().live === 1 && __q.marks().rng === 1"),
                           ("Inspector from -> removed", "__q.rows().some(r => r.includes('drop · 1.5 s') && r.includes('removed'))")],
    ('detect_beats', 0): [("on the mixing screen", "__q.mode() === 'mixing'"),
                          ("7 beat ticks on cam_a.mp4", "__q.clips().some(c => c.label === 'cam_a.mp4' && c.beats === 7)"),
                          ("tempo on the Audio rows", "__q.rows().some(r => r.includes('Tempo') && r.includes('120.1 bpm')) && __q.rows().some(r => r.includes('Beats') && r.includes('7'))")],
    ('detect_cuts', 0): [("edit screen", "__q.mode() === 'edit'"), ("none found", "__q.rows().some(r => r.includes('Cuts') && r.includes('none found')) && __q.marks().cuts === 0")],
    ('detect_cuts', 1): [("one cut tick through the tracks", "__q.marks().cuts === 1"),
                         ("Inspector hard cut at 2 s on cam_a.mp4", "__q.rows().some(r => r.includes('Hard cut') && r.includes('2 s')) && __q.rows().some(r => r.includes('Source') && r.includes('cam_a.mp4'))")],
    ('bind_beats', 0): [("on the Graph screen", "__q.mode() === 'graph'"),
                        ("Graph lane names clip + property", "__q.graph().lanes.includes('cam_a.mp4 · scale')"),
                        ("8 beat diamonds, an ember pulse curve with 8 hit keys", "__q.graph().diamonds === 8 && __q.graph().ember >= 1 && __q.graph().keys === 8"),
                        ("Inspector hit", "__q.rows().some(r => r.includes('Hit') && r.includes('+10%') && r.includes('pulse')) && __q.rows().some(r => r.includes('Beats') && r.includes('8'))")],
    ('set_time_mode', 0): [("reverse badge on cam_b.mp4", "__q.mode() === 'edit' && __q.clips().some(c => c.label === 'cam_b.mp4' && c.fx.includes('reverse'))"),
                           ("8 key diamonds left on cam_a.mp4 by bind_beats", "__q.clips().some(c => c.label === 'cam_a.mp4' && c.keys === 8)"),
                           ("Inspector normal -> reverse", "__q.rows().some(r => r.includes('Time') && r.includes('normal') && r.includes('reverse'))")],
    ('set_time_mode', 1): [("freeze badge replaces reverse", "__q.clips().some(c => c.label === 'cam_b.mp4' && c.fx.includes('freeze 1.25 s') && !c.fx.includes('reverse'))")],
    ('set_time_mode', 2): [("badge gone on normal", "__q.clips().some(c => c.label === 'cam_b.mp4' && !c.fx.some(f => /reverse|freeze/.test(f)))"),
                           ("Inspector freeze -> normal", "__q.rows().some(r => r.includes('freeze 1.25 s') && r.includes('normal'))")],
    ('sync_clips', 0): [("cam_b.mp4 slides right by 0.5 s", "(() => { const c = __q.clips(), a = c.find(x => x.label === 'cam_a.mp4'), b = c.find(x => x.label === 'cam_b.mp4'); return a && b && b.left - a.left > 4; })()"),
                        ("Inspector 0 s -> 0.5 s", "__q.rows().some(r => r.includes('cam_b.mp4') && r.includes('0 s') && r.includes('0.5 s')) && __q.rows().some(r => r.includes('By') && r.includes('timecode'))")],
    ('add_multicam', 0): [("sources move into one group clip", "(() => { const L = __q.clips().map(c => c.label); return L.includes('cam_a.mp4 · angle 1') && !L.includes('cam_a.mp4') && !L.includes('cam_b.mp4'); })()"),
                          ("multicam badge tag on the viewer", "__q.tags().includes('multicam · 2 angles')"),
                          ("Inspector angles", "__q.rows().some(r => r.includes('Angles') && r.includes('cam_a.mp4, cam_b.mp4'))")],
    ('switch_angle', 0): [("spans: angle 1 then angle 2", "(() => { const L = __q.clips().map(c => c.label).filter(l => l.includes('angle')); return JSON.stringify(L) === JSON.stringify(['cam_a.mp4 · angle 1', 'cam_b.mp4 · angle 2']); })()"),
                          ("viewer tag", "__q.tags().includes('angle 2 · cam_b.mp4')"),
                          ("Inspector from -> to", "__q.rows().some(r => r.includes('Angle') && r.includes('1 · cam_a.mp4') && r.includes('2 · cam_b.mp4'))")],
    ('switch_angle', 1): [("three spans", "(() => { const L = __q.clips().map(c => c.label).filter(l => l.includes('angle')); return JSON.stringify(L) === JSON.stringify(['cam_a.mp4 · angle 1', 'cam_b.mp4 · angle 2', 'cam_a.mp4 · angle 1']); })()")],
    ('match_events', 0): [("Inspector match rows", "__q.rows().some(r => r.includes('Matched') && r.includes('4 of 4')) && __q.rows().some(r => r.includes('Mean offset') && r.includes('−0.06 s')) && __q.rows().some(r => r.includes('Max offset') && r.includes('0.4 s'))")],
    ('remove_silence_cuts', 0): [("Inspector removed", "__q.rows().some(r => r.includes('Removed') && r.includes('2 ranges') && r.includes('0.75 s'))")],
  },
  1: {  # session 2
    ('burn_in', 0): [("captions screen", "__q.mode() === 'captions'"),
                     ("caption clip says burn-in off", "__q.clips().some(c => c.label === 'hello there' && c.fx.includes('burn-in off'))"),
                     ("viewer tag", "__q.tags().includes('captions off in preview + export')"),
                     ("Inspector on -> off", "__q.rows().some(r => r.includes('Burn-in') && r.includes('on') && r.includes('off'))")],
    ('burn_in', 1): [("badge cleared", "__q.clips().some(c => c.label === 'hello there' && !c.fx.includes('burn-in off'))"), ("viewer tag", "__q.tags().includes('captions burned in')")],
    ('transition_render_raw', 0): [("effects screen + viewer tag", "__q.mode() === 'effects' && __q.tags().includes('crossfade · 40%')"),
                                   ("Inspector rows", "__q.rows().some(r => r.includes('Frame') && r.includes('2 x 2')) && __q.rows().some(r => r.includes('Edge') && r.includes('clamp'))")],
  },
  2: {  # session 3: clip at 1 s
    ('detect_cuts', 0): [("clip source: cut at 3 s on the timeline", "__q.marks().cuts === 1 && __q.rows().some(r => r.includes('Hard cut') && r.includes('3 s'))")],
    ('detect_cuts', 1): [("timeline source: two cuts", "__q.marks().cuts === 2")],
    ('detect_cuts', 2): [("file source maps onto the clip (2 s + 1 s)", "__q.marks().cuts === 1 && __q.rows().some(r => r.includes('Hard cut') && r.includes('3 s')) && __q.rows().some(r => r.includes('Source') && r.includes('cam_a.mp4'))")],
    ('remove_silence_cuts', 0): [("the clip splits around the removed range, razor at the joint", "(() => { const c = __q.clips().filter(x => x.label === 'cam_a.mp4'); return c.length === 2 && __q.marks().razors >= 1; })()"),
                                 ("Inspector", "__q.rows().some(r => r.includes('Removed') && r.includes('1 range ·') && r.includes('0.5 s')) && __q.rows().some(r => r.includes('Track') && r.includes('V1'))")],
  },
}
# ---------------------------------------------------------------------------------------------------------
# PROOF (Naman 7:37 / 7:47): one real kimo-mcp session per tool (ev_cov_a.json "proof", capture script cap7.py).
# Each session: two tracks (cam_a.mp4 / cam_b.mp4), then a REAL failed call of the tool (the engine's own error
# event), the successful call, undo, redo. Per tool this asserts:
#   a) the affected entities by their engine ids (the model) and the exact values on screen, before -> after;
#   b) the failed call shows as failed (rail row ✕ + err class, HUD "<tool> failed") and changes NOTHING: the
#      timeline model (flash stamps included) and the Inspector rows / viewer tags are identical right after it,
#      and the tool's panels are byte-identical once settled;
#   c) undo restores the tool's panels byte-for-byte to the state before the call, redo byte-for-byte to the
#      state after it (and the model with them). The four read-only tools (mutated:false in the engine) push no
#      undo step; for them add_marker is undone / redone across their visual, which must come back byte-identical.
# Panels are read in the tool's own screen after the 700 ms flash has run out (the screen is put back each time).
PJS = r"""
window.__p = {
  model: () => { const tl = __kimo.state.liveTL || {}, g = (__kimo.state.views.graph || {}).graph || null;
    return JSON.stringify({ tracks: tl.tracks || [], markers: tl.markers || [], mc: tl._mc || {}, cuts: tl.cuts || null, graph: g }); },
  canon: () => { const tl = __kimo.state.liveTL || {}, g = (__kimo.state.views.graph || {}).graph || null;
    return JSON.stringify({ tracks: tl.tracks || [], markers: (tl.markers || []).filter(m => !m.gone), mc: tl._mc || {}, graph: g && Object.keys(g).length ? g : null }, (k, v) => k === '_flash' ? undefined : v); },   /* flash stamps and faded markers dropped */
  clip: id => { for (const t of (__kimo.state.liveTL || {}).tracks || []) for (const c of t.clips || []) if (c.id === id) return { track: t.label, s: c.s, e: c.e, label: c.label, fx: c.fx || [], keys: (c.keys || []).filter(k => k._beat).map(k => Math.round(k.t * 1000) / 1000), beats: (c.beats && (c.beats.list || c.beats).length) ?? 0 }; return null; },
  track: id => { const t = ((__kimo.state.liveTL || {}).tracks || []).find(t => t.id === id); return t ? (t.clips || []).map(c => [c.label, Math.round(c.s * 1000) / 1000, Math.round(c.e * 1000) / 1000]) : null; },
  marker: id => ((__kimo.state.liveTL || {}).markers || []).filter(m => m.id === id).map(m => ({ name: m.name, t: m.t, gone: !!m.gone })),
  last: () => { const c = __kimo.state.calls[__kimo.state.calls.length - 1] || {}, r = c.id != null ? document.querySelector(`[data-call="${CSS.escape(String(c.id))}"]`) : null;
    return { name: c.name, status: c.status, error: c.error, rowErr: !!r && r.classList.contains('err') }; },
  hud: () => { const h = document.querySelector('[data-panel=viewer] .hud'); return h ? [h.className, h.textContent] : ['', '']; },
  depth: () => ((__kimo.state._covHist || {}).undo || []).length,
};
"""
SCREEN = {'detect_beats': [('mixing', 'tracks'), ('edit', 'tl')], 'bind_beats': [('graph', 'graph'), ('edit', 'tl')], 'burn_in': [('captions', 'tl')],
          'transition_render_raw': [('effects', 'tl')]}
RO = {'detect_cuts', 'detect_beats', 'match_events', 'transition_render_raw'}

def J(x): return json.dumps(x)
# exact values after the successful call; R = the call's engine result, A = its engine args, ids = the session's engine clip ids
def SUCCESS(tool, R, A, ids, extra):
    c0, c1 = J(ids[0]), J(ids[1])
    rows = lambda L: f"JSON.stringify(__q.rows()) === JSON.stringify({J(L)})"
    return {
      'add_marker': [("marker 'drop' at 1.5 s in the model, by the engine's id", f"JSON.stringify(__p.marker({J(R.get('marker_id'))})) === JSON.stringify({J([{'name': 'drop', 't': 1.5, 'gone': False}])})"),
                     ("one ember tick on the ruler (none before)", "__q.marks().live === 1"),
                     ("Inspector: Marker drop, At 1.5 s", rows(['Markerdrop', 'At1.5 s']))],
      'remove_marker': [("the engine's marker 'outro' leaves the model (fading)", f"JSON.stringify(__p.marker({J(A.get('marker_id'))})) === JSON.stringify({J([{'name': 'outro', 't': 2.5, 'gone': True}])})"),
                        ("the tick fades: 0 live, 1 fading", "__q.marks().live === 0 && __q.marks().gone === 1"),
                        ("Inspector: outro · 2.5 s -> removed", rows(['Markeroutro · 2.5 s→removed']))],
      'add_multicam': [(f"both angles leave V1 / V2 ({ids[0]}, {ids[1]} gone from the model)", f"__p.clip({c0}) === null && __p.clip({c1}) === null && JSON.stringify(__p.track('track1')) === '[]' && JSON.stringify(__p.track('track2')) === '[]'"),
                       ("the engine's group track holds one span, angle 1 over 0-4 s", f"JSON.stringify(__p.track({J(R.get('track_id'))})) === JSON.stringify({J([['cam_a.mp4 · angle 1', 0, 4]])})"),
                       ("viewer tag", "JSON.stringify(__q.tags()) === '[\"multicam · 2 angles\"]'"),
                       ("Inspector rows", rows(['Multicam2 angles', 'Anglescam_a.mp4, cam_b.mp4', 'Audiocam_a.mp4', 'Span0–4 s']))],
      'switch_angle': [("group track: angle 1 over 0-2 s, then angle 2 over 2-4 s", f"JSON.stringify(__p.track({J(extra.get('mc_track'))})) === JSON.stringify({J([['cam_a.mp4 · angle 1', 0, 2], ['cam_b.mp4 · angle 2', 2, 4]])})"),
                       ("razor at the switch, viewer tag", "__q.marks().razors === 1 && JSON.stringify(__q.tags()) === '[\"angle 2 · cam_b.mp4\"]'"),
                       ("Inspector: angle 1 · cam_a.mp4 -> 2 · cam_b.mp4 at 2 s", rows(['Angle1 · cam_a.mp4→2 · cam_b.mp4', 'At2 s', 'Audiocam_a.mp4 (fixed)']))],
      'sync_clips': [(f"{ids[1]} (cam_b.mp4) moves 0 -> 0.5 s, {ids[0]} stays at 0", f"__p.clip({c1}).s === 0.5 && __p.clip({c1}).e === 4.5 && __p.clip({c0}).s === 0 && __p.clip({c0}).e === 4"),
                     ("on screen cam_b.mp4 starts 0.5 s (1/8 of the 4 s clip width) right of cam_a.mp4", "(() => { const c = __q.clips(), a = c.find(x => x.label === 'cam_a.mp4'), b = c.find(x => x.label === 'cam_b.mp4'); return Math.abs((b.left - a.left) - a.width / 8) < 1.5; })()"),
                     ("Inspector: cam_a.mp4 0 s, cam_b.mp4 0 s -> 0.5 s", rows(['Synced2 clips', 'Bytimecode', 'cam_a.mp40 s', 'cam_b.mp40 s→0.5 s']))],
      'detect_cuts': [("one dashed cut line through the tracks", "__q.marks().cuts === 1"),
                      ("it sits at 2 s: half way along the 4 s clips", "(() => { const x = parseFloat(document.querySelector('.tlcut').style.left), c = __q.clips()[0]; return Math.abs(x - (c.left + (c.width + 1) / 2)) < 1.5; })()"),
                      ("Inspector: 1 cut, cam_a.mp4, hard cut at 2 s", rows(['Cuts1 found', 'Sourcecam_a.mp4', 'Hard cut2 s']))],
      'detect_beats': [(f"7 beat ticks on {ids[0]} (cam_a.mp4), none on {ids[1]}", f"__p.clip({c0}).beats === 7 && __p.clip({c1}).beats === 0 && __q.clips().find(c => c.label === 'cam_a.mp4').beats === 7 && __q.clips().find(c => c.label === 'cam_b.mp4').beats === 0"),
                       ("Inspector: 7 beats, 120.1 bpm, cam_a.mp4", rows(['Beats7', 'Tempo120.1 bpm', 'Sourcecam_a.mp4']))],
      'bind_beats': [(f"8 beat keys on {ids[0]} at 0, 0.5 ... 3.5 s (120 bpm), none on {ids[1]}", f"JSON.stringify(__p.clip({c0}).keys) === JSON.stringify({J([0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5])}) && __p.clip({c1}).keys.length === 0"),
                     ("Graph lane cam_a.mp4 · scale with 8 beat diamonds and 8 ember hit keys", "JSON.stringify(__q.graph().lanes) === '[\"cam_a.mp4 · scale\"]' && __q.graph().diamonds === 8 && __q.graph().keys === 8"),
                     ("Inspector rows", rows(['Clipcam_a.mp4', 'Propertyscale', 'Hit+10% · pulse', 'Beats8', 'Tempo120 bpm']))],
      'remove_silence_cuts': [("V2 loses 1-1.5 s: cam_b.mp4 0-1 s + 1-3.5 s, V1 untouched", f"JSON.stringify(__p.track('track2')) === JSON.stringify({J([['cam_b.mp4', 0, 1], ['cam_b.mp4', 1, 3.5]])}) && JSON.stringify(__p.track('track1')) === JSON.stringify({J([['cam_a.mp4', 0, 4]])})"),
                              ("Inspector: V2, 1 range · 0.5 s, 1-1.5 s", rows(['TrackV2', 'Removed1 range · 0.5 s', 'Range1–1.5 s']))],
      'set_time_mode': [(f"reverse badge on {ids[1]} (cam_b.mp4) only", f"JSON.stringify(__p.clip({c1}).fx) === '[\"reverse\"]' && __p.clip({c0}).fx.length === 0 && JSON.stringify(__q.clips().find(c => c.label === 'cam_b.mp4').fx) === '[\"reverse\"]'"),
                        ("Inspector: cam_b.mp4, normal -> reverse", rows(['Clipcam_b.mp4', 'Timenormal→reverse']))],
      'match_events': [("Inspector: 4 of 4 -> 4, 2 within ±0.1 s, mean −0.06 s, max 0.4 s", rows(['Matched4 of 4 → 4', 'Within2 · ±0.1 s', 'Mean offset−0.06 s', 'Max offset0.4 s', 'Modenearest']))],
      'transition_render_raw': [("viewer tag crossfade · 40%", "JSON.stringify(__q.tags()) === '[\"crossfade · 40%\"]'"),
                                ("Inspector rows", rows(['Transitioncrossfade', 'Progress40%', 'Edgeclamp', 'Frame2 x 2']))],
      'burn_in': [(f"burn-in off chip on the group's drawn cue ({extra.get('cap_clips', ['?'])[0]})", f"JSON.stringify(__p.clip({J(extra.get('cap_clips', ['?'])[0])}).fx) === '[\"burn-in off\"]' && __q.clips().find(c => c.label === 'hello there').fx.includes('burn-in off')"),
                  ("Inspector: on -> off, 2 cues (the engine's count)", rows(['Burn-inon→off', 'Cues2']))],
    }[tool]

async def proof(b, url, fails):
    ok, done = 0, []
    P = json.loads(Path(__file__).with_name('ev_cov_a.json').read_text())['proof']
    for tool, events in P.items():
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); await ctx.add_init_script(JS); await ctx.add_init_script(PJS)
        pg = await ctx.new_page(); errs = []
        pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.goto(url); await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
        scr = SCREEN.get(tool, [('edit', 'tl')])
        async def panels():
            await pg.wait_for_timeout(720)   # flash stamps run out (FLASH_MS 700)
            out = []
            for m, name in scr:
                await pg.evaluate('m => __kimo.apply({ type: "mode", mode: m })', m); await pg.wait_for_timeout(260)
                out.append(await pg.evaluate('p => { const e = document.querySelector(`[data-panel=${p}]`); return e ? e.innerHTML : "<missing>" }', name))
            if len(scr) > 1: await pg.evaluate('m => __kimo.apply({ type: "mode", mode: m })', scr[0][0]); await pg.wait_for_timeout(260)   # back on the tool's own screen
            await pg.evaluate('() => { __kimo.state.explicitMode = false }')
            return out
        st = lambda: pg.evaluate("() => ({ model: __p.model(), canon: __p.canon(), rows: JSON.stringify(__q.rows()), tags: JSON.stringify(__q.tags()), depth: __p.depth() })")
        def check(desc, res):
            nonlocal ok
            if res is True: ok += 1; print(f'PASS  proof {tool}: {desc}')
            else: fails.append(f'proof {tool}: {desc}'); print(f'FAIL  proof {tool}: {desc} -> {res}')
        ids, extra = [], {}
        S0 = D0 = None
        for i, ev in enumerate(events):
            tn, ph = ev.get('tool_name'), ev.get('phase')
            if tn == tool and ph == 'start' and S0 is None:   # just before the failing call
                D0 = await panels(); S0 = await st()
            await pg.evaluate('e => __kimo.apply(e)', ev)
            if ph != 'complete' and ph != 'error': continue
            P_ = ev.get('payload') or {}; R = P_.get('result') if isinstance(P_.get('result'), dict) else {}
            if tn == 'add_clip': ids.append(R.get('clip_id'))
            if tn == 'add_multicam' and ph == 'complete' and tool != 'add_multicam': extra['mc_track'] = R.get('track_id')
            if tn == 'add_captions': extra['cap_clips'] = R.get('clip_ids')
            if tn == tool and ph == 'error':
                await pg.wait_for_timeout(60)
                L = await pg.evaluate('__p.last()'); H = await pg.evaluate('__p.hud()'); S1 = await st()
                check(f"real failed call shows as failed: its rail row in the error state ({P_.get('code')}: {P_.get('error')})", L['name'] == tool and L['status'] == 'error' and L['rowErr'] and L['error'] == P_.get('error') or L)
                check("HUD says it failed", ('err' in H[0] and H[1].endswith(f'{tool} failed')) or H)
                check("no state mutation: timeline model identical (flash stamps included)", S1['model'] == S0['model'])
                check("no fake visual change: Inspector rows and viewer tags identical", (S1['rows'], S1['tags']) == (S0['rows'], S0['tags']) or [S0['rows'], S1['rows'], S0['tags'], S1['tags']])
                D1 = await panels(); check(f"panels byte-identical after the failure ({', '.join(f'{m}:{n}' for m, n in scr)})", D1 == D0)
                S1 = await st()
            elif tn == tool and ph == 'complete':
                await pg.wait_for_timeout(60)
                if tool in RO: check(f"read-only in the engine (mutated false), no undo step in the window", P_.get('mutated') is False and (await pg.evaluate('__p.depth()')) == S1['depth'])
                for desc, js in SUCCESS(tool, R, (ev.get('payload') or {}).get('args') or next((e['payload'].get('args') for e in events[:i] if e.get('call_id') == ev.get('call_id') and e.get('phase') == 'start'), {}), ids, extra):
                    try: res = await pg.evaluate(js)
                    except Exception as e: res = f'error {e}'
                    if res is not True: res = [res, await pg.evaluate("() => ({ rows: __q.rows(), clips: __q.clips(), marks: __q.marks(), tags: __q.tags(), graph: __q.graph() })")]
                    check(desc, res)
                D2 = await panels(); S2 = await st()
                if tool not in ('match_events', 'transition_render_raw'): check("its visual is on the panels (they differ from before the call)", D2 != D1)
            elif tn == 'add_marker' and tool in RO and ph == 'complete':
                D2b = await panels(); S2b = await st()
            elif tn == 'undo':
                D3 = await panels(); S3 = await st()
                if tool in RO: check("undo of the next edit restores the panels byte-for-byte, the tool's own visual included", D3 == D2); check("  ... and the model", S3['canon'] == S2['canon'])
                else: check("undo restores the panels byte-for-byte to before the call", D3 == D1); check("  ... and the model", S3['canon'] == S1['canon'])
            elif tn == 'redo':
                D4 = await panels(); S4 = await st()
                if tool in RO: check("redo restores the panels byte-for-byte to after the next edit", D4 == D2b); check("  ... and the model", S4['canon'] == S2b['canon'])
                else: check("redo restores the panels byte-for-byte to after the call", D4 == D2); check("  ... and the model", S4['canon'] == S2['canon'])
        if errs: fails.append(f'proof {tool}: page errors {errs[:3]}')
        done.append(tool); await ctx.close()
    return ok, done

COV = set("add_marker remove_marker add_multicam switch_angle sync_clips detect_cuts detect_beats bind_beats remove_silence_cuts set_time_mode match_events transition_render_raw burn_in".split())

async def main():
    from playwright.async_api import async_playwright
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'
    fails, seen, ok, ran = [], set(), 0, set()
    async with async_playwright() as p:
        b = await p.chromium.launch()
        try:
            for si, events in enumerate(EV):
                ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); await ctx.add_init_script(JS)
                pg = await ctx.new_page(); errs = []
                pg.on('pageerror', lambda e: errs.append(str(e)))
                await pg.goto(url); await pg.wait_for_function('window.__kimo && __kimo.metrics.renders > 0')
                count = {}
                for ev in events:
                    tn = ev.get('tool_name')
                    snap = None
                    if tn in COV and ev.get('phase') == 'complete':
                        snap = await pg.evaluate("() => ({ rows: __q.rows().join('|'), clips: JSON.stringify(__q.clips()), marks: JSON.stringify(__q.marks()), tags: __q.tags().join('|'), graph: JSON.stringify(__q.graph()) })")
                    await pg.evaluate('e => __kimo.apply(e)', ev)
                    if snap is None: continue
                    await pg.wait_for_timeout(70)
                    k = count.get(tn, 0); count[tn] = k + 1; seen.add(tn)
                    after = await pg.evaluate("() => ({ rows: __q.rows().join('|'), clips: JSON.stringify(__q.clips()), marks: JSON.stringify(__q.marks()), tags: __q.tags().join('|'), graph: JSON.stringify(__q.graph()) })")
                    if after == snap: fails.append(f's{si} {tn}#{k}: nothing on screen changed')
                    bad = await pg.evaluate(f"""() => {{ const re = /{RAW}|\\b(undefined|NaN|null)\\b/; return [...__q.rows(), ...__q.secs().filter(s => !/^[a-z_]+$/.test(s)), ...__q.tags(), ...__q.clips().map(c => c.label + ' ' + c.fx.join(' '))].filter(s => re.test(s)); }}""")
                    if bad: fails.append(f's{si} {tn}#{k}: raw id / undefined on screen: {bad[:3]}')
                    ran.add((si, tn, k))
                    for desc, js in CHECKS.get(si, {}).get((tn, k), []):
                        try: res = await pg.evaluate(js)
                        except Exception as e: res = f'error {e}'
                        if res is True: ok += 1; print(f'PASS  s{si} {tn}#{k}: {desc}')
                        else:
                            fails.append(f's{si} {tn}#{k}: {desc}')
                            print(f'FAIL  s{si} {tn}#{k}: {desc}\n      rows={await pg.evaluate("__q.rows()")}\n      clips={await pg.evaluate("__q.clips()")}\n      marks={await pg.evaluate("__q.marks()")} tags={await pg.evaluate("__q.tags()")} mode={await pg.evaluate("__q.mode()")} graph={await pg.evaluate("__q.graph()")}')
                rail = await pg.evaluate(f"() => __kimo.state.calls.filter(c => /^({'|'.join(sorted(COV))})$/.test(c.name)).map(c => c.name + ' ' + JSON.stringify(c.args || {{}}))")
                badr = [r for r in rail if __import__('re').search(RAW + r'|undefined|NaN|"\s*→\s*,', r)]
                if badr: fails.append(f's{si}: rail args carry raw ids / junk: {badr[:3]}')
                else: ok += 1; print(f'PASS  s{si} rail: {len(rail)} COV-A calls, args read as names')
                if errs: fails.append(f's{si}: page errors {errs[:3]}')
                await ctx.close()
            pok, pdone = await proof(b, url, fails); ok += pok
            if sorted(pdone) != sorted(COV): fails.append(f'proof sessions missing for {sorted(COV - set(pdone))}')
        finally:
            await b.close(); srv.shutdown()
    unrun = [f's{si} {tn}#{k}' for si, c in CHECKS.items() for (tn, k) in c if (si, tn, k) not in ran]
    if unrun: fails.append(f'checks that never ran: {unrun}')
    missing = sorted(COV - seen)
    if missing: fails.append(f'tools never exercised: {missing}')
    print(f'\ncov_a: {ok} checks passed, {len(seen)}/13 tools exercised, proof sessions (real failure, exact values, undo/redo byte-for-byte) {len(pdone)}/13, {len(fails)} failures')
    for f in fails: print('  -', f)
    sys.exit(1 if fails else 0)

asyncio.run(main())
