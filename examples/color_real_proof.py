"""Real footage smoke tests + known distorted frame recovery, not pro-tool parity.
Usage: python3 examples/color_real_proof.py /absolute/reference.mp4
"""
import sys
exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=25);asset=call('import_media',path=sys.argv[1])['asset_id'];tr=call('add_track')['track_id']
a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=2,in_point=16)['clip_id'];ref=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=3,duration=2,in_point=16)['clip_id']
call('render_frame',time=.8,path='/downloads/color-real-reference.png');fx=call('add_effect',clip_id=a,kind='cdl',params=dict(slope_r=1.12,slope_b=.87,power_g=1.08,saturation=.8))['effect_id'];call('render_frame',time=.8,path='/downloads/color-real-distorted.png')
start=time.perf_counter();match=call('match_grade',clip_id=a,ref_clip_id=ref,time=.8,ref_time=3.8,strength=1);print('match_seconds',time.perf_counter()-start)
call('render_frame',time=.8,path='/downloads/color-real-recovered.png');call('delete_clip',clip_id=ref)
start=time.perf_counter();call('export',path='/downloads/color-real-recovery.mp4');print('recovery_export_seconds',time.perf_counter()-start)
# Separate blue qualifier demo on original.
call('remove_effect',clip_id=a,effect_id=match['effect_id']);call('remove_effect',clip_id=a,effect_id=fx)
qual=call('add_effect',clip_id=a,kind='qualifier',params=dict(hue_center=210,hue_width=70,hue_soft=20,sat_min=.2,matte_blur=1,matte_refine=4,saturation=1.6,exposure=.25))['effect_id'];call('render_frame',time=.8,path='/downloads/color-real-qualified.png')
start=time.perf_counter();call('export',path='/downloads/color-real-qualifier.mp4');print('qualifier_export_seconds',time.perf_counter()-start)
call('set_effect_param',clip_id=a,effect_id=qual,param='show_matte',value=1);call('render_frame',time=.8,path='/downloads/color-real-qualifier-matte.png')
p.terminate()
from PIL import Image,ImageDraw
import numpy as np
names=['reference','distorted','recovered'];ims=[Image.open('/downloads/color-real-'+n+'.png').convert('RGB') for n in names];out=Image.new('RGB',(960,210));d=ImageDraw.Draw(out)
for i,(im,label) in enumerate(zip(ims,names)):out.paste(im.resize((320,180)),(320*i,30));d.text((320*i+10,8),label.upper(),fill='white')
out.save('/downloads/color-real-comparison.png')
def rgb2lab(rgb):
 lin=np.where(rgb<=.04045,rgb/12.92,((rgb+.055)/1.055)**2.4);xyz=lin@np.array([[.412390799,.357584339,.180480788],[.212639006,.715168679,.072192315],[.019330819,.11919478,.950532152]]).T;xyz/=np.array([.950455927,1,1.089057751]);f=np.where(xyz>216/24389,np.cbrt(xyz),(24389/27*xyz+16)/116);return np.stack([116*f[:,:,1]-16,500*(f[:,:,0]-f[:,:,1]),200*(f[:,:,1]-f[:,:,2])],axis=-1)
lab=[rgb2lab(np.asarray(im)/255.) for im in ims]
for j in [1,2]:print(names[j]+'_mean_DeltaE76',np.linalg.norm(lab[j]-lab[0],axis=2).mean())
