"""Whisper alignment pass: silero VAD chunks + faster-whisper small per chunk, word timestamps.
Usage: python asr.py <voice16k.wav> <out.json>. ASR text is used only for timing/take selection, never for captions."""

import json,sys,time,numpy as np
from faster_whisper import WhisperModel, decode_audio
from faster_whisper.vad import get_speech_timestamps, VadOptions
wav,out=sys.argv[1],sys.argv[2]
t=time.time(); a=decode_audio(wav,sampling_rate=16000)
ch=get_speech_timestamps(a,VadOptions(threshold=0.45,min_silence_duration_ms=280,speech_pad_ms=60,min_speech_duration_ms=200))
m=WhisperModel('small',device='cpu',compute_type='int8',cpu_threads=4)
R=[]
for c in ch:
    s,e=c['start']/16000,c['end']/16000
    seg=a[max(0,c['start']-1600):c['end']+1600]; off=max(0,c['start']-1600)/16000
    res=[]
    for lang in ['en']:
        ss,info=m.transcribe(seg,language=lang,word_timestamps=True,beam_size=5,initial_prompt='Kimo, an AI video editor built in Rust.',condition_on_previous_text=False,vad_filter=False)
        ss=list(ss)
        words=[dict(w=w.word,s=w.start+off,e=w.end+off,p=w.probability) for x in ss for w in x.words]
        lp=float(np.mean([x.avg_logprob for x in ss])) if ss else -9
        res.append(dict(lang=lang,text=' '.join(x.text.strip() for x in ss),lp=lp,words=words))
    R.append(dict(start=s,end=e,asr=res))
    print(f'{s:6.2f}-{e:6.2f} {res[0]["lp"]:.2f} {res[0]["text"]}',flush=True)
json.dump(dict(chunks=R,secs=time.time()-t),open(out,'w'),indent=1)
print('done',time.time()-t)
