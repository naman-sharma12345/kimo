#!/usr/bin/env python3
"""Run after unit-1 and operator numeric gates. Real 1080p frames, no visual PASS claim."""
import argparse,json,subprocess
from pathlib import Path
from PIL import Image,ImageDraw
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/gate/kimo-mcp');p.add_argument('--output',default='/tmp/kimo-fields-1080p');a=p.parse_args()
out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True);log=[]
err=open(out/'server.stderr','w');proc=subprocess.Popen([str(Path(a.binary).resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True)
def call(name,**args):
    req=dict(jsonrpc='2.0',id=len(log)+1,method='tools/call',params=dict(name=name,arguments=args));proc.stdin.write(json.dumps(req)+'\n');proc.stdin.flush();raw=proc.stdout.readline()
    if not raw:raise RuntimeError('MCP exited; inspect stderr')
    r=json.loads(raw);log.append(dict(request=req,response=r));assert 'error' not in r,r;assert not r['result'].get('isError'),r
    return json.loads(r['result']['content'][0]['text'])
cases=[('worley',{'op':'worley','seed':77},.4,0),('fbm',{'op':'fbm','seed':77,'kind':'perlin','octaves':3,'lacunarity':2,'octave_gain':.5,'normalize':True},.6,.5),('warp',{'op':'warp','seed':77,'warp_seed':19,'warp_amplitude':1.5,'warp_frequency':1.7},.45,.5),('curl',{'op':'curl','seed':77,'kind':'perlin','curl_frequency':1,'curl_amplitude':1,'channel':0},.18,.5)]
try:
    for label,config,gain,bias in cases:
        call('project_set',width=1920,height=1080,fps=25);tr=call('add_track')['track_id'];cl=call('add_clip',track_id=tr,source_type='solid',color=[0,0,0],start=0,duration=3)['clip_id']
        call('add_effect',clip_id=cl,kind='field_operator',strings={'field':json.dumps(config)},params={'frequency':8,'gain':gain,'bias':bias,'rate_x':.15,'rate_y':.07})
        images=[];times=[0,.125,1,.5,2,0]
        for i,t in enumerate(times):
            path=out/f'{label}-{i:02}.png';call('render_frame',time=t,path=str(path),inline='never')
            with Image.open(path) as im:assert im.size==(1920,1080);images.append(im.convert('RGB').copy())
        assert images[0].tobytes()==images[-1].tobytes()
        sheet=Image.new('RGB',(1920,1740),(18,18,18));d=ImageDraw.Draw(sheet)
        for i,(im,t) in enumerate(zip(images,times)):
            thumb=im.copy();thumb.thumbnail((960,540));x=i%2*960;y=i//2*580;sheet.paste(thumb,(x,y+30));d.text((x+12,y+8),f'{label} 1080p t={t}, display gain={gain} bias={bias}',fill='white')
        sheet.save(out/f'{label}-contact-sheet.png');images[2].crop((480,270,1440,810)).save(out/f'{label}-1to1-crop.png');call('delete_clip',clip_id=cl)
    (out/'STATUS.txt').write_text('Rendered through MCP; numeric gates separate. Visual inspection and premium sign-off PENDING.\n')
finally:
    (out/'calls.json').write_text(json.dumps(log,indent=2));proc.terminate()
    try:proc.wait(timeout=10)
    except subprocess.TimeoutExpired:proc.kill();proc.wait()
    err.close()
print(out)
