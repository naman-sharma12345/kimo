# SCUMBLE 8.92-11.8 s: photo + boxes, ink-threshold flash, DRAW ME A BOX word slams, hard cut to FILL UP A SPOT.
import sys, time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); D=2.88; O=8.92
def trk(): return C("add_track")["track_id"]
def media(path,t,st,d,**kw):
    a=C("import_media",path=path)["asset_id"]; return C("add_clip",track_id=t,source_type="media",asset_id=a,start=st,duration=d,fit="stretch",**kw)["clip_id"]
def L(a): return a-O
def key(c,p,pts,ease="linear",bez=None):
    for tm,v in pts: C("set_keyframe",clip_id=c,property=p,time=tm,value=v,ease=ease)
    if bez:
        for tm,v in pts[:-1]: C("set_keyframe_bezier",clip_id=c,property=p,time=tm,bezier=bez)
# --- photo with boxes (reference frame 9.24 = clean photo + boxes) until the cut to the dark FILL shot at 10.6
photo=media("/tmp/db/r_9.24.png",trk(),0,L(10.6))
slow=C("add_adjustment_layer",track_id=trk(),start=0,duration=L(10.6))["clip_id"]
# ink threshold flash 9.60-9.72
fl=C("add_adjustment_layer",track_id=trk(),start=L(9.68),duration=0.20)["clip_id"]; C("add_effect",clip_id=fl,kind="threshold",params={"level":0.5})
# --- words
def word(txt,cx,st,col,size=124):
    t=trk(); c=C("add_text",track_id=t,text=txt,start=L(st),duration=L(10.6)-L(st),size=size,color=col,font="ArchivoBlack-Regular")["clip_id"]
    C("set_property",clip_id=c,property="x",value=cx-640); C("set_property",clip_id=c,property="y",value=406-360)
    C("add_effect",clip_id=c,kind="drop_shadow",params={"distance":8,"blur":10,"opacity":0.7}); C("set_motion_blur",clip_id=c,shutter_angle=180)
    key(c,"scale",[(0,1.5),(0.2,1.0)],"linear",bez=[0.2,1.4,0.4,1]); key(c,"opacity",[(0,0),(0.05,1)])
    key(c,"x",[(0,cx-640+ (-260 if cx<400 else 160)),(0.22,cx-640)],"out_expo")
    return c
cream=[0.96,0.94,0.9]
word("DRAW",316,9.88,cream); word("ME",636,9.96,cream); word("A",813,10.04,cream); word("BOX",1023,10.12,[1,0.36,0.1])
# --- FILL UP A SPOT (10.6 - 11.8): hatch bg, yellow box growing, photo wipe, words
S=L(10.6); H=D-S
hat=media("/tmp/ex2/hatch.png",trk(),S,H)
FILLBOX=1
def _bx():
    return C("add_detection_box",track_id=trk(),start=S,duration=H,x=0.238,y=0.078,w=0.52,h=0.59,label="spot_1",confidence=0.99,color=[0.95,0.8,0.15],caption="her, deadpan, chewing",fill_opacity=0.12,instant=True)
from PIL import Image
cv=Image.new("RGBA",(1280,720),(0,0,0,0)); cv.paste(Image.open("/tmp/db/fillcrop.png").convert("RGBA"),(305,56)); cv.save("/tmp/db/fillfull.png")
fill=media("/tmp/db/fillfull.png",trk(),L(11.0),D-L(11.0))
top,bot=56/720,480/720
C("set_mask",clip_id=fill,shape="rect",cx=0.5,cy=top,w=1.0,h=0.0,feather=0.0)
C("set_mask_keyframe",clip_id=fill,property="h",time=0.0,value=0.0,ease="linear"); C("set_mask_keyframe",clip_id=fill,property="h",time=0.28,value=bot-top,ease="linear")
C("set_mask_keyframe",clip_id=fill,property="cy",time=0.0,value=top,ease="linear"); C("set_mask_keyframe",clip_id=fill,property="cy",time=0.28,value=(top+bot)/2,ease="linear")
bx=_bx()
def w2(txt,cx,st,col,pop=False):
    t=trk(); c=C("add_text",track_id=t,text=txt,start=L(st),duration=D-L(st),size=104,color=col,font="ArchivoBlack-Regular")["clip_id"]
    C("set_property",clip_id=c,property="x",value=cx-640); C("set_property",clip_id=c,property="y",value=598-360)
    C("add_effect",clip_id=c,kind="drop_shadow",params={"distance":6,"blur":8,"opacity":0.6})
    if pop: C("add_effect",clip_id=c,kind="outer_glow",params={"radius":16,"amount":1.0,"r":1,"g":1,"b":1}); key(c,"scale",[(0,2.6 if txt=="SPOT" else 1.7),(0.2,1.0)],"linear",bez=[0.2,1.3,0.4,1])
    return c
w2("FILL",335,10.6,cream); w2("UP",600,10.8,cream,True); w2("A",760,11.1,cream,True); w2("SPOT",1000,11.28,[1,0.36,0.1],True)
# --- beat-locked shake, per-hit kick tables measured on the reference (phase correlation on the static top band, 125 BPM grid)
for hits,prof in [([8.92],[[-10,-10],[-4,-6],[-1,-2]]),([9.16,10.88],[[-4,-4],[-1,-1]]),([9.88],[[-3,-25],[-8,-15],[-6,-6],[0,-4]]),([10.12],[[-3,-10],[-3,-6],[-1,-2]]),([11.32],[[-8,-11],[-6,-7],[-2,-2]])]:
    C("add_camera_shake",track_id=trk(),start=0,duration=D,hits=[h-O for h in hits],profile=prof)
a=time.time()
if len(sys.argv)>1 and sys.argv[1]=="export": print(C("export",path="/tmp/db.mp4",crf=17), "render s", time.time()-a)
else:
    for tm in [9.8,10.0,10.2,10.4,10.7,10.9,11.1,11.3,11.5,11.7]:
        C("render_frame",time=tm-O,path=f"/tmp/db_{tm}.png")
