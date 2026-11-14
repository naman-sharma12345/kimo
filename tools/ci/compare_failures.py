"""Compare a cargo test log with the list of tests already known to fail on this OS.

usage: compare_failures.py <cargo test log> <known failures file> <write current failures here>

Exit 1 when a test fails that is not in the known list, or when a test binary failed to compile. Tests that now pass
but are still listed are reported so the list can shrink. With no known list yet, the failures are reported and saved
(exit 0) so the first run on an OS can record its baseline.
"""
import re
import sys


def failures(log: str) -> set[str]:
    return {m.group(1) for m in re.finditer(r"^test (\S+) \.\.\. FAILED$", log, re.M)}


def main() -> int:
    log_path, known_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    log = open(log_path, encoding="utf-8", errors="replace").read()
    now = failures(log)
    open(out_path, "w", encoding="utf-8").write("".join(f"{t}\n" for t in sorted(now)))
    passed = sum(int(m.group(1)) for m in re.finditer(r"^test result: \w+\. (\d+) passed", log, re.M))
    compile_errors = re.findall(r"^error: could not compile .*$", log, re.M)
    print(f"{passed} passed, {len(now)} failed")
    if compile_errors:
        print("COMPILE ERRORS:\n  " + "\n  ".join(compile_errors))
        return 1
    try:
        known = {l.strip() for l in open(known_path, encoding="utf-8") if l.strip() and not l.startswith("#")}
    except FileNotFoundError:
        print(f"no {known_path} yet: these {len(now)} failures are the baseline (saved to {out_path}; commit it as {known_path})")
        return 0
    new = sorted(now - known)
    fixed = sorted(known - now)
    if fixed:
        print(f"{len(fixed)} known failure(s) now pass; remove them from {known_path}:\n  " + "\n  ".join(fixed))
    if new:
        print(f"{len(new)} NEW failure(s):\n  " + "\n  ".join(new))
        return 1
    print("no new failures")
    return 0


if __name__ == "__main__":
    sys.exit(main())
