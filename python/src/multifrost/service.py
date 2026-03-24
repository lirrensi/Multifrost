"""
FILE: python/src/multifrost/service.py
PURPOSE: Expose the lightweight service worker base and async service runner for v5 peers.
OWNS: ServiceWorker, ServiceContext, run_service, run_service_sync, and call dispatch on public methods.
EXPORTS: ServiceWorker, ServiceContext, run_service, run_service_sync.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import asyncio
import inspect
import os
import sys
import traceback
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import __main__

from .errors import ErrorOrigin, RemoteCallError, TransportError
from .frame import FrameParts, decode_body, encode_body
from .protocol import (
    CallBody,
    ErrorBody,
    PeerClass,
    ResponseBody,
    build_error_envelope,
    build_response_envelope,
)
from .router_bootstrap import RouterBootstrapConfig, bootstrap_router
from .transport import TransportHandlers, WebSocketTransport


class ServiceWorker:
    """Lightweight base class for service peers."""


@dataclass(slots=True)
class ServiceContext:
    peer_id: str | None = None
    router_port: int | None = None
    runtime_metadata: dict[str, Any] = field(default_factory=dict)


async def run_service(service_worker: ServiceWorker, service_context: ServiceContext) -> None:
    peer_id = _resolve_service_peer_id(service_context)
    bootstrap_config = RouterBootstrapConfig.from_env(service_context.router_port)
    await bootstrap_router(bootstrap_config)

    transport = WebSocketTransport(
        peer_id=peer_id,
        peer_class=PeerClass.SERVICE,
        router_endpoint=bootstrap_config.endpoint,
        register_timeout=10.0,
        request_timeout=None,
    )

    async def on_call(frame: FrameParts) -> None:
        await _handle_call(service_worker, transport, peer_id, frame)

    transport.handlers = TransportHandlers(on_call=on_call)

    try:
        await transport.start()
        await transport.wait_closed()
    except asyncio.CancelledError:
        await transport.close(graceful=True)
        raise
    finally:
        await transport.close(graceful=False)


def run_service_sync(service_worker: ServiceWorker, service_context: ServiceContext) -> None:
    asyncio.run(run_service(service_worker, service_context))


async def _handle_call(
    service_worker: ServiceWorker,
    transport: WebSocketTransport,
    peer_id: str,
    frame: Any,
) -> None:
    try:
        call_body = _decode_call_body(frame.body_bytes)
        result = await _invoke_service_method(service_worker, call_body)
        response_envelope = build_response_envelope(
            peer_id,
            frame.envelope.from_peer,
            frame.envelope.msg_id,
        )
        await transport.send(response_envelope, encode_body(ResponseBody(result)))
    except RemoteCallError as err:
        await _send_remote_error(transport, peer_id, frame.envelope.from_peer, frame.envelope.msg_id, err)
    except Exception as err:
        error = RemoteCallError(
            "application_error",
            str(err),
            kind="application",
            origin=ErrorOrigin.REMOTE,
            details={"type": type(err).__name__},
            stack=traceback.format_exc(),
        )
        await _send_remote_error(
            transport,
            peer_id,
            frame.envelope.from_peer,
            frame.envelope.msg_id,
            error,
        )


def _decode_call_body(body_bytes: bytes) -> CallBody:
    payload = decode_body(body_bytes)
    if not isinstance(payload, dict):
        raise RemoteCallError(
            "invalid_call_body",
            f"invalid call body type: {type(payload)!r}",
            kind="library",
            origin=ErrorOrigin.REMOTE,
        )
    return CallBody.from_mapping(payload)


async def _invoke_service_method(
    service_worker: ServiceWorker,
    call_body: CallBody,
) -> Any:
    function_name = call_body.function.strip()
    if not function_name:
        raise RemoteCallError(
            "function_not_found",
            "call body is missing a function name",
            kind="library",
            origin=ErrorOrigin.REMOTE,
        )
    if function_name.startswith("_"):
        raise RemoteCallError(
            "private_method",
            f"refusing to call private method {function_name!r}",
            kind="library",
            origin=ErrorOrigin.REMOTE,
        )

    method = getattr(service_worker, function_name, None)
    if method is None or not callable(method):
        raise RemoteCallError(
            "function_not_found",
            f"service method {function_name!r} was not found",
            kind="library",
            origin=ErrorOrigin.REMOTE,
        )

    result = method(*call_body.args)
    if inspect.isawaitable(result):
        return await result
    return result


async def _send_remote_error(
    transport: WebSocketTransport,
    from_peer: str,
    to_peer: str,
    msg_id: str,
    err: RemoteCallError,
) -> None:
    envelope = build_error_envelope(from_peer, to_peer, msg_id)
    body = ErrorBody(
        code=err.code,
        message=err.message,
        kind=err.kind,
        stack=err.stack,
        details=err.details,
    )
    await transport.send(envelope, encode_body(body))


def _resolve_service_peer_id(context: ServiceContext) -> str:
    if context.peer_id:
        return context.peer_id

    entrypoint = os.environ.get("MULTIFROST_ENTRYPOINT_PATH")
    if entrypoint:
        return _canonicalize_path(entrypoint)

    main_file = context.runtime_metadata.get("main_file")
    if main_file is None:
        main_file = getattr(__main__, "__file__", None)
    if main_file:
        return _canonicalize_path(str(main_file))

    argv0 = context.runtime_metadata.get("argv0")
    if argv0 is None and sys.argv:
        argv0 = sys.argv[0]
    if argv0:
        return _canonicalize_path(str(argv0))

    raise TransportError("unable to resolve service peer_id")


def _canonicalize_path(path_value: str) -> str:
    path = Path(path_value).expanduser()
    return str(path.resolve(strict=False))


__all__ = [
    "ServiceWorker",
    "ServiceContext",
    "run_service",
    "run_service_sync",
]
