"""light_wrap: backdrop light bleeds over a layer's edge. Dark grey disc over a backdrop that is red on the left half and blue on the right.
Checks: just inside the left edge the disc picks up red (not blue), inside the right edge blue; the wrap fades inward (4 px > 20 px > 70 px
inside) and is gone at the centre; pixels outside the disc and the disc centre are bit-identical to effect-off; a black backdrop and
intensity 0 change nothing; desaturate pulls the wrap toward grey. Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25); t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']; t3 = c('add_track')['track_id']
c('add_shape', track_id=t1, shape='rect', width=320, height=360, fill=[1, 0, 0, 1], start=0, duration=2)
c('set_property', clip_id=c('add_shape', track_id=t1, shape='rect', width=320, height=360, fill=[1, 0, 0, 1], start=0, duration=2)['clip_id'], property='x', value=-160)
rb = c('add_shape', track_id=t2, shape='rect', width=320, height=360, fill=[0, 0, 1, 1], start=0, duration=2)['clip_id']; c('set_property', clip_id=rb, property='x', value=160)
sid = c('add_shape', track_id=t3, shape='ellipse', width=240, height=240, fill=[0.1, 0.1, 0.1, 1], start=0, duration=2)['clip_id']
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float)
def shot(n): c('render_frame', time=0.5, path='/tmp/lw_%s.png' % n); return img('/tmp/lw_%s.png' % n)
off = shot('off'); fx = c('add_effect', clip_id=sid, kind='light_wrap')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(width=30, intensity=1, blur=10); on = shot('on')
# disc spans x 200..440 at row 180. left edge x=200, right edge x=440
L = lambda dx: on[180, 200 + dx] - off[180, 200 + dx]; R = lambda dx: on[180, 439 - dx] - off[180, 439 - dx]
print('left edge +4:', L(4).round(1), '+20:', L(20).round(1), '+70:', L(70).round(1)); print('right edge +4:', R(4).round(1))
assert L(4)[0] > 8 and L(4)[2] < 0.35 * L(4)[0], 'left edge should pick up red'; assert R(4)[2] > 8 and R(4)[0] < 0.35 * R(4)[2], 'right edge should pick up blue'
assert L(4)[0] > L(20)[0] > L(70)[0] - 0.5 and abs(L(70)).max() < 1.5, 'wrap must fade inward and be gone deep inside'
assert (on[180, 300:340] == off[180, 300:340]).all(), 'disc centre must be untouched'
outside = np.ones((360, 640), bool); yy, xx = np.mgrid[0:360, 0:640]; outside &= ((xx - 320) ** 2 + (yy - 180) ** 2) > 125 ** 2
assert (on[outside] == off[outside]).all(), 'pixels outside the layer must not change'
setp(intensity=0); assert (shot('z') == off).all(), 'intensity 0 must be a no-op'
setp(intensity=1, desaturate=1); d = shot('d'); e = d[180, 204] - off[180, 204]; print('desaturated left +4:', e.round(1)); assert max(e) - min(e) < 0.25 * max(L(4)), 'desaturate=1 must give a grey wrap'
print('light_wrap proof OK')
