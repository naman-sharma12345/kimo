"""bloom: light-intensity-responsive, energy-conserving bloom. Black frame, 160x160 light square (grey 0.9, then exposure +1 stop = 1.8).
Checks: (1) intensity response: the bloom tail outside the square scales with the light's excess over the threshold (hard knee, so 1.3/0.4 = 3.25x
when the light goes 0.9 -> 1.8, even though the square itself clips at 1.0 in the PNG); (2) conserve=1 keeps the frame sum within 1% of the input
(away from borders) and conserve=0 adds amount * total excess within 4%; (3) a red light blooms red; (4) amount 0 is bit-identical to effect-off;
(5) a threshold above the light's level is a no-op. Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
W, H = 640, 360
k = Kimo(); c = k.call; c('project_set', width=W, height=H, fps=25); t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_shape', track_id=t1, shape='rect', width=W, height=H, fill=[0, 0, 0, 1], start=0, duration=2)
sid = c('add_shape', track_id=t2, shape='rect', width=160, height=160, fill=[0.9, 0.9, 0.9, 1], start=0, duration=2)['clip_id']
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(H, W, 3).astype(float) / 255
def shot(n): c('render_frame', time=0.5, path='/tmp/bl_%s.png' % n); return img('/tmp/bl_%s.png' % n)
off = shot('off')
ex = c('add_effect', clip_id=sid, kind='exposure')['effect_id']; c('set_effect_param', clip_id=sid, effect_id=ex, param='stops', value=0.0)
fx = c('add_effect', clip_id=sid, kind='bloom')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
yy, xx = np.mgrid[0:H, 0:W]; outside = (np.abs(xx - 320) > 90) | (np.abs(yy - 180) > 90); ring = outside & (np.abs(xx - 320) < 150) & (np.abs(yy - 180) < 150)
setp(amount=1.0, threshold=0.5, knee=0.0, radius=12, levels=4, conserve=1)
a0 = shot('ev0'); c('set_effect_param', clip_id=sid, effect_id=ex, param='stops', value=1.0); a1 = shot('ev1')
t0, t1_ = a0[:, :, 1][ring].mean(), a1[:, :, 1][ring].mean(); pred = (1.8 - 0.5) / (0.9 - 0.5)
print('tail mean (0..1) ev0 %.4f ev1 %.4f ratio %.2f predicted %.2f' % (t0, t1_, t1_ / t0, pred)); assert t0 > 0.004 and abs(t1_ / t0 - pred) < 0.12 * pred, 'bloom tail must scale with light excess'
# energy
c('set_effect_param', clip_id=sid, effect_id=ex, param='stops', value=0.0); si = off[:, :, 1].sum()
sq = shot('sq0'); c('set_effect_param', clip_id=sid, effect_id=fx, param='amount', value=0.0); noamt = shot('amt0'); base = noamt[:, :, 1].sum()
setp(amount=1.0); cons = a0[:, :, 1].sum(); print('frame sum input %.0f conserve %.0f (%.2f%%)' % (base, cons, 100 * (cons / base - 1))); assert abs(cons / base - 1) < 0.01, 'conserve must preserve the frame sum'
c('set_effect_param', clip_id=sid, effect_id=ex, param='stops', value=-1.0); setp(conserve=0, threshold=0.2, amount=0.0); base = shot('dim')[:, :, 1].sum(); setp(amount=1.0); add = shot('add')[:, :, 1].sum(); exc = 160 * 160 * (0.45 - 0.2); print('additive adds %.0f, predicted %.0f' % (add - base, exc)); assert abs((add - base) - exc) < 0.04 * exc, 'additive must add amount * excess'
# red light blooms red
c('set_effect_param', clip_id=sid, effect_id=ex, param='stops', value=0.0); setp(threshold=0.5)
c('set_effect_param', clip_id=sid, effect_id=fx, param='conserve', value=1)
setp(tint_g=0.3, tint_b=0.3); tl = shot('tint'); print('tint tail rgb', [round(tl[:, :, j][ring].mean(), 4) for j in range(3)]); assert tl[:, :, 0][ring].mean() > 2.5 * tl[:, :, 1][ring].mean(), 'tint must colour the bloom'
setp(tint_g=1, tint_b=1)
# identity cases
setp(amount=0.0); z = shot('zero'); assert np.array_equal(z, noamt), 'amount 0 must equal effect-off render'
setp(amount=1.0, threshold=1.5); n_ = shot('hi'); assert np.array_equal(n_, noamt), 'threshold above the light level must be a no-op'
print('bloom proof OK')
