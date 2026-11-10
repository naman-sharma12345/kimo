import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
def trk(): return k.call("add_track")["track_id"]
t=trk(); k.call("add_clip",track_id=t,source_type="solid",color=[0.1,0.1,0.12],start=0,duration=2)
ids=[]
for (x,y,w,h,l,col) in [(0.3,0.2,0.2,0.5,"giant_1",[0.25,0.85,0.45]),(0.55,0.5,0.3,0.3,"traffic_1",[0.2,0.8,0.95]),(0.45,0.3,0.15,0.2,"doner_1",[1.0,0.55,0.1])]:
    t=trk(); ids+=k.call("add_detection_box",track_id=t,start=0,duration=2,x=x,y=y,w=w,h=h,label=l,color=col,instant=True)["clip_ids"]
g=k.call("group_clips",clip_ids=ids)["clip_id"]; print(g)
k.call("set_keyframe",clip_id=g,property="scale",time=0,value=1.0,ease="in_out"); k.call("set_keyframe",clip_id=g,property="scale",time=1.5,value=0.45,ease="in_out")
k.call("set_keyframe",clip_id=g,property="rotation",time=0,value=0,ease="in_out"); k.call("set_keyframe",clip_id=g,property="rotation",time=1.5,value=12,ease="in_out")
k.call("add_effect",clip_id=g,kind="rgb_split",params={"amount":6})
for tt in (0.2,1.0,1.5): k.call("render_frame",time=tt,path=f"/tmp/pc_{tt}.png")
