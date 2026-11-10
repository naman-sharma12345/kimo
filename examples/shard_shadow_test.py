# Contact shadows: shard_burst contact_shadow darkens the receiving plate near airborne shards. Measures plate darkening on pixels the shards do not cover, on vs off.
import sys,subprocess,numpy as np,time;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
Image.new('RGB',(640,440),(255,0,255)).save('/tmp/mag.png')
def run(v,T=1.0):
    subprocess.run(['pkill','kimo-mcp']);k=Kimo();C=k.call;C('project_set',width=1280,height=720,fps=30)
    t1=C('add_track')['track_id'];C('add_clip',track_id=t1,source_type='solid',color=[.5,.5,.5],start=0,duration=3)
    t2=C('add_track')['track_id']
    cfg={'plane':{'quad':[[320,180],[960,180],[960,540],[320,540]],'region':[640,440],'impact_uv':[0.5,0.5],'cells':60,'radius':70,'wave_speed':300,'impulse':900,'lift':1.0,'texture':'/tmp/mag.png','opaque':1.0,'hidden_until_birth':False,'max_delay':0.4},'gravity':0,'drag':1.6,'spin':120,'thickness':[.02,.06],'dof':0.0,'contact_shadow':v}
    C('add_shard_burst',track_id=t2,start=0.5,duration=2.0,config=cfg)
    t0=time.time();C('render_frame',time=T,path='/tmp/ss_%s.png'%v);return time.time()-t0
ta=run(0.0);tb=run(0.7)
a=np.array(Image.open('/tmp/ss_0.0.png').convert('RGB')).astype(float);b=np.array(Image.open('/tmp/ss_0.7.png').convert('RGB')).astype(float)
mag=(a[...,0]>200)&(a[...,1]<80)&(a[...,2]>200);plate=(np.abs(a-127.5).max(axis=2)<3)
d=(a-b).mean(axis=2);sh=plate&(d>1);print('render s off/on %.2f %.2f'%(ta,tb))
print('shard px',int(mag.sum()),'plate px darkened >1/255:',int(sh.sum()),'mean darkening on those %.1f'%(d[sh].mean() if sh.any() else 0),'max %.1f'%d.max())
print('brightened px (should be 0):',int((d<-1).sum()),'| shard px changed:',int((np.abs(d)>1)[mag].sum()))
