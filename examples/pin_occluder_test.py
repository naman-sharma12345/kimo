# Pin a graphic onto the occluded plate via the context route + occluder_matte; renders frames showing the graphic tucking under the "finger". Writes /tmp/pin_occ.png
import sys,os,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples')
os.environ['CTX']='1'
src=open('/home/sandbox/kimo/examples/planar_occlusion_test.py').read().split("Q=np.array")[0]
sys.argv=['x',sys.argv[1]];exec(src)
from PIL import Image,ImageDraw
g=Image.new('RGBA',(640,440),(255,200,0,255));ImageDraw.Draw(g).rectangle([10,10,630,430],outline=(0,0,0,255),width=12);g.save('/tmp/pin_g.png')
ga=C('import_media',path='/tmp/pin_g.png')['asset_id'];t1=C('add_track')['track_id'];gc=C('add_clip',track_id=t1,source_type='media',asset_id=ga,start=0,duration=N/30)['clip_id']
if os.environ.get('RAMP'):C('set_speed_ramp',clip_id=cl,keys=[[0,float(os.environ.get('RA','1'))],[N/30,float(os.environ.get('RB','1'))]],ease='linear')
q0=inner(0);r=C('track_planar',clip_id=cl,corners=[v for p in q0 for v in (p[0]/W,p[1]/H)],start=0,end=N-1,target_clip_id=gc,occluder_matte=True,occluder_chroma=float(os.environ.get('OC','0')),occluder_mode=os.environ.get('OM','anchor'),occluder_tolerance=float(os.environ.get('OT','1')),occluder_min_area=float(os.environ.get('MA','0')),occluder_temporal=float(os.environ.get('TM','0')),occluder_thin=float(os.environ.get('TH','0')),occluder_close=float(os.environ.get('CL','0')),occluder_luma_gate=float(os.environ.get('LG','0')),occluder_expand=float(os.environ.get('EX','0')),**EXTRA);print({k:r[k] for k in ('frames','mean_conf')})
tiles=[]
for i in (5,20,32,52):
    C('render_frame',time=i/30,path='/tmp/po_%d.png'%i);tiles.append(Image.open('/tmp/po_%d.png'%i).convert('RGB').resize((640,360)))
s=Image.new('RGB',(1280,720));[s.paste(im,((j%2)*640,(j//2)*360)) for j,im in enumerate(tiles)];s.save('/tmp/pin_occ.png')
# persistence: save, restart the server (anchors are session memory), reload, re-render a frame; must match the pre-reload render
C('render_frame',time=32/30,path='/tmp/po_before.png');C('save_project',path='/tmp/pin_occ.kimo.json')
import subprocess;subprocess.run(['pkill','kimo-mcp']);k2=Kimo();C2=k2.call;C2('load_project',path='/tmp/pin_occ.kimo.json');C2('render_frame',time=32/30,path='/tmp/po_after.png')
a=np.array(Image.open('/tmp/po_before.png').convert('RGB')).astype(float);b=np.array(Image.open('/tmp/po_after.png').convert('RGB')).astype(float);print('reload mean abs diff %.4f max %.1f'%(np.abs(a-b).mean(),np.abs(a-b).max()))
