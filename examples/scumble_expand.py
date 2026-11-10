# SCUMBLE 7.6-8.8 s: doner box expands at constant speed to full frame, 2-frame white flash, hard cut to the zoomed-out scene
# where boxes are filled with image crops via animated masks.
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
def trk(): return k.call("add_track")["track_id"]
D=1.3
A=k.call("import_media",path="/tmp/ref/pA.png")["asset_id"]
# part 1 (0-0.44 s): HUD with the finished boxes, doner box expanding
t=trk(); k.call("add_hud_frame",track_id=t,start=0,duration=0.32,label="BOXES",tc_start=7.6)
static=[(0.30,0.20,0.17,0.5,"giant_1",0.98,[0.25,0.85,0.35],""),(0.08,0.58,0.45,0.25,"traffic_1",0.91,[0.2,0.85,0.95],"traffic jam"),(0.70,0.62,0.12,0.2,"phones_1",0.87,[1.0,0.35,0.7],"")]
for (x,y,w,h,l,cf,col,cap) in static:
    t=trk(); k.call("add_detection_box",track_id=t,start=0,duration=0.32,x=x,y=y,w=w,h=h,label=l,confidence=cf,color=col,caption=cap,fill_opacity=0.1)
t=trk(); r=k.call("add_box_expand",track_id=t,start=0.0,frames=8,label="doner_1 0.96"); end=r["end"]; print("flash end",end)
# part 2: hard cut to zoomed-out scene, boxes filled by photo via mask growth
t=trk(); k.call("add_clip",track_id=t,source_type="solid",color=[0.13,0.14,0.15],start=end,duration=D-end)
S=end
boxes=[(0.34,0.16,0.14,0.42,[0.25,0.85,0.35],"giant_1 0.98"),(0.20,0.50,0.52,0.50,[0.2,0.85,0.95],"traffic_1 0.91"),(0.41,0.26,0.17,0.28,[1.0,0.55,0.1],"doner_1 0.96"),(0.45,0.60,0.07,0.06,[1.0,0.35,0.7],"phones_1 0.87")]
for i,(x,y,w,h,col,l) in enumerate(boxes):
    t=trk(); k.call("add_detection_box",track_id=t,start=S,duration=D-S,x=x,y=y,w=w,h=h,label=l.split()[0],confidence=float(l.split()[1]),color=col,fill_opacity=0.1)
    t=trk(); c=k.call("add_clip",track_id=t,source_type="media",asset_id=A,start=S+0.12+0.1*i,duration=D-S-0.12-0.1*i)["clip_id"]
    k.call("set_mask",clip_id=c,shape="rect",cx=x+w/2,cy=y+h/2,w=w,h=0.0,feather=0.0)
    k.call("set_mask_keyframe",clip_id=c,property="h",time=0.0,value=0.0,ease="linear"); k.call("set_mask_keyframe",clip_id=c,property="h",time=0.3,value=h,ease="linear")
    k.call("set_property",clip_id=c,property="opacity",value=0.9) if False else None
if len(sys.argv)>1: k.call("export",path="/tmp/expand.mp4",crf=17)
else:
    for i in range(0,32,2): k.call("render_frame",time=i/25,path=f"/tmp/ex/k_{i:02d}.png")
