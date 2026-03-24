"""
FILE: python/src/multifrost/process.py
PURPOSE: Launch and manage spawned service processes without owning caller transport state.
OWNS: SpawnOptions, ServiceProcess, and the immediate child-process launch helper.
EXPORTS: SpawnOptions, ServiceProcess, spawn.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path
from typing import Any, TypedDict

from typing_extensions import Unpack

from .protocol import ROUTER_PORT_ENV

_DEFAULT_STOP_TIMEOUT = 5.0


class SpawnOptions(TypedDict, total=False):
    router_port: int


class ServiceProcess:
    """Thin wrapper around a spawned child process."""

    def __init__(self, child: subprocess.Popen[Any]) -> None:
        self._child = child

    def id(self) -> int | None:
        return self._child.pid

    def stop(self) -> None:
        if self._child.poll() is not None:
            return
        self._child.terminate()
        try:
            self._child.wait(timeout=_DEFAULT_STOP_TIMEOUT)
        except subprocess.TimeoutExpired:
            self._child.kill()
            self._child.wait()

    def wait(self) -> int:
        return self._child.wait()


def spawn(
    service_entrypoint: str,
    executable: str | None = None,
    **options: Unpack[SpawnOptions],
) -> ServiceProcess:
    entrypoint_path = _canonical_service_entrypoint(service_entrypoint)
    command = [executable or sys.executable, entrypoint_path]

    env = os.environ.copy()
    env["MULTIFROST_ENTRYPOINT_PATH"] = entrypoint_path
    router_port = options.get("router_port")
    if router_port is not None:
        env[ROUTER_PORT_ENV] = str(router_port)

    child = subprocess.Popen(
        command,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return ServiceProcess(child)


def _canonical_service_entrypoint(service_entrypoint: str) -> str:
    path = Path(service_entrypoint).expanduser()
    try:
        return str(path.resolve(strict=False))
    except RuntimeError:
        return str(path.absolute())


__all__ = ["SpawnOptions", "ServiceProcess", "spawn"]
