# plane_fracture on a perspective quad with a carried texture: the intact key plate shatters from its own pose. Writes /tmp/pp_sheet.png
import sys,time,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
TEX=sys.argv[1];Q=[[253,299],[749,140],[1061,458],[521,620]]
tex=Image.open(TEX).convert('RGB').resize((640,440))
src=np.array([[0,0],[640,0],[640,440],[0,440]],float);dst=np.array(Q,float)
A=[];B=[]
for (x,y),(X,Y) in zip(dst,src):
    A+= [[x,y,1,0,0,0,-X*x,-X*y],[0,0,0,x,y,1,-Y*x,-Y*y]];B+=[X,Y]
c=np.linalg.solve(np.array(A),np.array(B))
plate=tex.transform((1280,720),Image.PERSPECTIVE,tuple(c),Image.BICUBIC)
m=Image.new('L',(640,440),255).transform((1280,720),Image.PERSPECTIVE,tuple(c));bg=Image.new('RGB',(1280,720),(40,30,28));bg.paste(plate,(0,0),m);bg.save('/tmp/pq_bg.png')
k=Kimo();C=k.call;C('project_set',width=1280,height=720,fps=25)
t0=C('add_track')['track_id'];a=C('import_media',path='/tmp/pq_bg.png')['asset_id'];C('add_clip',track_id=t0,source_type='media',asset_id=a,start=0,duration=1.2)
t=C('add_track')['track_id']
cfg={'plane':{'quad':Q,'region':[640,440],'impact_uv':[0.62,0.55],'cells':140,'radius':70,'wave_speed':1400,'impulse':900,'lift':1.0,'under_depth':0,'texture':TEX,'occlusion':(sys.argv[2] if len(sys.argv)>2 else None),'opaque':1.0,'hidden_until_birth':False,'max_delay':0.2},'physics':True,'floor':300,'restitution':0.3,'gravity':1400,'drag':1.6,'spin':380,'refract':0,'fresnel':0.6,'glass':0.3,'thickness':[.02,.06],'dof':0.4}
C('add_shard_burst',track_id=t,start=0.2,duration=1.0,config=cfg)
ts=[0.2,0.3,0.6,1.1];tiles=[];t1=time.time()
for i,x in enumerate(ts):
    C('render_frame',time=x,path='/tmp/pp%d.png'%i);tiles.append(Image.open('/tmp/pp%d.png'%i).convert('RGB').resize((640,360)))
print('render s/frame',round((time.time()-t1)/len(ts),2))
s=Image.new('RGB',(1280,720));[s.paste(im,((i%2)*640,(i//2)*360)) for i,im in enumerate(tiles)];s.save('/tmp/pp_sheet.png')
r=C('shards_project',time=0.5);print('shards',r['count'],r['shards'][:2])
