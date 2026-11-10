
"""Kimo reel build: every visual/audio decision is a Kimo MCP call. plan = dict from assemble.py."""
import time, json, math
CORAL=(1.0,0.36,0.157); IVORY=(0.957,0.941,0.910); INK=(0.02,0.02,0.03); YEL=(1.0,0.84,0.2); RUST=(0.87,0.33,0.16); GREEN=(0.45,0.95,0.6); DIM=(0.62,0.62,0.66)
SANS='Geist-Variable'; MONO='GeistMono-Variable'
class B:
    def __init__(s,k,log=None,stub=False):
        s.k=k; s.n=0; s.fail=[]; s.log=log; s.stub=stub; s.calls=[]; s.ids=0
    def c(s,tool,**a):
        s.n+=1; t0=time.time()
        if s.stub:
            s.ids+=1; i=str(s.ids); r={'clip_id':'c'+i,'track_id':'t'+i,'asset_id':'a'+i,'animator_id':'n'+i,'effect_id':'e'+i,'path':a.get('path','')}
        else:
            try: r=s.k.call(tool,timeout=2400 if tool=='export' else 300,**a)
            except Exception as e:
                s.fail.append((tool,str(e)[:300])); r=None
                if s.log: s.log.write(f"FAIL {tool} {json.dumps(a)[:300]} :: {e}\n"); s.log.flush()
        ms=(time.time()-t0)*1000; s.calls.append((tool,a,ms))
        if s.log: s.log.write(f"{time.time():.2f} {s.n} {tool} {ms:.0f}ms\n"); s.log.flush()
        return r if r is not None else {}
    def track(s,name):
        t=s.c('add_track')['track_id'] if True else None
        s.c('set_track',track_id=t,name=name); return t
    def key(s,cid,prop,pts):
        for t,v,e in pts: s.c('set_keyframe',clip_id=cid,property=prop,time=round(t,3),value=v,ease=e)
    def prop(s,cid,**kv):
        for p,v in kv.items(): s.c('set_property',clip_id=cid,property=p,value=v)
    def text(s,tr,txt,st,d,size,color=IVORY,font=SANS,weight=None,x=0,y=0,upper=False,spacing=None,**fx):
        a=dict(track_id=tr,text=txt,start=round(st,3),duration=round(d,3),font=font,size=size,color=list(color),align='center')
        if spacing is not None: a['letter_spacing']=spacing
        if x: a['x']=x
        cid=s.c('add_text',**a).get('clip_id')
        if weight or upper: s.c('set_text_weight',clip_id=cid,**({'weight':weight} if weight else {}),uppercase=upper)
        if fx: s.c('set_text_style',clip_id=cid,**fx)
        if y: s.prop(cid,y=y)
        return cid
    def shape(s,tr,kind,st,d,w,h,fill,x=0,y=0,radius=0,**kw):
        a=dict(track_id=tr,shape=kind,start=round(st,3),duration=round(d,3),width=w,height=h,fill=list(fill),**kw)
        if radius: a['corner_radius']=radius
        cid=s.c('add_shape',**a).get('clip_id')
        if x or y: s.prop(cid,**({'x':x} if x else {}),**({'y':y} if y else {}))
        return cid
    def pop(s,cid,t0,dur=0.28,s0=0.6,s1=1.0,over=1.08):
        s.key(cid,'scale',[(t0,s0,'out'),(t0+dur*0.65,s1*over,'in_out'),(t0+dur,s1,'linear')])
        s.key(cid,'opacity',[(t0,0,'linear'),(t0+dur*0.4,1,'linear')])
    def fadeout(s,cid,t,d=0.25): s.key(cid,'opacity',[(t,1,'linear'),(t+d,0,'linear')])

def gear_points(r_out,r_in,teeth,phase=0.0):
    pts=[]
    for i in range(teeth):
        a0=2*math.pi*i/teeth+phase; st=2*math.pi/teeth
        for f,r in [(0.0,r_in),(0.18,r_out),(0.5,r_out),(0.68,r_in)]:
            a=a0+f*st; pts.append([round(r*math.cos(a),2),round(r*math.sin(a),2)])
    return pts


def sec2_gui(b,P,tr,l4):
    """Spectator-style widgets driven during line 4: colour wheels (drive a real color_wheels grade on the preview), audio faders, timeline cuts."""
    T0=l4['T0']+0.45; T1=l4['T1']+0.1; D=T1-T0; X=270; cid=P['sec2_clip']; clip0=l4['T0']-l4.get('trans_in',0)
    card=b.shape(tr['monitor'],'rect',T0,D,480,470,(0.06,0.06,0.08,0.95),x=X,y=-655,radius=18,stroke_color=[1,1,1,0.10],stroke_width=2)
    b.pop(card,0,0.25,0.85)
    b.text(tr['monitor'],'COLOR · AUDIO',T0,D,24,DIM,font=MONO,weight=600,x=X,y=-860)
    cw=b.c('add_effect',clip_id=cid,kind='color_wheels',params={'lift_r':0,'lift_g':0,'lift_b':0,'gamma_r':1,'gamma_g':1,'gamma_b':1,'gain_r':1,'gain_g':1,'gain_b':1}).get('effect_id')
    wheels=[('lift',X-115,0.10,(-20,-26),{'lift_b':0.05,'lift_g':0.015,'lift_r':-0.02}),('gain',X+115,0.30,(26,-12),{'gain_r':1.12,'gain_g':1.03,'gain_b':0.9})]
    for name,wx,f,(dx,dy),params in wheels:
        wy=-745
        ring=b.shape(tr['monitor'],'ellipse',T0,D,150,150,(0.10,0.10,0.13,1),x=wx,y=wy,stroke_color=[1,1,1,0.35],stroke_width=3)
        for ang,col in [(90,(1,0.3,0.3)),(210,(0.3,1,0.4)),(330,(0.35,0.55,1))]:
            r=58; b.shape(tr['monitor'],'ellipse',T0,D,10,10,col,x=wx+r*math.cos(math.radians(ang)),y=wy-r*math.sin(math.radians(ang)))
        pk=b.shape(tr['monitor'],'ellipse',T0,D,22,22,IVORY,x=wx,y=wy,stroke_color=[0,0,0,0.6],stroke_width=2)
        ta=T0+D*f-T0
        b.key(pk,'x',[(ta,wx,'in_out'),(ta+0.5,wx+dx,'linear')]); b.key(pk,'y',[(ta,wy,'in_out'),(ta+0.5,wy+dy,'linear')])
        b.text(tr['monitor'],name,T0,D,24,IVORY,font=MONO,weight=500,x=wx,y=wy+98)
        if cw:
            lt=T0+D*f-clip0
            for p_,v in params.items():
                v0=0.0 if p_.startswith('lift') else 1.0
                b.c('set_keyframe',clip_id=cid,effect_id=cw,property=p_,time=round(lt,3),value=v0,ease='in_out')
                b.c('set_keyframe',clip_id=cid,effect_id=cw,property=p_,time=round(lt+0.5,3),value=v,ease='linear')
    for j,(name,f,k0,k1,val) in enumerate([('voice',0.50,-60,120,'-14 LUFS'),('music',0.64,90,-30,'-3.5 dB')]):
        y=-570+j*72
        b.text(tr['monitor'],name,T0,D,24,IVORY,font=MONO,weight=500,x=X-190,y=y)
        b.shape(tr['monitor'],'rect',T0,D,250,6,(1,1,1,0.18),x=X+5,y=y,radius=3)
        kn=b.shape(tr['monitor'],'rect',T0,D,18,34,CORAL if j==0 else (0.30,0.70,1.0),x=X+k0,y=y,radius=4)
        ta=D*f; b.key(kn,'x',[(ta,X+k0,'in_out'),(ta+0.4,X+k1,'linear')])
        rd=b.text(tr['monitor'],val,T0+ta+0.4,D-ta-0.4,22,GREEN,font=MONO,weight=600,x=X+185,y=y)
        b.key(rd,'opacity',[(0,0,'linear'),(0.15,1,'linear')])
    # mini timeline: lanes, clips popping in, a playhead, two blade cuts (whoosh + monitor punch on each)
    tl=b.shape(tr['monitor'],'rect',T0,D,480,260,(0.06,0.06,0.08,0.95),x=X,y=-270,radius=18,stroke_color=[1,1,1,0.10],stroke_width=2)
    b.pop(tl,0,0.25,0.85)
    b.text(tr['monitor'],'TIMELINE',T0,D,24,DIM,font=MONO,weight=600,x=X-150,y=-375)
    cols=[CORAL,(0.30,0.70,1.0),(0.6,0.45,1.0)]
    for lane in range(3):
        for c in range(3):
            w=120-lane*10; x=X-150+c*135+lane*12; y=-318+lane*56
            t=T0+0.1+0.12*(lane*3+c)
            r=b.shape(tr['monitor'],'rect',t,T1-t,w,44,cols[lane],x=x,y=y,radius=8)
            b.pop(r,0,0.18,0.3)
    ph=b.shape(tr['panel'],'rect',T0,D,4,200,IVORY,x=X-220,y=-262)
    b.key(ph,'x',[(0,X-220,'linear'),(D,X+220,'linear')])
    for f in (0.42,0.82):
        tc=T0+D*f
        bl=b.shape(tr['panel'],'rect',tc,0.35,6,230,YEL,x=X-220+440*f,y=-262)
        b.key(bl,'opacity',[(0,1,'linear'),(0.35,0,'linear')]); b.key(bl,'scale',[(0,1.4,'out'),(0.15,1,'linear')])
        P['hits_soft']=P.get('hits_soft',[])+[tc]
        lt=tc-clip0
        b.key(cid,'scale',[(lt-0.001,0.42,'hold'),(lt,0.455,'out'),(lt+0.2,0.42,'linear')])

def build(b,P):
    W,H=1080,1920
    L=P['lines']; total=P['total']; N_CALLS=P.get('n_calls',0)
    b.c('project_set',name='Kimo reel v2 — made by Kimo',width=W,height=H,fps=30)
    asset=b.c('import_media',path=P['input'])['asset_id']
    P['sfx']={n:(b.c('import_media',path=p).get('asset_id'),d) for n,(p,d) in P['sfx_files'].items()}
    tr={n:b.track(n) for n in ['bg','vA','vB','monitor','grade','panel','ticker','fx','fx2','caps','end','prog','shake']}
    sec2=[i for i,l in enumerate(L) if l['idx']==3]
    # ---------- backgrounds (section 2 monitor backdrop + end card)
    l4=L[3] if len(L)>3 else None
    if l4:
        bg=b.c('add_clip',track_id=tr['bg'],source_type='gradient',start=l4['T0']-0.3,duration=l4['T1']-l4['T0']+0.6,color=[0.03,0.03,0.05],color2=[0.10,0.05,0.04])['clip_id']
        b.c('add_effect',clip_id=bg,kind='grid',params={'size':54,'width':1,'r':1,'g':1,'b':1,'opacity':0.05})
        b.c('add_effect',clip_id=bg,kind='vignette',params={'amount':0.5,'softness':0.6})
    # ---------- picture: one media clip per line, alternating tracks, punch-in on each cut
    pics=[]
    for i,l in enumerate(L):
        if l['idx']==9: break   # end card replaces picture for "This is Kimo."
        pre=l.get('trans_in',0.0); nxt=L[i+1] if i+1<len(L) else None
        post=(nxt.get('trans_in',0.0) if nxt else 0.0)
        st=l['T0']-pre; en=(nxt['T0'] if nxt else total)+post
        src_in=max(0.0,l['in']-pre-(l['T0']-l['T0']))
        cid=b.c('add_clip',track_id=tr['vA' if i%2==0 else 'vB'],source_type='media',asset_id=asset,start=round(st,3),duration=round(en-st,3),in_point=round(src_in,3),fit='cover')['clip_id']
        b.c('set_audio',clip_id=cid,mute=True)
        b.c('add_effect',clip_id=cid,kind='sharpen',params={'amount':0.35,'radius':1.2})
        d=en-st
        if l['idx']==3:   # section 2: shrink into a monitor
            b.prop(cid,scale=0.42,x=-250,y=-470)
            b.c('add_effect',clip_id=cid,kind='drop_shadow',params={'dx':0,'dy':18,'blur':40,'opacity':0.6})
            b.key(cid,'scale',[(0,1.0,'out'),(0.45,0.42,'linear')])
            b.key(cid,'x',[(0,0,'out'),(0.45,-250,'linear')])
            b.key(cid,'y',[(0,-60,'out'),(0.45,-470,'linear')])
            P['sec2_clip']=cid
        else:
            z0=1.16 if l['idx'] in (0,1,2,5,7) else 1.10
            b.key(cid,'scale',[(pre,z0,'out'),(pre+0.22,1.07,'linear'),(d,1.10,'linear')])
            b.prop(cid,y=-60)
        if l.get('trans_kind'): b.c('add_transition',clip_id=cid,kind=l['trans_kind'],duration=pre,ease='in_out_cubic')
        pics.append(cid)
    # monitor frame around section-2 picture
    if l4:
        fr=b.shape(tr['monitor'],'rect',l4['T0']+0.45,l4['T1']-l4['T0']-0.45,462,814,(0,0,0,0),x=-250,y=-470,radius=14,stroke_color=list(CORAL),stroke_width=4)
        b.key(fr,'opacity',[(0,0,'linear'),(0.2,1,'linear')])
        lab=b.text(tr['monitor'],'● PREVIEW',l4['T0']+0.5,l4['T1']-l4['T0']-0.5,26,CORAL,font=MONO,weight=600,x=-250,y=-905)
        sec2_gui(b,P,tr,l4)
    # ---------- grade: adjustment layer over picture (not end card)
    g_end=L[9]['T0'] if len(L)>9 else total
    adj=b.c('add_adjustment_layer',track_id=tr['grade'],start=0,duration=round(g_end,3))['clip_id']
    for kind,pr in [('brightness_contrast',{'brightness':0.02,'contrast':0.12}),('saturation',{'amount':1.12}),('teal_orange',{'amount':0.28}),('vignette',{'amount':0.35,'softness':0.55}),('grain',{'amount':0.04})]:
        b.c('add_effect',clip_id=adj,kind=kind,params=pr)
    # ---------- section 2: tool-call ticker drawn by Kimo (real calls of this run)
    if l4:
        T0=l4['T0']+0.35; T1=l4['T1']+0.1
        pan=b.shape(tr['panel'],'rect',T0,T1-T0,980,700,(0.04,0.04,0.06,0.92),y=500,radius=24,stroke_color=[1,1,1,0.12],stroke_width=2)
        b.pop(pan,0,0.3,0.85)
        hdr=b.text(tr['panel'],'kimo-mcp  ·  tools/call  ·  stdio',T0,T1-T0,30,DIM,font=MONO,weight=500,y=192)
        rows=P['ticker_fn'](b); P['ticker']=rows; P['ticker_upto']=len(b.calls); n=len(rows); slots=11; lh=50; y0=250
        times=[T0+0.25+(T1-T0-0.6)*i/max(1,n-1) for i in range(n)]
        for i,(txt,ms) in enumerate(rows):
            t=times[i]; end=times[i+slots] if i+slots<n else T1
            col=CORAL if i==n-1 else IVORY
            cid=b.c('add_text',track_id=tr['ticker'],text=txt,start=round(t,3),duration=round(end-t,3),font=MONO,size=28,color=list(col),align='left',anchor='left',x=-450)['clip_id']
            b.c('set_text_weight',clip_id=cid,weight=500)
            s0=min(i,slots-1); b.prop(cid,y=y0+lh*s0)
            b.key(cid,'reveal',[(0,0,'linear'),(0.16,1,'linear')])
            for j in range(max(i+1,slots),min(n,i+slots)):
                sl=s0-(j-max(i,slots-1)); tt=round(times[j]-t,3)
                b.key(cid,'y',[(max(0.001,tt-0.001),y0+lh*(sl+1),'hold'),(tt+0.07,y0+lh*sl,'linear')])
        b.c('add_counter',track_id=tr['panel'],start=T0+0.2,duration=min(2.0,T1-T0-0.4),hold=max(0.0,T1-T0-2.6),**{'from':1,'to':P['ticker_upto']},prefix='calls: ',font=MONO,size=34,color=list(GREEN),x=300,y=840)
    # ---------- captions: exact script strings, word pop synced to the line
    for l in L:
        if l['idx']==9: continue
        dur=l['T1']-l['T0']+0.12
        cid=b.text(tr['caps'],l['caption'],l['T0'],dur,82 if len(l['caption'])<40 else 70,IVORY,weight=800,y=-600,
                   stroke_width=0) if False else None
        a=dict(track_id=tr['caps'],text=l['caption'],start=round(l['T0'],3),duration=round(dur,3),font=SANS,size=(62 if l['idx']==3 else 84 if len(l['caption'])<34 else 72),color=list(IVORY),align='center',stroke_color=[0,0,0],stroke_width=5)
        cid=b.c('add_text',**a)['clip_id']
        b.c('set_text_weight',clip_id=cid,weight=800)
        b.c('set_text_style',clip_id=cid,box_width=(980 if l['idx']==3 else 860),line_spacing=1.08,shadow_blur=18,shadow_opacity=0.7,shadow_angle=90,shadow_color=[0,0,0])
        b.prop(cid,y=(50 if l['idx']==3 else -600))
        b.c('add_text_animator',clip_id=cid,unit='word',preset='pop',duration=round(max(0.3,min(dur*0.7,l['speech']*0.85)),3))
    # ---------- section 3: proof graphics
    def wt(l,word,frac):
        return l['T0']+l['words'].get(word, frac*(l['T1']-l['T0']))
    if len(L)>4:   # L5: HUD
        l=L[4]; b.c('add_hud_frame',track_id=tr['fx'],start=round(l['T0'],3),duration=round(l['T1']-l['T0']+0.1,3),background=False,label='THIS REEL · MADE IN KIMO',color=list(IVORY),bpm=100,tc_start=0)
    if len(L)>5:   # L6: counter = real tool-call count, then ZERO CLICKS slam
        l=L[5]; t0=l['T0']; t1=l['T1']+0.1
        dim=b.shape(tr['fx'],'rect',t0,t1-t0,1080,1920,(0,0,0,0.55))
        b.key(dim,'opacity',[(0,0,'linear'),(0.25,1,'linear')])
        tc=wt(l,'two',0.3)
        b.c('add_counter',track_id=tr['fx2'],start=round(tc-0.15,3),duration=1.2,hold=round(max(0.1,t1-tc-1.05),3),**{'from':0,'to':N_CALLS},font=SANS,size=300,color=list(IVORY),y=120)
        tl=b.text(tr['fx2'],'TOOL CALLS',tc,t1-tc,72,CORAL,weight=800,y=330,spacing=8)
        b.pop(tl,0,0.3,0.7)
        tz=wt(l,'zero',0.8)
        z=b.text(tr['fx2'],'ZERO CLICKS',tz,t1-tz,118,YEL,weight=900,y=520,stroke_color=[0,0,0],stroke_width=6) if False else None
        z=b.c('add_text',track_id=tr['fx2'],text='ZERO CLICKS',start=round(tz,3),duration=round(t1-tz,3),font=SANS,size=120,color=list(YEL),align='center',stroke_color=[0,0,0],stroke_width=6)['clip_id']
        b.c('set_text_weight',clip_id=z,weight=900); b.prop(z,y=500)
        b.key(z,'scale',[(0,2.4,'out'),(0.16,0.94,'in_out'),(0.26,1.0,'linear')]); b.key(z,'opacity',[(0,0,'linear'),(0.06,1,'linear')])
        P['hits'].append(tz)
    if len(L)>6:   # L7: three chips
        l=L[6]; t1=l['T1']+0.1
        chips=[('COLOR GRADE','color',0.02,0),('MOTION GRAPHICS','motion',0.3,105),('SOUND MIX','sound',0.55,210)]
        for j,(txt,w,fr,yy) in enumerate(chips):
            tt=wt(l,w,fr)
            cid=b.c('add_text',track_id=tr['fx'],text=txt,start=round(tt,3),duration=round(t1-tt,3),font=SANS,size=64,color=list(INK),align='center')['clip_id']
            b.c('set_text_weight',clip_id=cid,weight=800)
            b.c('set_text_style',clip_id=cid,bg=[*([1.0,0.84,0.2] if j==0 else [0.957,0.941,0.910] if j==1 else [1.0,0.36,0.157]),1.0],bg_pad=22,bg_radius=18)
            b.prop(cid,y=400+yy,x=(-120 if j==0 else 90 if j==1 else -60))
            b.key(cid,'x',[(0,(-700 if j%2==0 else 700),'out'),(0.25,(-120 if j==0 else 90 if j==1 else -60),'linear')])
            P['whooshes'].append(tt-0.327+0.12)
        ta=wt(l,'all',0.8)
        al=b.c('add_text',track_id=tr['fx2'],text='ALL OF IT.',start=round(ta,3),duration=round(t1-ta,3),font=SANS,size=110,color=list(IVORY),align='center',stroke_color=[0,0,0],stroke_width=6)['clip_id']
        b.c('set_text_weight',clip_id=al,weight=900); b.prop(al,y=780); b.pop(al,0,0.25,1.6)
        # the grade itself visibly kicks in on "color grade"
        g=b.c('add_effect',clip_id=adj,kind='saturation',params={'amount':1.0}).get('effect_id')
        tg=wt(l,'color',0.02)
        if g:
            b.c('set_keyframe',clip_id=adj,effect_id=g,property='amount',time=round(tg,3),value=0.35,ease='hold')
            b.c('set_keyframe',clip_id=adj,effect_id=g,property='amount',time=round(tg+0.5,3),value=1.3,ease='out')
            b.c('set_keyframe',clip_id=adj,effect_id=g,property='amount',time=round(t1,3),value=1.0,ease='linear')
    if len(L)>7:   # L8: Rust badge
        l=L[7]; t1=l['T1']+0.12; tr_=wt(l,'rust',0.85)
        gear=b.c('add_path_shape',track_id=tr['fx'],start=round(tr_,3),duration=round(t1-tr_,3),points=gear_points(170,140,18),closed=True,fill=list(RUST),stroke_color=[0,0,0],stroke_width=4)['clip_id']
        hole=b.shape(tr['fx'],'ellipse',tr_,t1-tr_,210,210,INK,y=0)
        R=b.c('add_text',track_id=tr['fx2'],text='R',start=round(tr_,3),duration=round(t1-tr_,3),font=SANS,size=170,color=list(IVORY),align='center')['clip_id']
        b.c('set_text_weight',clip_id=R,weight=900)
        cap=b.text(tr['fx2'],'BUILT IN RUST',tr_,t1-tr_,60,IVORY,weight=800,spacing=6,y=250,stroke_color=[0,0,0],stroke_width=4) if False else None
        cap=b.c('add_text',track_id=tr['fx2'],text='BUILT IN RUST',start=round(tr_,3),duration=round(t1-tr_,3),font=SANS,size=60,color=list(IVORY),align='center',letter_spacing=6,stroke_color=[0,0,0],stroke_width=4)['clip_id']
        b.c('set_text_weight',clip_id=cap,weight=800)
        for cid,y in [(gear,520),(hole,520),(R,520),(cap,760)]:
            b.prop(cid,y=y); b.pop(cid,0,0.3,0.3,over=1.12)
        b.key(gear,'rotation',[(0,-60,'out'),(0.5,0,'linear'),(t1-tr_,12,'linear')])
        P['hits'].append(tr_)
    # ---------- end card: Kimo mark drawn from shapes (kimo-mark.svg geometry), high contrast
    if len(L)>9:
        l=L[9]; t0=l['T0']-0.05; d=total-t0
        bgc=b.c('add_clip',track_id=tr['end'],source_type='solid',start=round(t0,3),duration=round(d,3),color=[0.012,0.012,0.016])['clip_id']
        b.c('add_transition',clip_id=bgc,kind='flash',duration=0.25,ease='out')
        sc=0.6; cx=512; cy=520; y_mark=-300
        bars=[(148,200,624),(232,200,624),(316,300,524),(400,400,424),(484,500,324),(568,400,424),(652,300,524),(736,200,624),(820,200,624)]
        for j,(bx,by,bh) in enumerate(bars):
            x=(bx+28-cx)*sc; y=(by+bh/2+8-cy)*sc+y_mark
            cid=b.shape(tr['end'],'rect',t0+0.05+0.035*j,d-0.05-0.035*j,56*sc,bh*sc,IVORY,x=x,y=y,radius=14*sc)
            b.key(cid,'y',[(0,y+120,'out'),(0.35,y,'linear')]); b.key(cid,'opacity',[(0,0,'linear'),(0.15,1,'linear')])
        ln=b.shape(tr['end'],'rect',t0+0.4,d-0.4,12*sc,794*sc,CORAL,x=(512-cx)*sc,y=(110+397+8-cy)*sc+y_mark)
        b.key(ln,'opacity',[(0,0,'linear'),(0.12,1,'linear')])
        ar=b.c('add_path_shape',track_id=tr['end'],start=round(t0+0.4,3),duration=round(d-0.4,3),d=' '.join(f'{c}{(px-cx)*sc:.2f},{(py+8-cy)*sc+y_mark:.2f}' if c in 'ML' else c for c,px,py in [('M',480,96),('L',544,96),('L',544,104),('L',512,140),('L',480,104)] )+' Z',fill=list(CORAL))['clip_id']
        b.key(ar,'opacity',[(0,0,'linear'),(0.12,1,'linear')])
        km=b.c('add_text',track_id=tr['end'],text='KIMO',start=round(t0+0.25,3),duration=round(d-0.25,3),font=SANS,size=200,color=list(IVORY),align='center',letter_spacing=28)['clip_id']
        b.c('set_text_weight',clip_id=km,weight=900); b.prop(km,y=260)
        b.c('add_text_animator',clip_id=km,unit='char',preset='rise',duration=0.5)
        ti=b.c('add_text',track_id=tr['end'],text=l['caption'],start=round(l['T0'],3),duration=round(total-l['T0'],3),font=SANS,size=84,color=list(CORAL),align='center')['clip_id']
        b.c('set_text_weight',clip_id=ti,weight=800); b.prop(ti,y=430)
        b.c('add_text_animator',clip_id=ti,unit='word',preset='pop',duration=0.5)
        t2=l['T1']+0.3
        tag=b.c('add_text',track_id=tr['end'],text='The video editor that edits itself.',start=round(t2,3),duration=round(total-t2,3),font=SANS,size=50,color=list(IVORY),align='center')['clip_id']
        b.c('set_text_weight',clip_id=tag,weight=500); b.prop(tag,y=560); b.c('add_text_animator',clip_id=tag,unit='word',preset='blur_in',duration=0.6)
        st=b.c('add_text',track_id=tr['end'],text=f'{N_CALLS} tool calls · 0 clicks · built in Rust',start=round(t2+0.5,3),duration=round(total-t2-0.5,3),font=MONO,size=34,color=list(DIM),align='center')['clip_id']
        b.prop(st,y=650); b.key(st,'opacity',[(0,0,'linear'),(0.4,1,'linear')])
        P['hits'].append(t0+0.05)
    # ---------- progress bar (top) and impact shakes
    pb=b.shape(tr['prog'],'rect',0,total,1080,10,CORAL,y=-955)
    b.key(pb,'x',[(0,-1080,'linear'),(total,0,'linear')])
    hits=sorted(set(round(h,3) for h in P['hits']+[L[i]['T0'] for i in (1,2) if i<len(L)]))
    b.c('add_camera_shake',track_id=tr['shake'],start=0,duration=round(total,3),hits=hits,amplitude=22,amplitude_y=8,zoom=0.02)
    # ---------- audio: voice (exact take spans), SFX, ducked bed
    vtr=[None,None]; vclips=[]; XF=0.008
    for i,l in enumerate(L):
        nxt=L[i+1]['T0'] if i+1<len(L) else l['T1']+0.4
        a=dict(asset_id=asset,start=round(l['T0'],3),duration=round(nxt-l['T0']+XF,3),in_point=round(l['in'],3))
        if vtr[i%2]: a['track_id']=vtr[i%2]
        r=b.c('add_audio_clip',**a); vtr[i%2]=vtr[i%2] or r.get('track_id'); vclips.append(r.get('clip_id'))
    for cid in vclips:
        b.c('set_audio',clip_id=cid,highpass=80,fade_in=XF,fade_out=(0.3 if cid==vclips[-1] else XF))
        b.c('add_audio_effect',clip_id=cid,kind='denoise',params={'reduction_db':8,'floor_db':-50})
        b.c('add_audio_effect',clip_id=cid,kind='compressor',params={'threshold_db':-22,'ratio':3,'attack_ms':8,'release_ms':150,'makeup_db':2})
        b.c('add_audio_effect',clip_id=cid,kind='deesser',params={'intensity':0.35,'max':0.4,'frequency':0.55})
        b.c('add_audio_effect',clip_id=cid,kind='eq',params={'frequency':3500,'width':1.0,'gain_db':2.0})
        b.c('normalize_loudness',clip_id=cid,target_lufs=-14.0)
    sfx=P['sfx']; atr=None
    def place(name,t,vol):
        nonlocal atr
        if t<0: return
        a=dict(asset_id=sfx[name][0],start=round(t,3),duration=sfx[name][1],volume=vol)
        if atr: a['track_id']=atr
        r=b.c('add_audio_clip',**a); atr=atr or r.get('track_id')
    for t in P['whooshes']: place('whoosh1',t,0.5)
    for t in P['hits']: place('hit',t,0.55)
    for t in P.get('hits_soft',[]): place('whoosh2',t-0.436,0.45)
    if l4:
        for i in range(len(P['ticker'])):
            T0=l4['T0']+0.35; T1=l4['T1']+0.1; n=len(P['ticker'])
            place('tick',T0+0.25+(T1-T0-0.6)*i/max(1,n-1),0.22)
    if len(L)>5: place('riser',wt(L[5],'two',0.3)-1.1,0.35)
    bed=b.c('add_audio_clip',asset_id=sfx['bed'][0],start=0,duration=round(total,3),volume=P['bed_vol'])
    bed_clip=bed.get('clip_id'); bed_tr=bed.get('track_id')
    b.c('set_audio',clip_id=bed_clip,fade_in=0.4,fade_out=1.5)
    b.c('auto_duck',track=bed_tr,speech_windows=[[round(l['T0'],3),round(l['T1'],3)] for l in L],amount_db=-3.5,attack=0.05,release=0.25)
    m=b.c('measure_loudness',true_peak=True)
    P['mix_lufs']=m.get('integrated_lufs', m.get('lufs')) if isinstance(m,dict) else None
    return dict(vclips=vclips,bed=bed_clip,adj=adj)
