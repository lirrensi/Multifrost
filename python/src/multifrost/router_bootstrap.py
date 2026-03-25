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
    ):
        if await router_is_reachable(config.endpoint):
            return config

        process = await _spawn_router_process(config)
        try:
            await _wait_for_router_ready(config, process)
        except Exception:
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
    _fd: int | None = field(init=False, default=None)

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
                if await self._remove_if_stale():
                    continue
                if time.monotonic() >= deadline:
                    raise BootstrapError(
                        "timed out waiting for router bootstrap lock"
                    ) from None
                await asyncio.sleep(self.poll_interval)
                continue

            os.write(
                self._fd,
                f"pid={os.getpid()} ts={time.time():.6f}\n".encode(),
            )
            os.close(self._fd)
            self._fd = None
            return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        self.release()

    async def _remove_if_stale(self) -> bool:
        try:
            age = time.time() - self.path.stat().st_mtime
        except FileNotFoundError:
            return True
        if age > self.stale_after:
            with contextlib.suppress(FileNotFoundError):
                self.path.unlink()
            return True
        return False

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
