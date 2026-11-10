# Roto + guided refine on SCUMBLE's real-crossing protocol: keys 0.64/0.96, held-out 0.80 (their hand trace), IoU / missing / false px.
import sys,subprocess,json,numpy as np;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image,ImageDraw
W,H=720,1280;P=json.load(open(sys.argv[1]));mp4=sys.argv[2]
def norm(pl): return [{'x':x/W,'y':y/H} for x,y in pl]
def gt(pl): im=Image.new('L',(W,H),0);ImageDraw.Draw(im).polygon([tuple(p) for p in pl],fill=255);return np.array(im)>0
g=gt(P['0.8'])
def run(snap,refine,eps,tag):
    subprocess.run(['pkill','kimo-mcp']);k=Kimo();C=k.call;C('project_set',width=W,height=H,fps=25)
    a=C('import_media',path=mp4)['asset_id'];t=C('add_track')['track_id'];c=C('add_clip',track_id=t,source_type='media',asset_id=a,start=0,duration=3,in_point=5.0)['clip_id']
    m=C('add_mask',clip_id=c,kind='path',points=norm(P['0.64']),closed=True,interp='linear')['mask_id']
    for tt in ('0.64','0.96'): C('set_mask_path_key',clip_id=c,mask_id=m,time=float(tt),points=norm(P[tt]))
    if snap is not None: C('roto_propagate',clip_id=c,mask_id=m,snap=snap)
    if refine>0: C('update_mask',clip_id=c,mask_id=m,params={'refine':refine,'refine_eps':eps})
    C('render_mask',clip_id=c,time=0.8,path='/tmp/rr_%s.png'%tag,view='matte');return np.array(Image.open('/tmp/rr_%s.png'%tag).convert('L'))>127
for name,snap,r,e in (('interp',None,0,0),('roto6',6.0,0,0),('roto6+g4',6.0,4,1e-3),('roto6+g8',6.0,8,1e-3),('roto6+g8 eps1e-2',6.0,8,1e-2),('roto6+g16',6.0,16,1e-3),('interp+g8',None,8,1e-3)):
    m=run(snap,r,e,name.replace('+','_').replace(' ','_'));print('%-18s IoU %.4f missing %.2f%% false %d'%(name,(m&g).sum()/(m|g).sum(),100*(g&~m).sum()/g.sum(),(m&~g).sum()))
