# Masking / selection / matte stack tour (6 s): bezier roto + star subtract, track matte, per-effect masks, noise dissolve + edge mask.
import sys, time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25)
PL="/tmp/db/r_9.24.png"; A=C("import_media",path=PL)["asset_id"]
def trk(): return C("add_track")["track_id"]
def med(st,d,t=None): return C("add_clip",track_id=t or trk(),source_type="media",asset_id=A,start=st,duration=d,fit="stretch")["clip_id"]
def label(txt,st,d):
    c=C("add_text",track_id=trk(),text=txt,start=st,duration=d,size=34,color=[1,1,1],font="ArchivoBlack-Regular")["clip_id"]
    C("set_property",clip_id=c,property="y",value=300); C("add_effect",clip_id=c,kind="drop_shadow",params={"distance":3,"blur":6,"opacity":0.9}); return c
def pts(cx,cy,r,n,wob,ph):
    import math; out=[]
    for i in range(n):
        a=i/n*math.tau; rr=r*(1+wob*math.sin(a*2+ph)); x=cx+rr*math.cos(a)/1.7778; y=cy+rr*math.sin(a)
        tx,ty=-math.sin(a),math.cos(a); k_=rr*0.55/n*3.2
        out.append({"x":x,"y":y,"ix":-tx*k_/1.7778,"iy":-ty*k_,"ox":tx*k_/1.7778,"oy":ty*k_})
    return out
# A 0-1.5: animated bezier roto, star subtract, feather
base=med(0,1.5); C("add_effect",clip_id=base,kind="saturation",params={"amount":0.0}); C("add_effect",clip_id=base,kind="exposure",params={"stops":-1.5})
top=med(0,1.5); m=C("add_mask",clip_id=top,kind="path",points=pts(0.5,0.5,0.34,6,0.25,0.0),feather=14,name="roto")["mask_id"]
C("set_mask_path_key",clip_id=top,mask_id=m,time=0.0,points=pts(0.5,0.5,0.34,6,0.25,0.0))
C("set_mask_path_key",clip_id=top,mask_id=m,time=0.75,points=pts(0.45,0.5,0.40,6,0.30,2.0))
C("set_mask_path_key",clip_id=top,mask_id=m,time=1.5,points=pts(0.55,0.5,0.34,6,0.20,4.0))
st=C("add_mask",clip_id=top,kind="path",ngon={"sides":5,"inner":0.45,"r":0.16,"cx":0.5,"cy":0.5,"rot":0},mode="subtract",feather=4,name="star hole")["mask_id"]
C("set_mask_prop_key",clip_id=top,mask_id=st,property="expand",time=0,value=-6); C("set_mask_prop_key",clip_id=top,mask_id=st,property="expand",time=1.5,value=14)
label("BEZIER ROTO + STAR SUBTRACT (animated)",0.0,1.5)
# B 1.5-3: track matte, text as matte of the plate over a blurred copy
bl=med(1.5,1.5); C("add_effect",clip_id=bl,kind="blur",params={"radius":18}); C("add_effect",clip_id=bl,kind="exposure",params={"stops":-1.2})
pl=med(1.5,1.5)
tx=C("add_text",track_id=trk(),text="MASK",start=1.5,duration=1.5,size=330,color=[1,1,1],font="ArchivoBlack-Regular")["clip_id"]
C("set_keyframe",clip_id=tx,property="scale",time=0,value=0.85); C("set_keyframe",clip_id=tx,property="scale",time=1.5,value=1.1)
C("set_track_matte",clip_id=pl,matte_clip_id=tx,mode="alpha",hide_source=True)
label("TRACK MATTE (text alpha, hidden source)",1.5,1.5)
# C 3-4.5: per-effect masks: pixelate inside a moving radial, invert inside a color-range selection
c3=med(3,1.5)
r=C("add_mask",clip_id=c3,kind="radial",params={"cx":0.25,"cy":0.5,"r":0.22,"soft":0.05},mode="none",name="lens")["mask_id"]
C("set_mask_prop_key",clip_id=c3,mask_id=r,property="cx",time=0,value=0.2); C("set_mask_prop_key",clip_id=c3,mask_id=r,property="cx",time=1.5,value=0.8)
e=C("add_effect",clip_id=c3,kind="pixelate",params={"size":22}); C("set_effect_mask",clip_id=c3,effect_id=e["effect_id"],mask_id=r)
h=C("add_mask",clip_id=c3,kind="hsl",params={"hue":355,"hue_width":50,"s_lo":0.25,"s_hi":1.0,"l_lo":0.25,"l_hi":0.9,"soft":0.15},mode="none",feather=3,name="reds")["mask_id"]
e2=C("add_effect",clip_id=c3,kind="color_isolate" if False else "invert",params={}); C("set_effect_mask",clip_id=c3,effect_id=e2["effect_id"],mask_id=h)
label("PER-EFFECT MASKS: moving radial + HSL qualifier",3,1.5)
# D 4.5-6: noise dissolve reveal + edge mask glow
g=med(4.5,1.5); C("add_effect",clip_id=g,kind="saturation",params={"amount":0.0}); C("add_effect",clip_id=g,kind="contrast",params={"amount":1.3})
c=med(4.5,1.5)
n=C("add_mask",clip_id=c,kind="noise",params={"scale":90,"thr":1.0,"soft":0.05,"seed":4,"speed":0.4},name="dissolve")["mask_id"]
C("set_mask_prop_key",clip_id=c,mask_id=n,property="thr",time=0,value=1.0); C("set_mask_prop_key",clip_id=c,mask_id=n,property="thr",time=1.5,value=0.0)
label("NOISE DISSOLVE (animated threshold)",4.5,1.5)
a=time.time(); print(C("export",path="/tmp/maskdemo.mp4",crf=17),"render s",round(time.time()-a,1))
for t,i in [(0.8,'a'),(2.4,'b'),(3.8,'c'),(5.2,'d')]: C("render_frame",time=t,path=f"/tmp/md_{i}.png")
C("render_mask",clip_id=top,mask_id=m,time=0.75,path="/tmp/md_mask.png",view="matte")
