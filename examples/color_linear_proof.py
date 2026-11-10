"""Real-footage scene-linear grading + explicit decode/encode. Usage: path.mp4"""
import sys
exec(open('examples/color_engine_proof.py').read().split("call('project_set'")[0])
call('project_set',width=640,height=360,fps=25);asset=call('import_media',path=sys.argv[1])['asset_id'];tr=call('add_track')['track_id'];a=call('add_clip',track_id=tr,source_type='media',asset_id=asset,start=0,duration=2,in_point=10)['clip_id'];call('render_frame',time=.4,path='/downloads/color-linear-before.png')
call('add_effect',clip_id=a,kind='color_managed',params=dict(from_transfer=1,to_transfer=0,from_gamut=0,to_gamut=3))
call('add_effect',clip_id=a,kind='linear_grade',params=dict(gamut=3,exposure=.25,contrast=1.06,saturation=.9,gain_r=1.02,gain_b=.98))
call('add_effect',clip_id=a,kind='color_managed',params=dict(from_transfer=0,to_transfer=1,from_gamut=3,to_gamut=0,gamut_compress=1))
call('render_frame',time=.4,path='/downloads/color-linear-after.png');start=time.perf_counter();call('export',path='/downloads/color-linear-real.mp4');print('linear_export_seconds',time.perf_counter()-start)
start=time.perf_counter();print(call('export_grade_lut',clip_id=a,time=.4,size=33,path='/downloads/color-linear-grade.cube'));print('linear_lut_bake_seconds',time.perf_counter()-start)
p.terminate()
from PIL import Image,ImageDraw
ims=[Image.open('/downloads/color-linear-'+n+'.png').convert('RGB') for n in ['before','after']];out=Image.new('RGB',(960,300));d=ImageDraw.Draw(out)
for i,(im,label) in enumerate(zip(ims,['SOURCE','SCENE-LINEAR AP1 GRADE'])):out.paste(im.resize((480,270)),(i*480,30));d.text((i*480+12,8),label,fill='white')
out.save('/downloads/color-linear-comparison.png')
