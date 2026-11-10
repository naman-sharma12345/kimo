"""Step B bench: set_property latency at N clips and peak RSS after 100 mutations (history steps) at 3000 clips, for the binary in argv[1]. /tmp only."""
import sys, os, time, statistics; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
exe = sys.argv[1]
def rss(k):
    for l in open('/proc/%d/status' % k.p.pid):
        if l.startswith('VmHWM'): return int(l.split()[1]) / 1024
for n in (10, 200, 2000, 3000):
    k = Kimo(exe=exe); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
    ids = [c('add_shape', track_id=t, shape='rect', width=10, height=10, fill=[1, 1, 1, 1], start=i * 0.01, duration=1)['clip_id'] for i in range(n)]
    ts = []
    for i in range(40):
        a = time.perf_counter(); c('set_property', clip_id=ids[i % n], property='opacity', value=0.3 + i * 0.01); ts.append((time.perf_counter() - a) * 1000)
    print('%5d clips: set_property median %.2f ms  p90 %.2f ms' % (n, statistics.median(ts), sorted(ts)[36]))
    if n == 3000:
        r0 = rss(k)
        for i in range(100): c('set_property', clip_id=ids[i], property='x', value=i)
        print('  peak RSS %.0f MB after load, %.0f MB after 100 more history steps' % (r0, rss(k)))
    k.p.kill()
