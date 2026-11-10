# Measures how close the pre-birth (preload) plane_fracture surface is to the untouched plate. usage: prebirth_seam_test.py <texture>
import sys,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
TEX=sys.argv[1];Q=[[253,299],[749,140],[1061,458],[521,620]]
exec(open('/home/sandbox/kimo/examples/plane_quad_demo.py').read().split("k=Kimo()")[0].split("TEX=sys.argv[1]")[1].replace(";Q=[[253,299],[749,140],[1061,458],[521,620]]","",1)) if False else None
tex=Image.open(TEX).convert('RGB').resize((640,440))
src=np.array([[0,0],[640,0],[640,440],[0,440]],float);dst=np.array(Q,float);A=[];B=[]
for (x,y),(X,Y) in zip(dst,src): A+=[[x,y,1,0,0,0,-X*x,-X*y],[0,0,0,x,y,1,-Y*x,-Y*y]];B+=[X,Y]
c=np.linalg.solve(np.array(A),np.array(B))
plate=tex.transform((1280,720),Image.PERSPECTIVE,tuple(c),Image.BICUBIC);m=Image.new('L',(640,440),255).transform((1280,720),Image.PERSPECTIVE,tuple(c))
bg=Image.new('RGB',(1280,720),(40,30,28));bg.paste(plate,(0,0),m);bg.save('/tmp/pq_bg.png')
mk=np.array(m.resize((1280,720)))>250
k=Kimo();C=k.call;C('project_set',width=1280,height=720,fps=25)
t0=C('add_track')['track_id'];a=C('import_media',path='/tmp/pq_bg.png')['asset_id'];C('add_clip',track_id=t0,source_type='media',asset_id=a,start=0,duration=1.2)
t=C('add_track')['track_id']
cfg={'plane':{'quad':Q,'region':[640,440],'impact_uv':[0.62,0.55],'cells':140,'radius':70,'wave_speed':300,'impulse':900,'lift':1.0,'texture':TEX,'opaque':1.0,'hidden_until_birth':False,'max_delay':0.2,'preload_depth':45,'preload_time':0.15},'gravity':700,'drag':1.6,'spin':380,'refract':0,'fresnel':0.6,'glass':0.3,'thickness':[.02,.06],'dof':0.4}
C('add_shard_burst',track_id=t,start=0.2,duration=1.0,config=cfg)
ref=np.array(bg).astype(float)
for x in (0.2,0.23,0.26):
    C('render_frame',time=x,path='/tmp/pb.png');im=np.array(Image.open('/tmp/pb.png').convert('RGB')).astype(float)
    d=np.abs(im-ref).mean(2)[mk];print('t',x,'mean abs diff vs plate in quad',round(d.mean(),2),'p99',round(np.percentile(d,99),1),'max',round(d.max(),1))
    Image.open('/tmp/pb.png').save('/tmp/pb_%d.png'%int(x*100))
