import sys; sys.path.insert(0,'examples')
from mcp_client import Kimo
from motion_lib import MG
W,H=int(sys.argv[1]) if len(sys.argv)>1 else 640, int(sys.argv[2]) if len(sys.argv)>2 else 360
k=Kimo(); k.call("project_set",width=W,height=H,fps=24); m=MG(k,W,H); u=m.u; D=6.0; PX=W/1920.0
tr={n:k.call("add_track")["track_id"] for n in ["bg","geo","dust","rings","text","fx"]}
def ring_shape(track,start,dur,size,color,sw,shape="ellipse",sides=6):
    c=k.call("add_shape",track_id=track,shape=shape,start=start,duration=dur,width=size*u,height=size*u,fill=[-1,-1,-1],stroke_width=sw*u,stroke_color=list(color),sides=sides)["clip_id"]; return c
# animated flowing tritone background
bg=k.call("add_clip",track_id=tr["bg"],source_type="gradient",start=0,duration=D,color=[0.1,0.1,0.1],color2=[0.6,0.6,0.6])["clip_id"]
k.call("add_effect",clip_id=bg,kind="fractal_noise",params={"scale":700*u,"complexity":5,"speed":0.05,"contrast":1.35,"brightness":-0.28,"opacity":1,"mode":0})
k.call("add_effect",clip_id=bg,kind="gradient_map",params={"sr":0.008,"sg":0.012,"sb":0.035,"mr":0.09,"mg":0.15,"mb":0.34,"hr":0.78,"hg":0.56,"hb":0.34,"amount":1})
k.call("add_effect",clip_id=bg,kind="light_rays",params={"amount":0.35,"threshold":0.55,"cx":0.5,"cy":0.30,"length":0.5})
k.call("add_effect",clip_id=bg,kind="vignette",params={"amount":0.75,"softness":0.6}); k.call("add_effect",clip_id=bg,kind="grain",params={"amount":0.05})
m.key(bg,"scale",[(0,1.0,"linear"),(D,1.12,"linear")])
gold=(0.96,0.76,0.45)
# geometric parallax frames
for i,(size,rot0,rot1,op,sc0,sc1) in enumerate([(760,-6,10,0.28,0.95,1.08),(1180,8,-12,0.16,1.0,1.18)]):
    g=ring_shape(tr["geo"],0,D,size,gold,1.5,"polygon",6); m.key(g,"rotation",[(0,rot0,"linear"),(D,rot1,"linear")]); m.key(g,"scale",[(0,sc0,"out_quart"),(D,sc1,"linear")])
    m.key(g,"opacity",[(0,0,"linear"),(0.9+i*0.2,op,"linear"),(D-0.7,op,"linear"),(D,0,"linear")])
# shockwave rings at title hit
for i in range(4):
    r=ring_shape(tr["rings"],0,D,300,gold,2.5); t0=0.32+i*0.1
    m.key(r,"scale",[(t0,0.05,"out_expo"),(t0+2.2,7.5-i*0.9,"linear")]); m.key(r,"opacity",[(t0,0,"linear"),(t0+0.05,0.55-i*0.1,"out_quart"),(t0+1.8,0,"linear")])
# dust in two parallax layers
for j,(cnt,sz,op,seed) in enumerate([(70,1.2,0.45,3),(30,3.0,0.25,11)]):
    d=k.call("add_particles",track_id=tr["dust"],kind="dust",start=0,duration=D,count=cnt,color=[1,0.9,0.75],size=sz*PX*2,seed=seed)["clip_id"]; k.call("set_property",clip_id=d,property="opacity",value=op)
    if j==1: k.call("add_effect",clip_id=d,kind="blur",params={"radius":6*u})
# title
t=m.clip_text(tr["text"],"THE LAST HORIZON",0,D,"CormorantGaramond",156,color=(0.98,0.95,0.9),weight=500,upper=True,shadow_blur=26*u,shadow_opacity=0.6,shadow_angle=90,glow=10*u,glow_color=gold)
k.call("set_motion_blur",clip_id=t,shutter_angle=180)
a=m.animator(t,"char",0.0,0,1,"linear",tracking=80*u)
k.call("set_animator_keyframe",clip_id=t,animator_id=a,prop="tracking",time=0.3,value=80*u,ease="out_expo"); k.call("set_animator_keyframe",clip_id=t,animator_id=a,prop="tracking",time=3.8,value=10*u,ease="linear")
m.animator(t,"char",0.5,0.35,1.9,"out_quart",opacity=0,blur=20,y=40,scale=1.0)
m.key(t,"scale",[(0,1.0,"linear"),(D,1.05,"linear")]); m.fade(t,0.3,5.2,D,0.5,0.7)
s=m.clip_text(tr["text"],"A  FILM  BY  NAMAN  SHARMA",0,D,"Montserrat",30,color=(0.9,0.86,0.8),weight=500,line_mask=True)
m.animator(s,"line",0.3,2.0,2.8,"out_expo",y=60,opacity=0); a3=m.animator(s,"char",0.0,0,1,"linear",tracking=30*u)
k.call("set_animator_keyframe",clip_id=s,animator_id=a3,prop="tracking",time=2.0,value=30*u,ease="out_expo"); k.call("set_animator_keyframe",clip_id=s,animator_id=a3,prop="tracking",time=5.0,value=16*u,ease="linear")
k.call("set_property",clip_id=s,property="y",value=95*u); m.fade(s,2.0,5.2,D,0.4,0.7)
m.sweep_light(tr["fx"],1.4,2.6,D,angle=16,opacity=0.2)
k.call("export",path=f"/tmp/v3_{W}.mp4",crf=18)
