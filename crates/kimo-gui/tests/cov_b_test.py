#!/usr/bin/env python3
"""Coverage lane B: every graph-editor / motion / measure tool has its own reactive visual.

Replays tests/ev_engine_covb.json (a real kimo-mcp capture: import, add_clip, add_text, add_text_animator,
apply_motion_preset, set_keyframe x3, set_expression, bake_property, simplify_curve, smooth_curve, move_key,
remove_key, copy_curve, set_motion_path, remove_motion_path, set_animator_keyframe, motion_extract,
motion_diff, solve_constraints, compute_text_bounds, add_managed_linear_effect, raycast_mesh_raw,
compile_scene3d_descriptor) into a live page (feed=off). For each of the 17 tools it asserts a
tool-specific DOM change that was NOT there before the call (not the rail row or HUD pulse every call
gets), that the call landed on its own screen, that no raw id / undefined / NaN reaches the Inspector,
and that the router files the tool under a dedicated family. preview_subject_match succeeds only on a clip with an
identity transform, with mask layer ids as its regions and the colour pipeline off; ev_engine_covb.json has only its
refusal, so its visual is checked on a replay of tests/ev_engine_covb_hist.json, where the engine really answered it.
No event in this test is simulated.

    python3 crates/kimo-gui/tests/cov_b_test.py      # exit 1 on failure
"""
import asyncio, functools, http.server, json, socketserver, sys, threading
from pathlib import Path
CRATE = Path(__file__).resolve().parents[1]
EV = json.loads(Path(__file__).with_name('ev_engine_covb.json').read_text())

ROWS = "() => [...document.querySelectorAll('.panel .kv .row')].map(e => e.textContent)"
CHIPS = "() => [...document.querySelectorAll('[data-panel=graph] .labchip')].map(e => e.textContent)"
G = "[data-panel=graph] svg"
# tool -> (screen, JS predicate that must be false before the call and true after it)
CHECKS = {
  'apply_motion_preset': ('graph', f"""(() => {{ const c = ({CHIPS})(); return ['position x', 'position y', 'scale', 'rotation', 'opacity'].every(p => c.some(x => x.startsWith(p))) && document.querySelectorAll('{G} path.cvb-cur').length === 5 && document.querySelectorAll('{G} path.cvb-em').length === 1 && ({ROWS})().some(r => r.startsWith('Keys') && r.includes('19 each (8 drawn)')) && ({ROWS})().some(r => r.startsWith('Preset') && r.includes('pop_in')) && ({ROWS})().some(r => r.startsWith('Frames') && r.includes('19')); }})()"""),
  'bake_property': ('graph', f"""(() => ({CHIPS})().some(x => x === 'rotation · 25 keys') && ({ROWS})().some(r => r.startsWith('Expression') && r.includes('wiggle') && r.includes('dropped')) && ({ROWS})().some(r => r.startsWith('Baked') && r.includes('25 keys') && r.includes('0–2 s')) && ({ROWS})().some(r => r.startsWith('Max error') && r.includes('0.133')))()"""),
  'simplify_curve': ('graph', f"""(() => ({CHIPS})().some(x => x === 'rotation · 7 keys') && ({ROWS})().some(r => r.startsWith('Keys') && r.includes('25') && r.includes('→') && r.endsWith('7')) && document.querySelectorAll('{G} path.cvb-prev').length === 1 && document.querySelectorAll('{G} path.cvb-em').length === 1)()"""),
  'smooth_curve': ('graph', f"""(() => ({ROWS})().some(r => r.startsWith('Sigma') && r.includes('2 keys')) && ({ROWS})().some(r => r.startsWith('Max change')) && document.querySelector('{G} path.cvb-prev[stroke-dasharray]') && document.querySelectorAll('{G} path.cvb-em').length === 1 && ({CHIPS})()[0] === 'position x')()"""),
  'move_key': ('graph', f"""(() => ({ROWS})().some(r => r.startsWith('Key time') && r.includes('1 s') && r.includes('→') && r.includes('1.2 s')) && ({ROWS})().some(r => r.startsWith('Value') && r.includes('39.968') && r.includes('150')) && document.querySelectorAll('{G} rect[fill=none][stroke]').length === 1 && !!document.querySelector('{G} path.cvb-new'))()"""),
  'remove_key': ('graph', f"""(() => ({ROWS})().some(r => r.startsWith('Keys') && r.includes('3') && r.includes('→') && r.endsWith('2')) && ({ROWS})().some(r => r.startsWith('Removed') && r.includes('2 s')) && document.querySelectorAll('{G} rect[fill=none][stroke]').length === 1)()"""),
  'copy_curve': ('graph', f"""(() => ({CHIPS})().join('|') === 'position x|position y' && document.querySelectorAll('{G} path.cvb-em').length === 1 && ({ROWS})().some(r => r.startsWith('Time offset') && r.includes('+0.5 s')) && ({ROWS})().some(r => r.startsWith('Value') && r.includes('×0.5')) && ({ROWS})().some(r => r.startsWith('From') && r.includes('shot_') === false && !/clip\\d/.test(r)))()"""),
  'remove_motion_path': ('graph', """(() => !!document.querySelector('[data-panel=viewer] svg.gz path.gone') && document.querySelectorAll('[data-panel=viewer] svg.gz path[fill=none][stroke-width="1.3"]').length === 3 && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Motion path') && r.textContent.includes('3 points') && r.textContent.includes('none')))()"""),
  'set_animator_keyframe': ('graph', f"""(() => ({CHIPS})().some(x => x === 'rise · tracking') && ({ROWS})().some(r => r.startsWith('Value') && r.endsWith('12')) && ({ROWS})().some(r => r.startsWith('Ease') && r.includes('out')) && !!document.querySelector('{G} path.cvb-new'))()"""),
  'motion_extract': ('graph', f"""(() => ({CHIPS})().join('|') === 'x (px)|y (px)|scale|rotation (°)' && ({ROWS})().some(r => r.startsWith('Frames') && r.includes('11')) && ({ROWS})().some(r => r.startsWith('Applied to') && !/clip\\d/.test(r)))()"""),
  'motion_diff': ('graph', f"""(() => ({CHIPS})().some(x => x.startsWith('y Δ 3.05 px')) && ({ROWS})().some(r => r.startsWith('Worst frame') && r.includes('3.28 px')) && ({ROWS})().some(r => r.startsWith('Scale RMS') && r.includes('1.36%')) && !!document.querySelector('[data-panel=graph] .playhead'))()"""),
  'solve_constraints': (None, """(() => { const t = [...document.querySelectorAll('[data-panel=viewer] svg.ov')].map(e => e.textContent).join('|'); return t.includes('title 800×120') && t.includes('logo 200×200') && t.includes('cta 400×80') && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Satisfied') && r.textContent.includes('3 of 3')); })()"""),
  'compute_text_bounds': ('captions', """(() => { const t = [...document.querySelectorAll('[data-panel=viewer] svg.ov')].map(e => e.textContent).join('|'); return t.includes('358×110') && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Font') && r.textContent.includes('DejaVuSans (substituted)')) && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Size') && r.textContent.includes('358 × 110')); })()"""),
  'add_managed_linear_effect': ('effects', """(() => [...document.querySelectorAll('[data-panel=tl] .fxchip')].some(e => e.textContent === '{kind}') && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Effect') && r.textContent.includes('{kind}'.replace('_', ' '))) && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('State') && r.textContent.includes('working linear')))()"""),
  'raycast_mesh_raw': ('modelling', """(() => [...document.querySelectorAll('[data-panel=stack]')].some(e => e.textContent.includes('raycast · tri 1')) && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Barycentric') && r.textContent.includes('0.4, 0.3, 0.3')))()"""),
  'compile_scene3d_descriptor': ('scene3d', """(() => { const t = (document.querySelector('[data-panel=scene]') || {}).textContent || ''; return t.includes('primitive 1') && t.includes('primitive 2') && [...document.querySelectorAll('.panel .kv .row')].some(r => r.textContent.startsWith('Triangles') && r.textContent.includes('236')); })()"""),
  'preview_subject_match': ('grading', """(() => { const r = [...document.querySelectorAll('.panel .kv .row')].map(e => e.textContent); return r.some(x => x.startsWith('Skin') && x.includes('face → ref face')) && r.some(x => x.startsWith('Skin L*') && x.includes('→') && x.includes('(ref ')) && r.some(x => x.startsWith('Preview') && x.includes('nothing changed')); })()"""),
}

# ---------------------------------------------------------------------------------------------------------------
# Phase 2 (Naman 7:37 / 7:47): ids + before -> after, real failed calls, undo / redo.
# tests/ev_engine_covb_hist.json is one real kimo-mcp session (k4-kimo-mcp-bin over stdio, KIMO_SPECTATOR=file:):
# for each of the 17 tools, a call the engine REFUSED (its own error event), then the real call, then the engine's
# own undo and redo. Asserted per tool:
#  (a) the affected entities by name (clip / animator / items; never a raw id) and the exact before -> after values,
#      each expected value computed here from the engine's own payload (from / to / changed / result), not from the GUI;
#  (b) the refused call shows as failed (its rail row in the error state with the engine's words, the HUD "failed"),
#      leaves every screen's state exactly as it was (all views, the timeline, the remembered key lists, JSON-equal)
#      and leaves the tool's own screen's panel DOM byte-for-byte as it was (no fake change);
#  (c) the engine's undo puts the screen's panel DOM back byte-for-byte as it was before the step, and the redo puts
#      it back byte-for-byte as it was after; a read-only tool is not a step (the engine's undo_left does not move),
#      so the undo after it undoes the step before it, exactly, and the redo restores the read's panel exactly.
# Panel DOM = innerHTML of every panel of the screen except the Agent rail (the call log, which lists the undo).
# The page runs on a paused clock (page.clock): the 0.7 s flashes, the 1.6 s HUD and the 2.4 s gizmo are run
# out before each read, so two reads of the same state are the same bytes.
EVH = json.loads(Path(__file__).with_name('ev_engine_covb_hist.json').read_text())
TOOLS = list(CHECKS)
SETTLE = 2800

def jsnum(x):   # the GUI's number format (fmtVal): 3 decimals, no float noise
    v = round(x * 1000) / 1000
    s = repr(float(v)); return s[:-2] if s.endswith('.0') else s
def sec_(x): return jsnum(x) + ' s'
def nkeys(L):  # the engine's key list, possibly cut short ("…+N")
    n = 0
    for k in L or []:
        if isinstance(k, dict): n += 1
        elif isinstance(k, str) and '+' in k: n += int(''.join(c for c in k.split('+')[-1] if c.isdigit()) or 0)
    return n
PROPL = {'x': 'position x', 'y': 'position y', 'scale': 'scale', 'rotation': 'rotation', 'opacity': 'opacity'}

def expectations(evs):
    """tool -> (sec header, [exact Inspector rows "k" + "from→to" | "k" + "v"], extra JS check or None), from the payloads."""
    names, assets, anim, X, A, masks = {}, {}, {}, {}, {}, {}
    starts = {e['call_id']: (e.get('payload') or {}).get('args') or {} for e in evs if e.get('phase') == 'start'}
    for e in evs:
        if e.get('phase') != 'complete': continue
        n, P = e['tool_name'], e['payload']; R = P.get('result') or {}; a = starts.get(e['call_id'], {})
        if n == 'import_media': assets[R['asset_id']] = a['path'].split('/')[-1]
        if n == 'add_clip': names[R['clip_id']] = assets[a['asset_id']]
        if n == 'add_text': names[R['clip_id']] = a['text']
        if n == 'add_text_animator': anim[R['animator_id']] = a['preset']
        if n == 'add_mask': masks[R['mask_id']] = a['name']
        if n in TOOLS and n not in X: X[n] = (a, P, R, e.get('target') or {})
    ex = {}
    a, P, R, T = X['apply_motion_preset']; ch = P['changed']['clip']; ap = R['applied'][0]
    props = [p for p in ['x', 'y', 'scale', 'rotation', 'opacity'] if f'props.{p}.keys' in ch]
    nb, na = nkeys(ch['props.x.keys'][0]), nkeys(ch['props.x.keys'][1])
    ex['apply_motion_preset'] = (f"{names[T['clip_id']]} · motion preset", ['Preset' + ap['preset'], 'Frames' + str(ap['frames']), 'Duration' + sec_(ap['duration']), 'Channels' + ', '.join(props), f"Keys{nb}→{na} each" + (' (8 drawn)' if na > 8 else '')], ('chips', [PROPL[p] for p in props]))
    a, P, R, T = X['bake_property']; ch = P['changed']['clip']; ops = ' + '.join(o['op'] for o in ch['props.rotation.expr'][0])
    ex['bake_property'] = (f"{names[T['clip_id']]} · bake", ['Property' + PROPL[a['property']], f"Keys{nkeys(ch['props.rotation.keys'][0])}→{R['keys']}", f"Expression{ops}→dropped", f"Baked{R['keys']} keys · {jsnum(a['start'])}–{jsnum(a['end'])} s"], None)
    a, P, R, T = X['simplify_curve']
    ex['simplify_curve'] = (f"{names[T['clip_id']]} · simplify curve", ['Property' + PROPL[a['property']], f"Keys{R['before']}→{R['after']}", 'Tolerance' + jsnum(a['tolerance'])], None)
    a, P, R, T = X['smooth_curve']; fr, to = P['from'], P['to']; d = [abs(t['v'] - f['v']) for f, t in zip(fr, to)]; i = d.index(max(d))
    ex['smooth_curve'] = (f"{names[T['clip_id']]} · smooth curve", ['Property' + PROPL[a['property']], f"Value @ {sec_(to[i]['t'])}{jsnum(fr[i]['v'])}→{jsnum(to[i]['v'])}"], None)
    a, P, R, T = X['move_key']; fr, to = P['from'], P['to']
    old = [k for k in fr if not any(abs(k['t'] - q['t']) < 1e-9 and abs(k['v'] - q['v']) < 1e-9 for q in to)][0]
    new = [k for k in to if not any(abs(k['t'] - q['t']) < 1e-9 and abs(k['v'] - q['v']) < 1e-9 for q in fr)][0]
    ex['move_key'] = (f"{names[T['clip_id']]} · move key", ['Property' + PROPL[a['property']], f"Key time{sec_(old['t'])}→{sec_(new['t'])}", f"Value{jsnum(old['v'])}→{jsnum(new['v'])}"], None)
    a, P, R, T = X['remove_key']; fr, to = P['from'], P['to']; gone = [k for k in fr if not any(abs(k['t'] - q['t']) < 1e-9 for q in to)][0]
    ex['remove_key'] = (f"{names[T['clip_id']]} · remove key", ['Property' + PROPL[a['property']], f"Removed{sec_(gone['t'])} · {jsnum(gone['v'])}", f"Keys{len(fr)}→{len(to)}"], None)
    a, P, R, T = X['copy_curve']
    had = nkeys(X['apply_motion_preset'][1]['changed']['clip'][f"props.{a['to_property']}.keys"][1])   # the target's keys before: the preset's, as the engine reported them
    ex['copy_curve'] = (f"{names[a['to_clip']]} · paste curve", [f"From{names[a['from_clip']]} · {PROPL[a['from_property']]}", f"To{names[a['to_clip']]} · {PROPL[a['to_property']]}", f"Time offset+{sec_(a['time_offset'])}", f"Value×{jsnum(a['value_scale'])}", f"Keys{had}→{R['keys']}"], None)
    a, P, R, T = X['remove_motion_path']; pts = P['changed']['clip']['motion_path.points'][0]
    ex['remove_motion_path'] = (f"{names[T['clip_id']]} · motion path", [f"Motion path{len(pts)} points→none"], None)
    a, P, R, T = X['set_animator_keyframe']
    ex['set_animator_keyframe'] = (f"{names[T['clip_id']]} · text animator", ['Animator' + anim[a['animator_id']], 'Property' + a['prop'], 'Key time' + sec_(a['time']), 'Value' + jsnum(a['value']), 'Ease' + a['ease'], f"Keys{nkeys(P['from'])}→{nkeys(P['to'])}"], ('chips', [f"{anim[a['animator_id']]} · {a['prop']}"]))
    a, P, R, T = X['motion_extract']
    ex['motion_extract'] = (f"{names[T['clip_id']]} · motion extract", ['Applied to' + names[a['target_clip_id']], 'Frames' + str(R['frames']) + (f" ({min(8, R['frames'])} drawn)" if R['frames'] > 8 else '')], None)
    a, P, R, T = X['motion_diff']; ch2 = R['channels']
    ex['motion_diff'] = (f"{names[a['clip_a']]} · vs · {names[a['clip_b']]}", [f"Y RMS{jsnum(round(ch2['dy_px']['rms_diff'] * 100) / 100)} px · r {jsnum(round(ch2['dy_px']['correlation'] * 100) / 100)}"], None)
    a, P, R, T = X['solve_constraints']
    ex['solve_constraints'] = ('layout solve', [f"Items{len(a['items'])}", f"Satisfied{len(a['constraints'])} of {len(a['constraints'])}"], ('overlay', [f"{it['id']} {it['w']}×{it['h']}" for it in a['items']]))
    a, P, R, T = X['compute_text_bounds']
    ex['compute_text_bounds'] = (f"{names[T['clip_id']]} · text bounds", [f"Size{round(R['width'])} × {round(R['height'])} px", 'At' + sec_(a['t'])], None)
    a, P, R, T = X['add_managed_linear_effect']; prev = P['changed']['clip']['effects'][0]
    ex['add_managed_linear_effect'] = (f"{names[T['clip_id']]} · linear grade", ['Effect' + a['kind'].replace('_', ' '), f"Effects{'none' if not prev else ', '.join(prev)}→{a['kind'].replace('_', ' ')}", 'Working gamut' + {3: 'ACEScg'}[R['working_gamut']], 'Exposure' + jsnum(a['params']['exposure'])], None)
    a, P, R, T = X['raycast_mesh_raw']; h = R['hit']
    ex['raycast_mesh_raw'] = ('raycast', ['Result' + f"hit triangle {h['triangle']}", 'Distance t' + jsnum(h['t']), 'Barycentric' + ', '.join(jsnum(x) for x in h['barycentric']), 'Origin' + ', '.join(jsnum(x) for x in a['ray']['origin'])], None)
    a, P, R, T = X['compile_scene3d_descriptor']; st = R['stats']
    ex['compile_scene3d_descriptor'] = ('scene compile', ['Draws' + str(st['draws']), 'Triangles' + str(st['triangles']), 'Vertices' + str(st['vertices'])], None)
    a, P, R, T = X['preview_subject_match']; L = lambda k: R[k][0]['lab_mean'][0]
    assert P.get('mutated') is False and not P.get('changed')   # read-only in the engine: nothing to undo
    ex['preview_subject_match'] = (f"{names[a['clip_id']]} · match · {names[a['ref_clip_id']]}", [f"Skin{masks[a['source_regions']['skin']]} → {masks[a['ref_regions']['skin']]}", f"Skin L*{jsnum(L('source_stats'))}→{jsnum(L('result_stats'))} (ref {jsnum(L('reference_stats'))})", f"Frames{sec_(a['time'])} ↔ {sec_(a['ref_time'])}", 'Domain' + R['domain'].replace('_', ' '), 'Previewbefore / after only, nothing changed'], None)
    return ex, names

async def history_phase(p, port):
    fails = []
    def bad(msg): print('FAIL ', msg, flush=True); fails.append(msg)
    EXP, NAMES = expectations(EVH)
    # preview_subject_match: its real refusal (a keyframed source clip: "preview requires default x") and its real
    # success (two clean clips, named ellipse masks, colour pipeline off) are both in the capture; nothing is inserted
    b = await p.chromium.launch()
    ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); pg = await ctx.new_page(); errs = []
    pg.on('pageerror', lambda e: errs.append(str(e)))
    await pg.clock.install()
    await pg.goto(f'http://127.0.0.1:{port}/ui/index.html?feed=off')
    for _ in range(100):
        await pg.clock.run_for(50)
        if await pg.evaluate('!!window.__kimo'): break
    await pg.evaluate("async () => { window.__M = (await import('./modes.js')).MODES; window.__inf = (await import('./modes.js')).inferMode; }")
    async def put(ev): await pg.evaluate('e => __kimo.apply(e)', ev); await pg.clock.run_for(40)
    async def settle(): await pg.clock.run_for(SETTLE); await pg.evaluate("() => __kimo.apply({type: 'stats'})"); await pg.clock.run_for(40)
    async def show(m):
        ex = await pg.evaluate('__kimo.state.explicitMode')
        await pg.evaluate('m => __kimo.apply({type: "mode", mode: m})', m); await pg.clock.run_for(40)
        await pg.evaluate('x => { __kimo.state.explicitMode = x; }', ex)
    async def dom(screen):   # the screen's panel DOM, read on that screen; the GUI is left on the screen it was on
        was = await pg.evaluate('__kimo.state.mode')
        if was != screen: await show(screen)
        await settle()
        html = await pg.evaluate("""s => Object.entries(__M[s].panels).filter(([, sp]) => sp.kind !== 'agent').map(([n]) => { const el = document.querySelector(`#main [data-panel="${n}"]`); return n + '=' + (el ? el.innerHTML : '<missing>'); }).join('\\n')""", screen)
        if was != screen: await show(was); await pg.clock.run_for(40)
        return html
    STATE = "() => JSON.stringify({ v: __kimo.state.views, tl: __kimo.state.liveTL || null, c: __kimo.state._covbCurves || null, a: __kimo.state._covbAnim || null, h: (__kimo.state._covbHist || {undo: []}).undo.length })"
    def screen_of(n): return CHECKS[n][0] or 'graphics'   # solve_constraints: the m09 graphics viewer
    ROWS2 = "() => [...document.querySelectorAll('.panel .kv .row')].map(e => e.textContent)"
    SECS = "() => [...document.querySelectorAll('.panel .kv .sec')].map(e => e.textContent)"
    BADRE = r"""() => { const re = /\b(undefined|NaN|null)\b|\bclip\d+\b|\ban\d+\b|\bfx\d+\b|\basset\d+\b|\btrack\d+\b/; return [...document.querySelectorAll('.panel .kv .row, .panel .kv .sec, [data-panel=graph] .labchip, [data-panel=viewer] svg.ov, [data-panel=stack], [data-panel=scene]')].map(e => e.textContent).filter(s => re.test(s)); }"""
    calls = {}   # call_id -> (name, phase seen)
    for e in EVH:
        if e.get('phase') == 'start': calls[e['call_id']] = e['tool_name']
    outcome = {}
    for e in EVH:
        if e.get('phase') in ('error', 'complete'): outcome.setdefault(e['call_id'], e['phase'])
    res = {n: {'a': None, 'b': None, 'c': None} for n in TOOLS}
    U, Rd = [], []   # the test's own mirror of the engine's history: (tool, screen, panel DOM before) | None
    last_undo_left = None; prev_redo_left = None; block = None; pending = {}
    seq = list(EVH)   # exactly the capture
    for e in seq:
        n, ph = e.get('tool_name'), e.get('phase')
        # ---- before the event
        if ph == 'start' and n in TOOLS:
            block = n; scr = screen_of(n)
            if outcome.get(e['call_id']) == 'error' and n not in pending:
                d0 = await dom(scr)   # read first: showing a screen for the first time makes its (empty) view
                pending[n] = {'state': await pg.evaluate(STATE), 'dom': d0}
            elif outcome.get(e['call_id']) != 'error':
                pending[n + '#ok'] = {'dom': await dom(scr), 'state': await pg.evaluate(STATE)}
        if ph == 'start' and n in ('undo', 'redo'):
            side = U if n == 'undo' else Rd
            top = side[-1] if side else None
            pending['hist'] = {'top': top, 'pre': await dom(top[1]) if top else None, 'reads': {t: await dom(screen_of(t)) for t in ([block] if block and block not in ('undo', 'redo') else [])}}
        await put(e)
        # ---- after the event
        if ph == 'error' and n in TOOLS and n in pending and 'done' not in pending[n]:
            pending[n]['done'] = True
            rail = await pg.evaluate("id => { const c = __kimo.state.calls.find(x => x.id === id); return c ? [c.status, c.error || ''] : null; }", e['call_id'])
            hudt = await pg.evaluate("() => (document.querySelector('[data-panel=viewer] .hud') || {}).textContent || ''")
            st_after = await pg.evaluate(STATE); d_after = await dom(screen_of(n))
            ok_rail = bool(rail) and rail[0] == 'error' and rail[1] == e['payload'].get('error')
            ok_state = st_after == pending[n]['state']; ok_dom = d_after == pending[n]['dom']
            ok = ok_rail and ok_state and ok_dom and (f'{n} failed' in hudt or screen_of(n) not in ('graph', 'captions', 'effects', 'graphics', 'grading', 'modelling', 'scene3d'))
            res[n]['b'] = ok
            print(f"{'PASS' if ok else 'FAIL'}  (b) {n:<28} refused: “{e['payload'].get('error', '')[:48]}” rail={rail and rail[0]} hud={'failed' in hudt} state-equal={ok_state} dom-equal={ok_dom}", flush=True)
            if not ok_state:
                s0, s1 = json.loads(pending[n]['state']), json.loads(st_after)
                diff = [k for k in s0 if s0[k] != s1.get(k)] + [k for k in s1 if k not in s0]
                if 'v' in diff: diff += [f"v.{m}.{p}" for m in set(s0['v']) | set(s1['v']) for p in set((s0['v'].get(m) or {})) | set((s1['v'].get(m) or {})) if (s0['v'].get(m) or {}).get(p) != (s1['v'].get(m) or {}).get(p)]
                print('      state changed at:', diff[:8], flush=True)
            if not ok: fails.append(f'(b) {n}')
        if ph == 'complete' and n in TOOLS:
            pre = pending.pop(n + '#ok', None); P = e.get('payload') or {}
            mode = await pg.evaluate('__kimo.state.mode'); rows = await pg.evaluate(ROWS2); secs = await pg.evaluate(SECS); badv = await pg.evaluate(BADRE)
            if n in EXP:
                head, want, extra = EXP[n]
                miss = [w for w in want if w not in rows] + ([] if head in secs else ['sec: ' + head])
                if extra and extra[0] == 'chips':
                    chips = await pg.evaluate(CHIPS)
                    miss += [f'chip {c}' for c in extra[1] if c not in chips]
                if extra and extra[0] == 'overlay':
                    ovt = await pg.evaluate("() => [...document.querySelectorAll('[data-panel=viewer] svg.ov')].map(e => e.textContent).join('|')")
                    miss += [f'overlay {c}' for c in extra[1] if c not in ovt]
            else:
                head, want, miss = '?', [], [f'no expectation for {n}']
            ok = not miss and not badv and mode == screen_of(n)
            res[n]['a'] = ok
            print(f"{'PASS' if ok else 'FAIL'}  (a) {n:<28} screen={mode:<9} {head} :: " + ' | '.join(want)[:150] + (f"  MISSING {miss}" if miss else '') + (f"  RAW {badv[:2]}" if badv else ''), flush=True)
            if not ok: fails.append(f'(a) {n}')
            if P.get('mutated') is True:
                after = await dom(screen_of(n))
                if after == pre['dom']: bad(f'(c) {n}: the step changed nothing on its screen, nothing to undo')
                U.append((n, screen_of(n), pre['dom'])); Rd.clear()
        elif ph == 'complete' and n not in ('undo', 'redo') and (e.get('payload') or {}).get('mutated') is True:
            U.append(None); Rd.clear()
        if ph == 'complete' and n in ('undo', 'redo'):
            Rr = e['payload']['result']; h = pending.pop('hist'); top = h['top']
            src, dst = (U, Rd) if n == 'undo' else (Rd, U)
            ent = src.pop() if src else None
            t = block
            gh = await pg.evaluate("() => { const H = __kimo.state._covbHist || {undo: [], redo: []}; return [H.undo.length, H.redo.length]; }")
            dst_len = len(dst) + 1
            ul, ra = (len(src), dst_len) if n == 'undo' else (dst_len, len(src))
            if not (Rr.get('undo_left') == ul == gh[0] and Rr.get('redo_available') == ra == gh[1]):
                bad(f"history out of step after {n} ({t}): engine undo_left {Rr.get('undo_left')} / redo {Rr.get('redo_available')}, GUI {gh}, test mirror {ul} / {ra}")
            if ent:
                now_ = await dom(ent[1]); want = ent[2]
                ok = now_ == want
                dst.append((ent[0], ent[1], h['pre']))
            else:
                ok = True; dst.append(None)
            for rt, d0 in h['reads'].items():   # the read's own screen, where it is not the screen the step was on
                if not ent or screen_of(rt) != ent[1]:
                    ok = ok and (await dom(screen_of(rt))) == d0
            if t in TOOLS:
                if n == 'undo':
                    res[t]['c_undo'] = (ok, ent[0] if ent else None, Rr.get('undo_left'))
                else:
                    u = res[t].get('c_undo'); ok2 = bool(u and u[0]) and ok
                    step_tool = u[1] if u else None
                    res[t]['c'] = ok2
                    print(f"{'PASS' if ok2 else 'FAIL'}  (c) {t:<28} undo → panel DOM == before {step_tool or '(no GUI step)'} · redo → panel DOM == after (byte-for-byte); engine undo_left {u[2] if u else '?'} → {Rr.get('undo_left')}", flush=True)
                    if not ok2: fails.append(f'(c) {t}')
                    block = None
    # every undo / redo above also asserted the engine's undo_left / redo_available == the GUI's history depth ==
    # the test's mirror: a read-only or refused call pushed no step on either side
    for n in TOOLS:
        for key in ('a', 'b', 'c'):
            if res[n][key] is None: bad(f'({key}) {n} not exercised')
    if errs: bad('page errors: ' + '; '.join(errs[:3]))
    await b.close()
    return fails, res

class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass

async def main():
    from playwright.async_api import async_playwright
    sys.path.insert(0, str(CRATE / 'tests'))
    srv = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=str(CRATE))); srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    fails, seen = [], set()
    async with async_playwright() as p:
        b = await p.chromium.launch()
        ctx = await b.new_context(viewport={'width': 1376, 'height': 768}); pg = await ctx.new_page(); errs = []
        pg.on('pageerror', lambda e: errs.append(str(e)))
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off')
        await pg.wait_for_function('window.__kimo'); await pg.wait_for_timeout(200)
        fam = await pg.evaluate("async names => { const r = await import('./router.js'); return Object.fromEntries(names.map(n => [n, r.family(n)])); }", list(CHECKS))
        for n, f in fam.items():
            ok = f in ('motion', 'measure', 'effects')
            print(f"{'PASS' if ok else 'FAIL'}  family {n:<28} {f}"); ok or fails.append('family ' + n)
        BAD = r"""() => { const re = /\b(undefined|NaN|null)\b|\bclip\d+\b|\ban\d+\b|\bfx\d+\b/; return [...document.querySelectorAll('.panel .kv .row, .panel .kv .sec, [data-panel=graph] .labchip')].map(e => e.textContent).filter(s => re.test(s)); }"""
        for ev in EV:
            n = ev.get('tool_name')
            if n in CHECKS and ev.get('phase') == 'start':
                kind = str(((ev.get('payload') or {}).get('args') or {}).get('kind', ''))
                before = await pg.evaluate(CHECKS[n][1].replace('{kind}', kind))
                if before: print(f'FAIL  {n}: the visual was already there before the call'); fails.append(n + ' pre')
            await pg.evaluate('e => __kimo.apply(e)', ev)
            if n in CHECKS and ev.get('phase') in ('complete', 'error'):
                await pg.wait_for_timeout(90)
                if ev['phase'] == 'error': continue
                want, chk = CHECKS[n]; chk = chk.replace('{kind}', kind)
                mode = await pg.evaluate('__kimo.state.mode'); got = await pg.evaluate(chk); bad = await pg.evaluate(BAD)
                ok = bool(got) and (want is None or mode == want) and not bad
                print(f"{'PASS' if ok else 'FAIL'}  {n:<28} screen={mode:<10} visual={bool(got)!s:<5}" + (f" bad={bad[:2]}" if bad else ''), flush=True)
                if not ok: fails.append(n)
                seen.add(n)
        # preview_subject_match's success exists only in the history capture: replay that session, checking its visual
        await pg.goto(f'http://127.0.0.1:{srv.server_address[1]}/ui/index.html?feed=off'); await pg.wait_for_function('window.__kimo'); await pg.wait_for_timeout(200)
        okc = {e['call_id'] for e in EVH if e.get('tool_name') == 'preview_subject_match' and e.get('phase') == 'complete'}
        for ev in EVH:
            n = ev.get('tool_name'); real = n == 'preview_subject_match' and ev.get('call_id') in okc
            if real and ev.get('phase') == 'start' and await pg.evaluate(CHECKS[n][1]): print(f'FAIL  {n}: the visual was already there before the call'); fails.append(n + ' pre')
            await pg.evaluate('e => __kimo.apply(e)', ev)
            if real and ev.get('phase') == 'complete':
                await pg.wait_for_timeout(90)
                mode = await pg.evaluate('__kimo.state.mode'); got = await pg.evaluate(CHECKS[n][1]); bad = await pg.evaluate(BAD)
                ok = bool(got) and mode == CHECKS[n][0] and not bad
                print(f"{'PASS' if ok else 'FAIL'}  {n:<28} screen={mode:<10} visual={bool(got)!s:<5} (real engine success, history capture)" + (f" bad={bad[:2]}" if bad else ''), flush=True)
                if not ok: fails.append(n)
                seen.add(n)
        missing = sorted(set(CHECKS) - seen)
        if missing: print('FAIL  not exercised:', missing); fails += missing
        if errs: print('page errors:', errs); fails.append('pageerror')
        await b.close()
        n1 = len(CHECKS) - len(set(f.split(' ')[0] for f in fails if not f.startswith('family')))
        print('--- phase 2: ids + before → after, real refused calls, undo / redo (tests/ev_engine_covb_hist.json)', flush=True)
        f2, res = await history_phase(p, srv.server_address[1]); fails += f2
    srv.shutdown()
    print(f"cov_b: {n1}/{len(CHECKS)} tools with a specific visual")
    print(f"cov_b phase 2: (a) {sum(1 for r in res.values() if r['a'])}/{len(res)} ids + before→after · (b) {sum(1 for r in res.values() if r['b'])}/{len(res)} refused calls change nothing · (c) {sum(1 for r in res.values() if r['c'])}/{len(res)} undo / redo byte-for-byte")
    print('RESULT', 'FAIL ' + ', '.join(fails) if fails else 'PASS'); sys.exit(1 if fails else 0)

if __name__ == '__main__':
    asyncio.run(main())
