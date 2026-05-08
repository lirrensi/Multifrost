"""
FILE: python/src/multifrost/router_bootstrap.py
PURPOSE: Discover, probe, and lazily start the shared v5 router process.
OWNS: Router endpoint resolution, startup lock handling, router reachability checks, and process spawning.
EXPORTS: RouterBootstrapConfig, router_port_from_env, default_router_lock_path, default_router_log_path, router_ws_url, router_is_reachable, bootstrap_router.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import os
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from websockets.asyncio.client import connect as ws_connect
from websockets.exceptions import InvalidHandshake, InvalidURI, WebSocketException

from .errors import BootstrapError
from .protocol import (
    DEFAULT_ROUTER_PORT,
    ROUTER_BIN_ENV,
    ROUTER_LOCK_PATH_SUFFIX,
    ROUTER_LOG_PATH_SUFFIX,
    ROUTER_PORT_ENV,
)

_DEFAULT_READINESS_TIMEOUT = 10.0
_DEFAULT_POLL_INTERVAL = 0.1
_DEFAULT_LOCK_STALE_AFTER = 10.0
_ROUTER_HOST = "127.0.0.1"

# Fields present in every JSON lock file (for reference, not consumed by code).
LOCK_FILE_FIELDS = [
    "format",
    "pid",
    "router_pid",
    "port",
    "created_at_unix",
    "expires_at_unix",
    "status",
    "language",
]


def router_port_from_env() -> int:
    raw_value = os.environ.get(ROUTER_PORT_ENV)
    if raw_value is None or raw_value.strip() == "":
        return DEFAULT_ROUTER_PORT
    try:
        return int(raw_value)
    except ValueError:
        return DEFAULT_ROUTER_PORT


def default_router_lock_path() -> Path:
    return Path.home() / ROUTER_LOCK_PATH_SUFFIX


def default_router_log_path() -> Path:
    return Path.home() / ROUTER_LOG_PATH_SUFFIX


def router_ws_url(port: int | None = None) -> str:
    return f"ws://{_ROUTER_HOST}:{port or router_port_from_env()}"


def router_bin_path() -> str:
    override = os.environ.get(ROUTER_BIN_ENV)
    if override:
        return override
    return "multifrost-router"


@dataclass(slots=True)
class RouterBootstrapConfig:
    port: int = DEFAULT_ROUTER_PORT
    router_bin: str = field(default_factory=router_bin_path)
    lock_path: Path = field(default_factory=default_router_lock_path)
    log_path: Path = field(default_factory=default_router_log_path)
    readiness_timeout: float = _DEFAULT_READINESS_TIMEOUT
    poll_interval: float = _DEFAULT_POLL_INTERVAL
    lock_stale_after: float = _DEFAULT_LOCK_STALE_AFTER

    @property
    def endpoint(self) -> str:
        return router_ws_url(self.port)

    @classmethod
    def from_env(cls, port_override: int | None = None) -> RouterBootstrapConfig:
        port = port_override if port_override is not None else router_port_from_env()
        return cls(port=port)


async def router_is_reachable(endpoint: str) -> bool:
    try:
        async with ws_connect(
            endpoint,
            open_timeout=1.0,
            close_timeout=1.0,
            ping_interval=None,
            max_size=None,
        ):
            return True
    except (OSError, TimeoutError, InvalidHandshake, InvalidURI, WebSocketException):
        return False


async def bootstrap_router(
    config: RouterBootstrapConfig | None = None,
) -> RouterBootstrapConfig:
    config = config or RouterBootstrapConfig.from_env()
    if await router_is_reachable(config.endpoint):
        return config

    async with _StartupLock(
        config.lock_path,
        config.lock_stale_after,
        timeout=config.readiness_timeout,
        poll_interval=config.poll_interval,
        port=config.port,
    ) as lock:
        if lock._skip_spawn:
            deadline = time.monotonic() + config.readiness_timeout
            while time.monotonic() < deadline:
                if await router_is_reachable(config.endpoint):
                    return config
                await asyncio.sleep(config.poll_interval)
            raise BootstrapError(
                f"router did not become reachable at {config.endpoint} within "
                f"{config.readiness_timeout:.1f}s",
                timed_out=True,
            )

        if await router_is_reachable(config.endpoint):
            lock.update(status="ready")
            return config

        process = await _spawn_router_process(config)
        lock.update(router_pid=process.pid)

        try:
            await _wait_for_router_ready(config, process)
            lock.update(status="ready")
        except Exception:
            lock.update(status="failed")
            await _terminate_router_process(process)
            raise

    return config


async def _wait_for_router_ready(
    config: RouterBootstrapConfig, process: asyncio.subprocess.Process
) -> None:
    deadline = time.monotonic() + config.readiness_timeout
    while time.monotonic() < deadline:
        if await router_is_reachable(config.endpoint):
            return
        if process.returncode is not None:
            raise BootstrapError(
                f"router exited before becoming reachable (code={process.returncode})"
            )
        await asyncio.sleep(config.poll_interval)

    raise BootstrapError(
        f"router did not become reachable at {config.endpoint} within "
        f"{config.readiness_timeout:.1f}s",
        timed_out=True,
    )


async def _spawn_router_process(
    config: RouterBootstrapConfig,
) -> asyncio.subprocess.Process:
    config.lock_path.parent.mkdir(parents=True, exist_ok=True)
    config.log_path.parent.mkdir(parents=True, exist_ok=True)
    log_file = config.log_path.open("ab", buffering=0)
    env = os.environ.copy()
    env[ROUTER_PORT_ENV] = str(config.port)

    try:
        process = await asyncio.create_subprocess_exec(
            config.router_bin,
            stdout=log_file,
            stderr=log_file,
            env=env,
        )
    except FileNotFoundError as err:
        raise BootstrapError(
            f"router binary not found: {config.router_bin}", cause=err
        ) from err
    except Exception as err:  # pragma: no cover - defensive
        raise BootstrapError(f"failed to start router: {err}", cause=err) from err
    finally:
        log_file.close()

    return process


async def _terminate_router_process(process: asyncio.subprocess.Process) -> None:
    if process.returncode is not None:
        return
    process.kill()
    with contextlib.suppress(ProcessLookupError):
        await process.wait()


@dataclass(slots=True)
class _StartupLock:
    path: Path
    stale_after: float
    timeout: float = _DEFAULT_READINESS_TIMEOUT
    poll_interval: float = _DEFAULT_POLL_INTERVAL
    port: int = DEFAULT_ROUTER_PORT
    _fd: int | None = field(init=False, default=None)
    _skip_spawn: bool = field(init=False, default=False)

    async def __aenter__(self) -> _StartupLock:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        deadline = time.monotonic() + self.timeout
        while True:
            try:
                self._fd = os.open(
                    self.path,
                    os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                    0o644,
                )
            except FileExistsError:
                if time.monotonic() >= deadline:
                    raise BootstrapError(
                        "timed out waiting for router bootstrap lock"
                    ) from None

                match self._evaluate_existing_lock():
                    case "reclaim":
                        with contextlib.suppress(FileNotFoundError):
                            self.path.unlink()
                        continue
                    case "skip_spawn":
                        self._skip_spawn = True
                        return self
                    case "wait":
                        await asyncio.sleep(self.poll_interval)
                        continue
                    case _:
                        await asyncio.sleep(self.poll_interval)
                        continue

            lock_data = {
                "format": "v1",
                "pid": os.getpid(),
                "router_pid": None,
                "port": self.port,
                "created_at_unix": time.time(),
                "expires_at_unix": time.time() + self.timeout,
                "status": "starting",
                "language": "python",
            }
            os.write(self._fd, json.dumps(lock_data).encode())
            os.close(self._fd)
            self._fd = None
            return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        if not self._skip_spawn:
            if exc_type is not None:
                self.update(status="failed")
            self.release()

    def _evaluate_existing_lock(self) -> str:
        """Apply the 6 stale-detection rules from the Lock File Format spec.

        Returns one of:
        - "reclaim"    — lock is stale/expired/we can take it
        - "skip_spawn" — holder has a live router, try connecting first
        - "wait"       — lock is valid, holder is alive, retry later
        """
        try:
            raw = self.path.read_text()
            data = json.loads(raw)
        except (OSError, json.JSONDecodeError, ValueError):
            return "reclaim"

        if not isinstance(data, dict) or data.get("format") != "v1":
            return "reclaim"

        if data.get("expires_at_unix", 0) < time.time():
            return "reclaim"

        if not self._is_process_alive(data.get("pid", 0)):
            return "reclaim"

        if data.get("status") == "failed":
            return "reclaim"

        router_pid = data.get("router_pid")
        if router_pid is not None and self._is_process_alive(router_pid):
            return "skip_spawn"

        return "wait"

    @staticmethod
    def _is_process_alive(pid: int) -> bool:
        if pid <= 0:
            return False
        if os.name == "nt":  # Windows
            try:
                import subprocess

                result = subprocess.run(
                    ["tasklist", "/FI", f"PID eq {pid}", "/FO", "csv", "/NH"],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                return str(pid) in result.stdout
            except Exception:
                return False
        else:  # Unix
            try:
                os.kill(pid, 0)
                return True
            except OSError:
                return False

    def update(self, **fields: Any) -> None:
        """Update lock file fields in-place (best-effort, no throw)."""
        try:
            raw = self.path.read_text()
            data = json.loads(raw)
        except Exception:
            return
        data.update(fields)
        with contextlib.suppress(Exception):
            self.path.write_text(json.dumps(data))

    def release(self) -> None:
        if self._fd is not None:
            with contextlib.suppress(OSError):
                os.close(self._fd)
            self._fd = None
        with contextlib.suppress(FileNotFoundError):
            self.path.unlink()


__all__ = [
    "RouterBootstrapConfig",
    "router_port_from_env",
    "default_router_lock_path",
    "default_router_log_path",
    "router_ws_url",
    "router_bin_path",
    "router_is_reachable",
    "bootstrap_router",
]
