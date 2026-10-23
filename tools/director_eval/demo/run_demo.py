#!/usr/bin/env python3
"""Director-mode demo: the exact kimo-CLI call sequence a small model in
director mode makes (see ../director_mode.md and docs/DIRECTOR_QUICKSTART.md),
scripted deterministically -- no live/network model call.

    run_demo.py <clips_dir> <out.mp4> [--kimo PATH] [--recipe PATH] [--project PATH]

<clips_dir> holds numbered 9:16 talking-head clips (clip1.mp4, clip2.mp4, ...).
<out.mp4>   is where the exported short is written.
--kimo      path to the kimo CLI binary (default: $KIMO_BIN, else "kimo" on PATH).
--recipe    a director_recipe_example.json-shaped file naming which clips to use,
            in what order, and the honest caption/hook/highlight text (default:
            director_recipe_example.json next to this script, which matches the
            clips this demo shipped with -- a different footage set needs its
            own recipe file: the words people actually say can't be guessed).
--project   where to keep the .kimo project file (default: <out>.kimo).

One call per director-mode step: project_set -> import_media (per clip) ->
auto_rough_cut -> auto_grade -> add_track + add_title (hook) ->
caption_from_words -> add_smart_punch + add_glow_text (highlight, if the
recipe names one) -> finish_audio -> lint_edit (auto-fix anything it flags
except the font-fallback caveat, up to 3 passes) -> export.
"""
import argparse
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def kimo_call(kimo_bin, tool, args, project):
    cmd = [kimo_bin, "--json", "call", tool, "--args", json.dumps(args), "--project", project]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
    if r.returncode != 0:
        raise RuntimeError(f"{tool} failed (rc={r.returncode}): {r.stderr.strip()[-800:]}\n{r.stdout.strip()}")
    try:
        return json.loads(r.stdout)
    except ValueError:
        raise RuntimeError(f"{tool}: non-JSON stdout: {r.stdout!r}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("clips_dir")
    ap.add_argument("out_path")
    ap.add_argument("--kimo", default=os.environ.get("KIMO_BIN", "kimo"))
    ap.add_argument("--recipe", default=os.path.join(HERE, "director_recipe_example.json"))
    ap.add_argument("--project", default=None)
    args = ap.parse_args()

    with open(args.recipe) as f:
        recipe = json.load(f)

    out_path = os.path.abspath(args.out_path)
    project = os.path.abspath(args.project or (out_path + ".kimo"))
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    if os.path.exists(project):
        os.remove(project)

    def call(tool, a):
        print(f"kimo call {tool} {json.dumps(a)}", file=sys.stderr)
        return kimo_call(args.kimo, tool, a, project)

    call("project_set", recipe.get("project", {"width": 720, "height": 1280, "fps": 30}))

    asset_ids = {}
    for clip_name in recipe["clips"]:
        path = os.path.join(args.clips_dir, clip_name)
        if not os.path.exists(path):
            raise SystemExit(f"missing clip: {path}")
        asset_ids[clip_name] = call("import_media", {"path": path})["asset_id"]
    if recipe.get("dropped_clips"):
        print(f"dropped as filler (not imported): {recipe['dropped_clips']}", file=sys.stderr)

    rough = call("auto_rough_cut", {"asset_ids": [asset_ids[c] for c in recipe["clips"]]})
    dialogue_track = rough["track_id"]

    call("auto_grade", {})

    title_track = call("add_track", {})["track_id"]
    hook = recipe["hook"]
    call("add_title", {"track_id": title_track, "style": "hook_box", "text": hook["text"],
                        "start": hook.get("start", 0.0), "duration": hook.get("duration", 1.8)})

    by_asset = {asset_ids[c]: text for c, text in recipe["captions"].items()}
    call("caption_from_words", {"track_id": dialogue_track, "by_asset": by_asset})

    highlight = recipe.get("highlight")
    if highlight:
        asset = asset_ids[highlight["clip"]]
        segs = [c for c in rough["clips"] if c["asset_id"] == asset]
        hl_clip = segs[highlight.get("segment_index", 0)]
        call("add_smart_punch", {"clip_id": hl_clip["clip_id"], "zoom": highlight.get("zoom", 1.25)})
        call("add_glow_text", {"track_id": title_track, "text": highlight["glow_text"],
                                "start": hl_clip["start"], "duration": highlight.get("glow_duration", 1.6)})

    call("finish_audio", {})

    for attempt in range(3):
        lint = call("lint_edit", {})
        real_problems = [p for p in lint.get("problems", []) if p["code"] != "font_substituted"]
        if not real_problems:
            break
        fix = real_problems[0]["fix"]
        call(fix["tool"], fix["args"])
        if "then" in fix:
            call(fix["then"]["tool"], fix["then"]["args"])
    else:
        raise SystemExit(f"lint_edit still unhappy after 3 fix attempts: {real_problems}")

    result = call("export", {"path": out_path, "crf": recipe.get("crf", 20)})
    print(json.dumps(result, indent=2))
    print(f"\nwrote {out_path} ({result['bytes']/1e6:.1f} MB, {result['duration']:.1f}s)", file=sys.stderr)


if __name__ == "__main__":
    main()
