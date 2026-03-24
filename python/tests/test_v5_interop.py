"""
FILE: python/tests/test_v5_interop.py
PURPOSE: Verify Python <-> Rust interoperability through the shared v5 router.
"""

from __future__ import annotations

import asyncio
import os
import socket
import subprocess
import time
from pathlib import Path

import pytest

from multifrost import connect, spawn


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def example_path(name: str) -> Path:
    return Path(__file__).resolve().parents[1] / "examples" / name


def rust_example_binary(name: str) -> Path:
    root = Path(__file__).resolve().parents[2]
    binary = root / "rust" / "target" / "debug" / "examples" / name
    if os.name == "nt":
        binary = binary.with_suffix(".exe")
    return binary


async def wait_for_peer_exists(handle, peer_id: str, timeout: float = 15.0) -> None:
    deadline = asyncio.get_running_loop().time() + timeout
    while True:
        if await handle.query_peer_exists(peer_id):
            return
        if asyncio.get_running_loop().time() >= deadline:
            raise AssertionError(f"peer {peer_id!r} did not register in time")
        await asyncio.sleep(0.05)


@pytest.mark.asyncio
async def test_python_caller_can_call_rust_service(monkeypatch: pytest.MonkeyPatch) -> None:
    port = free_port()
    monkeypatch.setenv("MULTIFROST_ROUTER_PORT", str(port))
    monkeypatch.setenv(
        "MULTIFROST_ROUTER_BIN",
        str(Path(__file__).resolve().parents[2] / "router" / "target" / "debug" / "multifrost-router"),
    )

    env = os.environ.copy()
    env["MULTIFROST_ROUTER_PORT"] = str(port)
    rust_service = subprocess.Popen(
        [str(rust_example_binary("e2e_math_worker")), "--service"],
        cwd=Path(__file__).resolve().parents[2] / "rust",
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    connection = connect("math-service", router_port=port)
    handle = connection.handle()

    try:
        await handle.start()
        await wait_for_peer_exists(handle, "math-service")

        assert await handle.call.add(10, 20) == 30
        assert await handle.call.multiply(6, 7) == 42
        assert await handle.call.factorial(6) == 720
    finally:
        await handle.stop()
        try:
            rust_service.wait(timeout=15)
        except subprocess.TimeoutExpired:
            rust_service.kill()
            rust_service.wait()


def test_rust_caller_can_call_python_service(monkeypatch: pytest.MonkeyPatch) -> None:
    port = free_port()
    monkeypatch.setenv("MULTIFROST_ROUTER_PORT", str(port))
    router_bin = Path(__file__).resolve().parents[2] / "router" / "target" / "debug" / "multifrost-router"
    if os.name == "nt":
        router_bin = router_bin.with_suffix(".exe")
    monkeypatch.setenv("MULTIFROST_ROUTER_BIN", str(router_bin))

    service = spawn(str(example_path("math_worker_service.py")), router_port=port)
    connection = connect("math-service", router_port=port)
    handle = connection.handle_sync()

    try:
        handle.start()
        deadline = time.monotonic() + 15.0
        while not handle.query_peer_exists("math-service"):
            if time.monotonic() >= deadline:
                raise AssertionError("python service did not register in time")
            time.sleep(0.05)

        env = os.environ.copy()
        env["MULTIFROST_ROUTER_PORT"] = str(port)
        result = subprocess.run(
            [str(rust_example_binary("connect")), "--target", "math-service", "--timeout-ms", "15000"],
            cwd=Path(__file__).resolve().parents[2] / "rust",
            env=env,
            capture_output=True,
            check=False,
            text=True,
            timeout=30,
        )

        assert result.returncode == 0, result.stderr
        assert "add(10, 20) = 30" in result.stdout
        assert "factorial(5) = 120" in result.stdout
    finally:
        handle.stop()
        service.stop()
        service.wait()
