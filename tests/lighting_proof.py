#!/usr/bin/env python3
"""Shared light/material proof over MCP stdio. Writes only to /tmp."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples'))
from mcp_client import Kimo
from PIL import Image, ImageChops
import numpy as np
k = Kimo(); C = k.call; C('project_set', width=640, height=360, fps=25)
b = C('add_track')['track_id']; C('add_clip', track_id=b, source_type='solid', color=[0.05, 0.05, 0.07], start=0, duration=2)
t2 = C('add_track')['track_id']; cid = C('add_text', track_id=t2, text='KIMO', start=0, duration=2, size=150, color=[0.85, 0.85, 0.9])['clip_id']
C('render_frame', time=1.0, path='/tmp/lp_plain.png')
C('set_light', id='key', kind='point', pos=[0.2, 0.15, 0.5], intensity=1.4, falloff=3)
C('set_relight', clip_id=cid, params={'bump': 2, 'blur': 8}, strings={'material': 'plastic'})
C('render_frame', time=1.0, path='/tmp/lp_a.png')
C('set_light', id='key', pos=[0.8, 0.15, 0.5])
C('render_frame', time=1.0, path='/tmp/lp_b.png')
pl, a, bb = [np.array(Image.open(p).convert('RGB')).astype(int) for p in ('/tmp/lp_plain.png', '/tmp/lp_a.png', '/tmp/lp_b.png')]
assert np.abs(pl - a).max() > 40, 'relight changed nothing'
assert np.abs(a - bb).max() > 40, 'moving the light changed nothing'
ys, xs = np.where(pl.min(2) > 150); assert len(xs) > 100
mid = (xs.min() + xs.max()) // 2; L = lambda im: im[ys, xs][xs < mid].sum(); R = lambda im: im[ys, xs][xs >= mid].sum()
assert L(a) > R(a) * 1.0 and R(bb) > L(bb) * 1.0, 'highlight did not follow the light side'
# alpha untouched, background untouched
assert (pl[:5, :5] == a[:5, :5]).all()
for bad in ({'id': 'x', 'kind': 'laser'}, {'id': 'x', 'intensity': -1}, {'id': 'x', 'cone': 500}):
    try: C('set_light', **bad); raise SystemExit('accepted bad light ' + str(bad))
    except SystemExit: raise
    except Exception: pass
try: C('set_relight', clip_id=cid, strings={'material': 'nope'}); raise SystemExit('accepted bad material')
except SystemExit: raise
except Exception: pass
print(C('preview_material', path='/tmp/lp_pv.png', material='gold')); print('PASS')
