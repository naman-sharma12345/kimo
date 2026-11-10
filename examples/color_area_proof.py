exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
import struct
from PIL import Image
w=h=256
raw=b''.join(struct.pack('<f',(-.25 if ((i%w)+(i//w))%2==0 else 4.) if ch!=3 else 1.) for ch in [1,2,0,3] for i in range(w*h))
subprocess.run(['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','gbrapf32le','-s','256x256','-i','-','-frames:v','1','-c:v','exr','-format','float','/downloads/area-checker.exr'],input=raw,check=True)
call('project_set',width=32,height=32,fps=25);asset=call('import_media',path='/downloads/area-checker.exr')['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=.08)['clip_id'];call('set_color_pipeline',settings=dict(enabled=True,working_gamut=0,input_gamut=0,input_transfer=0,output_gamut=0,output_transfer=1,tone_map=True))
start=time.perf_counter();call('render_float_exr',time=0,path='/downloads/area-checker-fit.exr');print('checker_export_seconds',time.perf_counter()-start);v=call('analyze_color',time=0);print('checker_min_max',v['min'],v['max']);assert abs(v['mean'][0]-1.875)<1e-6
call('render_frame',time=0,path='/downloads/area-checker-preview.png');p.terminate()
# Visible 256px contact sheet: checker source versus tone-mapped energy-preserving result.
a=Image.new('RGB',(256,256));a.putdata([(0,0,0) if (x+y)%2==0 else (255,255,255) for y in range(256) for x in range(256)])
b=Image.open('/downloads/area-checker-preview.png').convert('RGB').resize((256,256),Image.Resampling.NEAREST);out=Image.new('RGB',(512,256));out.paste(a,(0,0));out.paste(b,(256,0));out.save('/downloads/area-comparison.png')
