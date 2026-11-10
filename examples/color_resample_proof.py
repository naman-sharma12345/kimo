exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
from PIL import Image
import numpy as np
call('project_set',width=640,height=360,fps=25);a=call('import_media',path='/downloads/video-c61877f1.mp4')['asset_id'];tr=call('add_track')['track_id'];call('add_clip',track_id=tr,source_type='media',asset_id=a,start=0,in_point=3,duration=2)
for native in [False,True]:
 call('set_color_pipeline',settings=dict(enabled=True,linear_resample=native,working_gamut=3,input_gamut=0,input_transfer=8,output_gamut=0,output_transfer=1))
 tag='linear' if native else 'baseline';start=time.perf_counter();call('export_high_bit',format='ffv1_16',path='/downloads/resample-'+tag+'.mkv');print(tag,'50frame_seconds',time.perf_counter()-start)
 call('render_frame',time=.5,path='/downloads/integer-fit-'+tag+'.png')
p.terminate();a=Image.open('/downloads/integer-fit-baseline.png');b=Image.open('/downloads/integer-fit-linear.png');diff=np.abs(np.asarray(a).astype(float)-np.asarray(b).astype(float));print('preview_mean_code_difference',diff.mean(),'max',diff.max());c=Image.new('RGB',(1280,360));c.paste(a,(0,0));c.paste(b,(640,0));c.save('/downloads/integer-fit-comparison.png')
