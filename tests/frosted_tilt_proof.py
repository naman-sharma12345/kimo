"""frosted_glass on a 3D-tilted layer (rot_y): before this fix the effect was silently skipped. Now the pane is the projected
silhouette and frosts what is under it. Checks: effect changes the frame; pixels outside the pane are identical to the effect-off
render; detail inside the pane drops; the pane footprint is a trapezoid (its height differs between its left and right edge),
i.e. it really follows the tilted layer. Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
plate = '/tmp/ft_plate.mp4'; subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'mandelbrot=s=640x360:r=25', '-t', '2', '-pix_fmt', 'yuv420p', plate], check=True)
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25)
aid = c('import_media', path=plate)['asset_id']; t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_clip', track_id=t1, source_type='media', asset_id=aid, start=0, duration=2, in_point=0, fit='cover')
sid = c('add_shape', track_id=t2, shape='rect', width=360, height=180, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
c('set_property', clip_id=sid, property='rot_y', value=42)
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float)
subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', plate, '-vf', 'select=eq(n\\,12)', '-vsync', '0', '-frames:v', '1', '/tmp/ft_plate.png'], check=True); plate_f = img('/tmp/ft_plate.png')
c('render_frame', time=0.5, path='/tmp/ft_off.png'); off = img('/tmp/ft_off.png')
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
for kk, v in dict(blur=18, tint_amount=0.0, edge_light=0.0, refraction=0.0).items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
c('render_frame', time=0.5, path='/tmp/ft_on.png'); on = img('/tmp/ft_on.png')
diff = np.abs(on - off).sum(axis=2) > 6; assert diff.any(), 'frosted_glass did nothing on a tilted layer'
ys, xs = np.nonzero(diff); print('pane footprint: x %d..%d, y %d..%d, %d px changed' % (xs.min(), xs.max(), ys.min(), ys.max(), diff.sum()))
# the pane is the white tilted quad in the effect-off render
pane = (off.min(axis=2) > 240)
cols = np.nonzero(pane.any(axis=0))[0]; l, r = cols[:8].mean(), cols[-8:].mean(); hl = pane[:, int(cols[3])].sum(); hr = pane[:, int(cols[-4])].sum()
print('tilted pane height at left edge %d px, right edge %d px' % (hl, hr)); assert abs(hl - hr) > 10, 'test layer is not visibly tilted'
dh = diff[:, int(cols[3]) + 3].sum(); dr = diff[:, int(cols[-4]) - 3].sum(); print('changed-pixel column height near left %d, near right %d' % (dh, dr))
assert (dh > dr) == (hl > hr) and abs(dh - dr) > 10, 'frosted region does not follow the tilted silhouette'
outside = ~(np.array([[True]]) & (pane | diff)); grown = np.zeros_like(pane)
for dy in range(-3, 4):
    for dx in range(-3, 4): grown |= np.roll(np.roll(pane | diff, dy, 0), dx, 1)
assert (on[~grown] == off[~grown]).all(), 'pixels outside the pane changed'
cy, cx = int(ys.mean()), int(xs.mean()); R = (slice(cy - 15, cy + 15), slice(cx - 25, cx + 25))
d2 = lambda b: np.abs(b[R][:, 2:, :] - 2 * b[R][:, 1:-1, :][:, :, :].repeat(1, 0)[:, :0 + b[R].shape[1] - 2, :] + 0).sum() if False else np.abs(b[R][:, :-2, 1] - 2 * b[R][:, 1:-1, 1] + b[R][:, 2:, 1]).sum()
print('detail in pane core: frosted %.0f vs plate %.0f' % (d2(on), d2(plate_f))); assert d2(on) < 0.6 * d2(plate_f), 'pane not blurred'
print('frosted tilt proof OK')
