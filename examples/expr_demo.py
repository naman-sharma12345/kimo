# Expressions + bezier handles proof: 4 squares. A wiggle, B loop_out pingpong, C bezier overshoot vs D linear, with posterize_time on a 5th.
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
D=3.0
def box(x,y,col):
    t=k.call("add_track")["track_id"]; return k.call("add_shape",track_id=t,shape="rect",start=0,duration=D,width=110,height=110,fill=col,corner_radius=12)["clip_id"]
def pos(c,x,y): k.call("set_property",clip_id=c,property="x",value=x-640); k.call("set_property",clip_id=c,property="y",value=0)
a=box(0,0,[1,.4,.2]); pos(a,0.1*1280,360)
k.call("set_expression",clip_id=a,property="x",op="wiggle",freq=3,amp=60,octaves=3,seed=4)
k.call("set_expression",clip_id=a,property="y",op="wiggle",freq=2,amp=60,octaves=2,seed=9)
b=box(0,0,[.3,.8,1]); pos(b,0.3*1280,360)
for tm,v in [(0,-160),(0.5,160)]: k.call("set_keyframe",clip_id=b,property="y",time=tm,value=v,ease="in_out")
k.call("set_expression",clip_id=b,property="y",op="loop_out",mode="pingpong")
c=box(0,0,[.4,1,.5]); pos(c,0.5*1280,200)
for tm,v in [(0,-160),(1.0,160)]: k.call("set_keyframe",clip_id=c,property="y",time=tm,value=v,ease="linear")
k.call("set_keyframe_bezier",clip_id=c,property="y",time=0,bezier=[0.34,1.8,0.64,1])
d=box(0,0,[.8,.8,.8]); pos(d,0.7*1280,200)
for tm,v in [(0,-160),(1.0,160)]: k.call("set_keyframe",clip_id=d,property="y",time=tm,value=v,ease="linear")
e=box(0,0,[1,.3,.8]); pos(e,0.9*1280,360)
for tm,v in [(0,-160),(1.5,160)]: k.call("set_keyframe",clip_id=e,property="y",time=tm,value=v,ease="linear")
k.call("set_expression",clip_id=e,property="y",op="posterize_time",fps=6)
k.call("set_expression",clip_id=e,property="y",op="loop_out",mode="offset")
if len(sys.argv)>1: print(k.call("export",path="/tmp/expr.mp4",crf=18))
