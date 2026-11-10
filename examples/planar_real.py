# Real-footage planar track: overpass face in the reference (camera shake + zoom), translucent quad pinned to the tracked surface.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); a=C("import_media",path="/downloads/video-c61877f1.mp4")["asset_id"]; t=C("add_track")["track_id"]
D=float(sys.argv[2]); m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=D,in_point=float(sys.argv[1]))["clip_id"]
t2=C("add_track")["track_id"]; q=C("add_clip",track_id=t2,source_type="solid",color=[1,0.1,0.6],start=0,duration=D)["clip_id"]; C("set_opacity",clip_id=q,value=0.45) if False else None
C("set_keyframe",clip_id=q,property="opacity",time=0,value=0.45)
r=C("track_planar",clip_id=m,corners=[0.02,0.46,0.33,0.46,0.33,0.58,0.02,0.58],target_clip_id=q,return_quads=True)
print("track s",round(r["seconds"],2),"frames",r["frames"],"lost",r["lost_frames"],"mean conf",round(r["mean_conf"],3),"min",round(r["min_conf"],3))
for i in range(0,r["frames"],6): print(i,[round(v) for v in r["quads"][i]],round(r["conf"][i],2))
t0=time.time(); C("export",path=sys.argv[3],crf=17); print("render s",round(time.time()-t0,1))
