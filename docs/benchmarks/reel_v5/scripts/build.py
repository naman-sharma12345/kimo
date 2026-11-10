
import time, json
class B:
    def __init__(s, k, W, H, fps=30, pace=0.0, log=None):
        s.k=k; s.W=W; s.H=H; s.u=H/1080.0; s.pace=pace; s.n=0; s.fail=[]; s.log=log; s.fps=fps
    def c(s, tool, **a):
        s.n+=1
        try:
            r=s.k.call(tool, **a)
        except Exception as e:
            s.fail.append((tool, str(e)[:200])); r=None
            if s.log: s.log.write(f"FAIL {tool} {e}\n")
        if s.log: s.log.write(f"{time.time():.2f} {tool}\n"); s.log.flush()
        if s.pace: time.sleep(s.pace)
        return r
    def P(s,v): return v*s.u
    def key(s, cid, prop, pts):
        for t,v,e in pts: s.c("set_keyframe", clip_id=cid, property=prop, time=t, value=v, ease=e)
    def text(s, tr, txt, st, d, font, size, color=(1,1,1), x=0, y=0, weight=None, upper=False, **fx):
        r=s.c("add_text", track_id=tr, text=txt, start=st, duration=d, font=font, size=s.P(size), color=list(color), align="center")
        cid=r["clip_id"]
        if weight or upper: s.c("set_text_weight", clip_id=cid, **({"weight":weight} if weight else {}), uppercase=upper)
        if fx: s.c("set_text_style", clip_id=cid, **{k:(s.P(v) if k in("glow","shadow_blur","box_width","bg_pad","bg_radius") else v) for k,v in fx.items()})
        if x: s.c("set_property", clip_id=cid, property="x", value=s.P(x))
        if y: s.c("set_property", clip_id=cid, property="y", value=s.P(y))
        return cid
    def anim(s, cid, unit, sm, t0, t1, ease="out", **p):
        r=s.c("add_text_animator", clip_id=cid, unit=unit, smoothness=sm, **{k:(s.P(v) if k in("x","y","blur","tracking") else v) for k,v in p.items()})
        aid=r["animator_id"]; hi=1+2*max(sm,0.05)
        s.c("set_animator_keyframe", clip_id=cid, animator_id=aid, prop="offset", time=t0, value=0, ease=ease)
        s.c("set_animator_keyframe", clip_id=cid, animator_id=aid, prop="offset", time=t1, value=hi, ease=ease); return aid
    def shape(s, tr, st, d, w, h, fill, x=0, y=0, kind="rect", blend=None, radius=0, **kw):
        r=s.c("add_shape", track_id=tr, shape=kind, start=st, duration=d, width=s.P(w), height=s.P(h), fill=list(fill), corner_radius=s.P(radius), **kw)
        cid=r["clip_id"]
        if x: s.c("set_property", clip_id=cid, property="x", value=s.P(x))
        if y: s.c("set_property", clip_id=cid, property="y", value=s.P(y))
        if blend: s.c("set_blend", clip_id=cid, mode=blend)
        return cid
    def fade(s, cid, tin, tout, din=0.3, dout=0.35):
        s.key(cid,"opacity",[(tin,0,"out"),(tin+din,1,"linear"),(tout,1,"in"),(tout+dout,0,"linear")])

CORAL=(1.0,0.38,0.26); CYAN=(0.30,0.86,1.0); VIOLET=(0.56,0.36,1.0); IVORY=(0.96,0.95,0.98); INK=(0.03,0.03,0.07)

AUDIO='/tmp/reel/bed.wav'; SCOPE_ARGS={'time':21.5,'scope':'waveform','path':'/tmp/reel/scope_wave.png'}; EQ={'frequency':3200,'width':1.2,'gain_db':2.0}; COMP={'threshold_db':-18,'ratio':2.5,'attack_ms':12,'release_ms':180,'makeup_db':1.5}
def build(b, out_path, crf=20, total=25.0):
    u=b.u; P=b.P
    b.c("project_set", name="Kimo v5 — teaser", width=b.W, height=b.H, fps=b.fps)
    tr={}
    for n in ["bgA","bgB","glow","grid","shapes","text","text2","fx","grade"]:
        tr[n]=b.c("add_track")["track_id"]
    # ---- backgrounds: one gradient per shot, alternating tracks, overlapping 0.5s for transitions
    shots=[(0,4.0,(0.02,0.02,0.06),(0.10,0.05,0.22)),(4.0,3.5,(0.03,0.02,0.05),(0.20,0.05,0.12)),
           (7.5,4.0,(0.02,0.02,0.04),(0.06,0.04,0.16)),(11.5,4.0,(0.01,0.03,0.06),(0.03,0.12,0.20)),
           (15.5,4.0,(0.02,0.02,0.05),(0.12,0.06,0.25)),(19.5,5.5,(0.01,0.01,0.03),(0.09,0.05,0.20))]
    trans=[None,("zoom_blur",0.45),("whip_left",0.4),("glitch",0.4),("light_leak",0.6),("flash",0.35)]
    bgs=[]
    for i,(st,d,c1,c2) in enumerate(shots):
        s0=max(0,st-(0.5 if i else 0)); dd=d+(st-s0)
        cid=b.c("add_clip", track_id=tr["bgA" if i%2==0 else "bgB"], source_type="gradient", start=s0, duration=dd, color=list(c1), color2=list(c2))["clip_id"]
        b.c("add_effect", clip_id=cid, kind="fractal_noise", params={"scale":420*u,"complexity":5,"speed":0.25,"contrast":1.2,"brightness":-0.25,"opacity":0.35,"mode":1})
        b.key(cid,"scale",[(0,1.0,"linear"),(dd,1.08,"linear")])
        if trans[i]: b.c("add_transition", clip_id=cid, kind=trans[i][0], duration=trans[i][1], ease="in_out_cubic")
        bgs.append(cid)
    # ---- drifting light blobs across the whole piece
    for (col,x0,x1,y,op,w) in [(VIOLET,-650,-250,-200,0.55,1100),(CORAL,700,300,260,0.35,900),(CYAN,0,350,-380,0.22,800)]:
        cid=b.shape(tr["glow"],0,total,w,w*0.7,col,x=x0,y=y,kind="ellipse",blend="add")
        b.c("add_effect", clip_id=cid, kind="blur", params={"radius":P(230)})
        b.key(cid,"x",[(0,P(x0),"in_out"),(total,P(x1),"linear")])
        b.key(cid,"opacity",[(0,0,"linear"),(1.2,op,"linear"),(total-1.0,op,"in"),(total,0,"linear")])
    # ======== SHOT 1: EVERY FRAME (0-4)
    o=0.0; D=4.0
    for yy,dl in [(-120,0.2),(120,0.35)]:
        r=b.shape(tr["shapes"],o,D,620,2,IVORY,y=yy)
        b.key(r,"scale",[(dl,0.0001,"out_expo"),(dl+1.3,1,"linear")])
        b.key(r,"opacity",[(0,0,"linear"),(dl,0.7,"linear"),(3.3,0.7,"in"),(3.7,0,"linear")])
    dot=b.shape(tr["shapes"],o,D,18,18,CORAL,y=-120,kind="ellipse")
    b.c("add_effect", clip_id=dot, kind="outer_glow", params={"radius":P(24),"amount":1.6,"r":1,"g":0.4,"b":0.3})
    b.key(dot,"x",[(0.2,P(-310),"in_out_quint"),(1.6,P(310),"linear")])
    b.fade(dot,0.2,3.3,0.2,0.3)
    t=b.text(tr["text"],"EVERY FRAME",o,D,"InterDisplay-Thin",150,IVORY,upper=True,shadow_blur=30,shadow_opacity=0.5,shadow_angle=90)
    b.c("set_motion_blur", clip_id=t, shutter_angle=180)
    a=b.c("add_text_animator", clip_id=t, unit="char", smoothness=0, tracking=P(60))["animator_id"]
    b.c("set_animator_keyframe", clip_id=t, animator_id=a, prop="tracking", time=0.2, value=P(60), ease="out")
    b.c("set_animator_keyframe", clip_id=t, animator_id=a, prop="tracking", time=3.6, value=P(14), ease="linear")
    b.anim(t,"char",0.6,0.3,1.8,"out",opacity=0,blur=22,y=40)
    b.key(t,"scale",[(0,1.0,"linear"),(D,1.05,"linear")])
    b.key(t,"opacity",[(3.25,1,"in"),(3.7,0,"linear")])
    sw=b.shape(tr["fx"],o,D,300,2200,(1,0.95,0.9),rotation=0,blend="add") if False else b.shape(tr["fx"],o,D,300,2200,(1,0.95,0.9),blend="add")
    b.c("set_property", clip_id=sw, property="rotation", value=16)
    b.c("add_effect", clip_id=sw, kind="blur", params={"radius":P(200)})
    b.key(sw,"x",[(1.0,P(-1500),"in_out_quint"),(2.6,P(1500),"linear")])
    b.key(sw,"opacity",[(0,0.0,"linear"),(1.0,0.0,"linear"),(1.4,0.22,"linear"),(2.2,0.22,"linear"),(2.6,0,"linear")])
    # ======== SHOT 2: IS A DECISION. (4-7.5)
    o=4.0; D=3.5
    t1=b.text(tr["text"],"IS A",o,D,"InterDisplay-Light",90,(0.8,0.8,0.88),y=-150,upper=True,line_mask=True)
    b.anim(t1,"line",0.0,0.0,0.6,"out",y=110,opacity=0)
    b.fade(t1,0.0,3.0,0.05,0.35)
    t2=b.text(tr["text2"],"DECISION.",o,D,"InterDisplay-Black",260,IVORY,y=40,upper=True,line_mask=True,shadow_blur=40,shadow_opacity=0.6,shadow_angle=90)
    b.c("set_motion_blur", clip_id=t2, shutter_angle=220)
    b.anim(t2,"char",0.4,0.15,0.95,"out",y=280,opacity=0)
    b.key(t2,"scale",[(0.15,1.12,"out_expo"),(1.2,1.0,"linear"),(D,0.97,"linear")])
    b.key(t2,"opacity",[(3.0,1,"in"),(3.35,0,"linear")])
    bar=b.shape(tr["shapes"],o,D,900,14,CORAL,y=200)
    b.key(bar,"scale",[(0.55,0.0001,"out_expo"),(1.3,1,"linear"),(2.9,1,"in_expo"),(3.3,0.0001,"linear")])
    b.c("add_effect", clip_id=bar, kind="outer_glow", params={"radius":P(30),"amount":1.4,"r":1,"g":0.38,"b":0.26})
    # ======== SHOT 3: MAKE IT MOVE (7.5-11.5)
    o=7.5; D=4.0
    words=[("MAKE",VIOLET,IVORY,-270),("IT",IVORY,INK,0),("MOVE",CORAL,IVORY,270)]
    for i,(w,bc,tc,y) in enumerate(words):
        t0=0.2+i*0.25
        bx=b.shape(tr["shapes"],o,D,2300,250,bc,y=y)
        b.key(bx,"x",[(t0-0.05,P(-2300),"out_expo"),(t0+0.55,0,"linear"),(3.2+i*0.08,0,"in_expo"),(3.65+i*0.08,P(2400),"linear")])
        b.c("set_motion_blur", clip_id=bx, shutter_angle=240)
        tc_=b.text(tr["text"],w,o,D,"InterDisplay-Black",300,tc,y=y-6,upper=True)
        b.c("set_motion_blur", clip_id=tc_, shutter_angle=240)
        b.key(tc_,"opacity",[(0,0,"linear"),(t0+0.12,0,"linear"),(t0+0.2,1,"linear"),(3.2+i*0.08,1,"linear"),(3.3+i*0.08,0,"linear")])
        b.key(tc_,"scale",[(t0+0.1,1.6,"out_expo"),(t0+0.7,1.0,"linear")])
        b.key(tc_,"x",[(t0+0.1,P(-380),"out_expo"),(t0+0.7,0,"linear")])
    # ======== SHOT 4: FEATURE MONTAGE (11.5-15.5)
    o=11.5; D=4.0
    D4=D
    g=b.c("add_clip", track_id=tr["grid"], source_type="solid", start=o, duration=D+2.0, color=[0,0,0])["clip_id"]
    b.c("add_effect", clip_id=g, kind="grid", params={"size":P(90),"width":max(1,P(1.5)),"r":0.6,"g":0.8,"b":1.0,"opacity":0.16})
    b.c("set_blend", clip_id=g, mode="screen")
    b.key(g,"scale",[(0,1.25,"out"),(D,1.0,"linear")]); b.fade(g,0.0,3.6,0.4,0.35)
    ring=b.shape(tr["shapes"],o,D,620,620,(0,0,0,0),kind="polygon",sides=6,stroke_color=list(CYAN)+[1.0],stroke_width=P(4))
    b.c("add_effect", clip_id=ring, kind="glow", params={"amount":1.2,"radius":P(18),"threshold":0.3})
    b.key(ring,"rotation",[(0,-30,"linear"),(D,60,"linear")])
    b.key(ring,"scale",[(0,0.2,"out_back"),(0.6,1.0,"linear"),(3.4,1.0,"in_expo"),(3.9,2.4,"linear")])
    b.fade(ring,0.0,3.4,0.25,0.45)
    ring2=b.shape(tr["shapes"],o,D,760,760,(0,0,0,0),kind="ellipse",stroke_color=list(CORAL)+[0.8],stroke_width=P(2))
    b.key(ring2,"scale",[(0.15,0.3,"out_expo"),(0.9,1.0,"linear"),(3.4,1.0,"in_expo"),(3.9,2.0,"linear")])
    b.fade(ring2,0.15,3.4,0.25,0.45)
    for j,ang in enumerate([0,120,240]):
        d_=b.shape(tr["shapes"],o,D,22,22,[CORAL,CYAN,VIOLET][j],kind="ellipse",blend="add")
        import math
        pts=[(tt*0.25, ang+tt*0.25*90) for tt in range(17)]
        b.key(d_,"x",[(tt,P(380*math.cos(math.radians(a_))),"linear") for tt,a_ in pts[::4]])
        b.key(d_,"y",[(tt,P(380*math.sin(math.radians(a_))),"linear") for tt,a_ in pts[::4]])
        b.fade(d_,0.3,3.4,0.3,0.3)
    for i,w in enumerate(["TEXT","SHAPE","LIGHT","COLOR"]):
        st=o+0.3+i*0.85
        tw=b.text(tr["text"],w,st,0.85,"InterDisplay-ExtraBold",170,IVORY if i!=3 else CORAL,upper=True)
        b.key(tw,"scale",[(0,1.35,"out_expo"),(0.35,1.0,"linear"),(0.85,0.96,"linear")])
        b.key(tw,"opacity",[(0,0,"linear"),(0.06,1,"linear"),(0.75,1,"linear"),(0.85,0,"linear")])
        b.c("set_motion_blur", clip_id=tw, shutter_angle=200)
    # ======== SHOT 5: EVERY CALL. LIVE. (15.5-19.5)
    o=15.5; D=4.0
    for i in range(5):
        rw=b.shape(tr["shapes"],o,D,520-i*60,26,IVORY if i else CORAL,x=-560+(i*30),y=-170+i*58,radius=13)
        b.c("set_property", clip_id=rw, property="opacity", value=0.0)
        b.key(rw,"opacity",[(0.15+i*0.12,0,"out"),(0.3+i*0.12,0.85 if i==0 else 0.18,"linear"),(3.4,0.85 if i==0 else 0.18,"in"),(3.7,0,"linear")])
        b.key(rw,"x",[(0.15+i*0.12,P(-760),"out_expo"),(0.7+i*0.12,P(-560+i*30),"linear")])
    t5=b.text(tr["text"],"EVERY CALL.\nLIVE.",o,D,"InterDisplay-Black",150,IVORY,x=260,y=-30,upper=True,line_mask=True,line_spacing=0.95)
    b.anim(t5,"line",0.2,0.25,1.1,"out",y=170,opacity=0)
    b.key(t5,"opacity",[(3.35,1,"in"),(3.7,0,"linear")])
    t6=b.text(tr["text2"],"driven entirely through MCP",o,D,"Inter-Medium",44,(0.75,0.85,1.0),x=260,y=160,line_mask=True)
    b.anim(t6,"char",0.5,0.9,1.9,"out",opacity=0,blur=12,x=30)
    b.key(t6,"opacity",[(3.35,1,"in"),(3.7,0,"linear")])
    # ======== SHOT 6: LOGO (19.5-25)
    o=19.5; D=5.5
    halo=b.shape(tr["shapes"],o,D,1300,520,VIOLET,kind="ellipse",blend="add")
    b.c("add_effect", clip_id=halo, kind="blur", params={"radius":P(220)})
    b.key(halo,"opacity",[(0,0,"linear"),(0.6,0.0,"linear"),(1.4,0.55,"out"),(D-0.8,0.55,"in"),(D,0,"linear")])
    b.key(halo,"scale",[(0.6,0.6,"out_expo"),(2.2,1.0,"linear")])
    logo=b.text(tr["text"],"KIMO",o,D,"InterDisplay-Black",360,IVORY,upper=True,glow=16,glow_color=[0.6,0.45,1.0],gradient_top=[1,1,1],gradient_bottom=[0.78,0.74,0.95])
    b.c("set_motion_blur", clip_id=logo, shutter_angle=220)
    a=b.c("add_text_animator", clip_id=logo, unit="char", smoothness=0, tracking=P(140))["animator_id"]
    b.c("set_animator_keyframe", clip_id=logo, animator_id=a, prop="tracking", time=0.3, value=P(140), ease="out")
    b.c("set_animator_keyframe", clip_id=logo, animator_id=a, prop="tracking", time=1.6, value=P(10), ease="linear")
    b.anim(logo,"char",0.5,0.3,1.3,"out",opacity=0,blur=30,scale=1.6)
    b.key(logo,"scale",[(0,1.0,"linear"),(D,1.06,"linear")])
    b.key(logo,"opacity",[(D-0.7,1,"in"),(D,0,"linear")])
    badge=b.shape(tr["shapes"],o,D,170,96,CORAL,x=520,y=-140,radius=20)
    b.c("add_effect", clip_id=badge, kind="outer_glow", params={"radius":P(26),"amount":1.2,"r":1,"g":0.4,"b":0.3})
    b.key(badge,"scale",[(1.2,0.0001,"out_back"),(1.65,1,"linear")])
    b.key(badge,"rotation",[(1.2,-25,"out_back"),(1.65,-6,"linear")])
    b.key(badge,"opacity",[(0,0,"hold"),(1.2,1,"linear"),(D-0.7,1,"in"),(D,0,"linear")])
    v5=b.text(tr["text2"],"v5",o,D,"InterDisplay-Black",72,INK,x=520,y=-140)
    b.key(v5,"scale",[(1.25,0.0001,"out_back"),(1.7,1,"linear")])
    b.key(v5,"rotation",[(1.25,-25,"out_back"),(1.7,-6,"linear")])
    b.key(v5,"opacity",[(0,0,"hold"),(1.25,1,"linear"),(D-0.7,1,"in"),(D,0,"linear")])
    sub=b.text(tr["text2"],"THE VIDEO ENGINE YOUR AGENTS DRIVE",o,D,"Inter-SemiBold",44,(0.82,0.82,0.9),y=230,letter_spacing=P(10))
    b.anim(sub,"char",0.6,1.5,2.6,"out",opacity=0,blur=10,y=20)
    b.key(sub,"opacity",[(D-0.7,1,"in"),(D,0,"linear")])
    rule=b.shape(tr["shapes"],o,D,360,3,CORAL,y=170)
    b.key(rule,"scale",[(1.4,0.0001,"out_expo"),(2.2,1,"linear")]); b.key(rule,"opacity",[(D-0.7,1,"in"),(D,0,"linear")])
    sp=b.c("add_particles", track_id=tr["fx"], kind="sparks", start=o+0.5, duration=D-0.5, count=140, color=[1.0,0.7,0.5], size=P(2.4), seed=11)["clip_id"]
    b.c("set_blend", clip_id=sp, mode="add"); b.c("set_property", clip_id=sp, property="opacity", value=0.6)
    ds=b.c("add_particles", track_id=tr["glow"], kind="dust", start=0, duration=total, count=90, color=[0.8,0.85,1.0], size=P(1.6), seed=7)["clip_id"]
    b.c("set_blend", clip_id=ds, mode="screen"); b.c("set_property", clip_id=ds, property="opacity", value=0.45)
    # ---- edit pass: real timeline cuts (trim, split, move)
    b.c("trim_clip", clip_id=g, duration=D4)
    sp_r=b.c("split_clip", clip_id=ds, at=19.5)
    b.c("move_clip", clip_id=sp, start=19.5+0.35)
    # ---- camera kick on the logo impact
    b.c("add_camera_shake", track_id=tr["fx"], start=19.5, duration=1.0, hits=[19.8], amplitude=14)
    # ---- grading pass: adjustment layer + grade nodes + curve
    adj=b.c("add_adjustment_layer", track_id=tr["grade"], start=0, duration=total)["clip_id"]
    b.c("add_node", clip_id=adj, node_id="balance", kind="grade", effect_kind="color_wheels", params={"lift_b":0.02,"lift_r":-0.005,"gamma_r":1.0,"gamma_b":1.03,"gain_r":1.05,"gain_g":1.0,"gain_b":0.97})
    b.c("set_grade_output", clip_id=adj, node_id="balance")
    b.c("set_color_curve", clip_id=adj, channel="m", points=[[0,0],[0.25,0.2],[0.75,0.82],[1,1]])
    b.c("add_effect", clip_id=adj, kind="bloom", params={"amount":0.45,"threshold":0.72})
    b.c("add_effect", clip_id=adj, kind="anamorphic_streak", params={"threshold":0.85,"length":P(160),"amount":0.5})
    b.c("add_effect", clip_id=adj, kind="chromatic_aberration", params={"amount":P(2.5)})
    b.c("add_effect", clip_id=adj, kind="vignette", params={"amount":0.45,"softness":0.6})
    b.c("add_effect", clip_id=adj, kind="grain", params={"amount":0.035})
    b.c("get_scopes", time=21.5)
    b.c("render_scope", **SCOPE_ARGS) if SCOPE_ARGS else None
    # ---- audio bed + mix
    aid=b.c("import_media", path=AUDIO)["asset_id"]
    ac=b.c("add_audio_clip", asset_id=aid, start=0, duration=total, volume=0.9)["clip_id"]
    b.c("set_audio", clip_id=ac, fade_in=0.6, fade_out=1.6, highpass=30)
    b.c("add_audio_effect", clip_id=ac, kind="eq", params=EQ)
    b.c("add_audio_effect", clip_id=ac, kind="compressor", params=COMP)
    b.c("set_audio_pan", clip_id=ac, pan=-0.05)
    b.c("set_audio_volume_key", clip_id=ac, time=0, value=0.9)
    b.c("set_audio_volume_key", clip_id=ac, time=19.4, value=0.85, ease="in_out")
    b.c("set_audio_volume_key", clip_id=ac, time=19.8, value=1.0)
    b.c("measure_loudness", clip_id=ac)
    b.c("normalize_loudness", clip_id=ac, target_lufs=-17)
    b.c("measure_loudness", true_peak=True)
    return tr
