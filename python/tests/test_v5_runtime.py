"""
FILE: python/tests/test_v5_runtime.py
PURPOSE: Exercise caller/service round-trips, router queries, and duplicate peer rejection.
"""

from __future__ import annotations

import asyncio
import os
import socket
import time
from pathlib import Path

import pytest

from multifrost import connect, spawn
from multifrost.errors import TransportError
from multifrost.protocol import PeerClass


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def example_path(name: str) -> Path:
    return Path(__file__).resolve().parents[1] / "examples" / name


def router_binary() -> Path:
    root = Path(__file__).resolve().parents[2]
    binary = root / "router" / "target" / "debug" / "multifrost-router"
    if os.name == "nt":
        binary = binary.with_suffix(".exe")
    return binary


def wait_for_peer_exists_sync(handle, peer_id: str, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    while True:
        if handle.query_peer_exists(peer_id):
            return
        if time.monotonic() >= deadline:
            raise AssertionError(f"peer {peer_id!r} did not appear in time")
        time.sleep(0.05)


async def wait_for_peer_exists_async(
    handle, peer_id: str, timeout: float = 15.0
) -> None:
    deadline = asyncio.get_running_loop().time() + timeout
    while True:
        if await handle.query_peer_exists(peer_id):
            return
        if asyncio.get_running_loop().time() >= deadline:
            raise AssertionError(f"peer {peer_id!r} did not appear in time")
        await asyncio.sleep(0.05)


@pytest.mark.asyncio
async def test_async_handle_roundtrip_and_queries(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    port = free_port()
    monkeypatch.setenv("MULTIFROST_ROUTER_PORT", str(port))
    monkeypatch.setenv("MULTIFROST_ROUTER_BIN", str(router_binary()))

    service = spawn(str(example_path("math_worker_service.py")), router_port=port)
    connection = connect("math-service", router_port=port)
    handle = connection.handle()

    try:
        await handle.start()
        await wait_for_peer_exists_async(handle, "math-service")

        assert await handle.query_peer_exists("math-service") is True
        peer = await handle.query_peer_get("math-service")
        assert peer.exists is True
        assert peer.peer_class == PeerClass.SERVICE

        assert await handle.call.add(10, 20) == 30
        assert await handle.call.multiply(4, 7) == 28
        assert await handle.call.factorial(5) == 120
    finally:
        await handle.stop()
        service.stop()
        service.wait()


def test_sync_handle_roundtrip(monkeypatch: pytest.MonkeyPatch) -> None:
    port = free_port()
    monkeypatch.setenv("MULTIFROST_ROUTER_PORT", str(port))
    monkeypatch.setenv("MULTIFROST_ROUTER_BIN", str(router_binary()))

    service = spawn(str(example_path("math_worker_service.py")), router_port=port)
    connection = connect("math-service", router_port=port)
    handle = connection.handle_sync()

    try:
        handle.start()
        wait_for_peer_exists_sync(handle, "math-service")

        assert handle.query_peer_exists("math-service") is True
        peer = handle.query_peer_get("math-service")
        assert peer.exists is True
        assert peer.peer_class == PeerClass.SERVICE

        assert handle.call.add(11, 19) == 30
        assert handle.call.multiply(6, 7) == 42
    finally:
        handle.stop()
        service.stop()
        service.wait()


def test_sync_handle_stop_prevents_future_use() -> None:
    connection = connect("math-service")
    handle = connection.handle_sync()

    handle.stop()
    coro = handle._handle.query_peer_exists("math-service")

    try:
        with pytest.raises(TransportError, match="HandleSync has been stopped"):
            handle._run(coro)
    finally:
        coro.close()


@pytest.mark.asyncio
async def test_duplicate_live_peer_id_rejected(monkeypatch: pytest.MonkeyPatch) -> None:
    port = free_port()
    monkeypatch.setenv("MULTIFROST_ROUTER_PORT", str(port))
    monkeypatch.setenv("MULTIFROST_ROUTER_BIN", str(router_binary()))

    first = spawn(str(example_path("math_worker_service.py")), router_port=port)
    connection = connect("math-service", router_port=port)
    handle = connection.handle()

    try:
        await handle.start()
        await wait_for_peer_exists_async(handle, "math-service")

        second = spawn(str(example_path("math_worker_service.py")), router_port=port)
        exit_code = await asyncio.wait_for(asyncio.to_thread(second.wait), timeout=15.0)
        assert exit_code != 0
    finally:
        await handle.stop()
        first.stop()
        first.wait()
