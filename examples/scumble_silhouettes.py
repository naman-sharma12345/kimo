# SCUMBLE 11.8-12.52 s: three 6-frame segmentation-style shots. Plate = reference frame (confetti partly inpainted by tools/sil_prep.py),
# silhouette = alpha mask keyed from the reference, filled with the new silhouette_fill effect, plus add_glitch_confetti.
import sys, time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25); O=11.8; D=0.72
def trk(): return C("add_track")["track_id"]
def media(path,st,d):
    a=C("import_media",path=path)["asset_id"]; return C("add_clip",track_id=trk(),source_type="media",asset_id=a,start=st,duration=d,fit="stretch")["clip_id"]
shots=[("11.92",0.0,0.24,(75,211,133),0),("12.16",0.24,0.24,(186,236,70),1),("12.44",0.48,0.24,(245,204,20),1)]
for t,st,d,col,conf in shots:
    media(f"/tmp/sil/plate_{t}.png",st,d)
    m=media(f"/tmp/sil/mask_{t}.png",st,d); C("add_effect",clip_id=m,kind="silhouette_fill",params={"r":col[0]/255,"g":col[1]/255,"b":col[2]/255,"amount":0.9})
    if conf: C("add_glitch_confetti",track_id=trk(),start=st,duration=0.08,squares=26,size=[14,30],flicker_fps=25,seed=int(st*100)+3,lines=7)
a=time.time(); print(C("export",path="/tmp/sil.mp4",crf=17),"render s",time.time()-a)
