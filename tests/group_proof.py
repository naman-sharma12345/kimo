"""Tool grouping proof: the default tools/list is small, every flat tool is reachable through its group, flat names still work."""
import sys, json, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "examples"))
from mcp_client import Kimo
k = Kimo()
flat = k.rpc("tools/list", {"mode": "flat"})["tools"]; grouped = k.rpc("tools/list", {})["tools"]
fb, gb = len(json.dumps(flat)), len(json.dumps(grouped))
print(f"flat: {len(flat)} tools, {fb} bytes | grouped: {len(grouped)} tools, {gb} bytes | {fb/gb:.1f}x smaller")
assert len(grouped) <= 20 and len(flat) >= 250
# every registry tool is reachable through exactly one group (RV-23: the compact list no longer carries action names, kimo_describe(group) does)
META = {"search_tools", "kimo_describe", "kimo_enable_group"}
seen = {}
for g in grouped:
    if g["name"] in META: continue
    for a in k.call("kimo_describe", group=g["name"])["actions"]:
        a = a["action"] if isinstance(a, dict) else a
        assert a not in seen, (a, seen[a], g["name"]); seen[a] = g["name"]
assert set(seen) == {t["name"] for t in flat} - META, set(seen) ^ ({t["name"] for t in flat} - META)
assert gb <= 6144, gb
# grouped call equals flat call
k.call("project_set", width=640, height=360, fps=25)
a = k.call("get_project"); b = k.call("project", action="get_project", args={})
assert a == b
r = k.call("project", action="project_set", args={"width": 320, "height": 180, "fps": 24}); assert k.call("get_project")["width"] == 320
# wrong group and missing action are errors, not crashes
for bad in [dict(action="project_set"), dict(action="get_project"), dict(), dict(action="nope")]:
    try: k.call("audio", **bad); raise SystemExit(f"expected error for {bad}")
    except RuntimeError: pass
# search and describe
s = k.call("kimo_search", query="motion blur"); assert s["matches"], s
d = k.call("kimo_describe", action="add_scene3d"); assert d["group"] == "vfx3d" and d["args_schema"]
g = k.call("kimo_describe", group="audio"); assert g["actions"]
print("search 'motion blur':", [(m["group"], m["action"]) for m in s["matches"][:5]])
print("group proof OK")
