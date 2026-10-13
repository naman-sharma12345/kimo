"""Step B property test: a seeded random sequence of 90 mixed mutating tools with undo/redo interleaved is run on the NEW binary and on a reference binary
(env KIMO_ORACLE, built before the Arc<Clip> flip). After every step get_project JSON must be byte-identical. Writes only to a temp dir.
Note: the oracle predates the central track lock (u9l) and later intentional behaviour changes, so this proof never sets locked=true or uses set_track_matte (dangling-ref clearing, u9i); those changes are pinned by tool_contracts and error_table."""
import sys, os, json, random, hashlib; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
errs = {}
new = Kimo(); old = Kimo(exe=os.environ.get('KIMO_ORACLE', os.path.join(__import__('tempfile').gettempdir(), 'kimo-oracle')))
def both(tool, **a):
    r = []
    for k in (new, old):
        try: r.append(('ok', k.call(tool, **a)))
        except Exception as e: r.append(('err', str(e)[:80]))
    assert r[0][0] == r[1][0], (tool, a, r)
    errs.setdefault(tool, [0, 0])[r[0][0] == 'err'] += 1
    return r[0][1] if r[0][0] == 'ok' else None
def same(step):
    a = json.dumps(new.call('get_project'), sort_keys=True); b = json.dumps(old.call('get_project'), sort_keys=True)
    if a != b:
        i = next(j for j in range(min(len(a), len(b))) if a[j] != b[j]); print('first diff near:', a[max(0, i-120):i+80], '|||', b[max(0, i-120):i+80])
    assert a == b, 'diverged at step %d' % step
    return hashlib.sha1(a.encode()).hexdigest()[:8]
both('project_set', width=320, height=180, fps=25); t = both('add_track')['track_id']; t2 = both('add_track')['track_id']

import hashlib
seeds = [int(x) for x in os.environ.get('SEEDS', '20261012,7,424242').split(',')]; steps = int(os.environ.get('STEPS', 200))
def frames(tag):
    out = []
    for tm in (0.5, 1.7, 3.2):
        h = []
        for k, n in ((new, 'n'), (old, 'o')):
            p = os.path.join(__import__('tempfile').gettempdir(), 'arcp_%s.png' % n)
            try: k.call('render_frame', time=tm, path=p); h.append(hashlib.sha1(open(p, 'rb').read()).hexdigest()[:10])
            except Exception as e: h.append('err')
        assert h[0] == h[1], 'render diverged at %s t=%s: %s' % (tag, tm, h)
        out.append(h[0])
    return out
for seed in seeds:
    both('project_set', width=160, height=90, fps=25)
    rng = random.Random(seed); hs = []
    tr = [x['id'] for x in new.call('get_project')['tracks']]
    while len(tr) < 3:
        both('add_track'); tr = [x['id'] for x in new.call('get_project')['tracks']]
    tr = tr[-3:]
    for step in range(steps):
        P = new.call('get_project'); clips = [c['id'] for t in P['tracks'] for c in t['clips']]; r = rng.random()
        pick = lambda: rng.choice(clips)
        ops = [
            lambda: both('add_shape', track_id=rng.choice(tr), shape=rng.choice(['rect', 'ellipse']), width=20 + rng.randint(0, 40), height=20, fill=[rng.random(), 0.5, 0.5, 1], start=rng.random() * 3, duration=1 + rng.random()),
            lambda: both('add_text', track_id=rng.choice(tr), text='t%d' % rng.randint(0, 99), start=rng.random() * 3, duration=1.5),
            lambda: both('set_property', clip_id=pick(), property=rng.choice(['opacity', 'x', 'y', 'scale', 'rotation']), value=rng.random()),
            lambda: both('add_effect', clip_id=pick(), kind=rng.choice(['blur', 'vignette', 'invert', 'glow', 'pixelate'])),
            lambda: both('set_keyframe', clip_id=pick(), property='scale', time=rng.random(), value=0.5 + rng.random()),
            lambda: both('split_clip', clip_id=pick(), at=rng.random() * 4),
            lambda: both('move_clip', clip_id=pick(), start=rng.random() * 3),
            lambda: both('delete_clip', clip_id=pick()),
            lambda: both('duplicate_clip', clip_id=pick(), start=rng.random() * 3, **({'track_id': rng.choice(tr)} if rng.random() < .5 else {})),
            lambda: both('group_clips', clip_ids=rng.sample(clips, min(2, len(clips)))),
            lambda: both('ungroup_clip', clip_id=pick()),
            lambda: both('ripple_trim', clip_id=pick(), edge=rng.choice(['head', 'tail']), delta=rng.uniform(-0.3, 0.3)),
            lambda: both('slip_clip', clip_id=pick(), delta=rng.uniform(-0.3, 0.3)),
            lambda: both('trim_clip', clip_id=pick(), duration=0.3 + rng.random()),
            lambda: both('add_transition', clip_id=pick(), kind=rng.choice(['dissolve', 'wipe', 'slide']), duration=0.3),
            lambda: both('set_track', track_id=rng.choice(tr), locked=False),  # lock excluded: oracle predates u9l,
            lambda: both('add_mask', clip_id=pick(), kind='ellipse'),
            lambda: both('remove_mask', clip_id=pick()),
            # set_track_matte excluded: oracle predates dangling-ref clearing on delete (u9i)
            lambda: both('remove_track_matte', clip_id=pick()),
            lambda: both('set_text', clip_id=pick(), text='x%d' % rng.randint(0, 9)),
            lambda: both('undo', steps=rng.randint(1, 3)),
            lambda: both('redo', steps=rng.randint(1, 3)),
        ]
        if len(clips) < 4: ops[rng.randrange(2)]()
        else: ops[int(r * len(ops))]()
        hs.append(same(step))
        if step % 50 == 49: frames('seed %d step %d' % (seed, step))
    print('seed %d: %d steps byte-identical to oracle, %d distinct states, renders matched at 3 times every 50 steps' % (seed, steps, len(set(hs))))
print('ok/err per tool:', errs)
print('arc property proof OK')
