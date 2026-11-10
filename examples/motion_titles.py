import sys; sys.path.insert(0,'examples')
from mcp_client import Kimo
from motion_lib import MG
W,H=int(sys.argv[1]) if len(sys.argv)>1 else 640, int(sys.argv[2]) if len(sys.argv)>2 else 360
k=Kimo(); k.call("project_set",width=W,height=H,fps=24); m=MG(k,W,H); u=m.u; PX=W/1920.0
bgs=[k.call("import_media",path=f"/tmp/bg{x}.png")["asset_id"] for x in "ABC"]
tr={n:k.call("add_track")["track_id"] for n in ["bg","lights","dust","shapes","text","fx"]}
def bg(asset,off,dur):
    c=k.call("add_clip",track_id=tr["bg"],source_type="media",asset_id=asset,start=off,duration=dur)["clip_id"]; m.drift(c,dur,1.0,1.09,"linear"); return c
def lights(off,dur,cols):
    for i,(col,x0,x1,y,op) in enumerate(cols):
        c=m.rect(tr["lights"],off,dur,1000,760,col,x=x0,y=y,blend="add",shape="ellipse"); k.call("add_effect",clip_id=c,kind="blur",params={"radius":200*u})
        m.key(c,"x",[(0,x0*u,"linear"),(dur,x1*u,"linear")]); m.key(c,"opacity",[(0,0,"linear"),(1.0,op,"linear"),(dur-0.8,op,"linear"),(dur,0,"linear")])
def dust(off,dur,count=60):
    c=k.call("add_particles",track_id=tr["dust"],kind="dust",start=off,duration=dur,count=count,color=[1,0.92,0.8],size=1.6*PX*2,seed=7)["clip_id"]; k.call("set_property",clip_id=c,property="opacity",value=0.5); return c

# ---------- 1. CINEMATIC (0..5) ----------
o=0; D=5.0; m.off=o
bg(bgs[0],o,D); lights(o,D,[((0.35,0.45,0.9),-500,-250,-120,0.16),((1,0.6,0.35),500,300,160,0.09)]); dust(o,D)
r1=m.rect(tr["shapes"],o,D,380,2,(0.95,0.75,0.4),y=-150); m.grow_x(r1,380,0,0.25,1.2); m.fade(r1,0.1,4.4,D,0.1,0.5)
r2=m.rect(tr["shapes"],o,D,380,2,(0.95,0.75,0.4),y=170); m.grow_x(r2,380,0,0.4,1.35); m.fade(r2,0.1,4.4,D,0.1,0.5)
t=m.clip_text(tr["text"],"THE LAST HORIZON",o,D,"CormorantGaramond",150,color=(0.97,0.95,0.9),weight=500,upper=True,shadow_blur=22*u,shadow_opacity=0.5,shadow_angle=90)
k.call("set_motion_blur",clip_id=t,shutter_angle=180)
a=m.animator(t,"char",0.0,0,1,"linear",tracking=70*u)
k.call("set_animator_keyframe",clip_id=t,animator_id=a,prop="tracking",time=0.2,value=70*u,ease="out_expo"); k.call("set_animator_keyframe",clip_id=t,animator_id=a,prop="tracking",time=3.4,value=9*u,ease="linear")
m.animator(t,"char",0.55,0.3,1.7,"out_quart",opacity=0,blur=18,y=34,scale=1.0)
m.key(t,"scale",[(0,1.0,"linear"),(D,1.045,"linear")]); m.fade(t,0.2,4.45,D,0.5,0.55)
s=m.clip_text(tr["text"],"A  FILM  BY  NAMAN  SHARMA",o,D,"Montserrat",32,color=(0.82,0.8,0.78),weight=500,upper=False,line_mask=True)
a2=m.animator(s,"line",0.3,1.7,2.5,"out_expo",y=60,opacity=0)
a3=m.animator(s,"char",0.0,0,1,"linear",tracking=26*u); k.call("set_animator_keyframe",clip_id=s,animator_id=a3,prop="tracking",time=1.7,value=26*u,ease="out_expo"); k.call("set_animator_keyframe",clip_id=s,animator_id=a3,prop="tracking",time=4.0,value=14*u,ease="linear")
k.call("set_property",clip_id=s,property="y",value=78*u); m.fade(s,1.7,4.45,D,0.4,0.55)
m.sweep_light(tr["fx"],1.2,2.3,D,angle=16,opacity=0.22)

# ---------- 2. KINETIC (5..9) ----------
o=5.0; D=4.0; m.off=o
bg(bgs[1],o,D); lights(o,D,[((1,0.35,0.2),-500,-200,-250,0.12)])
words=[("MAKE",(0.07,0.07,0.08),(1,1,1),-270),("IT",(0.97,0.97,0.95),(0.07,0.07,0.08),0),("MOVE",(0.98,0.32,0.18),(1,1,1),270)]
for i,(w,bc,tc,y) in enumerate(words):
    t0=0.25+i*0.28
    b=m.rect(tr["shapes"],o,D,2300,240,bc,y=y,radius=0); m.key(b,"scale",[(t0-0.05,0.0001,"out_expo"),(t0+0.5,1,"linear")]); m.key(b,"x",[(t0-0.05,-1150*u,"out_expo"),(t0+0.5,0,"linear")])
    m.key(b,"x",[(3.3+i*0.07,0,"in_expo"),(3.75+i*0.07,2600*u,"linear")]); k.call("set_motion_blur",clip_id=b,shutter_angle=220)
    c=m.clip_text(tr["text"],w,o,D,"Anton",330,color=tc,upper=True); k.call("set_property",clip_id=c,property="y",value=y*u)
    k.call("set_motion_blur",clip_id=c,shutter_angle=220)
    m.key(c,"opacity",[(0,0,"linear"),(t0+0.12,0,"linear"),(t0+0.2,1,"linear"),(3.3+i*0.07,1,"linear"),(3.4+i*0.07,0,"linear")])
    m.key(c,"scale",[(t0+0.1,1.7,"out_expo"),(t0+0.75,1.0,"linear")]); m.key(c,"x",[(t0+0.1,-420*u,"out_expo"),(t0+0.75,0,"linear")]); m.key(c,"rotation",[(t0+0.1,-5,"out_expo"),(t0+0.75,0,"linear")])
m.sweep_light(tr["fx"],1.6,2.6,D,angle=-14,opacity=0.18)

# ---------- 3. LOWER THIRD (9..13.5) ----------
o=9.0; D=4.5; m.off=o
bg(bgs[2],o,D); lights(o,D,[((0.9,0.3,0.4),-300,-100,0,0.10)])
L=-840; plate=m.rect(tr["shapes"],o,D,980,190,(0.05,0.05,0.07),x=L+490,y=330,radius=4); k.call("set_property",clip_id=plate,property="opacity",value=0.0)
m.key(plate,"x",[(0.1,(L+490-1100)*u,"out_expo"),(0.8,(L+490)*u,"linear"),(3.6,(L+490)*u,"in_expo"),(4.1,(L+490-1100)*u,"linear")]); m.key(plate,"opacity",[(0.1,0,"linear"),(0.2,0.78,"linear"),(3.9,0.78,"linear"),(4.1,0,"linear")])
bar=m.rect(tr["shapes"],o,D,10,190,(0.98,0.32,0.18),x=L,y=330); m.key(bar,"scale",[(0.0,0.0001,"out_expo"),(0.55,1,"linear"),(3.6,1,"in_expo"),(4.0,0.0001,"linear")])
line=m.rect(tr["shapes"],o,D,800,2,(1,1,1),x=L+40+400,y=342); m.grow_x(line,800,L+440,0.65,1.55,"out_expo","left"); m.key(line,"opacity",[(0,0,"linear"),(0.65,0.45,"linear"),(3.6,0.45,"linear"),(4.0,0,"linear")])
n=m.clip_text(tr["text"],"Naman Sharma",o,D,"Montserrat",80,align="left",weight=700,box_width=900*u,line_mask=True)
m.animator(n,"line",0.0,0.45,1.15,"out_expo",y=100,opacity=0); k.call("set_property",clip_id=n,property="x",value=(-800+450)*u); k.call("set_property",clip_id=n,property="y",value=290*u)
k.call("set_motion_blur",clip_id=n,shutter_angle=180); m.key(n,"opacity",[(3.6,1,"in"),(3.95,0,"linear")])
r=m.clip_text(tr["text"],"FOUNDER  /  DIRECTOR",o,D,"Montserrat",32,color=(0.85,0.85,0.85),align="left",weight=500,box_width=900*u,line_mask=True)
m.animator(r,"line",0.0,0.7,1.4,"out_expo",y=70,opacity=0); k.call("set_property",clip_id=r,property="x",value=(-800+450)*u); k.call("set_property",clip_id=r,property="y",value=385*u)
a4=m.animator(r,"char",0.0,0,1,"linear",tracking=5*u)
m.key(r,"opacity",[(3.6,1,"in"),(3.95,0,"linear")])
m.sweep_light(tr["fx"],0.9,1.9,D,angle=14,opacity=0.14,width=100)

k.call("export",path=f"/tmp/mg_{W}.mp4",crf=18)
import os; print(os.path.getsize(f"/tmp/mg_{W}.mp4"))
