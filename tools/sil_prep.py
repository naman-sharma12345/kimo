import numpy as np, subprocess, sys
from PIL import Image
from scipy import ndimage as ndi
def frame(t):
    p=f'/tmp/sil/r_{t}.png'; subprocess.run(['ffmpeg','-v','error','-y','-ss',str(t),'-i','/downloads/video-c61877f1.mp4','-frames:v','1',p]); return np.array(Image.open(p).convert('RGB'))
def clean(im):
    a=im.astype(int); H,W,_=a.shape
    # flat pixels: identical to right and down neighbours and 2px away
    flat=np.zeros((H,W),bool)
    flat[:-3,:-3]=(np.abs(a[:-3,:-3]-a[3:,:-3]).sum(2)<=14)&(np.abs(a[:-3,:-3]-a[:-3,3:]).sum(2)<=14)
    lab,n=ndi.label(flat|(False))
    mask=np.zeros((H,W),bool)
    # group by color then components
    cols=a.reshape(-1,3)
    for sl_i,sl in enumerate(ndi.find_objects(lab)):
        if sl is None: continue
        h=sl[0].stop-sl[0].start; w=sl[1].stop-sl[1].start
        sq=(6<=h<=40 and 6<=w<=40); ln=(h<=9 and w>=40)
        if not(sq or ln): continue
        m=lab[sl]==sl_i+1
        if m.sum()<0.85*h*w: continue
        mask[sl]|=m
    mask=ndi.binary_dilation(mask,iterations=3)
    idx=ndi.distance_transform_edt(mask,return_distances=False,return_indices=True)
    out=im[idx[0],idx[1]]
    out=np.where(mask[...,None],ndi.uniform_filter(out.astype(float),size=(5,5,1)).astype(np.uint8),im)
    return out,mask
def sil(im,col=None):
    a=im.astype(int)
    if col is None:
        q=(a//16).reshape(-1,3); sat=q.max(1)-q.min(1); k=q[:,0]*256+q[:,1]*16+q[:,2]
        k=np.where(sat>=6,k,-1); u,c=np.unique(k[k>=0],return_counts=True); best=u[c.argmax()]
        col=a.reshape(-1,3)[k==best].mean(0)
    d=np.abs(a-col).sum(2); m=d<48
    m=ndi.binary_closing(ndi.binary_opening(m,iterations=2),iterations=4); lab,n=ndi.label(m)
    if n==0: return col,m
    sizes=ndi.sum(m,lab,range(1,n+1)); keep=np.isin(lab,[i+1 for i,s in enumerate(sizes) if s>0.25*sizes.max()])
    keep=ndi.binary_fill_holes(keep); return col,keep
for t in ['11.92','12.16','12.44']:
    im=frame(t); cl=im; msk=np.zeros(im.shape[:2],bool); col,m=sil(cl)
    Image.fromarray(cl).save(f'plate_{t}.png')
    rgba=np.zeros(cl.shape[:2]+(4,),np.uint8); rgba[...,:3]=np.array(col,np.uint8); rgba[...,3]=(m*255).astype(np.uint8)
    Image.fromarray(rgba).save(f'mask_{t}.png'); print(t,col.round(),m.mean().round(3),msk.mean().round(4))
