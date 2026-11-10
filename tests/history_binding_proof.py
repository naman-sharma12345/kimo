"""G-a/G-b proof: history is bound to the project bytes (project_sha), saves are atomic (no .tmp left), a save without history removes a stale .history,
history_steps=0 keeps checkpoints and zero undo steps, and the project file is byte-identical to the reference binary (KIMO_ORACLE) output. /tmp only."""
import sys, os, json, glob, shutil; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
D = '/tmp/hbind'; shutil.rmtree(D, ignore_errors=True); os.makedirs(D); P = D + '/a.kimo'
def mk(exe=None):
    k = Kimo(exe=exe) if exe else Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
    for i in range(4): c('add_shape', track_id=t, shape='rect', width=10 + i, height=10, fill=[1, 1, 1, 1], start=i, duration=1)
    c('checkpoint', name='cp1'); c('add_shape', track_id=t, shape='ellipse', width=9, height=9, fill=[1, 0, 0, 1], start=5, duration=1); return k, c, t
k, c, t = mk(); r = c('save_project', path=P, with_history=True); print('save:', r)
assert not glob.glob(D + '/*.tmp'), 'tmp left behind'; assert os.path.exists(P + '.history')
# 1: edit, save WITHOUT history, load WITH history -> stale history must not be used
c('add_shape', track_id=t, shape='rect', width=30, height=30, fill=[0, 1, 0, 1], start=7, duration=1)
r = c('save_project', path=P); print('save no history:', r); assert r['stale_history_removed'] and not os.path.exists(P + '.history')
k2 = Kimo(); c2 = k2.call
r = c2('load_project', path=P, with_history=True); print('missing history:', r); assert str(r['history_ignored']).startswith('missing') and r['tracks'] == 1
# 2: history from timeline A beside project from timeline B
c('save_project', path=P, with_history=True); shutil.copy(P + '.history', D + '/old.history')
c('add_shape', track_id=t, shape='rect', width=31, height=31, fill=[0, 1, 0, 1], start=8, duration=1); c('save_project', path=P)  # removes history
shutil.copy(D + '/old.history', P + '.history')
r = c2('load_project', path=P, with_history=True); print('mismatched load:', r); assert r['history_ignored'] and r['undo_steps'] <= 1, r
# 3: matching history loads
c('save_project', path=P, with_history=True); r = c2('load_project', path=P, with_history=True); print('matching load:', r); assert r['history_ignored'] is None and r['undo_steps'] > 1
# 4: history_steps=0
r = c('save_project', path=P, with_history=True, history_steps=0); print('steps=0:', r); assert r['undo_steps'] == 0 and r['checkpoints'] == 1
r = c2('load_project', path=P, with_history=True); assert r['undo_steps'] == 0 and r['checkpoints'] == 1, r
# 5: project bytes identical to reference binary
ex = os.environ.get('KIMO_ORACLE', '/tmp/kimo-oracle')
if os.path.exists(ex):
    ka, ca, _ = mk(ex); kb, cb, _ = mk(); ca('save_project', path=D + '/ref.kimo'); cb('save_project', path=D + '/new.kimo')
    assert open(D + '/ref.kimo', 'rb').read() == open(D + '/new.kimo', 'rb').read(), 'project bytes differ from reference'; print('project bytes identical to reference binary')
print('history binding proof OK')
