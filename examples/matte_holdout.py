# Hold-out test on real footage: track from seeds 0,2,4 (of 5 reviewed contours); score the matte against held-out seeds 1 and 3. usage: matte_holdout.py clip seeds.json [out_prefix]
import sys,json,time; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
import numpy as np
from PIL import Image,ImageDraw
SRC,SF=sys.argv[1],sys.argv[2]; PRE=sys.argv[3] if len(sys.argv)>3 else "/tmp/ho"; S=json.load(open(SF)); W,H,FPS=960,540,30
k=Kimo(); C=k.call; C("project_set",width=W,height=H,fps=FPS)
a=C("import_media",path=SRC)["asset_id"]; t=C("add_track")["track_id"]; m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=1.6,fit="cover")["clip_id"]
use=[S[0],S[2],S[4]]; r=C("track_matte",clip_id=m,seeds=[{"time":s["time"],"points":s["points"]} for s in use],temporal=0.6,band=float(__import__("os").environ.get("BAND","14")),colour=float(__import__("os").environ.get("COL","0.9")))
print({q:r[q] for q in ("seconds","flicker","area_jitter")})
def poly(p):
    im=Image.new("L",(W*4,H*4),0); ImageDraw.Draw(im).polygon([(x*4,y*4) for x,y in p],fill=255); return np.array(im.resize((W,H),Image.BOX)).astype(float)/255
for j in (1,3):
    C("render_mask",clip_id=m,mask_id=r["mask_id"],time=S[j]["time"],view="matte",path="/tmp/ho_%d.png"%j); A=np.array(Image.open("/tmp/ho_%d.png"%j).convert("L")).astype(float)/255; G=poly(S[j]["points"])
    print("held-out seed @%.2fs IoU %.4f"%(S[j]["time"],np.minimum(A,G).sum()/np.maximum(A,G).sum()))
    C("render_mask",clip_id=m,mask_id=r["mask_id"],time=S[j]["time"],view="overlay",path="%s_%d.png"%(PRE,j))
