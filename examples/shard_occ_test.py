# plane.occlusion_target: shard_burst plane consumes the live adaptive occluder matte. Measures magenta (shard plane) painted over skin pixels of the plate, with and without the target.
# usage: [EX=0.5] shard_occ_test.py <texture>   (reuses the pin_occluder_test scene)
import sys,os,subprocess,numpy as np
os.environ.setdefault('OM','adaptive');os.environ.setdefault('OC','1');os.environ.setdefault('OT','3');os.environ.setdefault('MA','0.004');os.environ.setdefault('TH','2')
src=open('/home/sandbox/kimo/examples/pin_occluder_test.py').read().split("tiles=[]")[0]
tex=sys.argv[1];sys.argv=['x',tex];exec(src)
from PIL import Image
Image.new('RGB',(640,440),(255,0,255)).save('/tmp/mag.png')
q=inner(32);Q=[[float(p[0]),float(p[1])] for p in q]
SC=float(os.environ.get('SC','1'))
if SC!=1:
    C('set_keyframe',clip_id=gc,property='scale',time=0,value=SC)
    C('set_keyframe',clip_id=gc,property='pos_z',time=0,value=float(os.environ.get('PZ','0')))
t2=C('add_track')['track_id']
C('render_frame',time=40/30,path='/tmp/plate50.png')
cfg={'plane':{'quad':Q,'region':[640,440],'impact_uv':[0.5,0.5],'cells':60,'radius':70,'wave_speed':300,'impulse':900,'lift':1.0,'texture':'/tmp/mag.png','opaque':1.0,'hidden_until_birth':False,'max_delay':0.4,'preload_depth':0,'preload_time':0.0,'occlusion_target':gc if os.environ.get('USE','1')=='1' else None},'gravity':0,'drag':1.6,'spin':0,'refract':0,'fresnel':0.0,'glass':0.0,'thickness':[.02,.06],'dof':0.0}
if cfg['plane']['occlusion_target'] is None: del cfg['plane']['occlusion_target']
C('add_shard_burst',track_id=t2,start=1.2,duration=0.5,config=cfg)
if SC==1: subprocess.run(['ffmpeg','-v','error','-y','-i','/tmp/po.mp4','-vf','select=eq(n\\,40)','-frames:v','1','/tmp/plate50.png'])
t=40/30;C('render_frame',time=t,path='/tmp/so.png')
im=np.array(Image.open('/tmp/so.png').convert('RGB')).astype(float);pl=np.array(Image.open('/tmp/plate50.png').convert('RGB').resize((im.shape[1],im.shape[0]))).astype(float)
mag=(im[...,0]>200)&(im[...,1]<80)&(im[...,2]>200);skin=np.abs(pl-np.array([225,160,130])).max(axis=2)<35
print('USE',os.environ.get('USE','1'),'magenta px',int(mag.sum()),'magenta over skin px',int((mag&skin).sum()),'skin px',int(skin.sum()))
