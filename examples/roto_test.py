# Rotoscope assist test: keys at two moments (hand-traced finger outline), held-out middle frame hand-traced as ground truth.
# Compares plain keyframe interpolation (baseline) with roto_propagate (flow warp + edge snap). Usage: roto_test.py <media.mp4> <in_point>
import sys,subprocess,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image,ImageDraw
mp4=sys.argv[1];ip=float(sys.argv[2]) if len(sys.argv)>2 else 5.0
W,H=720,1280;OX,OY=0,300
P={0.5:[(240,0),(330,0),(335,50),(300,95),(262,150),(250,190),(232,232),(195,247),(172,238),(165,212),(188,165),(210,115),(225,60)],
   1.0:[(200,0),(330,0),(345,35),(300,70),(255,115),(225,150),(205,178),(155,190),(125,182),(112,165),(130,140),(165,105),(185,50)]}
GT={0.75:[(235,0),(335,0),(345,50),(310,85),(255,140),(220,195),(195,232),(160,240),(147,232),(145,218),(160,190),(190,140),(215,80)]}
def norm(pl): return [{'x':(x+OX)/W,'y':(y+OY)/H} for x,y in pl]
def gtmask(pl):
    im=Image.new('L',(W,H),0);ImageDraw.Draw(im).polygon([(x+OX,y+OY) for x,y in pl],fill=255);return np.array(im)>0
def iou(a,b):
    box=np.zeros_like(a);box[OY:OY+400,OX:OX+400]=True;a=a&box;b=b&box;return (a&b).sum()/max(1,(a|b).sum())
def run(snap,tag):
    subprocess.run(['pkill','kimo-mcp']);k=Kimo();C=k.call;C('project_set',width=W,height=H,fps=25)
    a=C('import_media',path=mp4)['asset_id'];t=C('add_track')['track_id'];c=C('add_clip',track_id=t,source_type='media',asset_id=a,start=0,duration=3,in_point=ip)['clip_id']
    m=C('add_mask',clip_id=c,kind='path',points=norm(P[0.5]),closed=True,interp='linear')['mask_id']
    for tt in (0.5,1.0): C('set_mask_path_key',clip_id=c,mask_id=m,time=tt,points=norm(P[tt]))
    res={}
    if snap is not None: res['info']=C('roto_propagate',clip_id=c,mask_id=m,snap=snap)
    import time;t0=time.time();C('render_mask',clip_id=c,time=0.75,path='/tmp/rt_%s.png'%tag,view='matte');res['t']=time.time()-t0
    return np.array(Image.open('/tmp/rt_%s.png'%tag).convert('L'))>127,res
g=gtmask(GT[0.75])
for name,snap in (('baseline_interp',None),('roto_snap0',0.0),('roto_snap6',6.0)):
    m,res=run(snap,name);print(name,'IoU %.3f'%iou(m,g),{k:v for k,v in res.items() if k=='info'})
