"""save_project with_history streams the history to disk and caps persisted undo steps (history_steps, default 20). Before this change the history
was built as one in-memory JSON tree first; with 1000 clips and a full history the engine ran out of memory and died (measured). Checks: 3000 clips,
default save writes 20 steps and finishes under 3 s; history_steps=3 writes 3 and a fresh server loads it with exactly 3 undo steps. Writes only to /tmp."""
import sys, os, time, json; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25); t = c('add_track')['track_id']
for _ in range(3000): c('add_shape', track_id=t, shape='rect', width=50, height=50, fill=[1, 1, 1, 1], start=0, duration=1)
s = time.time(); r = c('save_project', path='/tmp/hs.kimo', with_history=True); dt = time.time() - s
print('default: steps %d of %d, history %.1f MB, %.2f s' % (r['undo_steps'], r['undo_steps_available'], r['history_bytes'] / 1e6, dt)); assert r['undo_steps'] == 20 and dt < 3.0
r = c('save_project', path='/tmp/hs.kimo', with_history=True, history_steps=3); assert r['undo_steps'] == 3
k2 = Kimo(); c2 = k2.call; r = c2('load_project', path='/tmp/hs.kimo', with_history=True); print('loaded: undo steps', r['undo_steps']); assert r['undo_steps'] == 3
print('history steps proof OK')
