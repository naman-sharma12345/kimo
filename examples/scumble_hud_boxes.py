# SCUMBLE 3.7-7.7s: HUD viewfinder screen, detection boxes snap in, then fill with the photo (box-fill reveal via animated mask).
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
D=4.0
def trk(): return k.call("add_track")["track_id"]
t=trk(); k.call("add_hud_frame",track_id=t,start=0,duration=D,label="BOXES",bpm=125,tc_start=4.32)
A=k.call("import_media",path="/tmp/ref/pA.png")["asset_id"]
boxes=[(0.30,0.20,0.34,0.52,"giant_1",0.98,[0.25,0.85,0.35],"a giant on the bridge",0.0),
       (0.08,0.58,0.20,0.26,"traffic_1",0.91,[0.2,0.85,0.95],"traffic jam",0.44),
       (0.66,0.14,0.22,0.40,"doner_1",0.96,[1.0,0.55,0.1],"huge doner kebab",0.92),
       (0.70,0.62,0.20,0.26,"phones_1",0.87,[1.0,0.35,0.7],"phones",1.40),
       (0.10,0.14,0.16,0.30,"comfy_dev_challenge_1",0.74,[0.95,0.85,0.15],"",1.88)]
for (x,y,w,h,l,cf,col,cap,s0) in boxes:
    t=trk(); k.call("add_detection_box",track_id=t,start=s0,duration=D-s0,x=x,y=y,w=w,h=h,label=l,confidence=cf,color=col,caption=cap,fill_opacity=0.1,impact=True)
# fill reveal: photo masked to the first box, mask grows from its centre
t=trk(); c=k.call("add_clip",track_id=t,source_type="media",asset_id=A,start=2.6,duration=D-2.6)["clip_id"]
x,y,w,h=boxes[0][:4]
k.call("set_mask",clip_id=c,shape="rect",cx=x+w/2,cy=y+h/2,w=0.0,h=0.0,feather=0.0)
for p,v0,v1 in [("w",0.0,w),("h",0.0,h)]:
    k.call("set_mask_keyframe",clip_id=c,property=p,time=0.0,value=v0,ease="out_expo"); k.call("set_mask_keyframe",clip_id=c,property=p,time=0.6,value=v1,ease="out_expo")
if len(sys.argv)>1: k.call("export",path="/tmp/hud3.mp4",crf=17)
else:
    for tt in (1.0,2.0,3.2): k.call("render_frame",time=tt,path=f"/tmp/hud_{tt}.png")
