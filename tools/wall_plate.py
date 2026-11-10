# Synthetic wall plate with known camera motion (ground truth) for planar tracker testing.
import sys,random,json,math; sys.path.insert(0,'/home/sandbox/kimo/examples')
from PIL import Image,ImageDraw,ImageFilter,ImageFont
random.seed(4); W,H=1280,720
im=Image.new('RGB',(W,H),(90,60,50)); d=ImageDraw.Draw(im)
bh,bw=36,96
for r in range(H//bh+1):
    for c in range(-1,W//bw+2):
        x=c*bw+(bw//2 if r%2 else 0); t=random.randint(-28,28); col=(150+t,80+t//2+random.randint(-8,8),62+t//2); d.rectangle([x+3,r*bh+3,x+bw-3,r*bh+bh-3],fill=col)
for i in range(60):
    x,y=random.randint(0,W),random.randint(0,H); d.line([x,y,x+random.randint(-90,90),y+random.randint(-40,40)],fill=(random.randint(30,230),random.randint(30,230),random.randint(30,230)),width=random.randint(2,6))
sign=[(450,260),(830,260),(830,440),(450,440)]
d.rectangle([440,250,840,450],fill=(235,235,225)); d.rectangle([452,262,828,438],outline=(40,40,40),width=4)
try: f=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf',36)
except: f=None
d.text((480,290),"BLANK SIGN",fill=(30,30,30),font=f); d.text((480,350),"no. 4417 / east",fill=(200,40,40),font=f)
im=im.filter(ImageFilter.GaussianBlur(0.6)); im.save('/tmp/wall.png'); json.dump(sign,open('/tmp/wall_sign.json','w'))
