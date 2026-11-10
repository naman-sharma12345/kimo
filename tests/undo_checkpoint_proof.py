"""History tools: history_list, checkpoint, restore (itself undoable), undo/redo with steps. Writes nothing."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; st = lambda: str(c('get_project'))
c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
cid = c('add_shape', track_id=t, shape='rect', width=100, height=60, fill=[1, 0, 0, 1], start=0, duration=1)['clip_id']
base = st(); c('checkpoint', name='before grade')
fx = c('add_effect', clip_id=cid, kind='blur')['effect_id']; c('set_property', clip_id=cid, property='opacity', value=0.3); c('add_track')
edited = st(); assert edited != base
h = c('history_list'); tools = [u['tool'] for u in h['undo']]; print('history', tools, 'checkpoints', h['checkpoints'])
assert tools[-3:] == ['add_effect ' + cid, 'set_property ' + cid, 'add_track'] and h['next_undo'] == 'add_track' and h['checkpoints'][0]['name'] == 'before grade'
n_before = len(h['undo']); r = c('restore', name='before grade'); assert st() == base, 'restore did not return the exact checkpointed project'
assert len(c('history_list')['undo']) == n_before + 1, 'restore must be one undo step'
c('undo'); assert st() == edited, 'undo of restore must bring the edits back'
r = c('undo', steps=3); assert r['undone'] == 3 and st() == base, r
r = c('redo', steps=3); assert r['redone'] == 3 and st() == edited
c('undo', steps=2); assert len(c('history_list')['redo']) == 3  # the undone restore is still ahead too
c('add_track'); assert c('history_list')['redo'] == [], 'a new edit must clear redo'
c('checkpoint', name='before grade'); assert len(c('history_list')['checkpoints']) == 1, 'same name overwrites'
c('restore', name='before grade'); assert st() != base
for tool, kw in (('restore', dict(name='nope')), ('checkpoint_delete', dict(name='nope')), ('checkpoint', dict(name=''))):
    try: c(tool, **kw); raise SystemExit('FAIL ' + tool)
    except SystemExit: raise
    except Exception as e: pass
c('checkpoint_delete', name='before grade'); assert c('history_list')['checkpoints'] == []
for i in range(30): c('checkpoint', name='c%d' % i)
assert len(c('history_list')['checkpoints']) == 24, 'checkpoint cap'
print('undo/checkpoint proof OK')
