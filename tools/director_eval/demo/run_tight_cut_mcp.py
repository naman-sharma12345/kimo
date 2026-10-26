#!/usr/bin/env python3
"""Tight cut in one command: chains tonight's new tools end to end on real phone footage over
MCP -- the same transport run_demo_mcp.py drives (stdio JSON-RPC against a spawned kimo-mcp).
Modelled on run_demo_mcp.py (same directory): reuses its MCP client (../mcp.py) and its "treat
'no speech model installed' as an expected, handled outcome" contract. Does NOT modify
run_demo_mcp.py itself -- that script's own demo export sha is a fixed reference other lanes
pin against.

    run_tight_cut_mcp.py [clips_dir] [out.mp4] [--kimo PATH] [--max-per-clip PCT]
                          [--filler-words w1,w2,...] [--project PATH] [--report PATH]

For each clip file in clips_dir (clip1.mp4, clip2.mp4, ... sorted), the chain is:

  1. import_media                  -- probe + register the asset.
  2. auto_captions (batched)       -- ONLY if KIMO_WHISPER_BIN and KIMO_WHISPER_MODEL are both
                                       set; auto_captions's own documented "no speech model is
                                       installed" error is treated as an expected skip (same
                                       contract run_demo_mcp.py uses for exactly this error),
                                       never a crash.
  3. trim_filler_words             -- per clip, using a short filler-word list (um/uh/like/...);
                                       only called when that clip has a linked transcript (i.e.
                                       captions ran); each clip's own trim_filler_words call only
                                       ever touches words bound to ITS OWN clip_id, so one shared
                                       word list is safe to reuse across every clip.
  4. fit_clip_duration             -- threshold_db:"auto" (measures this clip's own noise floor
                                       instead of guessing -60dBFS), protect_words:true (a pause
                                       cut can never land inside a transcribed word), max_duration
                                       = this clip's own ON-TIMELINE duration x (1 - --max-per-clip)
                                       (default 0.12, i.e. "cut ~12% off each clip" combined across
                                       steps 3+4 -- if the filler trim already got there, this step
                                       is a clean no-op with met_target:true).
  5. assemble on the timeline      -- auto_rough_cut lays every imported clip back to back with
                                       dead air cut out, same call run_demo_mcp.py uses to build
                                       its dialogue track; this necessarily runs right after every
                                       import (clips 3/4 above operate on a clip_id that only
                                       exists once it is ON a timeline track -- a bare asset_id
                                       has no clip_id yet), so steps 3-4 are applied to the
                                       resulting per-asset timeline segments, in the same order
                                       the brief lists them.
  6. export                        -- one mp4 of the whole assembled, trimmed timeline.

Without a whisper model: steps 2-3 are skipped cleanly with a printed note; fit_clip_duration
(step 4) still runs with protect_words:true, which is then a documented no-op (no linked
transcript words) and says so in its own `note` field -- never a crash.

Fails loudly (SystemExit, non-zero) on any MCP tools/call error, the same contract run_demo_mcp.py
uses for every REQUIRED call.
"""
import argparse
import glob
import json
import os
import re
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))  # tools/director_eval/
import mcp as kmcp  # noqa: E402

DEFAULT_FILLER_WORDS = ["um", "uh", "umm", "uhh", "hmm", "like", "matlab", "ah"]


def natural_key(path):
    """clip1, clip2, ..., clip10 in numeric order, not lexicographic (clip10 before clip2)."""
    name = os.path.basename(path)
    return [int(t) if t.isdigit() else t for t in re.split(r"(\d+)", name)]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("clips_dir", nargs="?",
                     default="/home/user/tab/tasks/7d760e41-fe54-4f1c-92b1-a089bbb03ac3/ks/footage_r/real/")
    ap.add_argument("out_path", nargs="?", default=os.path.join(HERE, "tight_cut_out.mp4"))
    ap.add_argument("--kimo", default=os.environ.get("KIMO_MCP_BIN", "kimo-mcp"))
    ap.add_argument("--project", default=None)
    ap.add_argument("--report", default=None, help="where to write the JSON summary (default: <out_path>.report.json)")
    ap.add_argument("--max-per-clip", type=float, default=0.12,
                     help="target fraction to cut off each clip's own on-timeline duration, combined across "
                          "trim_filler_words + fit_clip_duration (default 0.12 == cut ~12%% off each clip)")
    ap.add_argument("--filler-words", default=",".join(DEFAULT_FILLER_WORDS),
                     help="comma-separated filler words for trim_filler_words (default: %(default)s)")
    args = ap.parse_args()

    filler_words = [w.strip() for w in args.filler_words.split(",") if w.strip()]
    out_path = os.path.abspath(args.out_path)
    project = os.path.abspath(args.project or (out_path + ".kimo"))
    report_path = args.report or (out_path + ".report.json")
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    if os.path.exists(project):
        os.remove(project)

    whisper_bin = os.environ.get("KIMO_WHISPER_BIN")
    whisper_model = os.environ.get("KIMO_WHISPER_MODEL")
    whisper_on = bool(whisper_bin and whisper_model)

    clip_paths = sorted(glob.glob(os.path.join(args.clips_dir, "clip*.mp4")), key=natural_key)
    if not clip_paths:
        raise SystemExit(f"no clip*.mp4 found in {args.clips_dir}")

    calls = []

    def log(msg):
        print(msg, file=sys.stderr)

    k = kmcp.Kimo(args.kimo, env={"KIMO_SPECTATOR": "off"}, log=os.environ.get("KIMO_MCP_LOG"))

    def call(tool, a, required=True):
        """tools/call over MCP, timed. Fails loudly (SystemExit) on any MCP error, unless required=False."""
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
            raise SystemExit(f"{tool} failed over MCP (project at {project} for inspection): {text}")
        try:
            parsed = json.loads(text) if text else {}
        except ValueError:
            parsed = {"_raw": text}
        return (not is_err, parsed)

    per_clip = []  # one row per rough-cut segment clip
    notes = []

    try:
        call("project_set", {"width": 720, "height": 1280, "fps": 30})

        # ---- 1. import every clip --------------------------------------------------------
        asset_ids = {}
        asset_order = []
        for path in clip_paths:
            name = os.path.basename(path)
            _, r = call("import_media", {"path": path})
            asset_ids[name] = r["asset_id"]
            asset_order.append(name)
        log(f"imported {len(asset_order)} clips: {asset_order}")

        # ---- 5 (mechanically first: a clip_id only exists once it's on a track) ----------
        # assemble on the timeline, the same call run_demo_mcp.py uses to build its dialogue
        # track: dead air cut out, clips laid back to back in story (folder) order.
        _, rough = call("auto_rough_cut", {"asset_ids": [asset_ids[n] for n in asset_order]})
        segments = rough["clips"]  # [{clip_id, asset_id, start, end, in_point}, ...]
        asset_to_name = {v: k for k, v in asset_ids.items()}
        log(f"auto_rough_cut: {len(segments)} segment(s) on track {rough['track_id']} "
            f"({rough['duration']:.2f}s total)")

        seg_clip_ids = [seg["clip_id"] for seg in segments]

        # ---- 2. auto_captions, batched across every segment -------------------------------
        captions_ok = False
        if not whisper_on:
            note = "KIMO_WHISPER_BIN/KIMO_WHISPER_MODEL not set -- skipping auto_captions and trim_filler_words " \
                   "cleanly (auto_captions's own documented behaviour with no model installed); " \
                   "fit_clip_duration still runs, protect_words is then a no-op (no linked words)."
            log(f"NOTE: {note}")
            notes.append(note)
        else:
            ok, cap = call("auto_captions", {"clip_ids": seg_clip_ids}, required=False)
            if ok:
                captions_ok = True
                log(f"auto_captions: {cap.get('words', 0)} words, language={cap.get('language')}, "
                    f"confidence={cap.get('confidence')}")
            else:
                err_text = calls[-1].get("error", "")
                if "no speech model is installed" in err_text:
                    note = f"auto_captions skipped (no speech model installed, despite KIMO_WHISPER_* being set): {err_text}"
                    log(f"NOTE: {note}")
                    notes.append(note)
                else:
                    raise SystemExit(f"auto_captions failed for a reason other than the no-model case; not continuing silently: {err_text}")

        # ---- 3 + 4. per-clip trim_filler_words then fit_clip_duration ---------------------
        for seg in segments:
            clip_id = seg["clip_id"]
            asset_name = asset_to_name.get(seg["asset_id"], seg["asset_id"])
            original_duration = round(seg["end"] - seg["start"], 3)

            row = {"name": asset_name, "clip_id": clip_id, "original_duration": original_duration}

            # 3. trim_filler_words -- only when this clip has captions to pick filler spans from
            if captions_ok:
                _, trim = call("trim_filler_words", {"clip_id": clip_id, "words": filler_words, "retime_captions": True})
                trim_removed = trim.get("removed_seconds", 0.0)
                row["words_matched"] = trim.get("words_matched", 0)
                row["filler_removed_seconds"] = round(trim_removed, 3)
                row["after_filler_duration"] = round(original_duration - trim_removed, 3)
                row["trim_captions_shifted"] = trim.get("captions_shifted", 0)
                row["trim_captions_trimmed"] = trim.get("captions_trimmed", 0)
                row["trim_captions_removed"] = trim.get("captions_removed", 0)
            else:
                row["words_matched"] = 0
                row["filler_removed_seconds"] = 0.0
                row["after_filler_duration"] = original_duration
                row["trim_captions_shifted"] = 0
                row["trim_captions_trimmed"] = 0
                row["trim_captions_removed"] = 0

            # 4. fit_clip_duration -- target the clip's ORIGINAL on-timeline duration x (1 - pct),
            # so if step 3 already cut enough this is a clean no-op (met_target true, 0 needed).
            target = round(original_duration * (1.0 - args.max_per_clip), 3)
            target = max(target, 0.01)
            _, fit = call("fit_clip_duration", {
                "clip_id": clip_id, "max_duration": target,
                "threshold_db": "auto", "protect_words": True,
                "retime_captions": True,
            })
            row["target_duration"] = target
            row["after_fit_duration"] = round(fit.get("new_duration", row["after_filler_duration"]), 3)
            row["met_target"] = fit.get("met_target")
            row["shortfall_seconds"] = round(fit.get("shortfall_seconds", 0.0), 3)
            row["words_protected"] = fit.get("words_protected", 0)
            row["measured_noise_floor_db"] = fit.get("measured_noise_floor_db")
            row["chosen_threshold_db"] = fit.get("threshold_db")
            row["fit_captions_shifted"] = fit.get("captions_shifted", 0)
            row["fit_captions_trimmed"] = fit.get("captions_trimmed", 0)
            row["fit_captions_removed"] = fit.get("captions_removed", 0)
            if fit.get("note"):
                row["fit_note"] = fit["note"]
                notes.append(f"{asset_name}: {fit['note']}")

            per_clip.append(row)

        # ---- 6. export ----------------------------------------------------------------
        _, result = call("export", {"path": out_path, "crf": 20})

    finally:
        k.close()

    total_original = round(sum(r["original_duration"] for r in per_clip), 3)
    total_after_filler = round(sum(r["after_filler_duration"] for r in per_clip), 3)
    total_after_fit = round(sum(r["after_fit_duration"] for r in per_clip), 3)

    # ---- print the per-clip table -------------------------------------------------------
    hdr = f"{'clip':<10} {'orig':>7} {'+filler':>8} {'+fit':>7} {'matched':>8} {'protected':>10} {'met':>5} {'caps s/t':>9}"
    print(hdr)
    print("-" * len(hdr))
    for r in per_clip:
        caps = f"{r['trim_captions_shifted']+r['fit_captions_shifted']}/{r['trim_captions_trimmed']+r['fit_captions_trimmed']}"
        print(f"{r['name']:<10} {r['original_duration']:>7.2f} {r['after_filler_duration']:>8.2f} "
              f"{r['after_fit_duration']:>7.2f} {r['words_matched']:>8} {r['words_protected']:>10} "
              f"{str(r['met_target']):>5} {caps:>9}")
    print("-" * len(hdr))
    print(f"{'TOTAL':<10} {total_original:>7.2f} {total_after_filler:>8.2f} {total_after_fit:>7.2f}")
    print()
    for n in notes:
        print(f"NOTE: {n}")
    print(f"\nwrote {out_path} ({result['bytes']/1e6:.1f} MB, {result['duration']:.1f}s) -- "
          f"{len(calls)} MCP calls, {sum(1 for c in calls if c['is_error'])} errors")

    summary = {
        "clips_dir": args.clips_dir,
        "whisper_on": whisper_on,
        "max_per_clip_pct": args.max_per_clip,
        "filler_words": filler_words,
        "per_clip": per_clip,
        "totals": {
            "original_duration": total_original,
            "after_filler_duration": total_after_filler,
            "after_fit_duration": total_after_fit,
            "cut_seconds": round(total_original - total_after_fit, 3),
            "cut_pct": round((total_original - total_after_fit) / total_original, 4) if total_original else 0.0,
        },
        "notes": notes,
        "export": result,
        "total_calls": len(calls),
        "errors": [c for c in calls if c["is_error"]],
        "total_ms": round(sum(c["ms"] for c in calls), 1),
    }
    with open(report_path, "w") as f:
        json.dump(summary, f, indent=2)
    log(f"wrote summary to {report_path}")


if __name__ == "__main__":
    main()
