"""frosted_glass slab refraction (slab_thickness, ior) on a tilted pane: per-pixel Snell refraction through a glass slab.
Checks against the closed form shift = T*(tan(ti) - tan(tt)) * cos(tilt) (screen foreshortening) at the pane centre, and:
zero shift at normal incidence (rot_y 0) and at ior 1, shift grows with tilt, is linear in thickness, grows with ior (checked against the closed form too), and differs across the
pane (perspective gives a different incidence angle at each pixel; the old constant tilt_refraction cannot). Writes only to /tmp."""
import sys, subprocess, os, math, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
plate = '/tmp/fs_plate.mp4'; subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=s=640x360:r=25', '-t', '2', '-pix_fmt', 'yuv420p', plate], check=True)
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25)
aid = c('import_media', path=plate)['asset_id']; t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_clip', track_id=t1, source_type='media', asset_id=aid, start=0, duration=2, in_point=0, fit='cover')
sid = c('add_shape', track_id=t2, shape='rect', width=420, height=240, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(blur=0, tint_amount=0, edge_light=0, refraction=0, saturation=1, dispersion=0, layer_mix=0)
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float).mean(2)
def shot(n): c('render_frame', time=0.5, path='/tmp/fs_%s.png' % n); return img('/tmp/fs_%s.png' % n)
def shift(a, b, cx, cy=180):
    R = (slice(cy - 40, cy + 40), slice(cx - 60, cx + 60)); best = None
    for s in np.arange(-60, 60.5, 0.5):
        sh = np.array([np.interp(np.arange(640) - s, np.arange(640), row) for row in a[R[0]]])
        e = np.abs(sh[:, R[1]] - b[R[0]][:, R[1]]).mean()
        if best is None or e < best[0]: best = (e, s)
    return best[1]
def run(rot, T, ior, cx=320):
    c('set_property', clip_id=sid, property='rot_y', value=rot); setp(slab_thickness=0); base = shot('base%d' % rot)
    setp(slab_thickness=T, ior=ior); return shift(base, shot('s%d_%d_%s' % (rot, T, ior)), cx)
n0 = run(0, 100, 1.5); print('normal incidence shift', n0); assert abs(n0) < 0.6
s30 = run(30, 100, 1.5); s50 = run(50, 100, 1.5); print('shift at 30, 50 deg:', s30, s50); assert abs(s50) > abs(s30) > 1
# independent closed form for the centre pixel: pinhole camera at z=-f (f = 180 / tan(25 deg), fov 50), layer plane at z=0 rotated by rot_y
ti = math.radians(50); tt = math.asin(math.sin(ti) / 1.5); sh = 100 * (math.tan(ti) - math.tan(tt)); f = 180 / math.tan(math.radians(25))
# the seen backdrop point is the plane hit point moved in-plane by sh toward the normal's projection: x = sh*cos(rot), z = -sh*sin(rot)
exp = f * (sh * math.cos(ti)) / (f - sh * math.sin(ti)); print('closed form at 50 deg (perspective, centre pixel): %.1f px, measured %.1f' % (exp, abs(s50)))
assert abs(abs(s50) - exp) < 0.12 * exp, 'centre shift deviates from the closed form by more than 12%'
s50b = run(50, 50, 1.5); print('half thickness:', s50b); assert abs(abs(s50b) / abs(s50) - 0.5) < 0.1, 'not linear in thickness'
s50i = run(50, 100, 2.2); s50o = run(50, 100, 1.0); print('ior 2.2:', s50i, 'ior 1.0:', s50o)
assert abs(s50o) < 0.6, 'ior 1 is no glass: no shift'; assert abs(s50i) > abs(s50), 'higher ior bends more, so the lateral shift is larger'
tt2 = math.asin(math.sin(ti) / 2.2); sh2 = 100 * (math.tan(ti) - math.tan(tt2)); exp2 = f * (sh2 * math.cos(ti)) / (f - sh2 * math.sin(ti)); print('closed form ior 2.2: %.1f' % exp2)
assert abs(abs(s50i) - exp2) < 0.15 * exp2
l, r = run(50, 100, 1.5, cx=275), run(50, 100, 1.5, cx=400); print('left/right of the pane:', l, r); assert abs(l - r) > 1.0, 'shift is not per-pixel'
print('frosted slab proof OK')
