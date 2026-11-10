import subprocess,json
from pathlib import Path
import argparse
parser=argparse.ArgumentParser();parser.add_argument('--binary',default='target/debug/kimo-mcp');parser.add_argument('--output',default='/tmp/gap4-followup');args=parser.parse_args()
root=Path(args.output).resolve()
subprocess.run(['python3','tests/gap4_proof.py','--binary',args.binary,'--output',str(root)],check=True)
p=subprocess.Popen([str(Path(args.binary).resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,stderr=open(root/'followup.stderr','w'))
log=[]
def call(n,a):
 p.stdin.write(json.dumps(dict(jsonrpc='2.0',id=len(log)+1,method='tools/call',params=dict(name=n,arguments=a)))+'\n');p.stdin.flush();v=json.loads(p.stdout.readline())['result'];log.append(dict(name=n,args=a,result=v));assert not v.get('isError'),v;return json.loads(v['content'][0]['text'])
call('load_project',{'path':str(root/'project.json')});cg=next(iter(call('list_captions',{})))
call('burn_in',{'caption_id':cg,'enabled':False});call('export_with_subtitles',{'path':str(root/'soft.mp4')})
call('burn_in',{'caption_id':cg,'enabled':True});call('export_with_subtitles',{'path':str(root/'burn-and-soft.mkv')})
peak=call('measure_loudness',{'true_peak':True});assert peak['true_peak_dbfs']>=peak['sample_peak_dbfs']
for path in ['soft.mp4','burn-and-soft.mkv']:
 out=subprocess.check_output(['ffprobe','-v','error','-show_streams','-of','json',str(root/path)]);j=json.loads(out);assert any(s['codec_type']=='subtitle' for s in j['streams'])
call('project_set',{'fps':30000/1001})
tracks=[];clips=[]
for ip in [0,.2]:
 tr=call('add_track',{})['track_id'];asset=call('import_media',{'path':str(root/'red.mp4')})['asset_id'];clips.append(call('add_clip',dict(track_id=tr,source_type='media',asset_id=asset,start=0,duration=1,in_point=ip))['clip_id'])
r=call('sync_clips',dict(clip_ids=clips,by='timecode',fps=30000/1001,drop_frame=True,timecodes=['00:00:59;29','00:01:00;02']));assert abs(r['offsets_seconds'][1]-1001/30000)<1e-8
p.stdin.close();p.wait();(root/'followup-mcp.json').write_text(json.dumps(log,indent=2))
for label,filter in [('without','null'),('selectable','subtitles='+str(root/'soft.mp4'))]:
 subprocess.run(['ffmpeg','-v','error','-y','-ss','0.5','-i',str(root/'soft.mp4'),'-vf',filter,'-frames:v','1',str(root/(label+'.png'))],check=True)
from PIL import Image
sheet=Image.new('RGB',(1280,360));sheet.paste(Image.open(root/'without.png'),(0,0));sheet.paste(Image.open(root/'selectable.png'),(640,0));sheet.save(root/'soft-proof.png')
print('followup MCP proof:',len(log),'calls; MP4/MKV selectable streams; truepeak; dropframe shift 1 frame')
