exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=25);asset=call('import_media',path='/downloads/video-c61877f1.mp4')['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=2,in_point=10)['clip_id'];call('render_frame',time=.4,path='/downloads/color-camera-source.png')
call('add_effect',clip_id=a,kind='color_managed',params=dict(from_transfer=1,to_transfer=4,from_gamut=0,to_gamut=5));call('render_frame',time=.4,path='/downloads/color-camera-log.png')
call('add_effect',clip_id=a,kind='color_managed',params=dict(from_transfer=4,to_transfer=1,from_gamut=5,to_gamut=0));call('render_frame',time=.4,path='/downloads/color-camera-roundtrip.png');start=time.perf_counter();call('export',path='/downloads/color-camera-roundtrip.mp4');print('camera_roundtrip_export_seconds',time.perf_counter()-start)
p.terminate()
from PIL import Image
import numpy as np
x=np.asarray(Image.open('/downloads/color-camera-source.png')).astype(float);y=np.asarray(Image.open('/downloads/color-camera-roundtrip.png')).astype(float);print('camera_roundtrip_8bit_mae',np.abs(x-y).mean(),'max',np.abs(x-y).max())
