import numpy as np, subprocess, json, os
from scipy.ndimage import gaussian_filter, map_coordinates
W,H,N=640,360,40
rng=np.random.default_rng(3)
tex=gaussian_filter(rng.random((300,400)),1.2); tex=(tex-tex.min())/(tex.max()-tex.min())
tex=np.clip((tex-0.5)*3+0.5,0,1)
bg=gaussian_filter(rng.random((H,W)),6); bg=0.25+0.3*(bg-bg.min())/(bg.max()-bg.min())
# plane placed at (120,60)-(520,360 area): texture coords u in 0..400,v in 0..300 -> image via H0*Ht
def Hof(t):
    a=np.radians(0.25*t); s=1+0.002*t
    R=np.array([[np.cos(a),-np.sin(a),0],[np.sin(a),np.cos(a),0],[0,0,1]])
    S=np.array([[s,0,0],[0,s,0],[0,0,1]])
    T=np.array([[1,0,1.5*t],[0,1,-0.8*t],[0,0,1]])
    P=np.array([[1,0,0],[0,1,0],[2e-5*t,-1e-5*t,1]])
    c=np.array([[1,0,320],[0,1,180],[0,0,1]]); ci=np.linalg.inv(c)
    return T@c@P@R@S@ci
H0=np.array([[1,0,120],[0,1,50],[0,0,1.0]])
quad=np.array([[0,0],[400,0],[400,300],[0,300]],float)
ys,xs=np.mgrid[0:H,0:W]; os.makedirs('fr',exist_ok=True); gt=[]
for t in range(N):
    Hm=Hof(t)@H0; Hi=np.linalg.inv(Hm)
    p=Hi@np.stack([xs.ravel(),ys.ravel(),np.ones(W*H)]); u=p[0]/p[2]; v=p[1]/p[2]
    ins=(u>=0)&(u<399)&(v>=0)&(v<299)
    img=bg.copy().ravel(); val=map_coordinates(tex,[v,u],order=1,mode='nearest'); img[ins]=val[ins]; img=img.reshape(H,W)
    img=np.clip(img+rng.normal(0,0.004,img.shape),0,1)
    from PIL import Image
    Image.fromarray((img*255).astype(np.uint8)).save(f'fr/{t:04d}.png')
    q=np.c_[quad,np.ones(4)]@Hm.T; gt.append((q[:,:2]/q[:,2:]).tolist())
json.dump(gt,open('gt.json','w'))
subprocess.run(['ffmpeg','-v','error','-y','-framerate','24','-i','fr/%04d.png','-c:v','libx264','-crf','8','-pix_fmt','yuv420p','plate.mp4'])
# Usage: python3 tests/track_diag_gen_plate.py (writes fr/, gt.json, plate.mp4 in cwd), then python3 tests/track_diag_proof.py <dir with plate.mp4/gt.json>
