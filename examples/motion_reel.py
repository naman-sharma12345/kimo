# Motion reel: expressions (wiggle / sin orbits / loops), bezier overshoot, echo light trails, motion blur, glow.
import sys, math; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS=1280,720,25
k=Kimo(); k.call("project_set",width=W,height=H,fps=FPS); C=k.call
def trk(): return C("add_track")["track_id"]
def shape(t,s,st,d,w,h,col,x=0,y=0,**kw):
    c=C("add_shape",track_id=t,shape=s,start=st,duration=d,width=w,height=h,fill=col,**kw)["clip_id"]
    C("set_property",clip_id=c,property="x",value=x); C("set_property",clip_id=c,property="y",value=y); return c
def key(c,p,pts,ease="linear",bez=None):
    for tm,v in pts: C("set_keyframe",clip_id=c,property=p,time=tm,value=v,ease=ease)
    if bez:
        for tm,v in pts[:-1]: C("set_keyframe_bezier",clip_id=c,property=p,time=tm,bezier=bez)
def fade(c,d,a=0.5,b=0.5):
    key(c,"opacity",[(0,0),(a,1),(d-b,1),(d,0)])
def sc(st,D,bgcols):
    t=trk(); b=C("add_clip",track_id=t,source_type="gradient",color=bgcols[0],color2=bgcols[1],start=st,duration=D)["clip_id"]; return b
# ---------------- SCENE 1 (0-4.6): orbit + title ----------------
D=4.6
sc(0,D,([0.07,0.04,0.16],[0.01,0.01,0.03]))
t=trk()
for (col,x,y,seed,r) in [([0.9,0.15,0.55],-270,-90,3,200),([0.1,0.7,1.0],260,90,8,210),([1.0,0.6,0.2],0,180,5,170)]:
    o=shape(t,"ellipse",0,D,r*2,r*2,col,x,y,); C("add_effect",clip_id=o,kind="blur",params={"radius":70}); C("set_blend",clip_id=o,mode="add")
    C("set_property",clip_id=o,property="opacity",value=0.33)
    for p,s in (("x",seed),("y",seed+11)): C("set_expression",clip_id=o,property=p,op="wiggle",freq=0.4,amp=150,octaves=2,seed=s)
# thin guide ring
ring=shape(trk(),"ellipse",0,D,520,520,[0,0,0],stroke_color=[1,1,1],stroke_width=1.5)
C("set_property",clip_id=ring,property="opacity",value=0.0); key(ring,"opacity",[(0.2,0),(1.0,0.28),(D-0.6,0.28),(D,0)]); key(ring,"scale",[(0.2,0.6),(1.4,1.0)],"out_expo")
# orbiting dots: circular motion = sin on x and y with quarter-cycle phase
for i,(col,R,f,ph) in enumerate([([1,1,1],260,0.30,0.0),([0.4,0.85,1],260,0.30,0.5),([1,0.45,0.8],200,-0.45,0.25)]):
    d=shape(trk(),"ellipse",0,D,22,22,col)
    C("set_expression",clip_id=d,property="x",op="sin",freq=abs(f),amp=R*1.0,phase=ph+0.25)
    C("set_expression",clip_id=d,property="y",op="sin",freq=abs(f),amp=R*(1 if f>0 else -1)*0.62,phase=ph)
    C("set_echo",clip_id=d,count=18,spacing=0.022,decay=0.86,mode="add"); C("set_motion_blur",clip_id=d,shutter_angle=180)
    C("add_effect",clip_id=d,kind="outer_glow",params={"radius":18,"intensity":1.0}) ; key(d,"scale",[(0.2,0),(0.9,1)],"out_back"); fade(d,D,0.5,0.4)
# KIMO title
tt=trk(); ti=C("add_text",track_id=tt,text="KIMO",start=0,duration=D,size=230,color=[1,1,1],letter_spacing=26)["clip_id"]
C("add_effect",clip_id=ti,kind="outer_glow",params={"radius":26,"intensity":0.7}); C("set_motion_blur",clip_id=ti,shutter_angle=200)
key(ti,"scale",[(0.35,0.35),(1.15,1.0)],"linear",bez=[0.34,1.75,0.64,1]); key(ti,"opacity",[(0.35,0),(0.6,1),(D-0.5,1),(D,0)])
C("set_expression",clip_id=ti,property="rotation",op="wiggle",freq=0.8,amp=1.1,octaves=2,seed=2)
C("set_expression",clip_id=ti,property="y",op="wiggle",freq=0.9,amp=5,octaves=1,seed=4); C("set_property",clip_id=ti,property="y",value=-14)
sub=C("add_text",track_id=tt,text="MOTION, ENGINEERED",start=0,duration=D,size=30,color=[0.85,0.88,1],letter_spacing=14)["clip_id"]
C("set_property",clip_id=sub,property="y",value=130); key(sub,"opacity",[(1.2,0),(1.9,0.9),(D-0.5,0.9),(D,0)]); key(sub,"y",[(1.2,150),(2.0,130)],"out_expo")
# ---------------- SCENE 2 (4.6-9.4): elastic drop + wave ----------------
s0=4.6; D2=4.8
sc(s0,D2,([0.02,0.07,0.13],[0.0,0.0,0.02]))
t2=trk()
n=11
for i in range(n):
    u=i/(n-1); col=[0.25+0.75*u,0.55-0.35*u+0.2*math.sin(u*3),1.0-0.8*u]
    x=-(n-1)/2*92+i*92; st=s0+0.15+i*0.06
    b=shape(t2,"ellipse",st,D2-0.15-i*0.06,62,62,col,x,0)
    C("add_effect",clip_id=b,kind="outer_glow",params={"radius":14,"intensity":0.6}); C("set_motion_blur",clip_id=b,shutter_angle=180)
    key(b,"y",[(0,-460),(0.7,60)],"linear",bez=[0.3,1.5,0.5,1.0])
    key(b,"y",[(0.7,60),(1.4,0)],"in_out")
    C("set_expression",clip_id=b,property="y",op="wiggle",freq=0,amp=0) if False else None
    C("set_expression",clip_id=b,property="y",op="sin",freq=0.55,amp=70,phase=-i*0.07)
    C("set_echo",clip_id=b,count=6,spacing=0.03,decay=0.7,mode="composite"); fade(b,D2-0.15-i*0.06,0.1,0.4)
w=trk(); tx=C("add_text",track_id=w,text="EVERY PARAMETER. ONE CALL.",start=s0,duration=D2,size=44,color=[1,1,1],letter_spacing=10)["clip_id"]
C("set_property",clip_id=tx,property="y",value=215); C("add_text_animator",clip_id=tx,preset="tracking_in",delay=1.0,duration=1.1) ; key(tx,"opacity",[(0.9,0),(1.1,1),(D2-0.5,1),(D2,0)])
# stop-motion accent word, posterized
sm=C("add_text",track_id=w,text="STOP MOTION",start=s0,duration=D2,size=26,color=[1,0.7,0.35],letter_spacing=18)["clip_id"]
C("set_property",clip_id=sm,property="y",value=-250); C("set_posterize_time",clip_id=sm,fps=6); C("set_expression",clip_id=sm,property="x",op="wiggle",freq=3,amp=18,octaves=1,seed=1); key(sm,"opacity",[(0.6,0),(0.8,1),(D2-0.5,1),(D2,0)])
# global finish
adj=trk(); a=C("add_adjustment_layer",track_id=adj,start=0,duration=s0+D2)["clip_id"]
C("add_effect",clip_id=a,kind="glow",params={"radius":14,"intensity":0.45,"threshold":0.6}); C("add_effect",clip_id=a,kind="vignette",params={"amount":0.5}); C("add_effect",clip_id=a,kind="grain",params={"amount":0.04})
if len(sys.argv)>1: print(C("export",path="/tmp/reel.mp4",crf=16))
else:
    import time
    for tm in (1.8,6.2):
        a=time.time(); C("render_frame",time=tm,path=f"/tmp/r{tm}.png"); print(tm,time.time()-a)
