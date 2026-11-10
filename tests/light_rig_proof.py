"""light_rig: key/fill/rim lighting recipes in one call. Checks the numbers (fill = key / ratio exactly, rim = key * recipe gain, three lights, a repeat call
replaces instead of adding, warmth shifts the key colour, subject moves the rig, bad input is rejected) and the render: a relit disc under the rembrandt
recipe is brighter on the key side, and the left/right contrast rises with the key:fill ratio (2 -> 16). Writes only to /tmp."""
import sys, os, subprocess, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); C = k.call; W, H = 640, 360; C('project_set', width=W, height=H, fps=25)
r = C('light_rig', preset='three_point', intensity=2.0, ratio=4.0); L = {l['id']: l for l in r['lights']}
assert len(C('list_lights')['lights']) == 3
assert abs(L['rig_key']['intensity'] / L['rig_fill']['intensity'] - 4.0) < 1e-9 and abs(L['rig_rim']['intensity'] - 1.8) < 1e-9, L
assert L['rig_key']['pos'][0] < 0.5 < L['rig_fill']['pos'][0], 'key left, fill right'
C('light_rig', preset='three_point', intensity=2.0, ratio=4.0); assert len(C('list_lights')['lights']) == 3, 'repeat call must replace'
w = {l['id']: l for l in C('light_rig', preset='three_point', warmth=1.0)['lights']}['rig_key']['color']; c0 = L['rig_key']['color']; assert w[0] > c0[0] - 1e-9 and w[2] < c0[2] - 0.2, 'warmth must warm the key'
s = {l['id']: l for l in C('light_rig', preset='three_point', subject=[0.8, 0.3])['lights']}['rig_key']['pos']; assert abs(s[0] - (0.2 + 0.3)) < 1e-9 and abs(s[1] - (0.2 - 0.2)) < 1e-9, s
for bad in (dict(preset='nope'), dict(preset='split', ratio=0.5), dict(preset='split', subject=[2, 0]), dict(preset='split', intensity=-1)):
    try: C('light_rig', **bad); raise SystemExit('accepted bad input %s' % bad)
    except RuntimeError: pass
for p in ('rembrandt', 'butterfly', 'split', 'golden_hour'): C('light_rig', preset=p, prefix='x_' + p)
assert len(C('list_lights')['lights']) == 3 + 12, 'prefixes must coexist'
try: C('light_rig', preset='neon_night', prefix='over'); raise SystemExit('exceeded 16 lights')
except RuntimeError: pass
C('light_rig', preset='neon_night', prefix='x_golden_hour'); assert len(C('list_lights')['lights']) == 15
# render proof
k.close() if hasattr(k, 'close') else None
k = Kimo(); C = k.call; C('project_set', width=W, height=H, fps=25); t1 = C('add_track')['track_id']; t2 = C('add_track')['track_id']
C('add_clip', track_id=t1, source_type='solid', color=[0.02, 0.02, 0.03], start=0, duration=2)
cid = C('add_shape', track_id=t2, shape='ellipse', width=260, height=260, fill=[0.7, 0.7, 0.7, 1], start=0, duration=2)['clip_id']
C('set_relight', clip_id=cid, params={'bump': 4, 'blur': 40, 'height': 3, 'ambient': 0.05, 'shadow': 0.0}, strings={'material': 'plastic', 'height_source': 'alpha'})
def lum(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(H, W, 3).astype(float).mean(2)
yy, xx = np.mgrid[0:H, 0:W]; disc = (xx - 320) ** 2 + (yy - 180) ** 2 < 100 ** 2
res = []
for ratio in (2.0, 16.0):
    C('light_rig', preset='rembrandt', ratio=ratio, intensity=1.5); C('render_frame', time=0.5, path='/tmp/lr_%d.png' % ratio); a = lum('/tmp/lr_%d.png' % ratio)
    l, rr = a[disc & (xx < 320)].mean(), a[disc & (xx >= 320)].mean(); res.append((l, rr)); print('ratio %g: key-side %.1f, far side %.1f, contrast %.2f' % (ratio, l, rr, l / max(rr, 1e-3)))
assert res[0][0] > res[0][1] * 1.05, 'key side must be brighter'
assert res[1][0] / max(res[1][1], 1e-3) > 1.25 * res[0][0] / max(res[0][1], 1e-3), 'contrast must rise with the key:fill ratio'
print('light_rig proof OK')
