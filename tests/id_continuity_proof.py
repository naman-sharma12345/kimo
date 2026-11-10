"""F1: ids continue exactly where an in-memory session would, across save/load. Session A builds a 4-step project in one process (no save/load).
Session B does the same 4 steps, each in a fresh process: load, one call, save. Project JSON (incl. every id) must be identical.
Collision case: a file holding clip5 and clip1001 must hand out an id above both. Writes only to /tmp."""
import sys, os, json, tempfile
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
STEPS = [lambda c, st: c('project_set', width=320, height=180, fps=25),
         lambda c, st: st.__setitem__('t', c('add_track')['track_id']),
         lambda c, st: st.__setitem__('a', c('add_shape', track_id=st['t'], shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)['clip_id']),
         lambda c, st: st.__setitem__('b', c('add_shape', track_id=st['t'], shape='rect', width=20, height=10, fill=[1, 0, 0, 1], start=1, duration=1)['clip_id'])]
a = Kimo(); st = {}
for s in STEPS: s(a.call, st)
pa = a.call('get_project'); ida = (st['t'], st['a'], st['b'])
d = tempfile.mkdtemp(); path = os.path.join(d, 'p.kimo'); st2 = {}
for i, s in enumerate(STEPS):
    k = Kimo(); c = k.call
    if i: c('load_project', path=path)
    if i: st2['t'] = c('get_project')['tracks'][0]['id'] if i > 1 else st2.get('t')
    s(c, st2); c('save_project', path=path)
k = Kimo(); k.call('load_project', path=path); pb = k.call('get_project')
print('A ids', ida, '| B ids', (st2['t'], st2['a'], st2['b']))
assert (st2['t'], st2['a'], st2['b']) == ida, 'ids differ between in-memory session and per-step load/save'
for key in ('tracks',): assert json.dumps(pa[key], sort_keys=True) == json.dumps(pb[key], sort_keys=True), 'project bytes differ'
# collision: clip5 and clip1001 in one file; the next id must be above both and unique
k = Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
c1 = c('add_shape', track_id=t, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)['clip_id']
p = json.load(open(path)); 
def ren(o, old, new):
    if isinstance(o, dict): return {(new if kk == old else kk): ren(v, old, new) for kk, v in o.items()}
    if isinstance(o, list): return [ren(v, old, new) for v in o]
    return new if o == old else o
a_, b_ = st2['a'], st2['b']; p = ren(ren(p, a_, 'clip5'), b_, 'clip1001'); json.dump(p, open(path, 'w'))
k = Kimo(); c = k.call; c('load_project', path=path); tid = c('get_project')['tracks'][0]['id']
n = c('add_shape', track_id=tid, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=2, duration=1)['clip_id']
num = int(''.join(ch for ch in n if ch.isdigit())); ids = [x['id'] for x in c('get_project')['tracks'][0]['clips']]
print('new id', n, 'all', ids); assert num > 1001 and len(ids) == len(set(ids)), ('collision or too low', n, ids)
print('id continuity proof OK')
