# apply_motion_preset demo: words animated in by plain-language prompts, one per beat.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS,D=1280,720,25,6.0
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="solid",color=[0.06,0.07,0.1],start=0,duration=D)
rows=[("BOUNCE","bounce in from above, then shake",-340,-200,0.0),("ELASTIC","elastic spring",340,-200,0.5),("WHIP","whip in fast",-340,0,1.0),("SPIN","spin in",340,0,1.5),("SLAM","slam punch",-340,200,2.0),("HEART","heartbeat",340,200,2.5)]
out=[]
for txt,prompt,x,y,t0 in rows:
    t=C("add_track")["track_id"]; c=C("add_text",track_id=t,text=txt,start=t0,duration=D-t0,size=150,color=[1,0.85,0.2])["clip_id"]
    C("set_property",clip_id=c,property="x",value=x); C("set_property",clip_id=c,property="y",value=y); C("set_motion_blur",clip_id=c,shutter_angle=180)
    kw={"until_end":True} if txt=="HEART" else {}
    r=C("apply_motion_preset",clip_id=c,prompt=prompt,at=0.0,duration=0.8,**kw); out.append((txt,prompt,[a["preset"] for a in r["applied"]]))
for o in out: print(o)
t0=time.time(); C("export",path=sys.argv[1],crf=17); print("render s",round(time.time()-t0,1))
