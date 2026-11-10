#!/usr/bin/env python3
"""add_text x without anchor, add_shape RGBA fill. Writes only to /tmp."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples'))
from mcp_client import Kimo
from PIL import Image
import numpy as np
k = Kimo(); C = k.call; C('project_set', width=640, height=360, fps=25)
b = C('add_track')['track_id']; C('add_clip', track_id=b, source_type='solid', color=[0, 0, 0], start=0, duration=1)
t = C('add_track')['track_id']
c0 = C('add_text', track_id=t, text='KIMO', start=0, duration=1, size=80)['clip_id']
t2 = C('add_track')['track_id']; c1 = C('add_text', track_id=t2, text='KIMO', start=0, duration=1, size=80, x=150)['clip_id']
def cx(path):
    im = np.array(Image.open(path).convert('L')).astype(int); ys, xs = np.where(im > 128); return (xs.min() + xs.max()) / 2
C('remove_clip', clip_id=c1) if False else None
C('render_frame', time=0.5, path='/tmp/cf_a.png'); both = cx('/tmp/cf_a.png')
C('set_property', clip_id=c1, property='opacity', value=0.0); C('render_frame', time=0.5, path='/tmp/cf_b.png'); only0 = cx('/tmp/cf_b.png')
assert abs(only0 - 320) < 8, only0
C('set_property', clip_id=c0, property='opacity', value=0.0); C('set_property', clip_id=c1, property='opacity', value=1.0); C('render_frame', time=0.5, path='/tmp/cf_c.png'); sh = cx('/tmp/cf_c.png')
assert abs(sh - 470) < 8, f'x=150 without anchor should centre near 470, got {sh}'
C('set_property', clip_id=c1, property='opacity', value=0.0)
t3 = C('add_track')['track_id']; s1 = C('add_shape', track_id=t3, shape='rect', start=0, duration=1, width=200, height=200, fill=[1, 0, 0, 0.5])
C('render_frame', time=0.5, path='/tmp/cf_d.png'); px = np.array(Image.open('/tmp/cf_d.png').convert('RGB'))[180, 320]
assert 100 < px[0] < 160 and px[1] < 10, f'RGBA fill should give ~half red, got {px}'
t4 = C('add_track')['track_id']; r = C('add_shape', track_id=t4, shape='rect', start=0, duration=1, width=50, height=50, fill=[1, 1, 1], stroke_width=4, stroke_color=[1, 0, 0, 0.3]); assert 'warning' not in r and r.get('clip_id'), r   # RV-1: stroke alpha is supported (stroke_opacity), the old 'alpha not supported' warning was removed on purpose
print('PASS', only0, sh, px)
