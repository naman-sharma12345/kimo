"""Review-only matte-guided proof. Requires Pillow, numpy, scikit-image and Rust.
Real NASA portrait with manual semantic mattes and a known synthetic distortion.
This tests recovery on a real photograph, NOT unseen-shot skin detection.
Source: https://scikit-image.org/docs/0.26.x/api/skimage.data.html#skimage.data.astronaut
Usage: python3 examples/subject_match_proof.py [output_directory]
"""
from pathlib import Path
import subprocess, sys
import numpy as np
from PIL import Image, ImageDraw, ImageFilter
from skimage import data
out = Path(sys.argv[1] if len(sys.argv)>1 else '/downloads/subject-match-proof')
out.mkdir(parents=True, exist_ok=True)
rgb=data.astronaut().astype(np.float64)/255.
h,w=rgb.shape[:2]
# Explicit manually drawn facial skin region, not automatic classification.
skin=Image.new('L',(w,h));d=ImageDraw.Draw(skin)
d.polygon([(189,99),(206,105),(222,101),(244,97),(265,106),(269,135),(260,164),(241,183),(215,181),(192,167),(182,140),(182,115)],fill=255)
skin=np.asarray(skin.filter(ImageFilter.GaussianBlur(2))).astype(np.float64)/255.
# Explicit neutral lighting sample region, on the upper background only.
light=Image.new('L',(w,h));d=ImageDraw.Draw(light)
d.polygon([(309,1),(378,1),(377,135),(356,160),(317,195),(292,191),(309,140)],fill=255)
light=np.asarray(light.filter(ImageFilter.GaussianBlur(3))).astype(np.float64)/255.
light*=1-skin
weights=np.stack([skin,1-skin-light,light],axis=-1).astype('<f4')
lin=np.where(rgb<=.04045,rgb/12.92,((rgb+.055)/1.055)**2.4)
# Region-specific linear exposure + channel casts, on this one real photograph.
gains=np.array([[.78,.66,.54],[.72,.87,1.04],[.68,.72,.79]])
combined=weights@gains
altered=lin*combined
source=np.where(altered<=.0031308,12.92*altered,1.055*altered**(1/2.4)-.055).clip(0,1)
for name,img in [('reference',rgb),('source',source)]:
 rgba=np.concatenate([img,np.ones((h,w,1))],axis=2).astype('<f4');rgba.tofile(out/(name+'.f32'))
 Image.fromarray(np.round(img.clip(0,1)*255).astype('uint8')).save(out/(name+'.png'))
weights.tofile(out/'weights.f32')
Image.fromarray(np.round(weights*255).astype('uint8')).save(out/'regions.png')
result=subprocess.run(['cargo','run','-p','kimo-core','--release','--example','subject_match_proof','--',str(w),str(h),str(out)],check=True,capture_output=True,text=True)
print(result.stdout);(out/'rust-results.txt').write_text(result.stdout)
for name in ['matched','creative']:
 img=np.fromfile(out/(name+'.f32'),dtype='<f4').reshape(h,w,4)[...,:3]
 Image.fromarray(np.round(img.clip(0,1)*255).astype('uint8')).save(out/(name+'.png'))
 # Pixel-aligned known-distortion recovery only, not an objective creative score.
 err=np.sqrt(np.mean((img-rgb)**2));print(name,'RGB_RMSE',err)
print('source RGB_RMSE',np.sqrt(np.mean((source-rgb)**2)))
# Plain evidence panel; retain identical crops and scale.
labels=['Known distortion','Matte-guided match','Reference','Skin +0.35 stops']
panel=Image.new('RGB',(1024,1104),'white');d=ImageDraw.Draw(panel)
for i,name in enumerate(['source','matched','reference','creative']):
 x=(i%2)*512;y=(i//2)*552
 d.text((x+16,y+14),labels[i],fill=(37,31,33));panel.paste(Image.open(out/(name+'.png')),(x,y+40))
panel.save(out/'comparison.png')
