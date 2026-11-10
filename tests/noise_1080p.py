#!/usr/bin/env python3
"""Real MCP/render pipeline at 1920x1080. Generated sheets require pixel inspection."""
import argparse,json,subprocess,time
from pathlib import Path
from PIL import Image,ImageDraw
p=argparse.ArgumentParser();p.add_argument('--binary',default='target/gate/kimo-mcp');p.add_argument('--output',default='/tmp/kimo-noise-1080p');a=p.parse_args()
out=Path(a.output).resolve();out.mkdir(parents=True,exist_ok=True)
log=[];err=open(out/'server.stderr','w')
proc=subprocess.Popen([str(Path(a.binary).resolve())],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True)
def call(name,**args):
    rid=len(log)+1;req=dict(jsonrpc='2.0',id=rid,method='tools/call',params=dict(name=name,arguments=args))
    start=time.monotonic();proc.stdin.write(json.dumps(req)+'\n');proc.stdin.flush()
    line=proc.stdout.readline()
    if not line:raise RuntimeError('MCP exited; inspect server.stderr')
    r=json.loads(line);log.append(dict(name=name,args=args,response=r,elapsed=time.monotonic()-start))
    if 'error' in r:raise RuntimeError(r)
    body=r['result'];assert not body.get('isError'),body
    return json.loads(body['content'][0]['text'])
try:
    for kind,gain in [('perlin',0.45),('simplex',10.0)]:
        call('project_set',width=1920,height=1080,fps=25)
        track=call('add_track')['track_id'];clip=call('add_clip',track_id=track,source_type='solid',color=[0,0,0],start=0,duration=4)['clip_id']
        fx=call('add_effect',clip_id=clip,kind='coherent_noise',strings={'kind':kind,'seed':'20261014'},params={'frequency':9,'gain':gain,'bias':.5,'rate_x':.15,'rate_y':.07})['effect_id']
        cells=[]
        # Deliberately shuffled times test stateless repeat rendering; last equals first.
        for i,t in enumerate([0,.125,1,.5,2,0]):
            file=out/f'{kind}-{i:02d}.png';call('render_frame',time=t,path=str(file),inline='never')
            with Image.open(file) as im:
                assert im.size==(1920,1080);cells.append(im.convert('RGB').copy())
        assert cells[0].tobytes()==cells[-1].tobytes(),f'{kind} repeated t=0 changed'
        sheet=Image.new('RGB',(1920,3*580),(18,18,18));draw=ImageDraw.Draw(sheet)
        for i,(im,t) in enumerate(zip(cells,[0,.125,1,.5,2,0])):
            im.thumbnail((960,540));x=(i%2)*960;y=(i//2)*580;sheet.paste(im,(x,y+30));draw.text((x+12,y+8),f'{kind}: 1080p source, t={t}; gain={gain}, bias=0.5 (display mapping only)',fill='white')
        sheet.save(out/f'{kind}-contact-sheet.png')
        # Preserve central 1:1 pixels separately; resized sheets alone cannot reveal fine artifacts.
        cells[2].crop((480,270,1440,810)).save(out/f'{kind}-1to1-crop.png')
        call('delete_clip',clip_id=clip)
    (out/'STATUS.txt').write_text('1080p renders generated; repeated frames byte-equal. Numeric Rust tests are separate. Visual inspection pending, premium pending.\n')
finally:
    (out/'calls.json').write_text(json.dumps(log,indent=2));proc.terminate()
    try:proc.wait(timeout=10)
    except subprocess.TimeoutExpired:proc.kill();proc.wait()
    err.close()
print(out)
