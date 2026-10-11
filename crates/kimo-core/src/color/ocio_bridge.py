# Optional reference CPU bridge. Pinned OCIO config/version, no implicit install.
import sys, array
try:
 import PyOpenColorIO as o
except ImportError:
 raise SystemExit('PyOpenColorIO missing; install opencolorio==2.5.1 for optional reference export')
if o.GetVersion()!='2.5.1':raise SystemExit('reference bridge requires OpenColorIO2.5.1 exactly')
w,h=int(sys.argv[1]),int(sys.argv[2]);mode=sys.argv[3]
choices={'sdr':('sRGB - Display','ACES 2.0 - SDR 100 nits (Rec.709)'), 'pq2020':('Rec.2100-PQ - Display','ACES 2.0 - HDR 1000 nits (Rec.2020)')}
if mode not in choices:raise SystemExit('unsupported reference view')
c=o.Config.CreateFromBuiltinConfig('cg-config-v4.0.0_aces-v2.0_ocio-v2.5');d,v=choices[mode];cpu=c.getProcessor(o.DisplayViewTransform(src='ACEScg',display=d,view=v)).getDefaultCPUProcessor()
size=w*h*16
while True:
 data=bytearray()
 while len(data)<size:
  b=sys.stdin.buffer.read(size-len(data))
  if not b:break
  data.extend(b)
 if not data:break
 if len(data)!=size:raise SystemExit('truncated input frame')
 a=array.array('f');a.frombytes(data)
 if sys.byteorder!='little':a.byteswap()
 cpu.apply(o.PackedImageDesc(a,w,h,4))
 if sys.byteorder!='little':a.byteswap()
 sys.stdout.buffer.write(a.tobytes());sys.stdout.buffer.flush()
