# motion_extract / motion_diff: ground-truth test on a plate with known handheld-style motion, then match-move text and a stabilise.
import sys,time,math; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS,D=1280,720,25,4.0; N=int(D*FPS)
def gt(t): return (40*math.sin(t*2.1)+14*math.sin(t*7.3), 22*math.sin(t*1.7+1)+9*math.sin(t*9.1), 1.0+0.06*math.sin(t*1.3), 2.5*math.sin(t*1.9))
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
if sys.argv[1]=='plate':
    a=C("import_media",path="/tmp/flag.png")["asset_id"]; bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.5,0.6,0.7],start=0,duration=D)
    t=C("add_track")["track_id"]; c=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    C("set_property",clip_id=c,property="scale",value=1.3)
    for i in range(N):
        x,y,s,r=gt(i/FPS)
        for p,v in (("x",x),("y",y),("scale",1.3*s),("rotation",r)): C("set_keyframe",clip_id=c,property=p,time=i/FPS,value=v)
    t0=time.time(); C("export",path="/tmp/shakeplate.mp4",crf=16); print("plate s",round(time.time()-t0,1))
else:
    a=C("import_media",path="/tmp/shakeplate.mp4")["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D)["clip_id"]
    t2=C("add_track")["track_id"]; tx=C("add_text",track_id=t2,text="MATCHED",start=0,duration=D,size=170,color=[1,0.2,0.4])["clip_id"]; C("set_motion_blur",clip_id=tx,shutter_angle=180)
    r=C("motion_extract",clip_id=m,target_clip_id=tx,return_curves=True); print("extract s",round(r["seconds"],2),"frames",r["frames"],"range",r["range"])
    g0=gt(0); e=[[],[],[],[]]
    for i,c in enumerate(r["curves"]):
        g=gt(i/FPS); d=[c[0]-(g[0]-g0[0]),c[1]-(g[1]-g0[1]),(c[2]-g[2]/g0[2])*100,c[3]-(g[3]-g0[3])]
        for j in range(4): e[j].append(abs(d[j]))
    print("mean abs err: dx %.2f px, dy %.2f px, scale %.2f %%, rot %.3f deg"%tuple(sum(x)/len(x) for x in e))
    print("max abs err: dx %.2f px, dy %.2f px, scale %.2f %%, rot %.3f deg"%tuple(max(x) for x in e))
    print("diff vs itself:",C("motion_diff",clip_a=m,clip_b=m)["channels"]["dx_px"])
    t0=time.time(); C("export",path=sys.argv[2],crf=17); print("comp s",round(time.time()-t0,1))
