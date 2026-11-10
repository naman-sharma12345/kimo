# plane_fracture proof: a coplanar pane split into connected cells erupts from an impact point. Contact sheet at 4 times.
import sys,time;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
k=Kimo();C=k.call
C('project_set',width=960,height=540,fps=25)
t0=C('add_track')['track_id']; a=C('add_clip',track_id=t0,source_type='solid',color=[.06,.08,.12],start=0,duration=1.6)
tx=C('add_track')['track_id']; C('add_text',track_id=tx,text='KIMO',start=0,duration=1.6,size=300,color=[1,.6,.15])
t=C('add_track')['track_id']
cfg={'plane':{'impact':[0,40],'region':[1400,800],'cells':160,'radius':120,'wave_speed':2600,'impulse':1200,'lift':1.0,'under_depth':60},'gravity':900,'drag':1.6,'spin':420,'refract':10,'fresnel':1,'glass':1,'thickness':[.02,.07],'dof':0.6}
C('add_shard_burst',track_id=t,start=0.2,duration=1.4,config=cfg)
ts=[0.2,0.28,0.4,0.7]; tiles=[]
t1=time.time()
for i,x in enumerate(ts):
    C('render_frame',time=x,path='/tmp/pf%d.png'%i); tiles.append(Image.open('/tmp/pf%d.png'%i).convert('RGB').resize((480,270)))
print('render seconds per frame',round((time.time()-t1)/len(ts),2))
s=Image.new('RGB',(960,540)); [s.paste(im,((i%2)*480,(i//2)*270)) for i,im in enumerate(tiles)]; s.save('/tmp/plane_fracture.png')
