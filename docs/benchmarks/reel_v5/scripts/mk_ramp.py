
import json, subprocess, sys
R='/tmp/reel'
ev=[l.split(' ',1) for l in open(f'{R}/run_events.log').read().splitlines()]
ev=[(float(a),b) for a,b in ev]
t0=[t for t,m in ev if m=='REC_START'][0]
KEY={'trim_clip','split_clip','move_clip','add_node','set_grade_output','set_color_curve','get_scopes','render_scope','import_media','add_audio_clip','set_audio','add_audio_effect','set_audio_pan','set_audio_volume_key','measure_loudness','normalize_loudness','project_set','add_adjustment_layer'}
calls=[(t-t0,m.split()[1]) for t,m in ev if m.startswith('CALL ')]
bd=[t-t0 for t,m in ev if m.startswith('BUILD_DONE')][0]
es=[t-t0 for t,m in ev if m=='EXPORT_START'][0]
ed=[t-t0 for t,m in ev if m.startswith('EXPORT_DONE') or m.startswith('EXPORT_FAIL')][0]
stop=[t-t0 for t,m in ev if m.startswith('REC_STOP')][0]
FAST=float(sys.argv[1]) if len(sys.argv)>1 else 4.0
segs=[(0.0, calls[0][0], 1.0)]
for i,(t,name) in enumerate(calls):
    nt=calls[i+1][0] if i+1<len(calls) else bd
    sp=1.0 if name in KEY else FAST
    segs.append((t,nt,sp))
segs.append((bd, es+2.5, 1.0))
exp_len=(ed-3.0)-(es+2.5); EXP=max(1.0,exp_len/14.0)
segs.append((es+2.5, ed-3.0, EXP))
segs.append((ed-3.0, stop, 1.0))
m=[]
for a,b,s in segs:
    if b<=a: continue
    if m and m[-1][2]==s and abs(m[-1][1]-a)<1e-6: m[-1]=(m[-1][0],b,s)
    else: m.append((a,b,s))
out=0.0; parts=[]
for a,b,s in m:
    parts.append((a,b,s,out)); out+=(b-a)/s
expr='0'
for a,b,s,o in reversed(parts):
    expr=f"if(lt(T,{b:.4f}),{o:.4f}+(T-{a:.4f})/{s:.4f},{expr})"
json.dump({'segments':parts,'out_len':out,'export_speed':EXP,'fast':FAST},open(f'{R}/ramp.json','w'),indent=1)
print('segments',len(parts),'out_len',round(out,1),'export_speed',round(EXP,1))
open(f'{R}/ramp_expr.txt','w').write(expr)
