#!/usr/bin/env python3
"""Deterministic synthetic "phone footage" for the director eval: three sets, each with a truth.json.

  python3 gen_footage.py OUTDIR            # writes OUTDIR/{talk,cafe,vlog}/...

Set talk: a talking head (a flubbed take and a good take with dead air), shaky dark blue street, warm desk, street pan.
Set cafe: a product reel with no speech: a 120 BPM music bed and five b-roll shots (dark, shaky, green cast).
Set vlog: two speakers (the second 9 dB quieter, the first with a false start), shaky walk and food b-roll, an outro.
Needs: python3 with numpy + Pillow, ffmpeg, espeak-ng. Everything is seeded, so reruns give the same pixels and samples.
"""
import json, math, os, subprocess, sys, tempfile, wave
import numpy as np
from PIL import Image, ImageDraw, ImageFilter

W, H, FPS, SR = 720, 1280, 30, 48000

def sh(cmd, t=300):
    subprocess.run(["timeout", "-k", "5", str(t)] + cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

# ---------- audio ----------
def tts(text, voice, speed, pitch):
    fd, p = tempfile.mkstemp(suffix=".wav"); os.close(fd)
    sh(["espeak-ng", "-v", voice, "-s", str(speed), "-p", str(pitch), "-w", p, text])
    q = p + ".48k.wav"; sh(["ffmpeg", "-y", "-i", p, "-ar", str(SR), "-ac", "1", q])
    with wave.open(q) as w: a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(np.float32) / 32768
    os.remove(p); os.remove(q)
    nz = np.nonzero(np.abs(a) > 0.01)[0]
    return a[nz[0]:nz[-1] + 1] if len(nz) else a

def room_tone(n, seed, db=-52):
    r = np.random.default_rng(seed); x = r.standard_normal(n).astype(np.float32)
    x = np.convolve(x, np.ones(8) / 8, mode="same")  # soften
    return x / (np.sqrt(np.mean(x ** 2)) + 1e-9) * 10 ** (db / 20)

def place(dur, pieces, seed, gain_db=0.0, tone_db=-52):
    """pieces: [(t0, samples)]. Returns audio and the speech intervals [[s,e]]."""
    n = int(round(dur * SR)); a = room_tone(n, seed, tone_db); iv = []
    for t0, s in pieces:
        i = int(round(t0 * SR)); s = s[: max(0, n - i)]
        a[i:i + len(s)] += s * 10 ** (gain_db / 20); iv.append([round(t0, 3), round(t0 + len(s) / SR, 3)])
    return np.clip(a, -1, 1), iv

def word_gaps(a, iv, min_gap=0.06):
    """Pauses inside speech (between words): centres of low-energy runs >= min_gap inside each interval."""
    hop = int(0.01 * SR); e = np.array([np.sqrt(np.mean(a[i:i + hop] ** 2)) for i in range(0, len(a) - hop, hop)])
    out = []
    for s, t in iv:
        seg = e[int(s * 100):int(t * 100)]; thr = max(0.004, 0.08 * (seg.max() if len(seg) else 0)); run = 0
        for k, v in enumerate(seg):
            if v < thr: run += 1
            else:
                if run * 0.01 >= min_gap: out.append(round(s + (k - run / 2) * 0.01, 3))
                run = 0
    return out

def write_wav(path, a, ch=2):
    x = (np.clip(a, -1, 1) * 32767).astype(np.int16)
    if ch == 2: x = np.repeat(x[:, None], 2, axis=1)
    with wave.open(path, "wb") as w: w.setnchannels(ch); w.setsampwidth(2); w.setframerate(SR); w.writeframes(x.tobytes())

def music(dur, bpm=120, seed=7):
    n = int(dur * SR); t = np.arange(n) / SR; a = np.zeros(n, np.float32); beat = 60 / bpm
    r = np.random.default_rng(seed); roots = [55.0, 55.0, 43.65, 49.0]  # A, A, F, G
    for k in range(int(dur / beat) + 1):
        b0 = k * beat; i = int(b0 * SR); L = min(int(0.35 * SR), n - i)
        if L <= 0: break
        tt = np.arange(L) / SR
        kick = np.sin(2 * np.pi * (50 + 90 * np.exp(-tt * 30)) * tt) * np.exp(-tt * 9) * 0.9
        a[i:i + L] += kick
        f = roots[(k // 4) % 4]; bl = min(int(beat * SR), n - i); tb = np.arange(bl) / SR
        a[i:i + bl] += 0.22 * np.sign(np.sin(2 * np.pi * f * tb)) * np.exp(-tb * 3)
        for half in (0, 0.5):
            j = int((b0 + half * beat) * SR); hl = min(int(0.05 * SR), n - j)
            if hl > 0: a[j:j + hl] += r.standard_normal(hl).astype(np.float32) * np.exp(-np.arange(hl) / SR * 80) * (0.12 if half else 0.07)
        if k % 2 == 1:
            j = i; sl = min(int(0.18 * SR), n - j)
            a[j:j + sl] += r.standard_normal(sl).astype(np.float32) * np.exp(-np.arange(sl) / SR * 20) * 0.25
    pad = sum(np.sin(2 * np.pi * f * t) for f in (220, 277.2, 329.6)) * 0.03
    a = a + pad.astype(np.float32)
    a = a / np.max(np.abs(a)) * 0.5  # about -6 dBFS peak, quieter than -14 LUFS once measured
    return a, [round(k * beat, 4) for k in range(int(dur / beat) + 1)]

# ---------- pictures ----------
def rng(seed): return np.random.default_rng(seed)

def texture(d, box, seed, n=140, pal=((90, 90, 100), (140, 130, 120))):
    r = rng(seed); x0, y0, x1, y1 = box
    for _ in range(n):
        x = r.integers(x0, x1); y = r.integers(y0, y1); w = r.integers(8, 60); h = r.integers(8, 60)
        c = tuple(int(v) for v in (np.array(pal[r.integers(0, len(pal))]) + r.integers(-25, 25, 3)).clip(0, 255))
        d.rectangle([x, y, x + w, y + h], fill=c)

def scene(kind, seed, size=(W + 200, H + 260)):
    w, h = size; im = Image.new("RGB", size); d = ImageDraw.Draw(im); r = rng(seed)
    def grad(c0, c1):
        for y in range(h):
            f = y / h; d.line([(0, y), (w, y)], fill=tuple(int(c0[i] * (1 - f) + c1[i] * f) for i in range(3)))
    if kind == "room":
        grad((150, 140, 128), (95, 85, 75)); texture(d, (0, 0, w, h // 2), seed, 60, ((170, 160, 140), (120, 110, 100)))
        d.rectangle([60, 120, 330, 520], fill=(210, 220, 230)); d.rectangle([75, 135, 315, 505], fill=(150, 185, 210))  # window
        d.rectangle([w - 300, 300, w - 80, 900], fill=(110, 80, 60)); texture(d, (w - 290, 310, w - 90, 890), seed + 1, 40, ((200, 60, 60), (60, 90, 160), (220, 200, 90)))  # shelf
    elif kind == "street":
        grad((120, 150, 190), (90, 90, 95))
        for k in range(9):
            x = k * 120 - 40; top = int(150 + r.integers(0, 380)); d.rectangle([x, top, x + 110, h], fill=tuple(int(v) for v in r.integers(70, 170, 3)))
            for wy in range(top + 30, h - 400, 70):
                for wx in range(x + 12, x + 100, 34): d.rectangle([wx, wy, wx + 20, wy + 34], fill=(230, 220, 160) if r.random() < .35 else (50, 60, 75))
        d.rectangle([0, h - 380, w, h], fill=(60, 60, 64)); [d.rectangle([x, h - 200, x + 90, h - 185], fill=(230, 230, 220)) for x in range(0, w, 180)]
    elif kind == "desk":
        grad((190, 170, 150), (120, 95, 75)); d.rectangle([0, h * 0.55, w, h], fill=(140, 100, 70)); texture(d, (0, int(h * .55), w, h), seed, 90, ((150, 110, 75), (125, 90, 60)))
        d.polygon([(170, 520), (720, 520), (790, 860), (110, 860)], fill=(40, 40, 46)); d.rectangle([200, 300, 690, 620], fill=(30, 30, 35)); d.rectangle([215, 315, 675, 605], fill=(80, 140, 200))
        texture(d, (225, 325, 665, 595), seed + 2, 50, ((230, 230, 240), (120, 170, 220)))
        d.ellipse([600, 900, 780, 1000], fill=(235, 235, 230)); d.ellipse([625, 915, 755, 985], fill=(90, 55, 30))
    elif kind == "person":
        im = scene("room", seed, size); d = ImageDraw.Draw(im); cx = w // 2
        d.ellipse([cx - 260, h - 620, cx + 260, h + 300], fill=(60, 80, 120))  # shoulders
        d.rectangle([cx - 55, h - 760, cx + 55, h - 560], fill=(215, 170, 140))  # neck
        d.ellipse([cx - 150, h - 1110, cx + 150, h - 720], fill=(225, 180, 150))  # head
        d.chord([cx - 160, h - 1140, cx + 160, h - 880], 180, 360, fill=(50, 35, 30))  # hair
        for ex in (-60, 60): d.ellipse([cx + ex - 18, h - 960, cx + ex + 18, h - 935], fill=(40, 30, 30))
    elif kind == "person2":
        im = scene("street", seed, size); d = ImageDraw.Draw(im); cx = w // 2
        d.ellipse([cx - 250, h - 600, cx + 250, h + 300], fill=(170, 60, 60)); d.rectangle([cx - 50, h - 740, cx + 50, h - 560], fill=(160, 115, 85))
        d.ellipse([cx - 140, h - 1080, cx + 140, h - 710], fill=(170, 125, 95)); d.chord([cx - 150, h - 1110, cx + 150, h - 860], 180, 360, fill=(25, 20, 20))
        for ex in (-55, 55): d.ellipse([cx + ex - 16, h - 940, cx + ex + 16, h - 917], fill=(30, 25, 25))
    elif kind == "cafe_bar":
        grad((70, 55, 45), (40, 30, 25)); texture(d, (0, 0, w, h), seed, 120, ((110, 80, 55), (60, 45, 35), (180, 150, 110)))
        d.rectangle([w // 2 - 160, 380, w // 2 + 160, 700], fill=(200, 200, 205)); d.polygon([(w // 2 - 120, 700), (w // 2 + 120, 700), (w // 2 + 40, 860), (w // 2 - 40, 860)], fill=(230, 230, 235))
        d.line([(w // 2 + 150, 300), (w // 2 + 10, 640)], fill=(180, 120, 60), width=14); d.rectangle([w // 2 - 130, 1000, w // 2 + 130, 1250], fill=(250, 250, 250)); d.ellipse([w // 2 - 110, 1010, w // 2 + 110, 1080], fill=(70, 40, 20))
    elif kind == "latte":
        grad((130, 100, 80), (90, 70, 55)); texture(d, (0, 0, w, h), seed, 70, ((150, 115, 85), (110, 85, 65)))
        cx, cy = w // 2, h // 2; d.ellipse([cx - 330, cy - 330, cx + 330, cy + 330], fill=(245, 245, 240)); d.ellipse([cx - 270, cy - 270, cx + 270, cy + 270], fill=(150, 95, 55))
        for k in range(5): rr = 200 - k * 38; d.ellipse([cx - rr, cy - rr * 0.8 - k * 20, cx + rr, cy + rr * 0.8 - k * 20], outline=(250, 240, 225), width=16)
    elif kind == "pastry":
        grad((200, 185, 160), (150, 130, 110))
        for row in range(4):
            y = 250 + row * 330; d.rectangle([0, y + 200, w, y + 225], fill=(210, 210, 215))
            for k in range(6):
                x = 40 + k * 150 + (row % 2) * 60; c = tuple(int(v) for v in r.integers((170, 110, 40), (230, 170, 90)))
                d.ellipse([x, y + 80, x + 120, y + 200], fill=c); d.arc([x + 15, y + 95, x + 105, y + 185], 200, 340, fill=(120, 70, 30), width=6)
    elif kind == "storefront":
        grad((150, 170, 200), (110, 110, 110)); d.rectangle([60, 260, w - 60, h - 260], fill=(60, 40, 30)); d.rectangle([110, 520, w - 110, h - 330], fill=(230, 200, 140))
        texture(d, (120, 530, w - 120, h - 340), seed, 80, ((200, 160, 100), (120, 80, 50), (240, 230, 200))); d.rectangle([110, 300, w - 110, 470], fill=(30, 20, 15))
        d.text((w // 2 - 60, 360), "EMBER", fill=(240, 200, 120))
    elif kind == "beans":
        grad((40, 35, 32), (20, 18, 16)); cx, cy = w // 2, h // 2; d.rectangle([cx - 220, cy - 360, cx + 220, cy + 380], fill=(30, 30, 30)); d.rectangle([cx - 200, cy - 120, cx + 200, cy + 120], fill=(230, 120, 40))
        d.text((cx - 50, cy - 10), "EMBER", fill=(20, 20, 20)); texture(d, (cx - 300, cy + 400, cx + 300, h - 40), seed, 160, ((90, 55, 30), (70, 40, 20), (110, 70, 40)))
    elif kind == "ramen":
        grad((60, 45, 40), (30, 25, 22)); cx, cy = w // 2, h // 2 + 80; d.ellipse([cx - 350, cy - 260, cx + 350, cy + 260], fill=(200, 60, 50)); d.ellipse([cx - 310, cy - 225, cx + 310, cy + 225], fill=(230, 180, 110))
        texture(d, (cx - 250, cy - 170, cx + 250, cy + 170), seed, 60, ((245, 225, 160), (90, 140, 70), (250, 250, 240), (120, 60, 40)))
        d.line([(cx - 400, cy - 500), (cx + 50, cy - 60)], fill=(160, 120, 70), width=18); d.line([(cx - 330, cy - 560), (cx + 100, cy - 90)], fill=(160, 120, 70), width=18)
    return im.filter(ImageFilter.GaussianBlur(0.6))

def mouth(im, open_amt, cx, cy):
    d = ImageDraw.Draw(im); hgt = 6 + 30 * open_amt; d.ellipse([cx - 45, cy - hgt / 2, cx + 45, cy + hgt / 2], fill=(110, 40, 45))

def grade(arr, gain=(1, 1, 1), lift=0.0, gamma=1.0):
    x = arr.astype(np.float32) / 255; x = np.clip(x * np.array(gain, np.float32) + lift, 0, 1) ** gamma
    return (x * 255 + 0.5).astype(np.uint8)

def shake_path(n, seed, amp=34.0, rot=0.0):
    r = rng(seed); fr = r.uniform(0.6, 4.0, (4, 2)); ph = r.uniform(0, 6.28, (4, 2)); t = np.arange(n) / FPS
    xy = sum(np.stack([np.sin(2 * np.pi * fr[k, 0] * t + ph[k, 0]), np.sin(2 * np.pi * fr[k, 1] * t + ph[k, 1])], 1) / (k + 1) for k in range(4))
    jit = r.standard_normal((n, 2)) * 0.25
    return (xy + jit) * amp / 1.8

def render(path, dur, still, audio, seed, motion="static", g=None, speech_env=None, face=None, amp=34.0):
    n = int(round(dur * FPS)); sw, shh = still.size; base = np.asarray(still)
    wavp = path + ".wav"; write_wav(wavp, audio)
    p = subprocess.Popen(["timeout", "-k", "5", "600", "ffmpeg", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-", "-i", wavp,
                          "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k", "-ar", str(SR), "-shortest",
                          "-movflags", "+faststart", path], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    sp = shake_path(n, seed, amp) if motion == "shaky" else None
    for i in range(n):
        f = i / max(1, n - 1)
        if motion == "pushin":
            z = 1.0 + 0.12 * f; cw, ch = int(sw / z), int(shh / z); x0, y0 = (sw - cw) // 2, (shh - ch) // 2
            fr = Image.fromarray(base[y0:y0 + ch, x0:x0 + cw]).resize((W, H), Image.BILINEAR)
        elif motion == "pan":
            x0 = int(10 + (sw - W - 20) * f); fr = Image.fromarray(base[130:130 + H, x0:x0 + W])
        elif motion == "shaky":
            dx, dy = sp[i]; x0 = int(round(100 + dx)); y0 = int(round(130 + dy)); x0 = min(max(x0, 0), sw - W); y0 = min(max(y0, 0), shh - H)
            fr = Image.fromarray(base[y0:y0 + H, x0:x0 + W])
        else:
            fr = Image.fromarray(base[130:130 + H, 100:100 + W])
        if face is not None and speech_env is not None:
            fr = fr.copy(); mouth(fr, speech_env[min(len(speech_env) - 1, i)], *face)
        a = np.asarray(fr)
        if g: a = grade(a, **g)
        p.stdin.write(a.tobytes())
    p.stdin.close(); p.wait(); os.remove(wavp)
    if p.returncode: raise SystemExit(f"ffmpeg failed for {path}")

def env_frames(a, dur):
    n = int(round(dur * FPS)); hop = SR // FPS
    e = np.array([np.sqrt(np.mean(a[i * hop:(i + 1) * hop] ** 2)) if i * hop < len(a) else 0 for i in range(n)])
    return np.clip(e / (e.max() + 1e-9) * 1.6, 0, 1)


def talk_clip(path, dur, pieces, seed, voice_gain=0.0, g=None, kind="person", tone_db=-52):
    a, iv = place(dur, pieces, seed, voice_gain, tone_db)
    st = scene(kind, seed)
    cy = (H + 260) - (830 if kind == "person" else 800) - 130  # mouth row in the cropped frame
    render(path, dur, st, a, seed, "static", g, env_frames(a, dur), (W // 2, cy))
    return a, iv

def broll(path, dur, kind, seed, motion, g=None, amb_db=-38, amp=34.0):
    a = room_tone(int(dur * SR), seed + 99, amb_db); render(path, dur, scene(kind, seed), a, seed, motion, g, amp=amp); return a

def info(a, iv): return {"speech": iv, "word_gaps": word_gaps(a, iv)}

def set_talk(o):
    os.makedirs(o, exist_ok=True); V = ("en-us", 165, 45); tr = {"set": "talk", "clips": {}}
    l1 = tts("Um, okay so, uh, wait. Sorry. Let me start again.", *V)
    a, iv = talk_clip(f"{o}/A_take1.mp4", 7.5, [(1.0, l1)], 11, -4, g={"gain": (1.25, 1.22, 1.2), "lift": 0.12})
    tr["clips"]["A_take1.mp4"] = {"kind": "talk", "take": 1, "flub": True, "overexposed": True, **info(a, iv), "words": "Um, okay so, uh, wait. Sorry. Let me start again."}
    s1 = tts("Hey! This is Kimo Director. You shoot raw clips on your phone.", *V); s2 = tts("An agent cuts, colours, and captions them into a short. Watch this.", *V)
    t2 = 1.5 + len(s1) / SR + 1.6
    a, iv = talk_clip(f"{o}/A_take2.mp4", 14.2, [(1.5, s1), (t2, s2)], 12, -4)
    tr["clips"]["A_take2.mp4"] = {"kind": "talk", "take": 2, "best": True, **info(a, iv), "sentences": ["Hey! This is Kimo Director. You shoot raw clips on your phone.", "An agent cuts, colours, and captions them into a short. Watch this."]}
    broll(f"{o}/B_shaky_street.mp4", 6, "street", 13, "shaky", {"gain": (0.55, 0.62, 0.95), "lift": -0.02})
    tr["clips"]["B_shaky_street.mp4"] = {"kind": "broll", "shaky": True, "dark": True, "cast": "blue"}
    broll(f"{o}/C_desk_pushin.mp4", 5, "desk", 14, "pushin", {"gain": (1.18, 0.98, 0.72)})
    tr["clips"]["C_desk_pushin.mp4"] = {"kind": "broll", "cast": "warm"}
    broll(f"{o}/D_street_pan.mp4", 5, "street", 15, "pan")
    tr["clips"]["D_street_pan.mp4"] = {"kind": "broll"}
    tr["brief"] = {"duration": [9, 16], "speech_driven": True, "b_roll_during": "second sentence", "title": "KIMO DIRECTOR"}
    json.dump(tr, open(f"{o}/truth.json", "w"), indent=1)

def set_cafe(o):
    os.makedirs(o, exist_ok=True); tr = {"set": "cafe", "clips": {}}
    m, beats = music(18.0); write_wav(f"{o}/music.wav", m)
    tr["clips"]["music.wav"] = {"kind": "music", "bpm": 120, "beats": beats}
    broll(f"{o}/B1_pour.mp4", 6, "cafe_bar", 21, "pushin", {"gain": (0.45, 0.45, 0.48), "lift": -0.01}); tr["clips"]["B1_pour.mp4"] = {"kind": "broll", "dark": True}
    broll(f"{o}/B2_latte.mp4", 5, "latte", 22, "pushin"); tr["clips"]["B2_latte.mp4"] = {"kind": "broll"}
    broll(f"{o}/B3_pastry_shaky.mp4", 6, "pastry", 23, "shaky"); tr["clips"]["B3_pastry_shaky.mp4"] = {"kind": "broll", "shaky": True}
    broll(f"{o}/B4_storefront.mp4", 6, "storefront", 24, "pan", {"gain": (0.85, 1.18, 0.85)}); tr["clips"]["B4_storefront.mp4"] = {"kind": "broll", "cast": "green"}
    broll(f"{o}/B5_beans_hero.mp4", 5, "beans", 25, "pushin"); tr["clips"]["B5_beans_hero.mp4"] = {"kind": "broll"}
    tr["brief"] = {"duration": [11, 16], "speech_driven": False, "beat_cuts": True, "title": "EMBER COFFEE"}
    json.dump(tr, open(f"{o}/truth.json", "w"), indent=1)

def set_vlog(o):
    os.makedirs(o, exist_ok=True); tr = {"set": "vlog", "clips": {}}
    MA = ("en-us+f3", 160, 60); SA = ("en-us+m3", 150, 35)
    f0 = tts("Hi, I'm, sorry.", *MA); m1 = tts("Hi, I'm Maya and this is Sam.", *MA); m2 = tts("Today we're trying the new ramen place downtown.", *MA)
    t1 = 0.8 + len(f0) / SR + 0.9; t2 = t1 + len(m1) / SR + 0.45
    a, iv = talk_clip(f"{o}/C1_maya.mp4", round(t2 + len(m2) / SR + 1.2, 2), [(0.8, f0), (t1, m1), (t2, m2)], 31, -3)
    tr["clips"]["C1_maya.mp4"] = {"kind": "talk", "speaker": "Maya", "false_start": iv[0], **info(a, iv), "lines": ["Hi, I'm Maya and this is Sam.", "Today we're trying the new ramen place downtown."]}
    s1 = tts("The broth is incredible.", *SA); s2 = tts("Honestly, best bowl I've had this year.", *SA)
    u2 = 0.6 + len(s1) / SR + 2.0
    a, iv = talk_clip(f"{o}/C2_sam.mp4", round(u2 + len(s2) / SR + 0.9, 2), [(0.6, s1), (u2, s2)], 32, -12, kind="person2", tone_db=-60)
    tr["clips"]["C2_sam.mp4"] = {"kind": "talk", "speaker": "Sam", "quiet_db": -9, **info(a, iv), "lines": ["The broth is incredible.", "Honestly, best bowl I've had this year."]}
    broll(f"{o}/C3_walk_shaky.mp4", 6, "street", 33, "shaky", amp=40); tr["clips"]["C3_walk_shaky.mp4"] = {"kind": "broll", "shaky": True}
    broll(f"{o}/C4_ramen.mp4", 5, "ramen", 34, "pushin"); tr["clips"]["C4_ramen.mp4"] = {"kind": "broll"}
    o1 = tts("Follow for more food spots. See you next week!", *MA)
    a, iv = talk_clip(f"{o}/C5_maya_outro.mp4", round(0.7 + len(o1) / SR + 2.0, 2), [(0.7, o1)], 35, -3, g={"gain": (1.2, 1.2, 1.18), "lift": 0.1})
    tr["clips"]["C5_maya_outro.mp4"] = {"kind": "talk", "speaker": "Maya", "overexposed": True, **info(a, iv), "lines": ["Follow for more food spots. See you next week!"]}
    tr["brief"] = {"duration": [12, 24], "speech_driven": True, "speakers": 2}
    json.dump(tr, open(f"{o}/truth.json", "w"), indent=1)

if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "footage"; only = sys.argv[2:] or ["talk", "cafe", "vlog"]
    for s in only: {"talk": set_talk, "cafe": set_cafe, "vlog": set_vlog}[s](os.path.join(out, s)); print("wrote", s)
