# Masking quality before/after. usage: masking_quality.py legacy|new
import sys, time, math; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
mode=sys.argv[1]; leg=1.0 if mode=="legacy" else 0.0
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25)
def trk(): return C("add_track")["track_id"]
def img(p,st,d,t=None):
    a=C("import_media",path=p)["asset_id"]; return C("add_clip",track_id=t or trk(),source_type="media",asset_id=a,start=st,duration=d,fit="stretch")["clip_id"]
# part 1: sub-pixel drifting blob with variable feather (0-1.5 s)
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.12,0.12,0.15],start=0,duration=1.5)
def blob(cx,cy,r,ph,F):
    out=[]
    for i in range(7):
        a=i/7*math.tau; rr=r*(1+0.18*math.sin(a*2+ph)); x=cx+rr*math.cos(a)/1.7778; y=cy+rr*math.sin(a); tx,ty=-math.sin(a),math.cos(a); kk=rr*0.55/7*3.2
        out.append({"x":x,"y":y,"ix":-tx*kk/1.7778,"iy":-ty*kk,"ox":tx*kk/1.7778,"oy":ty*kk,"f":F[i] if leg==0 else 0})
    return out
wh=C("add_track")["track_id"]; w1=C("add_clip",track_id=wh,source_type="solid",color=[0.95,0.9,0.8],start=0,duration=1.5)["clip_id"]
F=[2,2,2,36,36,36,2]
m=C("add_mask",clip_id=w1,kind="path",points=blob(0.5,0.5,0.3,0,F),feather=(14 if leg else 0),params={"legacy":leg,"shutter":0 if leg else 180},interp="smooth" if leg else "spline")["mask_id"]
for t,(cx,ph) in [(0,(0.48,0)),(0.5,(0.5,1.2)),(1.0,(0.52,2.0)),(1.5,(0.5,3.0))]: C("set_mask_path_key",clip_id=w1,mask_id=m,time=t,points=blob(cx,0.5,0.3,ph,F))
# part 2: magenta-screen key over ice (1.5-3.0)
ice=img("/tmp/mq/ice.png",1.5,1.5); scr=img("/tmp/mq/screen.png",1.5,1.5)
p={"r":199/255,"g":60/255,"b":167/255,"tol":float(sys.argv[2]),"soft":float(sys.argv[3]),"legacy":leg}
import os
if not leg: p.update({"refine":float(os.environ.get("RF",3)),"decontam":float(os.environ.get("DC",5)),"despill":float(os.environ.get("DS",0.9))})
km=C("add_mask",clip_id=scr,kind="color",params=p,invert=True,mode="add")["mask_id"]
a=time.time(); print(C("export",path=f"/tmp/mq_{mode}.mp4",crf=16),"render s",round(time.time()-a,1))
C("render_frame",time=0.75,path=f"/tmp/mq_{mode}_p1.png"); C("render_frame",time=2.2,path=f"/tmp/mq_{mode}_p2.png")
