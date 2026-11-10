"""Shared by sse_feed.py and feed_demo.py: turn demo/scenarios.json into
the engine event stream (same order and shapes as demo/replay.js)."""
import base64, json, os
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def load():
    return json.load(open(os.path.join(ROOT, "demo", "scenarios.json")))

def inline_frames(obj):
    """Replace ../demo/frames/x.jpg paths with data: URLs (for the Tauri
    shell, which serves only ui/)."""
    if isinstance(obj, dict):
        return {k: inline_frames(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [inline_frames(v) for v in obj]
    if isinstance(obj, str) and obj.startswith("../demo/frames/"):
        p = os.path.join(ROOT, obj[3:])
        if os.path.exists(p):
            return "data:image/jpeg;base64," + base64.b64encode(open(p, "rb").read()).decode()
    return obj

def events(s, inline=False):
    """Yield (event, wait_seconds_before) for one scenario."""
    views = inline_frames(s["views"]) if inline else s["views"]
    yield {"type": "session", "reset": True, "project": s["project"], "agent": s["agent"], "elapsed_s": s["elapsed"]}, 0
    yield ({"type": "mode", "mode": s["mode"], "activity": s["activity"]} if s.get("activity") else {"type": "mode", "mode": s["mode"]}), 0
    if s["stats"]:
        yield {"type": "stats", **s["stats"]}, 0
    yield {"type": "view", "mode": s["mode"], "data": views}, 0
    n = len(s["calls"])
    for i, (name, args, dur) in enumerate(s["calls"]):
        cid = f'{s["id"]}-c{i}'
        st = {"type": "tool_start", "id": cid, "name": name, "args": args}
        last = i == n - 1
        if last and s["final"].get("count"):
            st["count"] = s["final"]["count"]
        if last and s["final"].get("progress"):
            st["progress"] = s["final"]["progress"]
        yield st, 0.38
        if not (last and s["final"].get("running")):
            yield {"type": "tool_end", "id": cid, "name": name, "dur_ms": dur, "status": "ok"}, min(dur, 900) / 1000
