#!/usr/bin/env python3
"""
Matrix harness for the v5 router-based caller/service topology.

This suite starts one router, keeps four service peers online, and exercises the
16 caller/service combinations across Rust, Python, Node.js, and Go callers.
"""

from __future__ import annotations

import asyncio
import os
import pwd
import socket
import subprocess
import sys
import time
from pathlib import Path

try:
    import pytest
except ModuleNotFoundError:  # pragma: no cover - convenience for direct execution
    pytest = None

REPO_ROOT = Path(__file__).resolve().parents[3]
PYTHON_SRC = REPO_ROOT / "python" / "src"
if str(PYTHON_SRC) not in sys.path:
    sys.path.insert(0, str(PYTHON_SRC))

try:  # noqa: E402
    from multifrost import PeerClass, connect
except ModuleNotFoundError:  # pragma: no cover - direct smoke fallback
    PeerClass = None
    connect = None

SERVICE_PEERS = {
    "rust": "math-rust",
    "python": "math-python",
    "node": "math-node",
    "go": "math-go",
}


def _log(message: str) -> None:
    print(f"[v5] {message}", flush=True)


def _router_binary() -> Path:
    binary = REPO_ROOT / "router" / "target" / "debug" / "multifrost-router"
    if os.name == "nt":
        binary = binary.with_suffix(".exe")
    return binary


def _python_service_entrypoint() -> Path:
    return REPO_ROOT / "python" / "examples" / "math_worker_service.py"


def _node_service_entrypoint() -> Path:
    return REPO_ROOT / "javascript" / "examples" / "math_worker_service.ts"


def _rust_service_command() -> list[str]:
    return ["cargo", "run", "--quiet", "--example", "e2e_math_worker", "--", "--service"]


def _go_service_command() -> list[str]:
    return ["go", "run", "./examples/e2e_math_worker"]


def _rust_caller_command(target_peer_id: str) -> tuple[list[str], Path]:
    return (
        [
            "cargo",
            "run",
            "--quiet",
            "--example",
            "connect",
            "--",
            "--target",
            target_peer_id,
            "--timeout-ms",
            "10000",
        ],
        REPO_ROOT / "rust",
    )


def _python_caller_command(target_peer_id: str) -> tuple[list[str], Path]:
    return (
        [
            sys.executable,
            str(REPO_ROOT / "python" / "examples" / "py_calls_rust.py"),
            "--target",
            target_peer_id,
        ],
        REPO_ROOT / "python",
    )


def _node_caller_command(target_peer_id: str) -> tuple[list[str], Path]:
    return (
        [
            "node",
            str(REPO_ROOT / "javascript" / "node_modules" / "tsx" / "dist" / "cli.mjs"),
            str(REPO_ROOT / "javascript" / "examples" / "node_calls_py.ts"),
            "--target",
            target_peer_id,
        ],
        REPO_ROOT / "javascript",
    )


def _go_caller_command(target_peer_id: str) -> tuple[list[str], Path]:
    return (
        ["go", "run", "./examples/go_calls_rust", "--target", target_peer_id],
        REPO_ROOT / "golang",
    )


def _free_tcp_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _prepare_python_deps(deps_dir: Path) -> None:
    # Install runtime deps into a plain target directory so the harness stays
    # portable and does not depend on a local virtual environment layout.
    if (deps_dir / "msgpack").exists() and (deps_dir / "websockets").exists():
        return

    deps_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--target",
            str(deps_dir),
            "msgpack",
            "websockets",
        ],
        cwd=str(REPO_ROOT),
        check=True,
        text=True,
    )


def _wait_for_tcp_listener(port: int, timeout: float = 15.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.25):
                return
        except OSError:
            time.sleep(0.05)
    raise AssertionError(f"router did not begin listening on port {port}")


def _base_env(port: int, python_deps_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["HOME"] = pwd.getpwuid(os.getuid()).pw_dir
    env["PYTHONNOUSERSITE"] = "1"
    env["MULTIFROST_ROUTER_PORT"] = str(port)
    env["MULTIFROST_ROUTER_BIN"] = str(_router_binary())

    python_paths = [str(python_deps_dir), str(PYTHON_SRC)]
    if env.get("PYTHONPATH"):
        python_paths.append(env["PYTHONPATH"])
    env["PYTHONPATH"] = os.pathsep.join(python_paths)

    return env


def _start_process(command: list[str], cwd: Path, env: dict[str, str]) -> subprocess.Popen[str]:
    return subprocess.Popen(
        command,
        cwd=str(cwd),
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        text=True,
    )


def _stop_process(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return

    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=10)


def _run_caller(command: list[str], cwd: Path, env: dict[str, str], label: str) -> None:
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        env=env,
        capture_output=True,
        text=True,
        check=False,
        timeout=120,
    )
    if completed.returncode != 0:
        raise AssertionError(
            f"{label} failed for matrix run\n"
            f"command: {command}\n"
            f"cwd: {cwd}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )

    expected = [
        "add(10, 20) = 30",
        "multiply(7, 8) = 56",
        "factorial(5) = 120",
    ]
    for needle in expected:
        if needle not in completed.stdout:
            raise AssertionError(
                f"{label} output missing {needle!r}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
            )


async def _wait_for_peer(handle: object, peer_id: str, timeout: float = 20.0) -> None:
    deadline = asyncio.get_running_loop().time() + timeout
    while True:
        if await handle.query_peer_exists(peer_id):  # type: ignore[attr-defined]
            return
        if asyncio.get_running_loop().time() >= deadline:
            raise AssertionError(f"peer {peer_id!r} did not appear in router presence")
        await asyncio.sleep(0.05)


async def _run_four_service_matrix(tmp_path: Path) -> None:
    port = _free_tcp_port()
    home_dir = tmp_path / "home"
    python_deps_dir = tmp_path / "python_deps"
    _log(f"preparing temp python deps in {python_deps_dir}")
    home_dir.mkdir()
    _prepare_python_deps(python_deps_dir)
    env = _base_env(port, python_deps_dir)
    router_env = env.copy()
    router_env["HOME"] = str(home_dir)

    _log(f"starting router on port {port}")
    router = _start_process([str(_router_binary())], REPO_ROOT / "router", router_env)
    services: list[subprocess.Popen[str]] = []

    try:
        _wait_for_tcp_listener(port)
        _log("router is listening")

        service_specs = [
            ("rust", _rust_service_command(), REPO_ROOT / "rust"),
            ("python", [sys.executable, str(_python_service_entrypoint())], REPO_ROOT / "python"),
            (
                "node",
                [
                    "node",
                    str(REPO_ROOT / "javascript" / "node_modules" / "tsx" / "dist" / "cli.mjs"),
                    str(_node_service_entrypoint()),
                ],
                REPO_ROOT / "javascript",
            ),
            ("go", _go_service_command(), REPO_ROOT / "golang"),
        ]

        for name, command, cwd in service_specs:
            service_env = env.copy()
            service_env["MULTIFROST_PEER_ID"] = SERVICE_PEERS[name]
            _log(f"starting {name} service as {SERVICE_PEERS[name]}")
            services.append(_start_process(command, cwd, service_env))

        if connect is not None and PeerClass is not None:
            _log("waiting for all four services to appear in router presence")
            probe = connect("math-rust", router_port=port, caller_peer_id="matrix-probe")
            handle = probe.handle()
            try:
                await handle.start()
                for peer_id in SERVICE_PEERS.values():
                    await _wait_for_peer(handle, peer_id)

                for peer_id in SERVICE_PEERS.values():
                    peer = await handle.query_peer_get(peer_id)
                    assert peer.exists is True
                    assert peer.connected is True
                    assert peer.peer_class is PeerClass.SERVICE
                _log("router presence verified")
            finally:
                await handle.stop()
        else:
            await asyncio.sleep(2.0)

        _log("running 16 caller/service combinations")
        caller_specs = [
            ("rust", _rust_caller_command),
            ("python", _python_caller_command),
            ("node", _node_caller_command),
            ("go", _go_caller_command),
        ]

        for caller_name, command_factory in caller_specs:
            for service_name, target_peer_id in SERVICE_PEERS.items():
                command, cwd = command_factory(target_peer_id)
                _log(f"{caller_name} caller -> {service_name} service")
                _run_caller(command, cwd, env, f"{caller_name} caller -> {service_name} service")
        _log("matrix complete")
    finally:
        for process in reversed(services):
            _stop_process(process)
        _stop_process(router)


if pytest is not None:
    test_four_service_matrix = pytest.mark.asyncio(_run_four_service_matrix)
else:  # pragma: no cover - convenience for direct execution
    test_four_service_matrix = _run_four_service_matrix


async def main() -> None:
    from tempfile import TemporaryDirectory

    with TemporaryDirectory() as temp_dir:
        await _run_four_service_matrix(Path(temp_dir))


if __name__ == "__main__":  # pragma: no cover - direct smoke execution helper
    asyncio.run(main())
