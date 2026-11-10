"""set_effect_param warns on a param name the effect does not list (typo, or a feature the AI imagined); listed params give no
warning; frosted_glass lists magnify_cx, magnify_cy, tilt_refraction (they were missing from list_effects). Writes nothing."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
cid = c('add_shape', track_id=t, shape='rect', width=100, height=60, fill=[1, 0, 0, 1], start=0, duration=1)['clip_id']
fx = c('add_effect', clip_id=cid, kind='frosted_glass')['effect_id']
assert 'warning' not in c('set_effect_param', clip_id=cid, effect_id=fx, param='blur', value=5)
for p in ('magnify_cx', 'magnify_cy', 'tilt_refraction'): assert 'warning' not in c('set_effect_param', clip_id=cid, effect_id=fx, param=p, value=0.5), p
r = c('set_effect_param', clip_id=cid, effect_id=fx, param='blurr', value=5); assert 'warning' in r and 'blur' in r['warning'], r
print('effect param warning proof OK:', r['warning'][:80])
