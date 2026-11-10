exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=24);tr=call('add_track')['track_id']
a=call('add_clip',track_id=tr,source_type='gradient',color=[.05,.02,.04],color2=[.65,.45,.35],start=0,duration=2)['clip_id']
b=call('add_clip',track_id=tr,source_type='gradient',color=[.02,.04,.05],color2=[.4,.6,.65],start=3,duration=2)['clip_id']
call('render_frame',time=1,path='/downloads/color-match-before.png');call('render_frame',time=4,path='/downloads/color-match-reference.png')
start=time.perf_counter();v=call('match_grade',clip_id=a,ref_clip_id=b,time=1,ref_time=4);print('match_seconds',time.perf_counter()-start,v)
call('render_frame',time=1,path='/downloads/color-match-after.png')
call('delete_clip',clip_id=b)
start=time.perf_counter();call('export',path='/downloads/color-match-proof.mp4');print('match_export_seconds',time.perf_counter()-start)
for v in [0,-1,1e12]:rpc('match_grade',clip_id=a,ref_clip_id='nope',time=0,ref_time=0,regularization=v)
p.terminate()
from PIL import Image,ImageDraw
ims=[Image.open('/downloads/color-match-'+n+'.png') for n in ['before','after','reference']];out=Image.new('RGB',(960,210));d=ImageDraw.Draw(out)
for i,(im,label) in enumerate(zip(ims,['SOURCE','MATCHED','REFERENCE'])):out.paste(im.resize((320,180)),(i*320,30));d.text((i*320+12,8),label,fill='white')
out.save('/downloads/color-match-comparison.png')
# Delta E76 is evaluated on this synthetic pair only, 8-bit rendered frames.
import numpy as np
def rgb2lab(rgb):
 lin=np.where(rgb<=.04045,rgb/12.92,((rgb+.055)/1.055)**2.4)
 xyz=lin@np.array([[.412390799,.357584339,.180480788],[.212639006,.715168679,.072192315],[.019330819,.11919478,.950532152]]).T
 xyz/=np.array([.950455927,1,1.089057751]);f=np.where(xyz>216/24389,np.cbrt(xyz),(24389/27*xyz+16)/116)
 return np.stack([116*f[:,:,1]-16,500*(f[:,:,0]-f[:,:,1]),200*(f[:,:,1]-f[:,:,2])],axis=-1)
lab=[rgb2lab(np.asarray(im)[:,:,:3]/255.) for im in ims];print('mean_DeltaE76_before',np.linalg.norm(lab[0]-lab[2],axis=2).mean());print('mean_DeltaE76_after',np.linalg.norm(lab[1]-lab[2],axis=2).mean())
