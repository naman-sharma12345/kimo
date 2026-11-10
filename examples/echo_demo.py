# Echo / temporal smear / posterize time proof: 4 rows moving the same object.
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25); D=3.0
def row(y,col,shape="ellipse"):
    t=k.call("add_track")["track_id"]; c=k.call("add_shape",track_id=t,shape=shape,start=0,duration=D,width=90,height=90,fill=col)["clip_id"]
    k.call("set_property",clip_id=c,property="y",value=y)
    for tm,v in [(0,-520),(1.2,520),(2.4,-520)]: k.call("set_keyframe",clip_id=c,property="x",time=tm,value=v,ease="in_out")
    return c
a=row(-240,[1,.5,.2]); k.call("set_echo",clip_id=a,count=8,spacing=0.04,decay=0.75,mode="composite")
b=row(-80,[.3,.8,1]); k.call("set_echo",clip_id=b,count=10,spacing=0.03,decay=0.8,mode="add")
c=row(80,[.5,1,.5]); k.call("set_echo",clip_id=c,count=6,spacing=0.012,mode="average")
d=row(240,[1,.4,.8],"rect"); k.call("set_posterize_time",clip_id=d,fps=5)
if len(sys.argv)>1: print(k.call("export",path="/tmp/echo.mp4",crf=18))
