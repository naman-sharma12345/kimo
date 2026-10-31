"""auto_edit_benchmark: raw clips folder -> one finished edit, via Kimo's own auto_edit tool (MCP).

Built to make the quality benchmark reproducible instead of a one-off shell session: point it at a folder of
raw clips and it imports them, calls auto_edit with sensible defaults, exports an mp4, and prints the numbers
(duration, mix_lufs, lint_clean, warnings) auto_edit itself already hands back -- the same honest read a human
watching the render would want before trusting it.

Clip discovery in <clips_dir> (by filename, case-insensitive, sorted within each group so multi-take talking
heads land in story order):
  talkinghead* / talking_head* / head*   -> talking_head_asset_ids (required: at least one)
  broll* / b_roll* / cutaway*            -> broll_asset_ids (optional)
  music* / bed*                          -> music_asset_id (optional, first match only)
Anything else in the folder is ignored (so a project file or a readme sitting alongside the clips is fine).

Usage:
  python3 examples/auto_edit_benchmark.py <clips_dir> <out.mp4> [--hook "Cold-open line"] [--no-music]
                                           [--caption-style pop|karaoke|clean|boxed|impact] [--platform none|tiktok|reels|shorts]
                                           [--exe path/to/kimo-mcp] [--project /tmp/scratch.kimo]

Exit: prints one JSON line (the fields that matter for a before/after comparison) then auto_edit's own full
result on stderr for anyone who wants the rest. Non-zero exit on any tool error (KimoError), its text on stderr.
"""
import argparse, json, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import Kimo, KimoError

GROUPS = {
    "talking_head": re.compile(r"^(talking[_-]?head|head)", re.I),
    "broll": re.compile(r"^(b[_-]?roll|cutaway)", re.I),
    "music": re.compile(r"^(music|bed)", re.I),
}


def discover(clips_dir):
    groups = {"talking_head": [], "broll": [], "music": []}
    for name in sorted(os.listdir(clips_dir)):
        path = os.path.join(clips_dir, name)
        if not os.path.isfile(path):
            continue
        for g, pat in GROUPS.items():
            if pat.match(name):
                groups[g].append(path)
                break
    return groups


def default_exe():
    for cand in (
        os.environ.get("KIMO_MCP_EXE"),
        "target/release/kimo-mcp",
        "target/gate/kimo-mcp",
        os.path.join(os.environ.get("CARGO_TARGET_DIR", ""), "gate", "kimo-mcp") if os.environ.get("CARGO_TARGET_DIR") else None,
        os.path.join(os.environ.get("CARGO_TARGET_DIR", ""), "release", "kimo-mcp") if os.environ.get("CARGO_TARGET_DIR") else None,
    ):
        if cand and os.path.isfile(cand):
            return cand
    return "target/release/kimo-mcp"  # let Kimo() raise a clear FileNotFoundError if truly absent


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("clips_dir")
    ap.add_argument("out")
    ap.add_argument("--hook", default=None, help="cold-open hook line; omitted = auto_edit skips the hook honestly")
    ap.add_argument("--no-music", action="store_true", help="skip even an auto-discovered music* file")
    ap.add_argument("--caption-style", default="boxed", help="auto_captions style (default boxed, YouTube-norm pill)")
    ap.add_argument("--platform", default="none")
    ap.add_argument("--target-lufs", type=float, default=-14.0)
    ap.add_argument("--exe", default=None)
    ap.add_argument("--project", default=None, help="scratch .kimo project path (default: alongside --out)")
    args = ap.parse_args()

    groups = discover(args.clips_dir)
    if not groups["talking_head"]:
        sys.exit(f"no talking-head clip found in {args.clips_dir} (name it talkinghead*.mp4 or head*.mp4)")

    project = args.project or (os.path.splitext(args.out)[0] + ".kimo")
    exe = args.exe or default_exe()
    k = Kimo(exe=exe)
    try:
        k.call("project_set", width=1280, height=720, fps=30)

        def import_all(paths):
            return [k.call("import_media", path=os.path.abspath(p))["asset_id"] for p in paths]

        th_ids = import_all(groups["talking_head"])
        broll_ids = import_all(groups["broll"])
        music_id = None
        if groups["music"] and not args.no_music:
            music_id = import_all(groups["music"][:1])[0]

        auto_edit_args = dict(
            talking_head_asset_ids=th_ids,
            broll_asset_ids=broll_ids,
            caption_style=args.caption_style,
            platform=args.platform,
            target_lufs=args.target_lufs,
        )
        if music_id:
            auto_edit_args["music_asset_id"] = music_id
        if args.hook:
            auto_edit_args["hook_text"] = args.hook

        result = k.call("auto_edit", **auto_edit_args)
        k.call("save_project", path=os.path.abspath(project))
        k.call("export", path=os.path.abspath(args.out))

        summary = {
            "out": args.out,
            "duration": result.get("duration"),
            "mix_lufs": result.get("mix_lufs"),
            "lint_clean": result.get("lint_clean"),
            "warnings": result.get("warnings", []),
            "remaining_problems": [p.get("code") for p in result.get("remaining_problems", [])],
            "talking_head_clips": len(th_ids),
            "broll_clips": len(broll_ids),
            "music": bool(music_id),
        }
        print(json.dumps(summary, indent=2))
        print(json.dumps(result, indent=2), file=sys.stderr)
    except KimoError as e:
        sys.exit(f"{e.tool} failed: {e.text}" + (f" (hint: {e.hint})" if e.hint else ""))
    finally:
        k.close()


if __name__ == "__main__":
    main()
