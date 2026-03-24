"""
FILE: python/src/multifrost/connection.py
PURPOSE: Expose the caller-side v5 configuration object and async handle surface.
OWNS: connect(), Connection, Handle, target validation, and caller-side remote call/query flow.
EXPORTS: ConnectOptions, connect, Connection, Handle.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from typing import Any, TypedDict
from uuid import uuid4

from typing_extensions import Unpack

from .errors import (
    ErrorOrigin,
    RemoteCallError,
    RouterError,
    TransportError,
    error_from_body,
)
from .frame import FrameParts, decode_body, encode_body
from .protocol import (
    ROUTER_PEER_ID,
    CallBody,
    ErrorBody,
    PeerClass,
    QueryBody,
    QueryExistsResponseBody,
    QueryGetResponseBody,
    ResponseBody,
    build_call_envelope,
    build_query_envelope,
    response,
)
from .protocol import (
    error as error_kind,
)
from .router_bootstrap import RouterBootstrapConfig, bootstrap_router
from .transport import WebSocketTransport


class ConnectOptions(TypedDict, total=False):
    caller_peer_id: str
    router_port: int
    bootstrap_timeout: float
    request_timeout: float
    register_timeout: float
    eager_validate_target: bool


def connect(target_peer_id: str, **options: Unpack[ConnectOptions]) -> Connection:
    return Connection(target_peer_id, **options)


class Connection:
    """Caller configuration only; no live socket or process state."""

    def __init__(self, target_peer_id: str, /, **options: Any) -> None:
        target_peer_id = str(target_peer_id).strip()
        if not target_peer_id:
            raise ValueError("target_peer_id must not be empty")

        self._target_peer_id = target_peer_id
        self._caller_peer_id = _clean_optional_str(options.get("caller_peer_id"))
        self._router_port = _clean_optional_int(options.get("router_port"))
        self._bootstrap_timeout = _clean_optional_float(
            options.get("bootstrap_timeout"), default=10.0
        )
        self._request_timeout = _clean_optional_float(
            options.get("request_timeout"), default=30.0
        )
        self._register_timeout = _clean_optional_float(
            options.get("register_timeout"), default=10.0
        )
        self._eager_validate_target = bool(options.get("eager_validate_target", False))

    @property
    def target_peer_id(self) -> str:
        return self._target_peer_id

    @property
    def caller_peer_id(self) -> str | None:
        return self._caller_peer_id

    @property
    def router_port(self) -> int | None:
        return self._router_port

    @property
    def bootstrap_timeout(self) -> float:
        return self._bootstrap_timeout

    @property
    def request_timeout(self) -> float:
        return self._request_timeout

    @property
    def register_timeout(self) -> float:
        return self._register_timeout

    @property
    def eager_validate_target(self) -> bool:
        return self._eager_validate_target

    def handle(self) -> Handle:
        return Handle(self)

    def handle_sync(self) -> HandleSync:
        from .sync import HandleSync

        return HandleSync(self)


class _AsyncCallNamespace:
    def __init__(self, handle: Handle) -> None:
        self._handle = handle

    def __getattr__(self, function_name: str) -> Callable[..., Awaitable[Any]]:
        async def invoke(*args: Any, namespace: str | None = None) -> Any:
            return await self._handle._invoke_remote(
                function_name,
                list(args),
                namespace=namespace,
            )

        return invoke


class Handle:
    """Async caller runtime that owns one live transport."""

    def __init__(self, connection: Connection) -> None:
        self._connection = connection
        self._caller_peer_id = connection.caller_peer_id or str(uuid4())
        self._transport: WebSocketTransport | None = None
        self.call = _AsyncCallNamespace(self)

    @property
    def peer_id(self) -> str:
        return self._caller_peer_id

    @property
    def target_peer_id(self) -> str:
        return self._connection.target_peer_id

    async def start(self) -> None:
        if self._transport is not None and self._transport.active:
            return

        bootstrap_config = RouterBootstrapConfig.from_env(self._connection.router_port)
        bootstrap_config.readiness_timeout = self._connection.bootstrap_timeout
        await bootstrap_router(bootstrap_config)

        self._transport = WebSocketTransport(
            peer_id=self._caller_peer_id,
            peer_class=PeerClass.CALLER,
            router_endpoint=bootstrap_config.endpoint,
            register_timeout=self._connection.register_timeout,
            request_timeout=self._connection.request_timeout,
        )

        try:
            await self._transport.start()
            if self._connection.eager_validate_target:
                target = await self.query_peer_get(self._connection.target_peer_id)
                self._validate_target_peer(target)
        except Exception:
            await self.stop()
            raise

    async def stop(self) -> None:
        if self._transport is None:
            return
        try:
            await self._transport.close(graceful=True)
        finally:
            self._transport = None

    async def query_peer_exists(self, peer_id: str) -> bool:
        body = QueryBody.peer_exists(peer_id)
        frame = await self._router_request(body)
        response_body = self._decode_query_exists(frame)
        return response_body.exists

    async def query_peer_get(self, peer_id: str) -> QueryGetResponseBody:
        body = QueryBody.peer_get(peer_id)
        frame = await self._router_request(body)
        return self._decode_query_get(frame)

    async def _router_request(self, body: QueryBody) -> FrameParts:
        transport = self._require_transport()
        envelope = build_query_envelope(self._caller_peer_id)
        frame = await transport.request(envelope, encode_body(body))
        if frame.envelope.kind == response:
            return frame
        if frame.envelope.kind == error_kind:
            raise self._error_from_frame(frame)
        raise TransportError(f"unexpected router reply kind: {frame.envelope.kind}")

    def _decode_query_exists(self, frame: FrameParts) -> QueryExistsResponseBody:
        payload = decode_body(frame.body_bytes)
        if not isinstance(payload, dict):
            raise TransportError(f"invalid query response body type: {type(payload)!r}")
        return QueryExistsResponseBody.from_mapping(payload)

    def _decode_query_get(self, frame: FrameParts) -> QueryGetResponseBody:
        payload = decode_body(frame.body_bytes)
        if not isinstance(payload, dict):
            raise TransportError(f"invalid query response body type: {type(payload)!r}")
        return QueryGetResponseBody.from_mapping(payload)

    async def _invoke_remote(
        self,
        function_name: str,
        args: list[Any],
        *,
        namespace: str | None = None,
    ) -> Any:
        transport = self._require_transport()
        call_body = CallBody(function=function_name, args=args, namespace=namespace)
        envelope = build_call_envelope(self._caller_peer_id, self._connection.target_peer_id)
        frame = await transport.request(envelope, encode_body(call_body))
        if frame.envelope.kind == response:
            payload = decode_body(frame.body_bytes)
            if not isinstance(payload, dict):
                raise TransportError(
                    f"invalid response body type: {type(payload)!r}"
                )
            response_body = ResponseBody.from_mapping(payload)
            return response_body.result
        if frame.envelope.kind == error_kind:
            raise self._error_from_frame(frame)
        raise TransportError(f"unexpected router reply kind: {frame.envelope.kind}")

    def _error_from_frame(self, frame: FrameParts) -> RouterError | RemoteCallError:
        payload = decode_body(frame.body_bytes)
        if not isinstance(payload, dict):
            raise TransportError(f"invalid error body type: {type(payload)!r}")
        error_body = ErrorBody.from_mapping(payload)
        origin = (
            ErrorOrigin.ROUTER
            if frame.envelope.from_peer == ROUTER_PEER_ID
            else ErrorOrigin.REMOTE
        )
        return error_from_body(error_body, origin=origin)

    def _validate_target_peer(self, target: QueryGetResponseBody) -> None:
        if not target.exists or not target.connected or target.peer_class is None:
            raise RouterError(
                "peer_not_found",
                f"target peer {self._connection.target_peer_id} is not live",
            )
        if target.peer_class is not PeerClass.SERVICE:
            raise RouterError(
                "invalid_target_class",
                f"target peer {self._connection.target_peer_id} is not a service",
            )

    def _require_transport(self) -> WebSocketTransport:
        if self._transport is None or not self._transport.active:
            raise TransportError("caller handle has not been started")
        return self._transport

    async def __aenter__(self) -> Handle:
        await self.start()
        return self

    async def __aexit__(self, exc_type: Any, exc: Any, tb: Any) -> None:
        await self.stop()


def _clean_optional_str(value: Any) -> str | None:
    if value is None:
        return None
    cleaned = str(value).strip()
    return cleaned or None


def _clean_optional_int(value: Any) -> int | None:
    if value is None:
        return None
    return int(value)


def _clean_optional_float(value: Any, *, default: float) -> float:
    if value is None:
        return default
    return float(value)


from .sync import HandleSync  # noqa: E402  # isort: skip

__all__ = ["ConnectOptions", "connect", "Connection", "Handle", "HandleSync"]
