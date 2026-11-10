"""save_project with_history + load_project with_history: undo steps and checkpoints survive a server restart (a second server process).
Without with_history nothing changes (no .history file). Writes only to /tmp."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
P = '/tmp/hp_proj.json'
for f in (P, P + '.history'):
    if os.path.exists(f): os.remove(f)
a = Kimo(); c = a.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
cid = c('add_shape', track_id=t, shape='rect', width=100, height=60, fill=[1, 0, 0, 1], start=0, duration=1)['clip_id']
base = str(c('get_project')); c('checkpoint', name='base'); c('add_effect', clip_id=cid, kind='blur'); c('set_property', clip_id=cid, property='opacity', value=0.3)
final = str(c('get_project')); r = c('save_project', path=P); assert not os.path.exists(P + '.history'), 'history written without with_history'
r = c('save_project', path=P, with_history=True); print(r); assert os.path.exists(P + '.history')
a.p.kill()
b = Kimo(); d = b.call; assert d('history_list')['undo'] == []
r = d('load_project', path=P); assert d('history_list')['checkpoints'] == [], 'plain load must not bring history'
r = d('load_project', path=P, with_history=True); print(r)
h = d('history_list'); print([u['tool'] for u in h['undo']], h['checkpoints']); assert h['checkpoints'][0]['name'] == 'base' and h['undo'][-1]['tool'] == 'load_project' and h['undo'][-2]['tool'].startswith('set_property')
d('undo', steps=3); assert str(d('get_project')) == base, 'undo through restored history does not reach the original state'
d('redo', steps=3); d('restore', name='base'); assert str(d('get_project')) == base, 'restore after restart does not match the original checkpoint'
try: d('load_project', path='/tmp/hp_nonexistent.json', with_history=True); raise SystemExit('FAIL')
except SystemExit: raise
except Exception: pass
print('history persist proof OK')
