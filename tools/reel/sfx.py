"""Synthesize SFX + music bed as 48k stereo wav with numpy (no external assets)."""
import numpy as np, wave, sys
SR=48000
def wr(path,x):
    x=np.asarray(x,dtype=np.float64)
    if x.ndim==1: x=np.stack([x,x],1)
    x=np.clip(x,-1,1); d=(x*32767).astype('<i2')
    with wave.open(path,'wb') as w: w.setnchannels(2); w.setsampwidth(2); w.setframerate(SR); w.writeframes(d.tobytes())
def onepole_lp(x,fc):
    # time-varying one-pole lowpass, fc array
    a=np.exp(-2*np.pi*np.asarray(fc)/SR); y=np.zeros_like(x); s=0.0
    if np.ndim(a)==0: a=np.full(len(x),a)
    for i in range(len(x)): s=a[i]*s+(1-a[i])*x[i]; y[i]=s
    return y
def whoosh(dur=0.5,f0=300,f1=6000,seed=1,peak=0.7):
    r=np.random.default_rng(seed); n=int(dur*SR); t=np.arange(n)/n
    nz=r.standard_normal(n)
    fc=f0*(f1/f0)**(np.sin(np.pi*t*0.5)**1.5)   # sweep up
    lp=onepole_lp(nz,fc); hp=lp-onepole_lp(lp,f0*0.6)
    env=(t**1.6)*(1-t)**0.6; env/=env.max()
    x=hp*env; x/=np.abs(x).max()+1e-9
    pan=np.sin(np.pi*t)*0.6; L=x*(1-pan*0.5); R=x*(0.5+pan*0.5)
    return np.stack([L,R],1)*peak
def hit(dur=0.6,seed=2,peak=0.9):
    r=np.random.default_rng(seed); n=int(dur*SR); tt=np.arange(n)/SR
    f=45+110*np.exp(-tt*28); ph=2*np.pi*np.cumsum(f)/SR
    body=np.sin(ph)*np.exp(-tt*7)
    click=onepole_lp(r.standard_normal(n),5000)*np.exp(-tt*60)*0.8
    x=body+click; x/=np.abs(x).max(); return x*peak
def tick(dur=0.05,f=2400,peak=0.5):
    n=int(dur*SR); tt=np.arange(n)/SR
    return np.sin(2*np.pi*f*tt)*np.exp(-tt*120)*peak
def riser(dur=1.2,seed=3,peak=0.6):
    r=np.random.default_rng(seed); n=int(dur*SR); t=np.arange(n)/n
    nz=onepole_lp(r.standard_normal(n),800+9000*t**2)
    tone=np.sin(2*np.pi*np.cumsum(200+800*t**2)/SR)*0.3
    x=(nz+tone)*(t**2); x/=np.abs(x).max(); return x*peak
def bed(dur=40.0,bpm=100,seed=4,peak=0.5):
    n=int(dur*SR); tt=np.arange(n)/SR; beat=60/bpm
    chords=[[110,164.81,220,277.18],[98,146.83,196,246.94],[87.31,130.81,174.61,220],[98,146.83,196,261.63]]
    x=np.zeros(n)
    bar=beat*4
    for i,c in enumerate(chords*int(dur/bar/4+2)):
        s=int(i*bar*SR); e=min(n,int((i+1)*bar*SR)+int(0.3*SR))
        if s>=n: break
        seg=tt[s:e]-tt[s]
        env=np.minimum(1,seg/0.4)*np.exp(-np.maximum(0,seg-bar)*6)
        for j,f in enumerate(c):
            x[s:e]+=env*(np.sin(2*np.pi*f*seg)+0.35*np.sin(2*np.pi*2*f*seg+j))*0.18
    # sidechain-ish pulse + soft kick on every beat, hat on offbeats
    ph=(tt%beat)/beat; pump=0.55+0.45*np.minimum(1,ph*3)
    x*=pump
    kt=tt%beat; kick=np.sin(2*np.pi*(50+90*np.exp(-kt*30))*kt)*np.exp(-kt*9)*0.6
    r=np.random.default_rng(seed); hat=onepole_lp(r.standard_normal(n),9000); hat=hat-onepole_lp(hat,5000)
    ht=(tt+beat/2)%beat; hat*=np.exp(-ht*40)*0.25
    y=x+kick+hat
    fade=np.minimum(1,tt/1.0)*np.minimum(1,(dur-tt)/1.5)
    y*=fade; y/=np.abs(y).max(); return y*peak
if __name__=='__main__':
    out=sys.argv[1] if len(sys.argv)>1 else 'sfx'
    dur=float(sys.argv[2]) if len(sys.argv)>2 else 40
    wr(f'{out}/whoosh1.wav',whoosh(0.45,300,7000,1)); wr(f'{out}/whoosh2.wav',whoosh(0.6,200,5000,7))
    wr(f'{out}/hit.wav',hit()); wr(f'{out}/tick.wav',tick()); wr(f'{out}/riser.wav',riser())
    wr(f'{out}/bed.wav',bed(dur))
    print('ok')
