"""Fetch the Linux font manifest and licenses concurrently for Windows packages."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import re
import urllib.request

root = Path(__file__).resolve().parents[2]
manifest = (root / "fonts/fetch.sh").read_text().split("<<'LIST'", 1)[1].split("\nLIST", 1)[0]
licenses = root / "target/font-licenses"
licenses.mkdir(parents=True, exist_ok=True)
base = "https://raw.githubusercontent.com/google/fonts/main/ofl/"

def fetch(line):
    family, original = line.split("/", 1)
    name = re.sub(r"%5B.*%5D", "-Var", original)
    font = root / "fonts" / name
    license = licenses / (family + "-OFL.txt")
    for url, dest, minimum in [(base + line, font, 10000), (base + family + "/OFL.txt", license, 100)]:
        if dest.exists() and dest.stat().st_size >= minimum:
            continue
        for attempt in range(3):
            try:
                with urllib.request.urlopen(url, timeout=30) as response:
                    data = response.read()
                if len(data) < minimum:
                    raise ValueError("Truncated download")
                if dest.suffix == ".ttf" and data[:4] not in (b"\x00\x01\x00\x00", b"OTTO", b"ttcf"):
                    raise ValueError("Download is not a font")
                dest.write_bytes(data)
                break
            except Exception as error:
                if attempt == 2:
                    print(f"Skipping {line}: {error}", flush=True)
                    return False
    return True

with ThreadPoolExecutor(max_workers=12) as pool:
    count = sum(pool.map(fetch, [line.strip() for line in manifest.splitlines() if line.strip()]))
print(f"Downloaded {count} fonts with licenses", flush=True)
if count < 70:
    raise SystemExit("Need at least 70 fonts")
