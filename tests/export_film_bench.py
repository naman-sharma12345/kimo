# Usage: python3 export_film_bench.py <project.json> <start> <end> <out.mp4>
# Times one export of a fixed range and prints frame md5s of the output so two builds can be compared.
import sys,time,subprocess;sys.path.insert(0,'/home/sandbox/kimo/examples');from mcp_client import Kimo
proj,st,en,out=sys.argv[1],float(sys.argv[2]),float(sys.argv[3]),sys.argv[4]
k=Kimo();C=k.call;C('load_project',path=proj)
t=time.time();C('export',path=out,crf=18,start=st,end=en);dt=time.time()-t
n=subprocess.run(['ffprobe','-v','error','-count_frames','-select_streams','v:0','-show_entries','stream=nb_read_frames','-of','csv=p=0',out],capture_output=True,text=True).stdout.strip()
print('export %.1fs for %s frames (%.2f s/frame)'%(dt,n,dt/max(int(n or 1),1)))
