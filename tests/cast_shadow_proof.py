"""cast_shadow: light-driven shadow with a finite-light penumbra. Grey plate, white 100x100 caster at the frame centre, point light at
(0.2, 0.1, z 0.8), caster height 0.1. By similar triangles the shadow shifts by (C - L).xy * H / (Lz - H): checks the measured shadow shift
within 2 px of that closed form; the shadow is only written outside the layer; the penumbra (10-90% edge width) follows light_size * H / (Lz - H): doubling the caster height H roughly doubles it, light_size 0 is a hard
edge (one constant height, so no per-point contact hardening); moving the light to the other side flips the shadow side; opacity 0 is a no-op;
softness 0 gives a hard edge. Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
W, H = 640, 360
k = Kimo(); c = k.call; c('project_set', width=W, height=H, fps=25); t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_shape', track_id=t1, shape='rect', width=W, height=H, fill=[0.6, 0.6, 0.6, 1], start=0, duration=2)
sid = c('add_shape', track_id=t2, shape='rect', width=100, height=100, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(H, W, 3).astype(float)[:, :, 1]
def shot(n): c('render_frame', time=0.5, path='/tmp/cs_%s.png' % n); return img('/tmp/cs_%s.png' % n)
off = shot('off'); fx = c('add_effect', clip_id=sid, kind='cast_shadow')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(use_scene_light=0, light_x=0.2, light_y=0.1, light_z=0.8, height=0.1, opacity=1.0, light_size=0.0)
on = shot('on'); dark = (on < 0.6 * 0.6 * 255 * 0.5) & (np.abs(on - off) > 20)
ys, xs = np.nonzero(dark & ~((np.abs(np.arange(W)[None, :] - 320) < 51) & (np.abs(np.arange(H)[:, None] - 180) < 51))); assert len(xs) > 500, 'no shadow'
# hard shadow bbox including the part under the caster: use difference to off over the plate, shadow = pixels darker than the plate
sh = (on < off - 40); sh[130:231, 270:371] = True   # caster itself counts as part of the silhouette footprint
ys, xs = np.nonzero(sh | ((on < off - 40))); 
dx = (320 - 0.2 * W) * (0.1 * H) / (0.8 * H - 0.1 * H); dy = (180 - 0.1 * H) * (0.1 * H) / (0.8 * H - 0.1 * H)
sx0, sx1, sy0, sy1 = xs.min(), xs.max(), ys.min(), ys.max()
print('shadow+caster bbox x %d..%d y %d..%d; closed-form shift (%.1f, %.1f)' % (sx0, sx1, sy0, sy1, dx, dy))
assert abs(sx1 - (370 + dx)) <= 2.5 and abs(sy1 - (230 + dy)) <= 2.5, 'shadow far edge is not where similar triangles put it'
assert abs(sx0 - 270) <= 2 and abs(sy0 - 130) <= 2, 'shadow should extend to the lit side only by the caster itself'
print('caster vs effect-off:', on[180,320], off[180,320], 'mean abs diff inside caster', np.abs(on[135:226,275:366]-off[135:226,275:366]).max()); assert np.abs(on[135:226, 275:366] - off[135:226, 275:366]).max() < 1.5, 'caster must stay unshadowed'
# opposite light flips the side
setp(light_x=0.8, light_y=0.9); fl = shot('flip'); ys2, xs2 = np.nonzero(fl < off - 40); assert xs2.min() < 270 - 5 and ys2.min() < 130 - 5, 'shadow must fall to the upper left for a lower-right light'
# penumbra width: 10-90% transition of the shadow's right edge along the row through the caster centre, for H and 2H
def edge_width(h_):
    setp(light_x=0.2, light_y=0.1, height=h_, light_size=0.08); r = shot('pw')[180]; plate = off[180, 0]; ex = int((370 + (320 - 0.2 * W) * (h_ * H) / (0.8 * H - h_ * H)))
    d = (plate - r[374:ex + 80]); seg = d / d.max(); lo = np.nonzero(seg < 0.9)[0]; hi = np.nonzero(seg < 0.1)[0]; print('  H', h_, 'dmax', d.max(), 'lo', lo[:1], 'hi', hi[:1]); return float(hi[0] - lo[0]) if len(lo) and len(hi) else 0.0
w1, w2 = edge_width(0.1), edge_width(0.2); k1 = 0.1 / (0.8 - 0.1); k2 = 0.2 / (0.8 - 0.2)
print('penumbra 10-90%% width: H=0.1 -> %.0f px, H=0.2 -> %.0f px (ratio %.2f, formula ratio %.2f)' % (w1, w2, w2 / max(w1, 1), k2 / k1)); assert w1 > 2 and abs(w2 / w1 - k2 / k1) < 0.25 * (k2 / k1), 'penumbra must scale with H / (Lz - H)'
setp(light_x=0.2, light_y=0.1, height=0.1, light_size=0.0); hard = shot('hard')[180]; ex = int(370 + dx); assert (off[180, 0] - hard[ex + 3]) < 1 and (off[180, 0] - hard[ex - 3]) > 100, 'light_size 0 must be a hard edge'
setp(light_size=0.0, opacity=0.0); assert (shot('z') == off).all(), 'opacity 0 must be a no-op'
print('cast_shadow proof OK')
