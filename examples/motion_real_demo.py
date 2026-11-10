# Real handheld footage (CC0 Wikimedia clip): motion_extract measures camera motion; match-move text and a stabilise, then re-measure the stabilised result.
import sys,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
W,H,FPS=540,720,25
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS); ph=sys.argv[1]
def rng(r): return {a:[round(x,2) for x in b] for a,b in r["range"].items()}
if ph=='stab':
    a=C("import_media",path="/tmp/petuel.mp4")["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=3.7)["clip_id"]
    C("set_property",clip_id=m,property="scale",value=1.12)
    r=C("motion_extract",clip_id=m,target_clip_id=m,invert=True,return_curves=True); print("before: track s",round(r["seconds"],2),"range",rng(r))
    import statistics as st; cu=r["curves"]; print("original camera motion std: dx %.2f dy %.2f px rot %.3f deg"%(st.pstdev([c[0] for c in cu]),st.pstdev([c[1] for c in cu]),st.pstdev([c[3] for c in cu])))
    t0=time.time(); C("export",path="/tmp/petuel_stab.mp4",crf=16); print("render s",round(time.time()-t0,1))
elif ph=='measure':
    import statistics as st
    a=C("import_media",path="/tmp/petuel_stab.mp4")["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=3.7)["clip_id"]
    r=C("motion_extract",clip_id=m,return_curves=True); cu=r["curves"]; print("stabilised: range",rng(r)); print("stabilised residual std: dx %.2f dy %.2f px rot %.3f deg"%(st.pstdev([c[0] for c in cu]),st.pstdev([c[1] for c in cu]),st.pstdev([c[3] for c in cu])))
else:
    a=C("import_media",path="/tmp/petuel.mp4")["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=3.7)["clip_id"]
    t2=C("add_track")["track_id"]; tx=C("add_text",track_id=t2,text="ROCK",start=0,duration=3.7,size=120,color=[1,0.2,0.4])["clip_id"]
    C("set_property",clip_id=tx,property="x",value=60); C("set_property",clip_id=tx,property="y",value=220); C("set_motion_blur",clip_id=tx,shutter_angle=180)
    r=C("motion_extract",clip_id=m,target_clip_id=tx); print("track s",round(r["seconds"],2))
    t0=time.time(); C("export",path=sys.argv[2],crf=17); print("render s",round(time.time()-t0,1))
