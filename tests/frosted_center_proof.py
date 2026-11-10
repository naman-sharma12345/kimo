"""frosted_glass magnify_cx / magnify_cy: the lens zooms about a chosen canvas point instead of the pane centroid.
A clear lens (no blur, tint, refraction, edge light) must show the plate resampled about that point; the error to the right
resample must be small and far smaller than to the centroid resample. Writes only to /tmp."""
import sys, subprocess, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
plate = '/tmp/fc_plate.mp4'; subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'mandelbrot=s=640x360:r=25', '-t', '2', '-pix_fmt', 'yuv420p', plate], check=True)
k = Kimo(); c = k.call; c('project_set', width=640, height=360, fps=25)
aid = c('import_media', path=plate)['asset_id']; t1 = c('add_track')['track_id']; t2 = c('add_track')['track_id']
c('add_clip', track_id=t1, source_type='media', asset_id=aid, start=0, duration=2, in_point=0, fit='cover')
sid = c('add_shape', track_id=t2, shape='rect', width=300, height=140, fill=[1, 1, 1, 1], start=0, duration=2)['clip_id']
def img(p): r = subprocess.run(['ffmpeg', '-v', 'error', '-i', p, '-f', 'rawvideo', '-pix_fmt', 'rgb24', '-'], capture_output=True, check=True).stdout; return np.frombuffer(r, np.uint8).reshape(360, 640, 3).astype(float)
subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', plate, '-vf', 'select=eq(n\\,12)', '-vsync', '0', '-frames:v', '1', '/tmp/fc_off.png'], check=True); plain = img('/tmp/fc_off.png')
fx = c('add_effect', clip_id=sid, kind='frosted_glass')['effect_id']
def setp(**kw):
    for kk, v in kw.items(): c('set_effect_param', clip_id=sid, effect_id=fx, param=kk, value=v)
setp(blur=0, tint_amount=0.0, saturation=1.0, edge_light=0.0, refraction=0.0, dispersion=0.0, bevel=0.0, magnify=3.0)
M = 3.0; CX, CY = 320.0, 180.0   # pane is centred on the canvas, so the alpha centroid is the canvas centre
def sample(src, xs, ys):
    xs = np.clip(xs, 0, 638.99); ys = np.clip(ys, 0, 358.99); x0 = xs.astype(int); y0 = ys.astype(int); fx_, fy_ = (xs - x0)[..., None], (ys - y0)[..., None]
    return src[y0, x0] * (1 - fx_) * (1 - fy_) + src[y0, x0 + 1] * fx_ * (1 - fy_) + src[y0 + 1, x0] * (1 - fx_) * fy_ + src[y0 + 1, x0 + 1] * fx_ * fy_
Y, X = np.mgrid[170:190, 270:370]; X = X + 0.5; Y = Y + 0.5      # a core region well inside the pane
def expect(cx, cy): return sample(plain, cx + (X - cx) / M, cy + (Y - cy) / M)
def err(img, cx, cy): return np.abs(img[170:190, 270:370] - expect(cx, cy)).mean()
c('render_frame', time=0.5, path='/tmp/fc_default.png'); d0 = img('/tmp/fc_default.png')
setp(magnify_cx=0.25, magnify_cy=0.4); c('render_frame', time=0.5, path='/tmp/fc_moved.png'); d1 = img('/tmp/fc_moved.png')
e_def = err(d0, CX, CY); e_moved_right = err(d1, 0.25 * 640, 0.4 * 360); e_moved_wrong = err(d1, CX, CY)
print('default centre: error to centroid resample %.2f' % e_def)
print('magnify_cx=0.25 cy=0.4: error to resample about (160,144) %.2f, to centroid resample %.2f' % (e_moved_right, e_moved_wrong))
assert e_def < 15 and e_def < 0.25 * err(d0, 0.25 * 640, 0.4 * 360), 'default is not a zoom about the pane centre'
assert e_moved_right < 6 and e_moved_wrong > 3 * max(e_moved_right, 1.0), 'magnify_cx/cy did not move the zoom centre'
setp(magnify_cx=-1.0, magnify_cy=-1.0); c('render_frame', time=0.5, path='/tmp/fc_back.png'); assert (img('/tmp/fc_back.png') == d0).all(), '-1 does not restore the centroid'
print('magnify_cx proof OK')
