#!/usr/bin/env python3
"""load_project must not reuse ids already in the project. Writes only to /tmp."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples'))
from mcp_client import Kimo
k = Kimo(); C = k.call; C('project_set', width=320, height=180, fps=25)
t = C('add_track')['track_id']; ids = [C('add_clip', track_id=t, source_type='solid', color=[0.5, 0.5, 0.5], start=i, duration=1)['clip_id'] for i in range(5)]
C('save_project', path='/tmp/lid.json')
import json; txt = open('/tmp/lid.json').read()
for i, c in enumerate(ids): txt = txt.replace('"%s"' % c, '"clip%d"' % (1000 + i))
ids = ['clip%d' % (1000 + i) for i in range(5)]; open('/tmp/lid.json', 'w').write(txt)
k2 = Kimo(); C2 = k2.call; r = C2('load_project', path='/tmp/lid.json'); print(r)
t2 = C2('add_track')['track_id']; new = [C2('add_clip', track_id=t2, source_type='solid', color=[0.1, 0.1, 0.1], start=i, duration=1)['clip_id'] for i in range(5)]
assert not set(new) & set(ids) and t2 != t, (new, ids, t, t2)
print('PASS', ids, new)
