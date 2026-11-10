# SCUMBLE 7.6-9.2 s v2: constant-speed box expansion, 2-frame flash, then boxes at the reference's measured coordinates,
# filled with crops taken from the reference frames themselves via top-down scan wipes with a white scan line.
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); k.call("project_set",width=1280,height=720,fps=25)
def trk(): return k.call("add_track")["track_id"]
D=1.6
imp=lambda n: k.call("import_media",path=f"/tmp/ex2/{n}.png")["asset_id"]
HATCH=imp("hatch")
def hatch(t0,dur):
    t=trk(); k.call("add_clip",track_id=t,source_type="media",asset_id=HATCH,start=t0,duration=dur)
hatch(0,0.32)
t=trk(); k.call("add_hud_frame",track_id=t,start=0,duration=0.32,label="BOXES",tc_start=7.6,background=False)
for (x,y,w,h,l,cf,col,cap) in [(0.30,0.20,0.17,0.5,"giant_1",0.98,[0.25,0.85,0.35],""),(0.08,0.58,0.45,0.25,"traffic_1",0.91,[0.2,0.85,0.95],"traffic jam"),(0.70,0.62,0.12,0.2,"phones_1",0.87,[1.0,0.35,0.7],"")]:
    t=trk(); k.call("add_detection_box",track_id=t,start=0,duration=0.32,x=x,y=y,w=w,h=h,label=l,confidence=cf,color=col,caption=cap,fill_opacity=0.1)
t=trk(); k.call("add_box_expand",track_id=t,start=0.0,frames=8,from_=None,color=[0.95,0.85,0.15],fill_opacity=0.0,flash=False,label="comfy_dev_challenge_1 1.00") if False else k.call("add_box_expand",track_id=t,start=0.0,frames=8,**{"from":[0.008,0.02,0.984,0.93]},color=[0.95,0.85,0.15],fill_opacity=0.0,flash=False,label="comfy_dev_challenge_1 1.00")
t=trk(); end=k.call("add_box_expand",track_id=t,start=0.0,frames=8,label="doner_1 0.96")["end"]
S=end
# dark hatched backdrop (no HUD brackets after the cut)
hatch(S,D-S)
def wipe(name,t0,dur,line=False,glitch=False):
    t=trk(); c=k.call("add_clip",track_id=t,source_type="media",asset_id=imp(name),start=S+t0,duration=D-S-t0)["clip_id"]
    k.call("set_mask",clip_id=c,shape="rect",cx=0.5,cy=0.0,w=1.0,h=0.0,feather=0.0)
    k.call("set_mask_keyframe",clip_id=c,property="h",time=0.0,value=0.0,ease="linear"); k.call("set_mask_keyframe",clip_id=c,property="h",time=dur,value=2.0,ease="linear")
    k.call("set_mask_keyframe",clip_id=c,property="cy",time=0.0,value=0.0,ease="linear"); k.call("set_mask_keyframe",clip_id=c,property="cy",time=dur,value=1.0,ease="linear")
    if glitch:
        e=k.call("add_effect",clip_id=c,kind="glitch_blocks",params={"amount":0.0,"slices":14,"rate":25})["effect_id"]
        for tm,v in [(0,0),(0.04,38),(dur*0.7,22),(dur+0.15,0)]: k.call("set_keyframe",clip_id=c,effect_id=e,property="amount",time=tm,value=v,ease="linear")
    if not line: return
    t=trk(); l=k.call("add_shape",track_id=t,shape="rect",start=S+t0,duration=dur,width=1280,height=3,fill=[1,1,1])["clip_id"]
    k.call("set_keyframe",clip_id=l,property="y",time=0.0,value=-360,ease="linear"); k.call("set_keyframe",clip_id=l,property="y",time=dur,value=360,ease="linear")
    k.call("set_keyframe",clip_id=l,property="opacity",time=0.0,value=0.9,ease="linear"); k.call("set_keyframe",clip_id=l,property="opacity",time=dur,value=0.0,ease="in")
wipe("giant",0.12,0.24); wipe("don",0.2,0.2); wipe("traf",0.24,0.3,True,True)
wipe("sky",0.52,0.45,True); wipe("brL",0.52,0.45); wipe("brR",0.52,0.45)
bx=[(0.336,0.18,0.178,0.56,"giant_1",0.98,[0.25,0.85,0.45],""),(0.229,0.597,0.667,0.45,"traffic_1",0.91,[0.2,0.8,0.95],"traffic jam"),(0.406,0.322,0.164,0.217,"doner_1",0.96,[1.0,0.55,0.1],"huge doner kebab"),(0.479,0.889,0.05,0.11,"phones_1",0.87,[1.0,0.35,0.7],"")]
for (x,y,w,h,l,cf,col,cap) in bx:
    t=trk(); k.call("add_detection_box",track_id=t,start=S-0.35 if False else S,duration=D-S,x=x,y=y,w=w,h=h,label=l,confidence=cf,color=col,caption=cap,fill_opacity=0.0,instant=True)
# FILL % tag riding the scan line (sky wipe, timeline S+0.52 .. S+0.97)
t=trk(); tg=k.call("add_text",track_id=t,text="FILL {pct:%.2f:%.2f}%%"%(S+0.52,S+0.97),start=S+0.52,duration=0.5,size=15,font="SpaceMono-Bold")["clip_id"]
k.call("set_property",clip_id=tg,property="x",value=560)
k.call("set_keyframe",clip_id=tg,property="y",time=0.0,value=-340,ease="linear"); k.call("set_keyframe",clip_id=tg,property="y",time=0.45,value=340,ease="linear")
t=trk(); k.call("add_camera_shake",track_id=t,start=0,duration=D,hits=[S])
if len(sys.argv)>1: k.call("export",path="/tmp/expand3.mp4",crf=17)
else:
    for i in range(0,40,2): k.call("render_frame",time=i/25,path=f"/tmp/ex2/k_{i:02d}.png")
