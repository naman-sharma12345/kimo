#!/usr/bin/env python3
"""Run a plan of director runs (a JSON list), score each, append to results.jsonl. Two runs at a time.

  python3 run_eval.py PLAN.json RESULTS_DIR
plan item: {"name", "model", "adapter", "set", "kimo", "tools"?, "docs"?: [paths], "extra"?: [...]}
Model credentials come from the environment: ANTHROPIC_BASE_URL/ANTHROPIC_API_KEY, OPENAI_BASE_URL/OPENAI_API_KEY.
FOOTAGE (dir with talk/, cafe/, vlog/) and SCORE_KIMO (the kimo-mcp used for scoring, fixed across runs) too.
"""
import json, os, shutil, subprocess, sys, time, concurrent.futures as cf
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import score as S

def one(it, root):
    out = os.path.join(root, it["name"]); os.makedirs(out, exist_ok=True)
    if os.path.exists(os.path.join(out, "score.json")): return json.load(open(os.path.join(out, "result.json")))
    fset = os.path.join(os.environ["FOOTAGE"], it["set"])
    # each run edits its own read-only copy of the footage: a model that writes over a source clip (seen: contact_sheet with
    # path = a source .mp4) must not change the footage of runs beside it, nor the set the scorer reads
    ro = os.path.join(out, "footage")
    if not os.path.isdir(ro):
        shutil.copytree(fset, ro, copy_function=os.link)  # hard links: no extra disk; the files are read-only
        for f in os.listdir(ro): os.chmod(os.path.join(ro, f), 0o444)
        os.chmod(ro, 0o555)
    cmd = ["timeout", "-k", "10", "3000", "nice", "-n", "5", sys.executable, os.path.join(HERE, "agent.py"), "--kimo", it["kimo"], "--brief", os.path.join(HERE, "briefs", it["set"] + ".txt"),
           "--footage", ro, "--out", out, "--model", it["model"], "--adapter", it.get("adapter", "anthropic"), "--system", os.path.join(HERE, it.get("system", "system.txt"))]
    if it.get("docs"): cmd += ["--doc", ",".join(it["docs"])]
    if it.get("tools"): cmd += ["--tools", it["tools"]]
    cmd += it.get("extra", [])
    env = dict(os.environ)
    if it.get("adapter") == "openai" and it.get("openai_base"): env["OPENAI_BASE_URL"] = it["openai_base"]
    with open(os.path.join(out, "agent.out"), "w") as fo, open(os.path.join(out, "agent.err"), "w") as fe:
        subprocess.run(cmd, stdout=fo, stderr=fe, stdin=subprocess.DEVNULL, env=env)
    run = json.load(open(os.path.join(out, "run.json"))) if os.path.exists(os.path.join(out, "run.json")) else {"stop": "crashed"}
    try: sc = S.score(out, fset, os.environ["SCORE_KIMO"], judge=True)
    except Exception as e: sc = {"finished": False, "composite": 0.0, "error": repr(e)[:300]}
    json.dump(sc, open(os.path.join(out, "score.json"), "w"), indent=1)
    res = {"name": it["name"], "set": it["set"], "model": it["model"], "tools": it.get("tools") or "grouped", "label": it.get("label", ""),
           "finished": sc.get("finished"), "composite": sc.get("composite"), "judge": (sc.get("judge") or {}).get("score"), "checks": sc.get("checks"),
           "calls": run.get("calls"), "errors": run.get("errors"), "wall_s": run.get("wall_s"), "cost_usd": run.get("cost_usd"), "stop": run.get("stop"),
           "judge_cost": (sc.get("judge") or {}).get("cost_usd"), "tokens": run.get("tokens"),
           "harness_finished": run.get("harness_finished"), "model_exported": run.get("model_exported"), "finish_phase_at_s": run.get("finish_phase_at_s"),
           "provider_retries": len(run.get("provider_retries") or [])}
    json.dump(res, open(os.path.join(out, "result.json"), "w"), indent=1)
    return res

if __name__ == "__main__":
    plan = json.load(open(sys.argv[1])); root = sys.argv[2]; os.makedirs(root, exist_ok=True); par = int(os.environ.get("PAR", "2"))
    with cf.ThreadPoolExecutor(par) as ex:
        for res in ex.map(lambda it: one(it, root), plan):
            with open(os.path.join(root, "results.jsonl"), "a") as f: f.write(json.dumps(res) + "\n")
            print(json.dumps({k: res[k] for k in ("name", "composite", "judge", "calls", "errors", "wall_s", "cost_usd", "stop")}), flush=True)
