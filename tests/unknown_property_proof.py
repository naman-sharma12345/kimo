"""set_property / set_keyframe must reject names nothing reads (before: {'ok': true} for 'banana' and 'crop_left', so an AI
believed it had cropped). Valid names still work. Writes nothing."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples')); from mcp_client import Kimo
k = Kimo(); c = k.call; c('project_set', width=320, height=180, fps=25); t = c('add_track')['track_id']
cid = c('add_shape', track_id=t, shape='rect', width=100, height=60, fill=[1, 0, 0, 1], start=0, duration=1)['clip_id']
for p in ('opacity', 'x', 'y', 'scale', 'rotation', 'rot_x', 'rot_y', 'pos_z', 'backface'):
    c('set_property', clip_id=cid, property=p, value=0.5); c('set_keyframe', clip_id=cid, property=p, time=0, value=0.5)
for p in ('banana', 'crop_left', 'flip_h', 'anchor_x'):
    for tool, kw in (('set_property', {}), ('set_keyframe', {'time': 0})):
        try: c(tool, clip_id=cid, property=p, value=1, **kw); raise SystemExit('FAIL: %s accepted %s' % (tool, p))
        except SystemExit: raise
        except Exception as e: assert 'unknown' in str(e).lower() or 'enum' in str(e).lower() or 'property' in str(e).lower(), e
print('unknown property proof OK')
