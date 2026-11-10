exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=24);tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='gradient',color=[.03,.05,.02],color2=[.85,.7,.4],start=0,duration=2)['clip_id']
fx=call('add_effect',clip_id=a,kind='cdl',params=dict(slope_r=1.05,offset_b=.02,power_g=1.1,saturation=.8))['effect_id'];call('render_frame',time=1,path='/downloads/color-cdl-direct.png')
start=time.perf_counter();print(call('export_grade_lut',clip_id=a,time=1,size=33,path='/downloads/color-grade.cube'));print('lut_bake_seconds',time.perf_counter()-start)
call('remove_effect',clip_id=a,effect_id=fx);call('add_effect',clip_id=a,kind='lut',strings=dict(path='/downloads/color-grade.cube'));call('render_frame',time=1,path='/downloads/color-cdl-baked.png')
start=time.perf_counter();call('export',path='/downloads/color-cdl-baked.mp4');print('baked_export_seconds',time.perf_counter()-start)
for n in [-1,0,1,1e12]:r=rpc('export_grade_lut',clip_id=a,time=0,size=n,path='/downloads/never.cube');assert r.get('isError')
p.terminate()
from PIL import Image
import numpy as np
x=np.asarray(Image.open('/downloads/color-cdl-direct.png')).astype(float);y=np.asarray(Image.open('/downloads/color-cdl-baked.png')).astype(float);print('baked_vs_direct_8bit_mae',np.abs(x-y).mean(),'max',np.abs(x-y).max())
