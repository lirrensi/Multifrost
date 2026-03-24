"""
FILE: python/tests/conftest.py
PURPOSE: Make the source tree importable during pytest and expose stable repo paths.
OWNS: sys.path setup plus repo-local environment defaults for subprocess-based tests.
"""

from __future__ import annotations

import os
import socket
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
RUST = ROOT.parent / "rust"
ROUTER = ROOT.parent / "router"

if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

existing_pythonpath = os.environ.get("PYTHONPATH")
if existing_pythonpath:
    parts = existing_pythonpath.split(os.pathsep)
    if str(SRC) not in parts:
        os.environ["PYTHONPATH"] = os.pathsep.join([str(SRC), existing_pythonpath])
else:
    os.environ["PYTHONPATH"] = str(SRC)

router_bin = ROUTER / "target" / "debug" / "multifrost-router"
if os.name == "nt":
    router_bin = router_bin.with_suffix(".exe")
if router_bin.exists():
    os.environ.setdefault("MULTIFROST_ROUTER_BIN", str(router_bin))


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])
