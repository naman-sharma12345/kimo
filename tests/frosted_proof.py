"""frosted_glass proof: inside the pane the picture is blurred (less detail than the plain backdrop),
outside it is bit-identical to the effect-off render. Uses a generated plate; writes only to /tmp."""
import sys,subprocess,os,struct,zlib,time;sys.path.insert(0,os.path.join(os.path.dirname(__file__),'..','examples'))
from mcp_client import Kimo
plate='/tmp/frosted_plate.mp4'
subprocess.run(['ffmpeg','-v','error','-y','-f','lavfi','-i','mandelbrot=s=640x360:r=25','-t','2','-pix_fmt','yuv420p',plate],check=True)
k=Kimo(); c=k.call
c('project_set',width=640,height=360,fps=25)
aid=c('import_media',path=plate)['asset_id']
t1=c('add_track')['track_id']; t2=c('add_track')['track_id']
c('add_clip',track_id=t1,source_type='media',asset_id=aid,start=0,duration=2,in_point=0,fit='cover')
sid=c('add_shape',track_id=t2,shape='rect',width=300,height=140,corner_radius=30,fill=[1,1,1,1],start=0,duration=2)['clip_id']
def png(p):
    out=subprocess.run(['ffmpeg','-v','error','-i',p,'-f','rawvideo','-pix_fmt','rgb24','-'],capture_output=True,check=True).stdout; return out
c('render_frame',time=0.5,path='/tmp/fp_off.png'); off=png('/tmp/fp_off.png')
fxid=c('add_effect',clip_id=sid,kind='frosted_glass')['effect_id']; t0=time.time(); c('render_frame',time=0.5,path='/tmp/fp_on.png'); dt=time.time()-t0; on=png('/tmp/fp_on.png')
W,H=640,360; px=lambda b,x,y:b[(y*W+x)*3:(y*W+x)*3+3]
def grad(b,x0,x1,y0,y1): g=lambda x,y:b[(y*W+x)*3+1]; return sum(abs(g(x-1,y)-2*g(x,y)+g(x+1,y)) for y in range(y0,y1) for x in range(x0,x1))  # second difference = fine detail
# pane is centered; use a core region well inside it
inner=(250,390,150,210); 
subprocess.run(['ffmpeg','-v','error','-y','-i',plate,'-vf','select=eq(n\,12)','-vsync','0','-frames:v','1','/tmp/fp_plate.png'],check=True); plate_f=png('/tmp/fp_plate.png')
assert on!=off,'effect changed nothing'
outside=all(px(on,x,y)==px(off,x,y) for x,y in ((5,5),(630,5),(5,350),(630,350),(100,180),(540,180)))
print('outside pane identical:',outside)
print('pane detail (sum |2nd difference| of green): on',grad(on,*inner),'vs unfrosted plate',grad(plate_f,*inner)); assert grad(on,*inner)<0.6*grad(plate_f,*inner),'pane not blurred'
print('render_frame with frosted_glass: %.2fs'%dt)
assert outside
# moved pane must frost what is under it, not what was under the unmoved layer
c('set_property',clip_id=sid,property='x',value=140); c('render_frame',time=0.5,path='/tmp/fp_mv.png'); mv=png('/tmp/fp_mv.png')
def mean(b,x0,x1,y0,y1): n=(x1-x0)*(y1-y0); return [sum(b[(y*W+x)*3+j] for y in range(y0,y1) for x in range(x0,x1))/n for j in range(3)]
d=lambda a,b:sum(abs(x-y) for x,y in zip(a,b))
m=mean(mv,330,450,150,210); here=mean(plate_f,330,450,150,210); origin=mean(plate_f,190,310,150,210)
print('moved pane mean vs plate under it %.1f, vs plate at unmoved spot %.1f'%(d(m,here),d(m,origin))); assert d(m,here)<d(m,origin),'pane frosts the wrong place'
# clear magnifier lens: pane centre shows the plate zoomed 2x about the pane centre
for kk,v in dict(blur=0,tint_amount=0.0,saturation=1.0,edge_light=0.0,refraction=0.0,magnify=2.0,x=0).items():
    if kk=='x': c('set_property',clip_id=sid,property='x',value=0)
    else: c('set_effect_param',clip_id=sid,effect_id=fxid,param=kk,value=v)
c('render_frame',time=0.5,path='/tmp/fp_lens.png'); ln=png('/tmp/fp_lens.png')
def zoomed_err(mag):
    e=n=0
    for y in range(160,200):
        for x in range(280,360):
            sx=int(W/2+(x-W/2)/mag); sy=int(H/2+(y-H/2)/mag); e+=sum(abs(a-b) for a,b in zip(px(ln,x,y),px(plate_f,sx,sy))); n+=1
    return e/n
print('lens vs plate zoomed 2x: mean abs err %.1f; vs unzoomed plate %.1f'%(zoomed_err(2.0),zoomed_err(1.0))); assert zoomed_err(2.0)<0.35*zoomed_err(1.0)
print('frosted proof OK')
