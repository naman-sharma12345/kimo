# track_matte ground-truth proof: synthetic textured subject with analytic alpha, tracked from the first and last frame only.
import os,sys,math,subprocess,json; sys.path.insert(0,'/home/sandbox/kimo/examples')
import numpy as np; from PIL import Image,ImageDraw,ImageFilter
from mcp_client import Kimo
W,H,FPS,N=640,360,30,60; rng=np.random.default_rng(3)
base=np.array([[0,-60],[18,-50],[22,-20],[60,-45],[66,-38],[30,-5],[26,40],[34,75],[18,78],[8,45],[-8,45],[-14,78],[-30,76],[-24,36],[-28,-5],[-62,-30],[-68,-38],[-24,-20],[-18,-50]],float)
def shape(i):
    t=i/(N-1); a=math.radians(25*math.sin(t*math.pi*1.5)); s=1.0+0.25*t; c,sn=math.cos(a),math.sin(a); cx=130+400*t; cy=180+40*math.sin(t*6)
    P=base*s; P=np.stack([P[:,0]*c-P[:,1]*sn,P[:,0]*sn+P[:,1]*c],1); J=float(os.environ.get('JIT','0')); P[:,0]+=J*np.sin(np.arange(len(P))*1.7+t*9); P[:,1]+=J*np.cos(np.arange(len(P))*2.3+t*7); return P+[cx,cy]
def poly_alpha(P):
    im=Image.new("L",(W*4,H*4),0); ImageDraw.Draw(im).polygon([(x*4,y*4) for x,y in P],fill=255); return np.array(im.resize((W,H),Image.BOX)).astype(np.float32)/255
tex=lambda sd,m,amp: np.clip(m+amp*np.array(Image.fromarray((np.random.default_rng(sd).random((H//4,W//4,3))*255).astype(np.uint8)).resize((W,H),Image.BICUBIC)).astype(np.float32)/255,0,1)
bg=tex(1,0.25,0.35); fgt=tex(2,0.35,0.35)
import os; os.makedirs("/tmp/gt",exist_ok=True); GT=[]
for i in range(N):
    P=shape(i); A=poly_alpha(P); A=np.array(Image.fromarray((A*255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.8 if i%7 else 2.5))).astype(np.float32)/255  # some motion-blurred frames
    t=i/(N-1); a=math.radians(25*math.sin(t*math.pi*1.5)); sc=1.0+0.25*t; c,sn=math.cos(a),math.sin(a); ci=np.array([130+400*t,180+40*math.sin(t*6)]); c0=np.array([130.0,180+0.0])  # texture rotates/scales/translates WITH the object
    ft=Image.fromarray((fgt*255).astype(np.uint8)).transform((W,H),Image.AFFINE,(c/sc,sn/sc,c0[0]-(c*ci[0]+sn*ci[1])/sc,-sn/sc,c/sc,c0[1]-(-sn*ci[0]+c*ci[1])/sc),Image.BICUBIC); f=np.array(ft).astype(np.float32)/255; b=bg
    fr=A[...,None]*f+(1-A[...,None])*b; Image.fromarray((fr*255).astype(np.uint8)).save("/tmp/gt/f%03d.png"%i); GT.append(A)
subprocess.run("ffmpeg -v error -y -framerate 30 -i /tmp/gt/f%03d.png -c:v libx264 -crf 12 -pix_fmt yuv420p /tmp/gt.mp4",shell=True,check=True)
k=Kimo();C=k.call;C("project_set",width=W,height=H,fps=FPS)
a=C("import_media",path="/tmp/gt.mp4")["asset_id"];t=C("add_track")["track_id"];m=C("add_clip",track_id=t,source_type="media",asset_id=a,start=0,duration=N/FPS,fit="cover")["clip_id"]
nseed=int(sys.argv[1]) if len(sys.argv)>1 else 2; idx=[0,N-1] if nseed==2 else [0,N//2,N-1]
r=C("track_matte",clip_id=m,seeds=[{"time":i/FPS,"points":shape(i).tolist()} for i in idx],temporal=float(sys.argv[2]) if len(sys.argv)>2 else 0.6,band=float(os.environ.get("BAND","14")),colour=float(os.environ.get("COL","0.9")))
print("track s",round(r["seconds"],1),"flicker",r["flicker"],"jitter",r["area_jitter"],r["warnings"])
ious=[];maes=[];bi=[]; ia,ib=idx[0],idx[-1]
for i in range(N):
    C("render_mask",clip_id=m,mask_id=r["mask_id"],time=i/FPS,view="matte",path="/tmp/gm.png"); A=np.array(Image.open("/tmp/gm.png").convert("L")).astype(np.float32)/255
    G=GT[i]; ious.append(np.minimum(A,G).sum()/max(np.maximum(A,G).sum(),1)); maes.append(np.abs(A-G)[(np.maximum(A,G)>0.02)].mean())
    if ia<i<ib or True:
        s=(i-ia)/(ib-ia); P=shape(ia)*(1-s)+shape(ib)*s; B=poly_alpha(P); bi.append(np.minimum(B,G).sum()/max(np.maximum(B,G).sum(),1))
held=[i for i in range(N) if i not in idx]
print("hold-out frames (%d): tracker IoU mean %.4f min %.4f | alpha MAE(on object+edge) %.4f | polygon-interpolation baseline IoU mean %.4f min %.4f"%(len(held),np.mean([ious[i] for i in held]),np.min([ious[i] for i in held]),np.mean([maes[i] for i in held]),np.mean([bi[i] for i in held]),np.min([bi[i] for i in held])))
print("IoU first 14:",[round(float(ious[i]),3) for i in range(14)])
