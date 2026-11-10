"""Real 16-bit decode/linear composite, tagged PQ HEVC and lossless 16-bit output."""
import sys
exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=25);asset=call('import_media',path=sys.argv[1])['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=2,in_point=10)['clip_id']
call('render_frame',time=.4,path='/downloads/pipeline-legacy.png')
print(call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=1,input_gamut=0,output_transfer=1,output_gamut=0)))
call('render_frame',time=.4,path='/downloads/pipeline-linear-sdr.png')
start=time.perf_counter();print(call('export_high_bit',format='ffv1_16',path='/downloads/pipeline-linear16.mkv'));print('ffv1_16_export_seconds',time.perf_counter()-start)
call('add_effect',clip_id=a,kind='linear_grade',params=dict(gamut=3,exposure=.25,saturation=.95))
print(call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=1,input_gamut=0,output_transfer=6,output_gamut=1,reference_white=203)))
start=time.perf_counter();print(call('export_high_bit',format='hevc10',path='/downloads/pipeline-pq10.mp4',crf=16));print('pq10_export_seconds',time.perf_counter()-start)
# SDR proof preview must explicitly encode to display, never display PQ as sRGB.
call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=1,input_gamut=0,output_transfer=1,output_gamut=0,gamut_compress=True))
call('render_frame',time=.4,path='/downloads/pipeline-grade-sdr.png')
for v in [dict(enabled=True,working_gamut=999),dict(enabled=True,reference_white=0),dict(enabled='wrong')]:assert rpc('set_color_pipeline',settings=v).get('isError')
p.terminate()
from PIL import Image,ImageDraw
import numpy as np
ims=[Image.open('/downloads/'+n+'.png').convert('RGB') for n in ['pipeline-legacy','pipeline-linear-sdr','pipeline-grade-sdr']];out=Image.new('RGB',(960,210));d=ImageDraw.Draw(out)
for i,(im,label)in enumerate(zip(ims,['LEGACY','LINEAR ROUNDTRIP','LINEAR GRADE SDR PREVIEW'])):out.paste(im.resize((320,180)),(320*i,30));d.text((320*i+6,8),label,fill='white')
out.save('/downloads/pipeline-comparison.png');x=np.asarray(ims[0]).astype(float);y=np.asarray(ims[1]).astype(float);print('legacy_vs_highdecode_MAE',np.abs(x-y).mean(),'max',np.abs(x-y).max())
