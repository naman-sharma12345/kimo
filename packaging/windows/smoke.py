"""Check the packaged MCP binary from an unrelated working directory."""
import json
import pathlib
import subprocess
import sys
import tempfile

binary = pathlib.Path(sys.argv[1]).resolve()
requests = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"mode": "flat"}},
    {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "list_fonts", "arguments": {}}},
]
with tempfile.TemporaryDirectory() as cwd:
    proc = subprocess.run([str(binary)], input="".join(json.dumps(r) + "\n" for r in requests),
                          text=True, capture_output=True, cwd=cwd, timeout=60, check=True)
responses = {r["id"]: r for r in map(json.loads, proc.stdout.splitlines())}
assert responses[1]["result"]["serverInfo"]["name"] == "kimo"
assert len(responses[2]["result"]["tools"]) >= 16
assert len(responses[3]["result"]["tools"]) >= 255
if (binary.parent.parent / "fonts").is_dir():
    assert not responses[4]["result"].get("isError"), responses[4]
    assert "Montserrat" in json.dumps(responses[4]), "Bundled fonts not discovered"
print("Windows MCP smoke passed (initialize, grouped and flat tools)")
