"""Optional video-only ACES2 proof: PYTHONPATH=/path/to/OCIO2.5.1 python3 examples/color_aces2_video_proof.py footage.mp4"""
import sys
"""Synthetic engine proof, measured exports + real-id hostile color tools. Run at repo root."""
import json,subprocess,time,select,pathlib
p=subprocess.Popen(['target/release/kimo-mcp'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=open('/tmp/color-engine-stderr.log','w'),text=True);seq=0
out=pathlib.Path('/downloads');out.mkdir(exist_ok=True)
def rpc(name,**args):
 global seq
 seq+=1;p.stdin.write(json.dumps(dict(jsonrpc='2.0',id=seq,method='tools/call',params=dict(name=name,arguments=args)))+'\n');p.stdin.flush()
 if not select.select([p.stdout],[],[],30)[0]:raise TimeoutError(name)
 r=json.loads(p.stdout.readline())['result'];return r

def call(name,**args):
 r=rpc(name,**args)
 if r.get('isError'):raise RuntimeError(r)
 return json.loads(r['content'][0]['text'])

try:
 call('project_set',width=640,height=360,fps=25)
 a=call('import_media',path=sys.argv[1])['asset_id'];tr=call('add_track')['track_id']
 call('add_clip',track_id=tr,source_type='media',asset_id=a,start=0,in_point=3,duration=2)
 call('set_color_pipeline',settings=dict(enabled=True,working_gamut=3,input_transfer=8))
 start=time.perf_counter();print(call('export_aces2_reference_video',path='/downloads/aces2-reference-video.mp4'));print('wall_seconds',time.perf_counter()-start)
finally:p.terminate();p.wait()
