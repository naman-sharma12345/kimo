"""RV-25 s1 proof: contact_sheet over real stdio. Temp files only."""
import base64, json, os, struct, subprocess, sys, tempfile
BIN = os.environ.get("KIMO_BIN", "target/release/kimo-mcp"); T = tempfile.mkdtemp(prefix="cs_proof_")
class S:
    def __init__(s):
        s.p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True); s.i = 0; s.rpc("initialize", {})
    def rpc(s, m, pr): s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": pr}) + "\n"); s.p.stdin.flush(); return json.loads(s.p.stdout.readline())
    def call(s, n, a): return s.rpc("tools/call", {"name": n, "arguments": a})["result"]
    def j(s, n, a):
        r = s.call(n, a); assert not r["isError"], (n, r["content"][0]["text"]); return json.loads(r["content"][0]["text"])
    def close(s): s.p.stdin.close(); s.p.wait()
ok = True
def check(c, m):
    global ok; print(("PASS " if c else "FAIL ") + m); ok &= bool(c)
def raw(path):
    w, h = struct.unpack(">II", open(path, "rb").read()[16:24])
    d = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], capture_output=True).stdout; return w, h, d
def box(src, sw, sh, tw, th):
    out = bytearray(tw * th * 4)
    for y in range(th):
        y0 = y * sh // th; y1 = min(sh, max((y + 1) * sh // th, y0 + 1))
        for x in range(tw):
            x0 = x * sw // tw; x1 = min(sw, max((x + 1) * sw // tw, x0 + 1)); c = (y1 - y0) * (x1 - x0); o = (y * tw + x) * 4
            for k in range(3):
                t = sum(src[(yy * sw + xx) * 4 + k] for yy in range(y0, y1) for xx in range(x0, x1)); out[o + k] = (t + c // 2) // c
            out[o + 3] = 255
    return bytes(out)
s = S(); s.j("project_set", {"width": 640, "height": 360, "fps": 24}); tid = s.j("add_track", {})["track_id"]
c1 = s.j("add_clip", {"track_id": tid, "source_type": "solid", "color": [0.9, 0.2, 0.2], "start": 0, "duration": 10})["clip_id"]
c2 = s.j("add_clip", {"track_id": tid, "source_type": "solid", "color": [0.2, 0.3, 0.9], "start": 10, "duration": 2})["clip_id"]
for i in range(100): s.j("set_keyframe", {"clip_id": c1, "property": "x", "time": i * 0.1, "value": i})
# 1: 6 times, 3 cols -> exact dims, tile == downscaled render_frame, labels drawn
f = f"{T}/s.png"; times = [0.2, 0.8, 1.4, 2.1, 2.9, 3.6]
r = s.call("contact_sheet", {"path": f, "times": times, "cols": 3, "tile_px": 320, "labels": "none"}); res = json.loads(r["content"][0]["text"]); sc = res["structuredContent"] if "structuredContent" in res else r.get("structuredContent")
check(not r["isError"] and (res["width"], res["height"], res["cols"], res["rows"]) == (3 * 320 + 4 * 4, 2 * 180 + 3 * 4, 3, 2), "6 times 3 cols: sheet dims exact")
check(len([c for c in r["content"] if c["type"] == "image"]) == 1, "inline image block present")
w, h, sheet = raw(f); sw_ = w
ok_tiles = True
for i, t in enumerate(times):
    rf = f"{T}/rf{i}.png"; s.j("render_frame", {"time": t, "path": rf, "inline": "never"}); fw, fh, fd = raw(rf); ref = box(fd, fw, fh, 320, 180)
    e = res["tiles"][i]; got = b"".join(sheet[((e["y"] + y) * sw_ + e["x"]) * 4:((e["y"] + y) * sw_ + e["x"] + 320) * 4] for y in range(180))
    ok_tiles &= (got == ref)
check(ok_tiles, "each tile equals the box-downscaled single render_frame (byte exact)")
f2 = f"{T}/s2.png"; s.j("contact_sheet", {"path": f2, "times": times, "cols": 3, "labels": "time"}); _, _, sheet2 = raw(f2)
check(sheet2 != sheet, "labels drawn (pixels differ from labels:none)")
# 2: keyframes_of with 100 keys: cap + 16 tiles, first/last kept, all_times reported
r = s.j("contact_sheet", {"path": f"{T}/k.png", "keyframes_of": c1, "labels": "time+props"})
check(r["tile_count"] == 16 and r["keyframe_candidates"] == 100 and r["omitted"] > 0 and len(r["all_times"]) <= 64, "100 keyframes -> 16 tiles, candidates 100, all_times <= 64, omitted reported")
check(r["tiles"][0]["timeline_time"] == r["all_times"][0] and r["tiles"][-1]["timeline_time"] == r["all_times"][-1], "first and last key times kept")
check(r["tiles"][0].get("clip_time") is not None and r["tiles"][0].get("props"), "tile reports clip_time + props keyed there")
r = s.j("contact_sheet", {"path": f"{T}/k2.png", "keyframes_of": c1, "keyframe_cap": 10}); check(r["keyframe_cap_applied"] == 10 and len(r["all_times"]) == 10, "keyframe_cap stride applies")
# 3: unknown clip -> not_found; at_cuts; cell cap; bad labels
e = s.call("contact_sheet", {"path": f"{T}/u.png", "keyframes_of": "nope"}); check(e["isError"] and e["structuredContent"]["code"] == "not_found", "unknown clip -> not_found")
r = s.j("contact_sheet", {"path": f"{T}/c.png", "at_cuts": True, "tile_px": 5000}); check(r["tile_count"] == 2 and r["tile"]["w"] <= 320 and [t["timeline_time"] for t in r["tiles"]] == [0.0, 10.0], "at_cuts samples clip starts; tile_px capped to 320")
r = s.j("contact_sheet", {"path": f"{T}/n.png", "start": 0, "end": 12, "n": 50}); check(r["tile_count"] == 16, "start/end/n capped at 16 tiles")
r = s.call("contact_sheet", {"path": f"{T}/e.png"}); check(r["isError"], "no times -> error")
s.close(); print("CONTACT SHEET PROOF OK" if ok else "CONTACT SHEET PROOF FAILED"); sys.exit(0 if ok else 1)
