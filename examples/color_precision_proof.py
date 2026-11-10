exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
import struct
# 1024 distinct 16-bit input levels (each repeated in rows). ffmpeg PNG16 fixture.
b=b''.join(struct.pack('<HHHH',round(x*65535/1023),round(x*65535/1023),round(x*65535/1023),65535) for y in range(16) for x in range(1024))
subprocess.run(['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','rgba64le','-s','1024x16','-i','-','-frames:v','1','/downloads/precision-source16.png'],input=b,check=True)
call('project_set',width=1024,height=16,fps=25);asset=call('import_media',path='/downloads/precision-source16.png')['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=.08)['clip_id']
call('set_color_pipeline',settings=dict(enabled=True,working_gamut=0,input_gamut=0,input_transfer=0,output_gamut=0,output_transfer=0))
start=time.perf_counter();call('export_high_bit',format='ffv1_16',path='/downloads/precision16.mkv');print('precision16_export_seconds',time.perf_counter()-start);p.terminate()
out=subprocess.check_output(['ffmpeg','-v','error','-i','/downloads/precision16.mkv','-frames:v','1','-f','rawvideo','-pix_fmt','rgba64le','-'])
import numpy as np
src=np.frombuffer(b,dtype='<u2').reshape(16,1024,4);dst=np.frombuffer(out,dtype='<u2').reshape(16,1024,4)
print('input_unique',len(np.unique(src[0,:,0])),'output_unique',len(np.unique(dst[0,:,0])),'max_16bit_error',np.abs(src.astype(int)-dst.astype(int)).max())
