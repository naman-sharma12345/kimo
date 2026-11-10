"""frosted_glass fresnel: Schlick reflection of a procedural environment, per pixel, flat and tilted. Black backdrop, so the pane pixel is
exactly F * env. Checks the centre pixel against an independent closed form (reflect the view ray about the pane normal, sky gradient,
Schlick with f0 0.04) at rot_y 0, 45 and 75; reflectance rises toward grazing; fresnel=0 is bit-identical to the effect without the
param; the softbox only appears when the reflected ray points at it. Writes only to /tmp."""
import sys, subprocess, os, math, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25); t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_shape', track_id=t1, shape='rect', width=640, height=360, fill=[0, 0, 0, 1], start=0, duration=2)
sid = c('add_shape', track_id=t2, shape='rect', width=300, height=180, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(blur=0, tint_amount=0, edge_light=0, refraction=0, saturation=1, dispersion=0, layer_mix=0, box_gain=0)
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float)
def shot(n): c('render_frame', time=0.5, path='/tmp/ff_%s.png' % n); return img('/tmp/ff_%s.png' % n)
def expect(rot):
    th = math.radians(rot); s, cc = math.sin(th), math.cos(th); n = (-s, 0, -cc); d = (0, 0, 1); dn = sum(a * b for a, b in zip(d, n)); sg = -1 if dn > 0 else 1; n = tuple(sg * v for v in n); dn = sum(a * b for a, b in zip(d, n))
    cosv = -dn; F = 0.04 + 0.96 * (1 - cosv) ** 5; r = [d[i] - 2 * dn * n[i] for i in range(3)]; t = ((-r[1]) * 0.5 + 0.5) ** 1.5
    return F, [255 * F * v for v in (0.18 + 0.42 * t, 0.22 + 0.55 * t, 0.3 + 0.7 * t)]
setp(fresnel=0); c('set_property', clip_id=sid, property='rot_y', value=0); off = shot('off'); assert off.max() < 3, 'black backdrop expected'
res = {}
for rot in (0, 45, 75):
    c('set_property', clip_id=sid, property='rot_y', value=rot); setp(fresnel=0); a = shot('a%d' % rot)
    setp(fresnel=1); b = shot('b%d' % rot); px = b[180, 320]; F, e = expect(rot); res[rot] = px[0]
    print('rot %d: F=%.3f expected rgb %s measured %s' % (rot, F, [round(v, 1) for v in e], px.round(1).tolist()))
    assert abs(px[0] - e[0]) <= max(2.0, 0.06 * e[0]) and abs(px[2] - e[2]) <= max(2.0, 0.06 * e[2]), 'centre pixel differs from the closed form'
    assert (a == off).all() or rot != 0
assert res[75] > 3 * res[45] > 3 * res[0], 'reflectance must rise toward grazing: %s' % res
# fresnel 0 equals no param at all (same as the first render of this pane with the effect param never touched)
c('set_property', clip_id=sid, property='rot_y', value=0); setp(fresnel=0, box_gain=0); z = shot('z'); assert (z == off).all()
# softbox: reflected ray at rot 0 points straight back (0,0,-1); box at (0,0) direction hits it, a box far away does not
setp(fresnel=1, box_gain=20, box_x=0.0, box_y=0.0, box_size=0.05); hit = shot('hit')[180, 320, 0]; setp(box_x=-0.9, box_y=0.0); miss = shot('miss')[180, 320, 0]
print('softbox on-axis %.1f, off-axis %.1f' % (hit, miss)); assert hit > miss + 20
print('frosted fresnel proof OK')
