# track_matte proof on real footage: hold-out accuracy against a seed that was NOT given to the tracker, flicker before/after.
import sys,json,time,subprocess; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
import numpy as np
from PIL import Image
SRC=sys.argv[1]; SEEDS=json.load(open(sys.argv[2])); W,H,FPS=960,540,30
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
a=C("import_media",path=SRC)["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=1.6,fit="cover")["clip_id"]
mode=sys.argv[3] if len(sys.argv)>3 else "holdout"
use=[SEEDS[0],SEEDS[-1]] if mode=="holdout" else SEEDS
seeds=[{"time":s[0],"points":s[1]} for s in use]
r=C("track_matte",clip_id=m,seeds=seeds,temporal=float(sys.argv[4]) if len(sys.argv)>4 else 0.6)
print({k_:r[k_] for k_ in ("seconds","flicker","area_jitter","warnings")}); print("conf min",min(r["confidence"]),"mean",sum(r["confidence"])/len(r["confidence"]))
mid=SEEDS[1]; 
def poly(pts):
    from PIL import ImageDraw
    im=Image.new("L",(W*4,H*4),0); ImageDraw.Draw(im).polygon([(x*4,y*4) for x,y in pts],fill=255); return np.array(im.resize((W,H),Image.BOX)).astype(float)/255
mid_id=C("render_mask",clip_id=m,mask_id=r["mask_id"],time=mid[0],view="matte",path="/tmp/mm_mid.png")
A=np.array(Image.open("/tmp/mm_mid.png").convert("L")).astype(float)/255; G=poly(mid[1])
inter=np.minimum(A,G).sum(); uni=np.maximum(A,G).sum(); print("hold-out IoU vs middle seed @%.2fs: %.4f"%(mid[0],inter/uni))
# per-frame sheet
import os
tiles=[]
for tm in (0.0,0.4,0.8,1.2,1.5):
    C("render_mask",clip_id=m,mask_id=r["mask_id"],time=tm,view="overlay",path="/tmp/mt.png"); tiles.append(Image.open("/tmp/mt.png").convert("RGB").resize((384,216)))
sheet=Image.new("RGB",(384*3,216*2)); [sheet.paste(im,((i%3)*384,(i//3)*216)) for i,im in enumerate(tiles)]; sheet.save("/tmp/matte_sheet_%s.png"%mode)
