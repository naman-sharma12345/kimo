# Beat-locked shake alignment test: HUD frame (corner bracket) + add_camera_shake at the reference hit times (ref 3.72 = t0).
import sys; sys.path.insert(0,'/home/sandbox/kimo/examples')
from mcp_client import Kimo
HITS_ABS=[4.12,4.60,5.08,5.48,5.56,6.04,6.12,6.52,6.80,7.04,7.52]   # measured from the reference (bracket tracker): 125 BPM grid + off-beat box landings
O=3.72; D=4.24
k=Kimo(); C=k.call; C("project_set",width=1280,height=720,fps=25)
t=C("add_track")["track_id"]; C("add_hud_frame",track_id=t,start=0,duration=D,label="BOXES",tc_start=3.72)
t2=C("add_track")["track_id"]; C("add_camera_shake",track_id=t2,start=0,duration=D,hits=[h-O for h in HITS_ABS])
C("export",path="/tmp/shake_align.mp4",crf=17)
