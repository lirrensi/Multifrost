"""
FILE: python/tests/test_v5_frame_and_transport.py
PURPOSE: Exercise frame encoding, register handling, disconnect invalidation, and bootstrap locking.
"""

from __future__ import annotations

import asyncio
from contextlib import asynccontextmanager
from pathlib import Path

import pytest
from websockets.asyncio.server import serve

from multifrost import (
    ErrorBody,
    MalformedFrameError,
    RegisterAckBody,
    bootstrap_router,
    build_error_envelope,
    build_response_envelope,
    connect,
    decode_frame,
    encode_body,
    encode_frame,
    register,
)
from multifrost.errors import RegistrationError, RouterError, TransportError
from multifrost.protocol import ROUTER_PEER_ID
from multifrost.router_bootstrap import RouterBootstrapConfig


@asynccontextmanager
async def fake_router(handler: object):
    async with serve(
        handler,
        "127.0.0.1",
        0,
        ping_interval=None,
        max_size=None,
    ) as server:
        port = int(server.sockets[0].getsockname()[1])
        yield port


def test_frame_roundtrip_preserves_body_bytes() -> None:
    envelope = build_response_envelope("caller-1", "service-1", "msg-1")
    body_bytes = b"\x00\x01hello\xff"

    encoded = encode_frame(envelope, body_bytes)
    decoded = decode_frame(encoded)

    assert decoded.envelope == envelope
    assert decoded.body_bytes == body_bytes


def test_frame_rejects_short_payload() -> None:
    with pytest.raises(MalformedFrameError):
        decode_frame(b"\x00\x00\x00")


@pytest.mark.asyncio
async def test_register_ack_success_and_rejection_handling() -> None:
    async def ok_handler(websocket) -> None:
        raw = await websocket.recv()
        frame = decode_frame(raw)
        assert frame.envelope.kind == register

        ack = RegisterAckBody(accepted=True, reason=None)
        await websocket.send(
            encode_frame(
                build_response_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(ack),
            )
        )
        await websocket.wait_closed()

    async with fake_router(ok_handler) as port:
        connection = connect("math-service", router_port=port)
        handle = connection.handle()
        await handle.start()
        await handle.stop()

    async def reject_handler(websocket) -> None:
        raw = await websocket.recv()
        frame = decode_frame(raw)
        assert frame.envelope.kind == register

        body = ErrorBody(
            code="duplicate_peer",
            message="peer already registered",
            kind="router",
        )
        await websocket.send(
            encode_frame(
                build_error_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(body),
            )
        )
        await websocket.wait_closed()

    async with fake_router(reject_handler) as port:
        connection = connect("math-service", router_port=port)
        handle = connection.handle()
        with pytest.raises(RegistrationError) as excinfo:
            await handle.start()
        assert "peer already registered" in str(excinfo.value)


@pytest.mark.asyncio
async def test_disconnect_invalidates_handle_after_websocket_close() -> None:
    closed = asyncio.Event()

    async def handler(websocket) -> None:
        raw = await websocket.recv()
        frame = decode_frame(raw)
        ack = RegisterAckBody(accepted=True, reason=None)
        await websocket.send(
            encode_frame(
                build_response_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(ack),
            )
        )
        await asyncio.sleep(0.05)
        await websocket.close()
        closed.set()

    async with fake_router(handler) as port:
        connection = connect("math-service", router_port=port)
        handle = connection.handle()
        await handle.start()

        await asyncio.wait_for(closed.wait(), timeout=5.0)
        await asyncio.sleep(0.05)

        with pytest.raises(TransportError):
            await handle.call.add(1, 2)


@pytest.mark.asyncio
async def test_pending_request_fails_when_router_closes_mid_call() -> None:
    async def handler(websocket) -> None:
        raw = await websocket.recv()
        frame = decode_frame(raw)
        ack = RegisterAckBody(accepted=True, reason=None)
        await websocket.send(
            encode_frame(
                build_response_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(ack),
            )
        )

        raw = await websocket.recv()
        frame = decode_frame(raw)
        assert frame.envelope.kind == "call"
        await websocket.close()

    async with fake_router(handler) as port:
        connection = connect("math-service", router_port=port, request_timeout=1.0)
        handle = connection.handle()
        await handle.start()

        with pytest.raises(TransportError, match="transport closed"):
            await handle.call.add(1, 2)

        with pytest.raises(TransportError):
            await handle.query_peer_exists("math-service")

        await handle.stop()


@pytest.mark.asyncio
async def test_eager_validate_target_rejects_missing_peer_and_stops_handle() -> None:
    async def handler(websocket) -> None:
        raw = await websocket.recv()
        frame = decode_frame(raw)
        ack = RegisterAckBody(accepted=True, reason=None)
        await websocket.send(
            encode_frame(
                build_response_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(ack),
            )
        )

        raw = await websocket.recv()
        frame = decode_frame(raw)
        assert frame.envelope.kind == "query"
        await websocket.send(
            encode_frame(
                build_response_envelope(
                    ROUTER_PEER_ID,
                    frame.envelope.from_peer,
                    frame.envelope.msg_id,
                ),
                encode_body(
                    {
                        "peer_id": "missing-service",
                        "exists": False,
                        "class": None,
                        "connected": False,
                    }
                ),
            )
        )

    async with fake_router(handler) as port:
        handle = connect(
            "missing-service",
            router_port=port,
            eager_validate_target=True,
            caller_peer_id="python-eager-validate",
        ).handle()

        with pytest.raises(RouterError) as excinfo:
            await handle.start()

        assert excinfo.value.code == "peer_not_found"

        with pytest.raises(TransportError, match="caller handle has not been started"):
            await handle.call.add(1, 2)


@pytest.mark.asyncio
async def test_bootstrap_router_lock_spawns_once(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = RouterBootstrapConfig(
        port=54321,
        lock_path=tmp_path / "router.lock",
        log_path=tmp_path / "router.log",
        readiness_timeout=0.5,
        poll_interval=0.01,
        lock_stale_after=5.0,
    )
    reachability = {"ready": False}
    spawn_calls: list[int] = []

    async def fake_reachable(endpoint: str) -> bool:
        return reachability["ready"]

    class DummyProcess:
        returncode = None

    async def fake_spawn(cfg: RouterBootstrapConfig) -> DummyProcess:
        spawn_calls.append(cfg.port)
        reachability["ready"] = True
        return DummyProcess()

    monkeypatch.setattr(
        "multifrost.router_bootstrap.router_is_reachable", fake_reachable
    )
    monkeypatch.setattr("multifrost.router_bootstrap._spawn_router_process", fake_spawn)

    await asyncio.gather(bootstrap_router(config), bootstrap_router(config))

    assert spawn_calls == [config.port]
    assert not config.lock_path.exists()
