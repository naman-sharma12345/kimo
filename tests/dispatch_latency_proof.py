"""Dispatch overhead proof. A read-only tool call (list_lights) must not cost O(project size): with 1500 clips its latency stays within 3x of the
empty-project latency and under 2 ms. Before the change (registry rebuilt per call + whole-project snapshot on every call) it was 1.9 ms empty and
about 4-6 ms at 1500-3000 clips, so this fails on the old code. Also checks undo still captures a mutating call. Writes nothing."""
import sys, os, time; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25); t = c('add_track')['track_id']
def tm(n=40):
    c('list_lights'); s = time.time()
    for _ in range(n): c('list_lights')
    return (time.time() - s) / n * 1000
e = tm()
for _ in range(1500): c('add_shape', track_id=t, shape='rect', width=50, height=50, fill=[1, 1, 1, 1], start=0, duration=1)
f = tm(); print('list_lights: %.2f ms empty, %.2f ms with 1500 clips (ratio %.1f)' % (e, f, f / e)); assert f < 2.0 and f < 3 * max(e, 0.1), 'read-only call cost grows with project size'
n0 = len(c('history_list')['undo']) if 'undo' in c('history_list') else None
c('set_light', id='a', intensity=1.0); c('undo'); assert not c('list_lights')['lights'], 'undo must remove the light a mutating call added'
print('dispatch latency proof OK')
