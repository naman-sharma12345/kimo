# Rotoscope assist test with exact ground truth: a textured, rotating, accelerating ellipse composited over real footage (first N frames of a video),
# keys at the two ends (exact outline), IoU measured on every held-out frame for baseline interpolation vs roto_propagate.
import sys,subprocess,math,numpy as np,time;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image,ImageDraw,ImageFilter
src=sys.argv[1];ip=float(sys.argv[2]);N=int(sys.argv[3]) if len(sys.argv)>3 else 25;snaps=[None,6.0]
W,H,FPS=720,1280,25
raw=subprocess.run(['ffmpeg','-v','error','-ss',str(ip),'-i',src,'-frames:v',str(N+1),'-vf','scale=%d:%d,format=gray'%(W,H),'-f','rawvideo','-'],capture_output=True).stdout
plate=np.frombuffer(raw,np.uint8).reshape(-1,H,W).astype(np.float32);rng=np.random.RandomState(3)
tex=Image.fromarray((rng.rand(256,256)*255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(2.0));tex=np.array(tex.resize((400,400))).astype(np.float32)
def pose(i):
    u=i/25.0;e=u*u*(3-2*u)*0.6+u*0.4; return 250+300*e+60*math.sin(u*6.0), 500+260*math.sin(e*3.0), 110+30*math.sin(u*4), 60+20*math.cos(u*5), 180*e  # cx,cy,rx,ry,rot_deg
def poly(i,n=24):
    cx,cy,rx,ry,rot=pose(i);r=math.radians(rot);o=[]
    for k in range(n): a=k/n*2*math.pi;x,y=rx*math.cos(a),ry*math.sin(a);o.append((cx+x*math.cos(r)-y*math.sin(r),cy+x*math.sin(r)+y*math.cos(r)))
    return o
def mask(i):
    im=Image.new('L',(W,H),0);ImageDraw.Draw(im).polygon(poly(i),fill=255);return np.array(im)>0
frames=[]
for i in range(N+1):
    m=mask(i);cx,cy,rx,ry,rot=pose(i);yy,xx=np.mgrid[0:H,0:W];r=math.radians(-rot);u=((xx-cx)*math.cos(r)-(yy-cy)*math.sin(r));v=((xx-cx)*math.sin(r)+(yy-cy)*math.cos(r))
    t=tex[(np.clip((v+200).astype(int),0,399)),(np.clip((u+200).astype(int),0,399))];f=plate[i].copy();f[m]=60+0.55*t[m]+40;frames.append(np.clip(f,0,255).astype(np.uint8))
sub=subprocess.Popen(['ffmpeg','-v','error','-y','-f','rawvideo','-pix_fmt','gray','-s','%dx%d'%(W,H),'-r',str(FPS),'-i','-','-c:v','libx264','-crf','10','-pix_fmt','yuv420p','/tmp/roto_synth.mp4'],stdin=subprocess.PIPE);[sub.stdin.write(f.tobytes()) for f in frames];sub.stdin.close();sub.wait()
def iou(a,b): return (a&b).sum()/max(1,(a|b).sum())
def pts(i): return [{'x':x/W,'y':y/H} for x,y in poly(i)]
out={}
for snap in snaps:
    subprocess.run(['pkill','kimo-mcp']);k=Kimo();C=k.call;C('project_set',width=W,height=H,fps=FPS)
    a=C('import_media',path='/tmp/roto_synth.mp4')['asset_id'];t=C('add_track')['track_id'];c=C('add_clip',track_id=t,source_type='media',asset_id=a,start=0,duration=N/FPS+0.04)['clip_id']
    m=C('add_mask',clip_id=c,kind='path',points=pts(0),closed=True,interp='linear')['mask_id'];C('set_mask_path_key',clip_id=c,mask_id=m,time=0.0,points=pts(0));C('set_mask_path_key',clip_id=c,mask_id=m,time=N/FPS,points=pts(N))
    t0=time.time();info=C('roto_propagate',clip_id=c,mask_id=m,snap=snap,trust=float(__import__('os').environ.get('TR','0.12'))) if snap is not None else {};dt=time.time()-t0;ious=[]
    for i in range(1,N,3):
        C('render_mask',clip_id=c,time=i/FPS,path='/tmp/rs.png',view='matte');ious.append(iou(np.array(Image.open('/tmp/rs.png').convert('L'))>127,mask(i)))
    print([round(x,2) for x in ious]);print('baseline' if snap is None else 'roto snap %.0f'%snap,'mean IoU %.3f min %.3f'%(np.mean(ious),np.min(ious)),'propagate %.1fs'%dt,info)
