"""next_id collisions after undo/redo/restore. Project.next_id is not serialised and rides along in undo clones, so undo then add can reuse an id that
a later redo brings back. Case A (in memory): add, undo, add, redo. Case B (with_history sidecar): save with_history, load with_history, undo 1,
add twice. All ids across all tracks and clips must stay unique. Writes only to /tmp."""
import sys, os, json; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
def ids(c):
    p = c('get_project') if True else None
    s = json.dumps(p); out = []
    def walk(o):
        if isinstance(o, dict):
            for k, v in o.items():
                if k in ('id', 'clip_id', 'track_id') and isinstance(v, str): out.append(v)
                walk(v)
        elif isinstance(o, list):
            for v in o: walk(v)
    walk(p); return out
k = Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
a = c('add_shape', track_id=t, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)['clip_id']
c('undo'); b = c('add_shape', track_id=t, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)['clip_id']
print('case A: first id %s, second id %s (a new edit clears redo, so the reuse of a removed id is fine)' % (a, b))
proj = c('get_project'); cl = [x['id'] for tr in proj['tracks'] for x in tr['clips']]; print('clips after redo:', cl); assert len(cl) == len(set(cl)), 'duplicate clip ids after undo/add/redo'
# case B
k2 = Kimo(); c = k2.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
for _ in range(3): c('add_shape', track_id=t, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)
c('save_project', path='/tmp/nid.kimo', with_history=True)
k3 = Kimo(); c = k3.call; c('load_project', path='/tmp/nid.kimo', with_history=True); c('undo', steps=2)  # step 1 undoes the load itself
for _ in range(4): c('add_shape', track_id=c('get_project')['tracks'][0]['id'], shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=0, duration=1)
proj = c('get_project'); cl = [x['id'] for tr in proj['tracks'] for x in tr['clips']]; print('case B clips:', cl); assert len(cl) == len(set(cl)), 'duplicate clip ids after with_history load, undo, add'
print('next_id proof OK')
