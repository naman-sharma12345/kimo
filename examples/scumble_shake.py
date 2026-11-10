# HUD shot with impact landings AND beat-locked camera shake (hits measured from the reference: every 0.48 s = 125 BPM).
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
src=open('/home/sandbox/kimo/examples/scumble_hud_boxes.py').read().split("if len(sys.argv)")[0]
exec(src)
t=trk(); k.call("add_camera_shake",track_id=t,start=0,duration=D,hits=[h-3.72 for h in [4.12,4.60,5.08,5.48,5.56,6.04,6.12,6.52,6.80,7.04,7.52] if h-3.72<D])
if len(sys.argv)>1: k.call("export",path="/tmp/shake2.mp4",crf=17)
