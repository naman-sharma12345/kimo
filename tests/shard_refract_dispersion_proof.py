"""shard burst refract_dispersion: chromatic dispersion of the canvas bent behind glass shards. Background = horizontal luminance ramp, so a
channel's displacement is read straight from its value change. Three renders: refract 0 (no bend), refract 1 / refract_dispersion 0, refract 1 /
refract_dispersion 1. By construction the channel offsets are scaled (1-rd, 1, 1+rd) for R, G, B (blue bends most, as in real glass), so with rd=1:
R must equal the unbent render (offset 0), G must equal the rd=0 render, and B's change from the unbent render must be twice the rd=0 change
(offset x2). rd=0 must be bit-identical to the old behaviour (same render as a config that omits the field). Writes only to /tmp."""
import sys, os, numpy as np; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
from PIL import Image
W, H = 640, 360; x = np.arange(W); r = np.clip((x - 170) / 300.0, 0, 1) * 255; Image.fromarray(np.tile(r.astype(np.uint8), (H, 1)), 'L').convert('RGB').save('/tmp/sdp_ramp.png')
def render(**extra):
    k = Kimo(); C = k.call; C('project_set', width=W, height=H, fps=25); t1 = C('add_track')['track_id']; t2 = C('add_track')['track_id']
    a = C('import_media', path='/tmp/sdp_ramp.png')['asset_id']; C('add_clip', track_id=t1, source_type='media', asset_id=a, start=0, duration=2, fit='cover')
    cfg = dict(count=90, size_min=40, size_max=110, origin=[320, 180, 0], speed=120, dof=0, shutter_deg=0, seed=7, thickness=[0.3, 0.9], toward_camera=0.3, material='glass'); cfg.update(extra)
    sb = C('add_shard_burst', track_id=t2, start=0, duration=2, config=cfg); C('set_property', clip_id=sb['clip_id'], property='opacity', value=0.0);  # opacity 0: only the bent canvas remains, shard colour cannot mask it
    C('render_frame', time=0.8, path='/tmp/sdp.png'); k.close() if hasattr(k, 'close') else None
    return np.array(Image.open('/tmp/sdp.png').convert('RGB')).astype(float)
nb = render(refract=0.0); r0 = render(refract=30.0); r0b = render(refract=30.0, refract_dispersion=0.0); r1 = render(refract=30.0, refract_dispersion=1.0)
assert np.array_equal(r0, r0b), 'rd=0 must equal the default render'
d0 = r0 - nb; print('rd=0: max |change| from unbent (R,G,B):', [round(float(np.abs(d0[:, :, j]).max()), 1) for j in range(3)]); assert np.abs(d0).max() > 6, 'no refraction happened; proof is vacuous'
assert np.abs(r1[:, :, 0] - nb[:, :, 0]).max() <= 1.01, 'R offset must be zero at rd=1'
assert np.abs(r1[:, :, 1] - r0[:, :, 1]).max() <= 1.01, 'G must be unchanged by dispersion'
m = np.abs(d0[:, :, 2]) > 4; db1 = (r1[:, :, 2] - nb[:, :, 2])[m]; ratio = np.median(db1 / d0[:, :, 2][m]); print('B change ratio rd=1 / rd=0: median %.2f over %d px (expected 2.00)' % (ratio, m.sum()))
assert 1.8 < ratio < 2.2, 'B offset must double at rd=1'
print('shard refract_dispersion proof OK')
