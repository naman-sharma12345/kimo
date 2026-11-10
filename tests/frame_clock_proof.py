#!/usr/bin/env python3
"""Exact frame-clock regression: rendering at exactly i/fps must equal rendering a hair later (never the previous frame).
Makes its own plate; writes only to /tmp."""
import sys, os, subprocess; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples'))
from mcp_client import Kimo
from PIL import Image
import numpy as np
plate = '/tmp/fc_plate.mp4'
subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=25:duration=4', '-pix_fmt', 'yuv420p', plate], check=True)
k = Kimo(); C = k.call; C('project_set', width=320, height=180, fps=25)
a = C('import_media', path=plate)['asset_id']; t = C('add_track')['track_id']; C('add_clip', track_id=t, source_type='media', asset_id=a, start=0, duration=4)
bad = []
for i in range(1, 99):
    ims = []
    for eps in (0, 1e-4): C('render_frame', time=i / 25 + eps, path='/tmp/fc_x.png'); ims.append(np.array(Image.open('/tmp/fc_x.png').convert('RGB')).astype(int))
    if np.abs(ims[0] - ims[1]).mean() > 0.0: bad.append(i)
assert not bad, f'exact-time frames differ from epsilon frames at {bad}'
print('PASS: 98 exact frame times match their epsilon renders')
