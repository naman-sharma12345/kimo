exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
import struct
# Dependency-free RGB float TIFF fixture, little-endian, one uncompressed strip.
w=h=16;count=10;offset=8+2+count*12+4;bits=offset;sample=bits+6;pixels=sample+6
fields=[(256,4,1,w),(257,4,1,h),(258,3,3,bits),(259,3,1,1),(262,3,1,2),(273,4,1,pixels),(277,3,1,3),(278,4,1,h),(279,4,1,w*h*12),(339,3,3,sample)]
header=b'II'+struct.pack('<HI',42,8)+struct.pack('<H',count)
header+=b''.join(struct.pack('<HHII',*f) for f in fields)+struct.pack('<I',0)+struct.pack('<HHH',32,32,32)+struct.pack('<HHH',3,3,3)
values=[-.25,.18,1.25,4.]
open('/downloads/float-source32.tiff','wb').write(header+b''.join(struct.pack('<fff',*([values[x//4]]*3)) for y in range(h) for x in range(w)))
raw=b''.join(struct.pack('<f',values[(i%w)//4] if ch!=3 else 1.) for ch in [1,2,0,3] for i in range(w*h))
subprocess.run(['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','gbrapf32le','-s','16x16','-i','-','-frames:v','1','-c:v','exr','-format','float','/downloads/float-source.exr'],input=raw,check=True)
for source in ['/downloads/float-source.exr','/downloads/float-source32.tiff']:
 call('project_set',width=32,height=32,fps=25)
 # separate projects to avoid overlapping prior clips
 # each source uses isolated clip scopes and previous track removal
 if 'prev' in globals():
  p.terminate();p=subprocess.Popen(['target/release/kimo-mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open('/tmp/color-float-stderr.log','w'),text=True)
 call('project_set',width=32,height=32,fps=25)
 asset=call('import_media',path=source)['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=.08)['clip_id'];prev=a
 call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_gamut=3,input_transfer=0,output_gamut=0,output_transfer=1,reference_white=100))
 start=time.perf_counter();v=call('analyze_color',time=0,units='nits',range_min=-50,range_max=500);print(source,'scope_seconds',time.perf_counter()-start,{k:v[k] for k in ['min','max','mean','units','gamut_id']})
 start=time.perf_counter();out=source+'.out.exr';call('render_float_exr',time=0,path=out);print('EXR_export_seconds',time.perf_counter()-start)
 b=subprocess.check_output(['ffmpeg','-v','error','-apply_trc','linear','-i',out,'-f','rawvideo','-pix_fmt','gbrapf32le','-']);q=struct.unpack('<'+'f'*(len(b)//4),b);print('range',min(q),max(q));assert min(q)<-.24 and max(q)>3.99
 call('render_frame',time=0,path=source+'.preview.png')
p.terminate()
