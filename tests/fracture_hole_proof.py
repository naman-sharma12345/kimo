"""fracture_glass hole_fill proof. Writes only to /tmp.
1) default and explicit hole_fill=0 render bit-identical; 2) hole_fill=1 (defocused pane) makes the hole bright vs black;
3) hole_source=1 over footage shows the footage below the glass."""
import sys,os,subprocess,hashlib;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
for n,src in (('a','testsrc2=s=640x360:r=25:d=3'),('b','smptebars=s=640x360:r=25:d=3')): subprocess.run(['ffmpeg','-v','error','-y','-f','lavfi','-i',src,'-pix_fmt','yuv420p',f'/tmp/fh_{n}.mp4'],check=True)
def raw(p): return subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout
W=640
def mean(b,x0,x1,y0,y1): n=(x1-x0)*(y1-y0); return [sum(b[(y*W+x)*3+j] for y in range(y0,y1) for x in range(x0,x1))/n for j in range(3)]
def scene(under):
    k=Kimo(); c=k.call; c('project_set',width=640,height=360,fps=25)
    a=c('import_media',path='/tmp/fh_a.mp4')['asset_id']; b=c('import_media',path='/tmp/fh_b.mp4')['asset_id']
    if under: t0=c('add_track')['track_id']; c('add_clip',track_id=t0,source_type='media',asset_id=b,start=0,duration=3,in_point=0,fit='cover')
    t1=c('add_track')['track_id']; cl=c('add_clip',track_id=t1,source_type='media',asset_id=a,start=0,duration=3,in_point=0,fit='cover')['clip_id']
    fx=c('add_effect',clip_id=cl,kind='fracture_glass',params={'t0':0.3,'impact_x':0.5,'impact_y':0.5})['effect_id']
    def shot(name,**kw):
        for kk,v in kw.items(): c('set_effect_param',clip_id=cl,effect_id=fx,param=kk,value=v)
        p=f'/tmp/fhp_{name}.png'; c('render_frame',time=1.2,path=p); return raw(p)
    return shot
s=scene(False); base=s('base'); zero=s('zero',hole_fill=0.0); fill=s('fill',hole_fill=1.0)
print('default == hole_fill 0:',base==zero); assert base==zero
core=(280,360,150,210); mb,mf=mean(base,*core),mean(fill,*core); print('hole centre mean RGB default',[round(v) for v in mb],'with hole_fill',[round(v) for v in mf]); assert sum(mf)>sum(mb)+30
s2=scene(True); over=s2('over',hole_fill=1.0,hole_source=1.0); bars=raw('/tmp/fh_b.mp4') if False else None
subprocess.run(['ffmpeg','-v','error','-y','-ss','1.2','-i','/tmp/fh_b.mp4','-frames:v','1','/tmp/fh_b1.png'],check=True); bw=raw('/tmp/fh_b1.png')
mo=mean(over,*core); mbars=mean(bw,*core); print('hole with footage below',[round(v) for v in mo],'footage below there',[round(v) for v in mbars])
d=sum(abs(x-y) for x,y in zip(mo,mbars)); assert d<40,'hole does not show the layer below'; print('fracture hole_fill proof OK')
