"""Motion-graphics recipes built only from kimo MCP tools (real easing, staggers, motion blur, restrained light)."""
import math
class MG:
    def __init__(s, k, W, H, fps=24): s.off=0; s.k=k; s.W=W; s.H=H; s.u=H/1080.0; s.fps=fps
    def key(s, c, prop, pts):
        for t,v,e in pts: s.k.call("set_keyframe", clip_id=c, property=prop, time=t, value=v, ease=e)
    def clip_text(s, tr, text, start, dur, font, size, color=(1,1,1), align="center", weight=None, upper=False, **fx):
        c=s.k.call("add_text", track_id=tr, text=text, start=start, duration=dur, font=font, size=size*s.u, color=list(color), align=align)["clip_id"]
        if weight or upper or fx: s.k.call("set_text_weight", clip_id=c, **({"weight":weight} if weight else {}), uppercase=upper)
        if fx: s.k.call("set_text_style", clip_id=c, **fx)
        return c
    def animator(s, c, unit, sm, t0, t1, ease, **p):
        r=s.k.call("add_text_animator", clip_id=c, unit=unit, smoothness=sm, **{k:(v*s.u if k in("x","y","blur") else v) for k,v in p.items()}); aid=r["animator_id"]
        hi=1+2*max(sm,0.05)
        s.k.call("set_animator_keyframe", clip_id=c, animator_id=aid, prop="offset", time=t0, value=0, ease=ease)
        s.k.call("set_animator_keyframe", clip_id=c, animator_id=aid, prop="offset", time=t1, value=hi, ease=ease); return aid
    def rect(s, tr, start, dur, w, h, color, x=0, y=0, rot=0, blend="normal", radius=0, shape="rect"):
        c=s.k.call("add_shape", track_id=tr, shape=shape, start=start, duration=dur, width=w*s.u, height=h*s.u, fill=list(color), corner_radius=radius*s.u)["clip_id"]
        s.k.call("set_property", clip_id=c, property="x", value=x*s.u); s.k.call("set_property", clip_id=c, property="y", value=y*s.u)
        if rot: s.k.call("set_property", clip_id=c, property="rotation", value=rot)
        if blend!="normal": s.k.call("set_blend", clip_id=c, mode=blend)
        return c
    def fade(s, c, t_in, t_out, dur, d_in=0.3, d_out=0.4, e_in="out", e_out="in"):
        s.k.call("set_property", clip_id=c, property="opacity", value=0)
        s.key(c,"opacity",[(t_in,0,e_in),(t_in+d_in,1,"linear"),(t_out,1,e_out),(t_out+d_out,0,"linear")])
    def grow_x(s, c, w, x0, t0, t1, ease="out_expo", anchor="center"):
        """scale a rect from 0 to 1 about its left/right/center edge (uniform scale + x compensation) -> use scale on a thin rect"""
        s.key(c,"scale",[(t0,0.0001,ease),(t1,1,"linear")])
        if anchor!="center":
            sgn=-1 if anchor=="left" else 1
            s.key(c,"x",[(t0,x0*s.u+sgn*w*s.u/2,ease),(t1,x0*s.u,"linear")])
    def sweep_light(s, tr, t0, t1, dur, angle=18, color=(1,0.95,0.85), opacity=0.35, width=320):
        c=s.rect(tr,s.off,dur,width,2200,color,rot=angle,blend="add")
        s.k.call("add_effect", clip_id=c, kind="blur", params={"radius":240*s.u})
        s.k.call("set_property", clip_id=c, property="opacity", value=0)
        W=s.W
        s.key(c,"x",[(t0,-W*0.85,"in_out_quint"),(t1,W*0.85,"linear")])
        s.key(c,"opacity",[(t0,0,"linear"),(t0+(t1-t0)*0.25,opacity,"linear"),(t0+(t1-t0)*0.75,opacity,"linear"),(t1,0,"linear")]); return c
    def drift(s, c, dur, s0=1.0, s1=1.06, e="linear"):
        s.key(c,"scale",[(0,s0,e),(dur,s1,"linear")])
