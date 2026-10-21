#!/usr/bin/env python3
"""Side by side: two vertical exports as 360x640 panels (720x640), labelled; audio from the chosen panel."""
import json, subprocess, sys
import os
FONT = os.environ.get("KIMO_LABEL_FONT", os.path.expanduser("~/.local/share/fonts/Inter.ttf"))
def dur(p):
    r = subprocess.run(["timeout", "-k", "5", "60", "ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", p], capture_output=True, text=True); return float(r.stdout.strip())
def esc(s): return s.replace("\\", "\\\\").replace(":", "\\:").replace("'", "\u2019").replace("%", "\\%")
def build(panels, out, audio=1, w=540, h=960):
    D = max(dur(p["path"]) for p in panels)
    args = ["timeout", "-k", "5", "900", "nice", "-n", "10", "ffmpeg", "-y", "-v", "error"]
    for p in panels: args += ["-i", p["path"]]
    f = []
    for i, p in enumerate(panels):
        f.append(f"[{i}:v]fps=30,scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,tpad=stop_mode=clone:stop_duration={D:.2f},trim=duration={D:.2f},"
                 f"drawbox=x=0:y=0:w={w}:h=78:color=black@0.65:t=fill,"
                 f"drawtext=fontfile={FONT}:text='{esc(p['l1'])}':x=12:y=9:fontsize=28:fontcolor=white,"
                 f"drawtext=fontfile={FONT}:text='{esc(p['l2'])}':x=12:y=44:fontsize=22:fontcolor=0xFFD54A[v{i}]")
    f.append("".join(f"[v{i}]" for i in range(len(panels))) + f"hstack=inputs={len(panels)}[v]")
    f.append(f"[{audio}:a]apad,atrim=duration={D:.2f}[a]")
    args += ["-filter_complex", ";".join(f), "-map", "[v]", "-map", "[a]", "-c:v", "libx264", "-preset", "veryfast", "-b:v", "2200k", "-maxrate", "2800k", "-bufsize", "4400k",
             "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart", "-t", f"{D:.2f}", out]
    r = subprocess.run(args, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    if r.returncode: raise SystemExit(r.stderr[-2000:])
    return D
if __name__ == "__main__":
    s = json.load(open(sys.argv[1])); print(build(s["panels"], s["out"], s.get("audio", 1)))
