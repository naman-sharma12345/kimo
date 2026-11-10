#!/usr/bin/env python3
"""Director-mode demo, over MCP (not the CLI): spawns kimo-mcp over stdio, lists the tools a
small model actually sees in director mode (tools/list {mode: "director"} -- see
../director_mode.md and ../../../docs/SMALL_MODEL_PREP.md), then makes the exact director-mode
call sequence ../run_demo.py makes, but as raw tools/call JSON-RPC instead of CLI subprocess
calls, and records each call's latency.

    run_demo_mcp.py <clips_dir> <out.mp4> [--kimo PATH] [--recipe PATH] [--project PATH] [--report PATH]

Difference from run_demo.py, on purpose: this script treats "no speech model installed" as an
expected outcome, not a crash. If auto_captions comes back isError with that message, the run
continues without captions (recipe's hook/highlight/export all still happen) -- the caption gap
is reported, honestly, in the JSON report, never papered over with invented words. This is what
a real director-mode small model, restricted to tools/list {mode: "director"}, should do too:
auto_captions is the documented way to caption a clip you don't have, or don't trust, a transcript
for; if no recognizer is installed, the edit still ships -- just without captions.
"""
import argparse
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))  # tools/director_eval/
import mcp as kmcp  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("clips_dir")
    ap.add_argument("out_path")
    ap.add_argument("--kimo", default=os.environ.get("KIMO_MCP_BIN", "kimo-mcp"))
    ap.add_argument("--recipe", default=os.path.join(HERE, "director_recipe_example.json"))
    ap.add_argument("--project", default=None)
    ap.add_argument("--report", default=None, help="where to write the JSON call/latency/error report")
    args = ap.parse_args()

    with open(args.recipe) as f:
        recipe = json.load(f)

    out_path = os.path.abspath(args.out_path)
    project = os.path.abspath(args.project or (out_path + ".kimo"))
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    if os.path.exists(project):
        os.remove(project)

    calls = []  # {tool, args, ms, is_error, error (if any)}

    def log(msg):
        print(msg, file=sys.stderr)

    k = kmcp.Kimo(args.kimo, env={"KIMO_SPECTATOR": "off"}, log=os.environ.get("KIMO_MCP_LOG"))
    tools_listed = k.list_tools(mode="director")
    tool_names = [t["name"] for t in tools_listed]
    log(f"tools/list mode=director: {len(tool_names)} tools: {tool_names}")

    def call(tool, a, required=True):
        """tools/call over MCP, timed. Raises on error unless required=False (caller inspects (ok, text))."""
        log(f"tools/call {tool} {json.dumps(a)}")
        t0 = time.monotonic()
        is_err, text, imgs = k.call(tool, a, timeout=180)
        ms = (time.monotonic() - t0) * 1000.0
        rec = {"tool": tool, "args_keys": sorted(a.keys()), "ms": round(ms, 1), "is_error": is_err}
        if is_err:
            rec["error"] = text
            log(f"  -> ERROR ({ms:.0f}ms): {text}")
        else:
            log(f"  -> ok ({ms:.0f}ms)")
        calls.append(rec)
        if is_err and required:
            raise SystemExit(f"{tool} failed (over MCP, project is at {project} for inspection): {text}")
        try:
            parsed = json.loads(text) if text else {}
        except ValueError:
            parsed = {"_raw": text}
        return (not is_err, parsed)

    try:
        _, proj_result = call("project_set", recipe.get("project", {"width": 720, "height": 1280, "fps": 30}))

        asset_ids = {}
        for clip_name in recipe["clips"]:
            path = os.path.join(args.clips_dir, clip_name)
            if not os.path.exists(path):
                raise SystemExit(f"missing clip: {path}")
            _, r = call("import_media", {"path": path})
            asset_ids[clip_name] = r["asset_id"]
        if recipe.get("dropped_clips"):
            log(f"dropped as filler (not imported): {recipe['dropped_clips']}")

        _, rough = call("auto_rough_cut", {"asset_ids": [asset_ids[c] for c in recipe["clips"]]})
        dialogue_track = rough["track_id"]

        call("auto_grade", {})

        _, track = call("add_track", {})
        title_track = track["track_id"]
        hook = recipe["hook"]
        call("add_title", {"track_id": title_track, "style": "hook_box", "text": hook["text"],
                            "start": hook.get("start", 0.0), "duration": hook.get("duration", 1.8)})

        # --- captions: auto_captions over MCP, no hand-typed transcript. Treat "no speech model
        # installed" as an expected, handled outcome (KM1): continue the edit without captions
        # rather than abort, and report the gap honestly instead of inventing words.
        cap_opts = recipe.get("captions", {})
        seg_clip_ids = [c["clip_id"] for c in rough["clips"]]
        cap_args = {"clip_ids": seg_clip_ids}
        for key in ("style", "hint", "language", "script"):
            if cap_opts.get(key):
                cap_args[key] = cap_opts[key]
        ok, cap = call("auto_captions", cap_args, required=False)
        captions_skipped_reason = None
        low_conf_words = []
        if not ok:
            err_text = calls[-1].get("error", "")
            if "no speech model is installed" in err_text:
                captions_skipped_reason = err_text
                log("captions SKIPPED (no speech model installed on this box) -- continuing without them, per KM1's contract")
                cap = {}
            else:
                raise SystemExit(f"auto_captions failed for a reason other than the no-model case; not continuing silently: {err_text}")
        else:
            if cap.get("low_confidence_words"):
                _, tget = call("transcript_get", {"transcript_id": cap["transcript_id"], "include_words": True})
                low_conf_words = [w for w in tget.get("words", []) if "low_confidence" in w.get("flags", [])]
            log(f"auto_captions: {cap.get('words', 0)} words, language={cap.get('language')}, "
                f"mode={cap.get('mode')}, confidence={cap.get('confidence')}, "
                f"low_confidence={cap.get('low_confidence_words', 0)}")
            for w in low_conf_words:
                log(f"  low-confidence word: \"{w['display']}\" [{w['start']:.2f}-{w['end']:.2f}]s (conf={w.get('confidence')})")

        highlight = recipe.get("highlight")
        if highlight:
            asset = asset_ids[highlight["clip"]]
            segs = [c for c in rough["clips"] if c["asset_id"] == asset]
            hl_clip = segs[highlight.get("segment_index", 0)]
            call("add_smart_punch", {"clip_id": hl_clip["clip_id"], "zoom": highlight.get("zoom", 1.25)})
            call("add_glow_text", {"track_id": title_track, "text": highlight["glow_text"],
                                    "start": hl_clip["start"], "duration": highlight.get("glow_duration", 1.6)})

        call("finish_audio", {})

        def has_placeholder(d):
            return any(isinstance(v, str) and v.strip().startswith("<") and v.strip().endswith(">") for v in (d or {}).values())

        for attempt in range(3):
            _, lint = call("lint_edit", {})
            # with no captions at all, don't chase caption_low_confidence/captions_missing fixes --
            # there is nothing honest to caption with; the font_substituted caveat is also accepted.
            skip_codes = {"font_substituted"}
            if captions_skipped_reason:
                skip_codes |= {"caption_low_confidence", "captions_missing"}
            real_problems = [p for p in lint.get("problems", []) if p["code"] not in skip_codes]
            if not real_problems:
                break
            prob = real_problems[0]
            fix = prob["fix"]
            chosen = fix
            if has_placeholder(fix.get("args", {})):
                alt = fix.get("or")
                if isinstance(alt, dict) and "tool" in alt and not has_placeholder(alt.get("args", {})):
                    chosen = alt
                else:
                    raise SystemExit(f"lint_edit flagged {prob['code']} with no concrete fix to apply: {json.dumps(prob)}")
            call(chosen["tool"], chosen["args"])
            if "then" in chosen:
                call(chosen["then"]["tool"], chosen["then"]["args"])
        else:
            raise SystemExit(f"lint_edit still unhappy after 3 fix attempts: {real_problems}")

        _, result = call("export", {"path": out_path, "crf": recipe.get("crf", 20)})

    finally:
        k.close()

    report = {
        "transport": "mcp",
        "director_profile_tool_count": len(tool_names),
        "director_profile_tools": tool_names,
        "calls": calls,
        "total_calls": len(calls),
        "errors": [c for c in calls if c["is_error"]],
        "total_ms": round(sum(c["ms"] for c in calls), 1),
        "export": result,
        "captions": {
            "skipped_no_model": captions_skipped_reason is not None,
            "skip_reason": captions_skipped_reason,
            "caption_id": cap.get("caption_id"),
            "transcript_id": cap.get("transcript_id"),
            "language": cap.get("language"),
            "mode": cap.get("mode"),
            "confidence": cap.get("confidence"),
            "total_words": cap.get("words"),
            "low_confidence_count": cap.get("low_confidence_words", 0),
            "low_confidence_words": [
                {"text": w["display"], "start": w["start"], "end": w["end"]} for w in low_conf_words
            ],
        },
    }
    if args.report:
        with open(args.report, "w") as f:
            json.dump(report, f, indent=2)
    print(json.dumps(report, indent=2))
    log(f"\nwrote {out_path} ({result['bytes']/1e6:.1f} MB, {result['duration']:.1f}s) -- "
        f"{len(calls)} MCP calls, {len(report['errors'])} errors, {report['total_ms']:.0f}ms total")


if __name__ == "__main__":
    main()
