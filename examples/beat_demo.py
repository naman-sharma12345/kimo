# Beat-driven motion: detect_beats on the reference audio (known 125 BPM), then bind_beats drives scale/rotation/opacity pulses.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); D=6.0; T0=float(sys.argv[2]) if len(sys.argv)>2 else 4.0
a=C("import_media",path="/downloads/video-c61877f1.mp4")["asset_id"]
r=C("track_beats",asset_id=a,t0=T0,duration=D,min_bpm=90,max_bpm=160); print("bpm",round(r["bpm"],1),"beats",r["count"],"first",[round(b-T0,2) for b in r["beats"][:6]],"conf",round(r["confidence"],1))
beats=[b-T0 for b in r["beats"]]
bg=C("add_track")["track_id"]; C("add_clip",track_id=bg,source_type="gradient",c1=[0.05,0.02,0.12],c2=[0.02,0.1,0.2],start=0,duration=D) if False else C("add_clip",track_id=bg,source_type="solid",color=[0.04,0.03,0.09],start=0,duration=D)
au=C("add_track")["track_id"]; C("add_audio_clip",asset_id=a,track_id=au,start=0,duration=D,in_point=T0)
def shp(sh,col,x,y,w,h):
    t=C("add_track")["track_id"]; c=C("add_shape",track_id=t,shape=sh,start=0,duration=D,width=w,height=h,fill=col)["clip_id"]; C("set_keyframe",clip_id=c,property="x",time=0,value=x); C("set_keyframe",clip_id=c,property="y",time=0,value=y); return c
c1=shp("ellipse",[1,0.3,0.5],-380,0,220,220); C("bind_beats",clip_id=c1,property="scale",beats=beats,amount=0.45,decay=0.2,curve="kick")
c2=shp("rect",[0.3,0.8,1],0,0,200,200); C("bind_beats",clip_id=c2,property="rotation",beats=beats,amount=35,decay=0.3,curve="swing"); C("bind_beats",clip_id=c2,property="scale",beats=beats,amount=0.25,decay=0.18,curve="pulse")
c3=shp("polygon",[1,0.8,0.2],380,0,220,220); C("bind_beats",clip_id=c3,property="y",beats=beats,amount=-90,decay=0.3,curve="kick"); C("bind_beats",clip_id=c3,property="rotation",beats=beats,amount=60,decay=0.4,curve="pulse",every=2)
t=C("add_track")["track_id"]; tx=C("add_text",track_id=t,text="KIMO",start=0,duration=D,size=140,color=[1,1,1])["clip_id"]; C("set_keyframe",clip_id=tx,property="y",time=0,value=-240); C("bind_beats",clip_id=tx,property="scale",beats=beats,amount=0.18,decay=0.15,curve="flash")
for c in (c1,c2,c3): C("set_motion_blur",clip_id=c,shutter_angle=180)
t0=time.time(); C("export",path=sys.argv[1],crf=17); print("render s",round(time.time()-t0,1))
