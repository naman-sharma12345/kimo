# Clip boundary reproducibility: clip A (red) ends and clip B (blue) starts at an accumulated-float time (0.1+0.2 style).
# The frame at k/fps must belong to B exactly (half-open [start,start+dur)). Writes only to /tmp.
import sys,os;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'));from mcp_client import Kimo
from PIL import Image
k=Kimo();C=k.call;bad=0;n=0
for fps in (24,25,30,50,60):
  for kb in [x for x in range(1,200) if sum([1.0/fps]*x)>x/fps][:6]+[3,7]:
    C('new_project') if False else None
    C('project_set',width=16,height=16,fps=fps)
    t=C('add_track')['track_id']
    b=sum([1.0/fps]*kb)           # accumulated, may differ from kb/fps by 1 ulp
    C('add_clip',track_id=t,source_type='solid',color=[1,0,0],start=0,duration=b)
    t2=C('add_track')['track_id'];C('add_clip',track_id=t2,source_type='solid',color=[0,0,1],start=b,duration=1)
    C('render_frame',time=kb/fps,path='/tmp/bt.png');px=Image.open('/tmp/bt.png').convert('RGB').getpixel((8,8));n+=1
    if not(px[2]>200 and px[0]<50): bad+=1;print('FAIL',fps,kb,px)
print('checked',n,'bad',bad);sys.exit(1 if bad else 0)
