"""frosted_glass tilt_refraction: a tilted pane refracts the backdrop along its 3D normal. Checks: default 0 leaves the frame
identical to the effect without the param; rot_y +40 and -40 shift the backdrop in opposite horizontal directions; shift grows with the
parameter. The shift is the best integer dx (cross-correlation of pane core vs the tilt_refraction=0 render). Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
plate = '/tmp/fn_plate.mp4'; subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=s=640x360:r=25', '-t', '2', '-pix_fmt', 'yuv420p', plate], check=True)
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25)
aid = c('import_media', path=plate)['asset_id']; t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_clip', track_id=t1, source_type='media', asset_id=aid, start=0, duration=2, in_point=0, fit='cover')
sid = c('add_shape', track_id=t2, shape='rect', width=360, height=200, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
for kk, v in dict(blur=2, tint_amount=0.0, edge_light=0.0, refraction=0.0).items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float)
def shot(n): c('render_frame', time=0.5, path='/tmp/fn_%s.png' % n); return img('/tmp/fn_%s.png' % n)
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
def rot(v): c('set_property', clip_id=sid, property='rot_y', value=v)
def dx(a, b):
    R = (slice(150, 210), slice(260, 380)); best = None
    for s in range(-40, 41):
        e = np.abs(np.roll(a, s, 1)[R] - b[R]).mean()
        if best is None or e < best[0]: best = (e, s)
    return best[1]
res = {}
for ang in (40, -40):
    rot(ang); setp(tilt_refraction=0.0); base = shot('b%d' % ang)
    setp(); same = shot('s%d' % ang); assert (same == base).all()
    for tk in (20, 40):
        setp(tilt_refraction=tk); res[(ang, tk)] = dx(base, shot('t%d_%d' % (ang, tk)))
print('shifts', res)
assert res[(40, 20)] != 0 and res[(-40, 20)] != 0, 'no shift'
assert res[(40, 20)] * res[(-40, 20)] < 0, 'shift not opposite for opposite tilt'
assert abs(res[(40, 40)]) > abs(res[(40, 20)]), 'shift does not grow with parameter'
print('frosted normal proof OK')
