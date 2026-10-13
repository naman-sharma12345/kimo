#!/usr/bin/env python3
"""Runs tests/fresh_clone_row.sh (git clone of HEAD to another temp path, tier entrypoints in list mode, hardcoded-path grep). Needs a git checkout with HEAD committed; writes only to a temp dir."""
import os, subprocess, sys
here = os.path.dirname(os.path.abspath(__file__))
sys.exit(subprocess.run(["bash", os.path.join(here, "fresh_clone_row.sh")]).returncode)
