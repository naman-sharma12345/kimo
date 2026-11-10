#!/usr/bin/env python3
"""Kimo MCP reel assembly: a recorded take in, a finished vertical reel out, every edit a Kimo tool call.

probe -> audio -> whisper chunks -> per-line take selection -> cut list -> Kimo MCP build -> Kimo export -> stills.
See README.md in this directory."""
import sys, os, json, time, subprocess, argparse, hashlib, re
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, '..', '..'))
ap = argparse.ArgumentParser(description='Assemble the Kimo reel from a recorded take, entirely through kimo-mcp tool calls.')
ap.add_argument('input', help='recorded take (video or audio with the 10 script lines spoken in order; retakes allowed)')
ap.add_argument('--out', default=os.environ.get('KIMO_REEL_OUT', 'kimo_reel.mp4'), help='output mp4 (env KIMO_REEL_OUT; default ./kimo_reel.mp4)')
ap.add_argument('--work', default=os.environ.get('KIMO_REEL_WORK', 'reel_work'), help='work dir for audio, ASR cache, SFX, logs (env KIMO_REEL_WORK; default ./reel_work)')
ap.add_argument('--kimo-mcp', dest='exe', default=os.environ.get('KIMO_MCP', os.path.join(REPO, 'target', 'release', 'kimo-mcp')), help='kimo-mcp binary (env KIMO_MCP; default <repo>/target/release/kimo-mcp)')
ap.add_argument('--fonts', default=os.environ.get('KIMO_FONTS'), help='font dir passed to Kimo as KIMO_FONTS (default <work>/fonts, built from the repo\'s bundled OFL fonts)')
ap.add_argument('--script', default=os.path.join(HERE, 'script.json'), help='JSON list of the 10 caption lines (default script.json here)')
ap.add_argument('--crf', type=float, default=20, help='export CRF (default 20; 27 gives a ~7 MB copy for WhatsApp)')
ap.add_argument('--no-export', action='store_true', help='build the timeline but skip the export')
ap.add_argument('--frames', default='', help='comma-separated times (s) to render as dev PNGs into the work dir')
A = ap.parse_args()
sys.path.insert(0, HERE); sys.path.insert(0, os.path.join(REPO, 'examples'))
from mcp_client import Kimo
import align, reelbuild, fonts
PY = sys.executable; EXE = A.exe
W = os.path.abspath(A.work); os.makedirs(W, exist_ok=True); T_START = time.time()
A.out = os.path.abspath(A.out); A.input = os.path.abspath(A.input)
os.environ['KIMO_FONTS'] = os.path.abspath(A.fonts) if A.fonts else fonts.ensure_fonts(REPO, os.path.join(W, 'fonts'))
def log(*a):
    print(f'[{time.time()-T_START:6.1f}s]',*a,flush=True)
SCRIPT=json.load(open(A.script, encoding='utf-8'))
# 1 probe
pr=json.loads(subprocess.run(['ffprobe','-v','error','-show_streams','-show_format','-of','json',A.input],capture_output=True,text=True,timeout=60).stdout)
vs=[s for s in pr['streams'] if s['codec_type']=='video']; aus=[s for s in pr['streams'] if s['codec_type']=='audio']
rot=0
if vs:
    for sd in vs[0].get('side_data_list',[]):
        if 'rotation' in sd: rot=int(sd['rotation'])
    rot=rot or int(vs[0].get('tags',{}).get('rotate',0))
log('probe',{'video':bool(vs),'w':vs[0]['width'] if vs else None,'h':vs[0]['height'] if vs else None,'rotation':rot,'audio':bool(aus),'dur':pr['format'].get('duration')})
assert aus, 'input has no audio stream'
# 2 audio + 3 whisper chunks (cached per input)
h=hashlib.sha1(f"{A.input}{os.path.getsize(A.input)}{os.path.getmtime(A.input)}".encode()).hexdigest()[:10]
wav=f'{W}/voice16k_{h}.wav'; chj=f'{W}/asr_chunks_{h}.json'
if not os.path.exists(wav): subprocess.run(['ffmpeg','-v','error','-y','-i',A.input,'-vn','-ac','1','-ar','16000',wav],check=True,timeout=300)
if not os.path.exists(chj):
    log('whisper small (chunked, word timestamps) ...'); subprocess.run([PY,os.path.join(HERE,'asr.py'),wav,chj],check=True,timeout=1800,stdout=open(f'{W}/asr2_{h}.log','w'),stderr=subprocess.STDOUT)
C=json.load(open(chj))['chunks']
ch=[dict(start=c['start'],end=c['end'],text=c['asr'][0]['text'],lp=c['asr'][0]['lp'],words=c['asr'][0]['words']) for c in C]
# 4 per-line take selection
picks=align.select(ch,SCRIPT); json.dump(picks,open(f'{W}/picks.json','w'),indent=1)
for p in picks: log('L%d'%(p['line']+1), 'MISSING' if p.get('missing') else f"{p['start']:.2f}-{p['end']:.2f} score={p['score']} heard={p['heard']!r}")
# 5 cut list on the reel timeline
def typo(s): return s.replace("'", "\u2019")
TRANS={3:('zoom_blur',0.32),4:('whip_left',0.28),6:('whip_right',0.25),8:('zoom',0.25)}
lines=[]; t=0.12
for p in picks:
    if p.get('missing'): continue
    i=p['line']; src_in=max(0,p['start']-0.04); src_out=p['end']+0.06
    if lines: t+= 0.18 if i in (3,4,8) else (0.30 if i==9 else 0.08)
    words={}
    for c in p['chunks']:
        for w in ch[c]['words']:
            k=re.sub(r"[^a-z0-9]","",w['w'].lower())
            if k in ('200',): k='two'
            if k.startswith('colo'): k='color'
            words.setdefault(k,max(0.0,w['s']-src_in))
    tk=TRANS.get(i)
    lines.append(dict(idx=i,caption=typo(SCRIPT[i]),**{'in':src_in},out=src_out,T0=round(t,3),T1=round(t+src_out-src_in,3),speech=src_out-src_in,words=words,
                      trans_kind=tk[0] if tk else None,trans_in=tk[1] if tk else 0.0,src_span=[round(p['start'],2),round(p['end'],2)]))
    t=lines[-1]['T1']
assert len(lines)==10 and [l['idx'] for l in lines]==list(range(10)), 'need all 10 lines for this layout: '+str([l['idx'] for l in lines])
total=round(lines[-1]['T1']+2.9,3)
log('timeline total',total,'s')
# SFX + bed (numpy synth), bed level ~ -25 LUFS before ducking
sfxd=f'{W}/sfx'; os.makedirs(sfxd,exist_ok=True)
subprocess.run([PY,os.path.join(HERE,'sfx.py'),sfxd,str(total+1)],check=True,timeout=300)
def dur(p): return float(subprocess.run(['ffprobe','-v','error','-show_entries','format=duration','-of','csv=p=0',p],capture_output=True,text=True).stdout)
def lufs(p):
    o=subprocess.run(['ffmpeg','-nostats','-i',p,'-af','ebur128','-f','null','-'],capture_output=True,text=True).stderr
    return float(re.findall(r'I:\s+(-?[\d.]+) LUFS',o)[-1])
bedL=lufs(f'{sfxd}/bed.wav'); bed_vol=round(10**((-25-bedL)/20),3)
P=dict(input=A.input,lines=lines,total=total,hits=[],whooshes=[],bed_vol=bed_vol,
       sfx_files={n:(f'{sfxd}/{n}.wav',round(dur(f'{sfxd}/{n}.wav'),3)) for n in ['whoosh1','whoosh2','hit','tick','riser','bed']})
for l in lines:
    if l['idx'] in (1,2,3,4,6,8): P['whooshes'].append(l['T0']-0.327)
P['whooshes'].append(lines[9]['T0']-0.05-0.327)
def short(v):
    if isinstance(v,float): return f'{v:.2f}'
    if isinstance(v,list): return '['+','.join(short(x) for x in v[:3])+(',…' if len(v)>3 else '')+']'
    if isinstance(v,str):
        v=os.path.basename(v) if v.startswith('/') else v
        return v if len(v)<14 else v[:12]+'…'
    return str(v)
def ticker_fn(b):
    calls=b.calls; seen={}; rows=[]
    for i,(tool,a,ms) in enumerate(calls):
        if tool in ('add_track','set_track','set_property') and seen.get(tool,0)>=1: continue
        if seen.get(tool,0)>=3: continue
        seen[tool]=seen.get(tool,0)+1
        args=', '.join(f'{k}={short(v)}' for k,v in list(a.items())[:2] if k not in ('track_id',))
        s=f'#{i+1:03d} {tool}({args})'
        if len(s)>44: s=s[:43]+'…'
        rows.append((s,ms))
    rows=rows[:21]
    return rows
P['ticker_fn']=ticker_fn
# 6 two passes: stub (count calls exactly) then real
def run(stub,n_calls):
    P2=dict(P,hits=list(P['hits']),whooshes=list(P['whooshes']),n_calls=n_calls)
    k=None if stub else Kimo(exe=EXE,timeout=600)
    b=reelbuild.B(k,log=open(f'{W}/calls{"_stub" if stub else ""}.log','w'),stub=stub)
    if not stub: b.stub_calls=None
    info=reelbuild.build(b,P2)
    return b,k,P2,info
bs,_,_,_=run(True,0)
N=bs.n+(0 if A.no_export else 1)
log('stub pass: calls =',N)
b,k,P2,info=run(False,N)
log('build done: calls',b.n,'failures',len(b.fail),b.fail[:5])
log('mix', P2.get('mix_lufs'))
fr=[float(x) for x in A.frames.split(',') if x]
for t in fr:
    k.call('render_frame',time=t,path=f'{W}/dev_{t:05.2f}.png')
res=dict(calls=b.n+(0 if A.no_export else 1),fail=b.fail,total=total,lines=[{kk:l[kk] for kk in ('idx','src_span','T0','T1')} for l in lines],ticker_upto=P2.get('ticker_upto'),n_counter=N)
if not A.no_export:
    log('export ->',A.out); t0=time.time()
    try:
        r=k.call('export',path=A.out,crf=A.crf,timeout=3000); res['export']=r
    except Exception as e:
        res['export_fail']=str(e); log('EXPORT FAIL',e)
    b.n+=1; res['export_s']=round(time.time()-t0,1); log('export done',res.get('export'))
    if os.path.exists(A.out):
        base=os.path.splitext(A.out)[0]
        for j,tt in enumerate([lines[0]['T0']+1.0,lines[5]['T1']-0.3,total-1.0]):
            subprocess.run(['ffmpeg','-v','error','-y','-ss',str(tt),'-i',A.out,'-frames:v','1','-q:v','2',f'{base}_s{j+1}.jpg'],timeout=120)
        res['stills']=[f'{base}_s{j+1}.jpg' for j in range(3)]
res['wall_s']=round(time.time()-T_START,1)
json.dump(res,open(f'{W}/result.json','w'),indent=1,default=str); log('RESULT',json.dumps(res,default=str)[:1500])
k.close()
