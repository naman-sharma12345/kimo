#!/usr/bin/env python3
"""Gate runner. Usage: python3 tests/gate_run.py fast|full [--no-build]
Builds the release binaries of this commit (unless --no-build), runs every entry of the tier from tests/gate_manifest.txt, prints a table
{test, status pass|fail|skipped(reason), seconds, counts}, and exits 1 on any fail. Python proofs run with fixed env: KIMO_BIN (release kimo-mcp of this
commit), KIMO_FONTS, PYTHONPATH=examples. A proof prints 'SKIP: <reason>' and exits 77 to be reported as skipped; a proof that needs the oracle binary
(KIMO_ORACLE, default <tempdir>/kimo-oracle) is reported skipped(oracle unavailable) when it is absent. Fails if a proof leaves a stray file in the repo."""
import tempfile
import os, re, subprocess, sys, time
root = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
os.chdir(root)
tier = sys.argv[1] if len(sys.argv) > 1 else "fast"; nobuild = "--no-build" in sys.argv; pyonly = "--py-only" in sys.argv; listonly = "--list" in sys.argv   # --py-only: skip every cargo step (proofs against the existing release binary)
assert tier in ("fast", "full"), "tier must be fast or full"
PROFILE = os.environ.get("KIMO_GATE_PROFILE", "gate")
env = dict(os.environ); tdir = env.get("CARGO_TARGET_DIR", os.path.join(root, "target"))
env["KIMO_BIN"] = os.path.join(tdir, "release", "kimo-mcp"); env.setdefault("KIMO_ORACLE", os.path.join(tempfile.gettempdir(), "kimo-oracle"))
env["PYTHONPATH"] = os.path.join(root, "examples") + os.pathsep + env.get("PYTHONPATH", "")
if os.path.isdir(os.path.join(root, "assets/fonts")): env.setdefault("KIMO_FONTS", os.path.join(root, "assets/fonts"))
if tier == "fast": env["KIMO_FUZZ_SMOKE"] = "1"
rows, t0 = [], time.time()
def sh(cmd, timeout=None, e=None):
    t = time.time()
    try: p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=e or env); rc, out = p.returncode, p.stdout + p.stderr
    except subprocess.TimeoutExpired as x: rc, out = 124, (x.stdout or b"").decode() if isinstance(x.stdout, bytes) else (x.stdout or "")
    return rc, out, time.time() - t
def git_state(): return subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True).stdout
def counts(out):
    p = f = 0
    for m in re.finditer(r"test result: \w+\. (\d+) passed; (\d+) failed", out): p += int(m.group(1)); f += int(m.group(2))
    return p, f
if not nobuild and not pyonly and not listonly:
    rc, out, sec = sh(["cargo", "build", "--release", "--workspace", "--bins"]); rows.append(("build release bins", "pass" if rc == 0 else "fail", sec, ""))
    if rc: print(out[-2000:])
ent = []
for line in open("tests/gate_manifest.txt"):
    s = line.split("#")[0].split()
    if len(s) == 3 and s[2] in ("fast", "full") and (tier == "full" or s[2] == "fast"): ent.append(s)
rc, out, sec = sh([sys.executable, "tests/gate_check.py"]); rows.append(("gate_check (manifest)", "pass" if rc == 0 else "fail", sec, out.strip().splitlines()[0] if out else ""))
if rc: print(out)
if listonly:   # --list: print the tier entries and the manifest check, run nothing else (used by tests/fresh_clone_row.sh)
    print(f"tier={tier} entries={len(ent)}"); [print("  " + " ".join(e)) for e in ent]; sys.exit(0 if not rc else 1)
if not pyonly:
    rc, out, sec = sh(["cargo", "test", "--profile", PROFILE, "-p", "kimo-core", "--lib"]); p, f = counts(out); rows.append(("rust lib", "pass" if rc == 0 else "fail", sec, f"{p} passed {f} failed"))
    if rc: print("\n".join(l for l in out.splitlines() if "FAILED" in l or "panicked" in l)[:2000])
before = git_state()
for kind, name, _ in ent:
    if kind == "rust":
        if pyonly: continue
        rc, out, sec = sh(["cargo", "test", "--profile", PROFILE, "-p", "kimo-core", "--test", name]); p, f = counts(out)
        rows.append((f"rust {name}", "pass" if rc == 0 else "fail", sec, f"{p} passed {f} failed"))
        if rc: print("\n".join(l for l in out.splitlines() if "FAILED" in l or "panicked" in l)[:1500])
    else:
        path = "tests/" + name
        if "kimo-oracle" in open(path).read() and not os.path.exists(env["KIMO_ORACLE"]):
            rows.append((f"py {name}", f"skipped(oracle unavailable: {env['KIMO_ORACLE']} missing)", 0, "")); continue
        subprocess.run(["pkill", "kimo-mcp"], capture_output=True)
        a = [sys.executable, path]
        rc, out, sec = sh([sys.executable, path] + ([env["KIMO_BIN"]] if name == "discovery_metric.py" else []), timeout=int(os.environ.get("PROOF_TIMEOUT", "300")))
        last = out.strip().splitlines()[-1] if out.strip() else ""
        st = "pass" if rc == 0 else (f"skipped({last.split('SKIP:', 1)[1].strip()})" if rc == 77 and "SKIP:" in last else "fail")
        after = git_state()
        if after != before: rows.append((f"py {name}", "fail", sec, "stray files: " + " ".join(sorted(set(after.splitlines()) ^ set(before.splitlines())))[:200])); before = after; continue
        rows.append((f"py {name}", st, sec, last[:80] if st != "pass" else ""))
        if st == "fail": print(f"--- {name} (rc={rc}) tail:\n" + "\n".join(out.strip().splitlines()[-12:]))
w = max(len(r[0]) for r in rows)
print(f"\n{'test'.ljust(w)}  status                 seconds  detail")
for r in rows: print(f"{r[0].ljust(w)}  {r[1][:22].ljust(22)} {r[2]:7.1f}  {r[3]}")
fails = [r for r in rows if r[1] == "fail"]; skips = [r for r in rows if r[1].startswith("skipped")]
print(f"\ntier={tier} total={len(rows)} pass={len(rows)-len(fails)-len(skips)} fail={len(fails)} skipped={len(skips)} wall={time.time()-t0:.0f}s sha={subprocess.run(['git','rev-parse','--short','HEAD'],capture_output=True,text=True).stdout.strip()}")
for r in skips: print("  skipped:", r[0], r[1])
sys.exit(1 if fails else 0)
