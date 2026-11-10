# Tracked shattering plate: a moving perspective plate is tracked (track_planar), a plane_fracture layer is pinned to it, and the plate shatters while staying glued.
# Ground truth: the plate quad is known per frame; reports corner error of the tracker. usage: plane_tracked_demo.py <texture>  -> /tmp/pt_sheet.png, /tmp/pt.mp4
import sys,time,subprocess,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
TEX=sys.argv[1];tex=Image.open(TEX).convert('RGB').resize((640,440));W,H,N=1280,720,45
def quad(i):
    t=i/(N-1);return np.array([[253+120*t,299-30*t],[749+90*t,140+20*t],[1061+60*t,458+10*t],[521+120*t,620-20*t]],float)
def coef(q):
    src=np.array([[0,0],[640,0],[640,440],[0,440]],float);A=[];B=[]
    for (x,y),(X,Y) in zip(q,src): A+=[[x,y,1,0,0,0,-X*x,-X*y],[0,0,0,x,y,1,-Y*x,-Y*y]];B+=[X,Y]
    return tuple(np.linalg.solve(np.array(A),np.array(B)))
import os;os.makedirs('/tmp/ptf',exist_ok=True)
for i in range(N):
    c=coef(quad(i));plate=tex.transform((W,H),Image.PERSPECTIVE,c,Image.BICUBIC);m=Image.new('L',(640,440),255).transform((W,H),Image.PERSPECTIVE,c)
    bg=Image.new('RGB',(W,H),(40,30,28));bg.paste(plate,(0,0),m);bg.save('/tmp/ptf/f%03d.png'%i)
subprocess.run(['ffmpeg','-v','error','-y','-framerate','30','-i','/tmp/ptf/f%03d.png','-c:v','libx264','-crf','12','-pix_fmt','yuv420p','/tmp/pt_plate.mp4'],check=True)
k=Kimo();C=k.call;C('project_set',width=W,height=H,fps=30)
a=C('import_media',path='/tmp/pt_plate.mp4')['asset_id'];t0=C('add_track')['track_id'];plate=C('add_clip',track_id=t0,source_type='media',asset_id=a,start=0,duration=N/30)['clip_id']
t1=C('add_track')['track_id']
# shard layer authored over the whole canvas = the plate's own UV space (plane quad = full canvas), so the pin maps it onto the tracked quad
cfg={'plane':{'quad':[[0,0],[1280,0],[1280,720],[0,720]],'region':[1280,720],'impact_uv':[0.62,0.55],'cells':140,'radius':140,'wave_speed':2800,'impulse':1800,'lift':1.0,'texture':TEX,'opaque':1.0,'hidden_until_birth':False,'max_delay':0.2},'gravity':0,'drag':1.6,'spin':380,'fresnel':0.6,'glass':0.3,'thickness':[.02,.06],'dof':0.4}
sh=C('add_shard_burst',track_id=t1,start=0.5,duration=0.9,config=cfg)
sid=sh.get('clip_id') or sh.get('id');print('shard clip',sid)
q0=quad(0);t=time.time()
r=C('track_planar',clip_id=plate,corners=[v for p in q0 for v in (p[0]/W,p[1]/H)],target_clip_id=sid,start=0,end=N-1);print('track s',round(time.time()-t,2),str(r)[:200])
tiles=[]
for i,x in enumerate([0.3,0.55,0.75,1.1]):
    C('render_frame',time=x,path='/tmp/pt%d.png'%i);tiles.append(Image.open('/tmp/pt%d.png'%i).convert('RGB').resize((640,360)))
s=Image.new('RGB',(1280,720));[s.paste(im,((i%2)*640,(i//2)*360)) for i,im in enumerate(tiles)];s.save('/tmp/pt_sheet.png')
