"""
FILE: python/src/multifrost/transport.py
PURPOSE: Own the live WebSocket transport used by both caller and service peers.
OWNS: WebSocket connection lifecycle, register ack handling, pending request tracking, and inbound frame routing.
EXPORTS: TransportHandlers, WebSocketTransport.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import asyncio
import contextlib
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any

from websockets.asyncio.client import connect as ws_connect
from websockets.exceptions import ConnectionClosed, WebSocketException

from .errors import ErrorOrigin, RegistrationError, TransportError
from .frame import FrameParts, decode_body, decode_frame, encode_body, encode_frame
from .protocol import (
    Envelope,
    ErrorBody,
    PeerClass,
    RegisterAckBody,
    RegisterBody,
    build_disconnect_envelope,
    build_register_envelope,
    call,
    disconnect,
    heartbeat,
    query,
    response,
)
from .protocol import (
    error as kind_error,
)

FrameHandler = Callable[[FrameParts], Awaitable[None] | None]


@dataclass(slots=True)
class TransportHandlers:
    on_response: FrameHandler | None = None
    on_error: FrameHandler | None = None
    on_call: FrameHandler | None = None
    on_query: FrameHandler | None = None
    on_heartbeat: FrameHandler | None = None
    on_disconnect: FrameHandler | None = None


@dataclass(slots=True)
class _PendingRequest:
    future: asyncio.Future[FrameParts]


class WebSocketTransport:
    def __init__(
        self,
        *,
        peer_id: str,
        peer_class: PeerClass,
        router_endpoint: str,
        handlers: TransportHandlers | None = None,
        register_timeout: float = 10.0,
        request_timeout: float | None = 30.0,
    ) -> None:
        self.peer_id = peer_id
        self.peer_class = peer_class
        self.router_endpoint = router_endpoint
        self.handlers = handlers or TransportHandlers()
        self.register_timeout = register_timeout
        self.request_timeout = request_timeout
        self._websocket: Any = None
        self._receiver_task: asyncio.Task[None] | None = None
        self._pending: dict[str, _PendingRequest] = {}
        self._send_lock = asyncio.Lock()
        self._closed_event = asyncio.Event()
        self._active = False
        self._closed = False
        self._close_reason: str | None = None

    @property
    def active(self) -> bool:
        return self._active and not self._closed

    async def start(self) -> RegisterAckBody:
        if self._websocket is not None:
            raise TransportError("transport already started")

        try:
            self._websocket = await ws_connect(
                self.router_endpoint,
                open_timeout=5.0,
                close_timeout=1.0,
                ping_interval=None,
                max_size=None,
            )
        except (OSError, TimeoutError, WebSocketException) as err:
            raise TransportError(
                f"failed to connect websocket to {self.router_endpoint}",
                cause=err,
            ) from err

        try:
            ack = await self._send_register_and_wait_ack()
        except Exception:
            await self.close(graceful=False)
            raise

        self._closed_event.clear()
        self._active = True
        self._receiver_task = asyncio.create_task(self._receive_loop())
        return ack

    async def request(
        self,
        envelope: Envelope,
        body_bytes: bytes,
        *,
        timeout: float | None = None,
    ) -> FrameParts:
        if not self.active:
            raise TransportError("transport is not active")

        loop = asyncio.get_running_loop()
        future: asyncio.Future[FrameParts] = loop.create_future()
        self._pending[envelope.msg_id] = _PendingRequest(future=future)
        frame_bytes = encode_frame(envelope, body_bytes)
        try:
            async with self._send_lock:
                await self._websocket.send(frame_bytes)
        except Exception as err:
            self._pending.pop(envelope.msg_id, None)
            raise TransportError("failed to send websocket frame", cause=err) from err

        effective_timeout = self.request_timeout if timeout is None else timeout
        try:
            if effective_timeout is None:
                return await future
            return await asyncio.wait_for(future, timeout=effective_timeout)
        except asyncio.TimeoutError as err:
            self._pending.pop(envelope.msg_id, None)
            raise TransportError(
                f"timed out waiting for response to msg_id={envelope.msg_id}",
                cause=err,
            ) from err

    async def send(self, envelope: Envelope, body_bytes: bytes) -> None:
        if not self.active:
            raise TransportError("transport is not active")
        frame_bytes = encode_frame(envelope, body_bytes)
        try:
            async with self._send_lock:
                await self._websocket.send(frame_bytes)
        except Exception as err:
            raise TransportError("failed to send websocket frame", cause=err) from err

    async def close(self, *, graceful: bool = True) -> None:
        if self._closed:
            return
        self._closed = True
        self._active = False

        if graceful and self._websocket is not None:
            with contextlib.suppress(Exception):
                disconnect_envelope = build_disconnect_envelope(self.peer_id)
                await self._websocket.send(encode_frame(disconnect_envelope, b""))

        if self._websocket is not None:
            with contextlib.suppress(Exception):
                await self._websocket.close()

        if self._receiver_task is not None:
            self._receiver_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._receiver_task

        self._fail_pending("transport closed")
        self._websocket = None
        self._closed_event.set()

    async def wait_closed(self) -> None:
        await self._closed_event.wait()

    async def _send_register_and_wait_ack(self) -> RegisterAckBody:
        register_body = RegisterBody(peer_id=self.peer_id, peer_class=self.peer_class)
        envelope = build_register_envelope(self.peer_id)
        await self._websocket.send(encode_frame(envelope, encode_body(register_body)))

        try:
            raw_message = await asyncio.wait_for(
                self._websocket.recv(), timeout=self.register_timeout
            )
        except asyncio.TimeoutError as err:
            raise RegistrationError(
                f"timed out waiting for register ack at {self.router_endpoint}",
                reason="timeout",
            ) from err
        except ConnectionClosed as err:
            raise RegistrationError(
                "router closed the websocket during registration",
                reason="router closed",
            ) from err
        except Exception as err:
            raise RegistrationError(
                f"failed while waiting for register ack: {err}",
                reason=str(err),
            ) from err

        if not isinstance(raw_message, (bytes, bytearray, memoryview)):
            raise RegistrationError(
                f"expected binary register ack, got {type(raw_message)!r}",
                reason="non-binary register reply",
            )

        frame = decode_frame(bytes(raw_message))
        if frame.envelope.kind == response:
            return self._decode_register_ack(frame)
        if frame.envelope.kind == kind_error:
            body = self._decode_error_body(frame)
            raise RegistrationError(
                body.message,
                reason=body.message,
                origin=ErrorOrigin.ROUTER,
            )
        raise RegistrationError(
            f"unexpected register reply kind: {frame.envelope.kind}",
            reason=f"unexpected kind: {frame.envelope.kind}",
        )

    def _decode_register_ack(self, frame: FrameParts) -> RegisterAckBody:
        body_data = decode_body(frame.body_bytes)
        if not isinstance(body_data, dict):
            raise RegistrationError(
                f"invalid register ack body type: {type(body_data)!r}",
                reason="invalid register ack body",
            )
        ack = RegisterAckBody.from_mapping(body_data)
        if not ack.accepted:
            raise RegistrationError(
                ack.reason or "router rejected registration",
                reason=ack.reason,
                origin=ErrorOrigin.ROUTER,
            )
        return ack

    def _decode_error_body(self, frame: FrameParts) -> ErrorBody:
        body_data = decode_body(frame.body_bytes)
        if not isinstance(body_data, dict):
            raise TransportError(
                f"invalid error body type: {type(body_data)!r}"
            )
        return ErrorBody.from_mapping(body_data)

    async def _receive_loop(self) -> None:
        try:
            while True:
                raw_message = await self._websocket.recv()
                if not isinstance(raw_message, (bytes, bytearray, memoryview)):
                    raise TransportError(
                        f"expected binary websocket frame, got {type(raw_message)!r}"
                    )

                frame = decode_frame(bytes(raw_message))
                kind = frame.envelope.kind

                if kind == response:
                    if await self._resolve_pending(frame):
                        continue
                    await self._dispatch(self.handlers.on_response, frame)
                    continue

                if kind == kind_error:
                    if await self._resolve_pending(frame):
                        continue
                    await self._dispatch(self.handlers.on_error, frame)
                    continue

                if kind == call:
                    await self._dispatch(self.handlers.on_call, frame)
                    continue

                if kind == query:
                    await self._dispatch(self.handlers.on_query, frame)
                    continue

                if kind == heartbeat:
                    await self._dispatch(self.handlers.on_heartbeat, frame)
                    continue

                if kind == disconnect:
                    await self._dispatch(self.handlers.on_disconnect, frame)
                    break

                raise TransportError(f"unexpected message kind: {kind}")
        except ConnectionClosed:
            pass
        except Exception as err:
            self._close_reason = str(err)
        finally:
            self._active = False
            self._closed = True
            self._fail_pending(self._close_reason or "transport closed")
            self._closed_event.set()

    async def _resolve_pending(self, frame: FrameParts) -> bool:
        pending = self._pending.pop(frame.envelope.msg_id, None)
        if pending is None:
            return False
        if not pending.future.done():
            pending.future.set_result(frame)
        return True

    async def _dispatch(self, handler: FrameHandler | None, frame: FrameParts) -> None:
        if handler is None:
            return
        result = handler(frame)
        if asyncio.iscoroutine(result):
            await result

    def _fail_pending(self, reason: str) -> None:
        error = TransportError(reason)
        for pending in self._pending.values():
            if not pending.future.done():
                pending.future.set_exception(error)
        self._pending.clear()


__all__ = ["TransportHandlers", "WebSocketTransport"]
