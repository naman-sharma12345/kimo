# denoise effect proof: real frame + Gaussian noise (sigma 12 codes) -> PSNR vs the clean frame, against OpenCV bilateral and NLM baselines. Writes only to /tmp.
import sys,time,numpy as np,cv2;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
from PIL import Image
clean=np.array(Image.open(sys.argv[1]).convert('RGB'));H,W=clean.shape[:2];rng=np.random.RandomState(4)
noisy=np.clip(clean.astype(float)+rng.randn(H,W,3)*12,0,255).astype(np.uint8);Image.fromarray(noisy).save('/tmp/dn_noisy.png')
psnr=lambda a:10*np.log10(255**2/((a.astype(float)-clean.astype(float))**2).mean())
print('noisy      PSNR %.2f'%psnr(noisy))
k=Kimo();C=k.call;C('project_set',width=W,height=H,fps=25);a=C('import_media',path='/tmp/dn_noisy.png')['asset_id']
t=C('add_track')['track_id'];c=C('add_clip',track_id=t,source_type='media',asset_id=a,start=0,duration=1)['clip_id']
for amt,rad,lu,ch in ((1.0,2,0.5,0.7),(1.0,3,0.8,0.9)):
    e=C('add_effect',clip_id=c,kind='denoise',params={'amount':amt,'radius':rad,'luma':lu,'chroma':ch});t0=time.time();C('render_frame',time=0,path='/tmp/dn_out.png');el=time.time()-t0
    out=np.array(Image.open('/tmp/dn_out.png').convert('RGB'));print('kimo amount %.1f r%d luma %.1f chroma %.1f  PSNR %.2f  %.2fs'%(amt,rad,lu,ch,psnr(out),el));C('remove_effect',clip_id=c,effect_id=e['effect_id'])
bgr=cv2.cvtColor(noisy,cv2.COLOR_RGB2BGR)
t0=time.time();b=cv2.cvtColor(cv2.bilateralFilter(bgr,7,40,5),cv2.COLOR_BGR2RGB);print('cv bilateral PSNR %.2f %.2fs'%(psnr(b),time.time()-t0))
t0=time.time();n=cv2.cvtColor(cv2.fastNlMeansDenoisingColored(bgr,None,10,10,7,21),cv2.COLOR_BGR2RGB);print('cv NLM       PSNR %.2f %.2fs'%(psnr(n),time.time()-t0))
