"""frosted_glass reflect_scene: the reflection shows the backdrop along the reflected ray. A white vertical stripe on black sits at x0.
The pane (300 wide) is flat or tilted; the reflected stripe (ghost) appears at the pixel whose lookup x + r.x*reflect_dist lands on the stripe, i.e. x0 - r.x*dist = x0 + sin(2*rot_y)*dist
(closed form at the pane centre). Checks the ghost position within 15 px at rot_y +35 and -35, the opposite side for opposite
tilt, no ghost at rot_y 0 (r.x = 0, ghost on top of the straight stripe), and that reflect_scene 0 gives no ghost. Writes only to /tmp."""
import sys, subprocess, os, math, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25); t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']; t3 = c('add_track')['track_id']
c('add_shape', track_id=t1, shape='rect', width=640, height=360, fill=[0, 0, 0, 1], start=0, duration=2)
bar = c('add_shape', track_id=t2, shape='rect', width=14, height=360, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']; 
sid = c('add_shape', track_id=t3, shape='rect', width=300, height=200, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(blur=0, tint_amount=0, edge_light=0, refraction=0, saturation=1, dispersion=0, layer_mix=0, fresnel=1, f0=0.6, box_gain=0, env_gain=1, reflect_scene=1, reflect_dist=120, env_blur=4)
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float).mean(2)
def shot(n): c('render_frame', time=0.5, path='/tmp/fr_%s.png' % n); return img('/tmp/fr_%s.png' % n)
def ghost(rot, x0, rs=1):
    c('set_property', clip_id=sid, property='rot_y', value=rot); setp(reflect_scene=rs); c('set_property', clip_id=bar, property='x', value=x0 - 320)
    c('set_property', clip_id=bar, property='opacity', value=1); a = shot('a'); c('set_property', clip_id=bar, property='opacity', value=0); b = shot('b'); c('set_property', clip_id=bar, property='opacity', value=1)
    d = np.abs(a - b)[120:240].mean(0); d[max(0, x0 - 16):x0 + 17] = 0; lo, hi = 215, 425   # inside the pane, minus the straight-through stripe
    return int(np.argmax(d[lo:hi])) + lo, float(d[lo:hi].max())
res = {}
for rot, x0 in ((35, 150), (-35, 490)):
    x, v = ghost(rot, x0); exp = x0 + math.sin(math.radians(2 * rot)) * 120; res[rot] = x; print('rot %d, stripe at %d: ghost peak at x=%d (value %.1f), closed form %.0f' % (rot, x0, x, v, exp))
    assert abs(x - exp) <= 15 and v > 4, 'ghost not where the reflected ray puts it'
assert res[35] != res[-35]
x, v = ghost(35, 150, rs=0); print('reflect_scene 0: peak value %.1f' % v); assert v < 1.0, 'ghost present with reflect_scene 0'
print('frosted reflect_scene proof OK')
