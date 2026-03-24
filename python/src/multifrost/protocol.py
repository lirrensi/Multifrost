"""
FILE: python/src/multifrost/protocol.py
PURPOSE: Define the canonical v5 wire constants, peer/body dataclasses, and envelope builders.
OWNS: Protocol key constants, peer class constants, envelope validation, and router-owned body shapes.
EXPORTS: PROTOCOL_KEY, PROTOCOL_VERSION, DEFAULT_ROUTER_PORT, ROUTER_PORT_ENV, ROUTER_LOG_PATH_SUFFIX, ROUTER_LOCK_PATH_SUFFIX, service, caller, Envelope, PeerClass, RegisterBody, RegisterAckBody, QueryBody, QueryExistsResponseBody, QueryGetResponseBody, CallBody, ResponseBody, ErrorBody, validate_envelope_mapping, envelope_from_mapping, now_ts, new_msg_id, build_*_envelope helpers.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

import time
from collections.abc import Mapping
from dataclasses import dataclass, field
from enum import Enum
from typing import Any
from uuid import uuid4

PROTOCOL_KEY = "multifrost_ipc_v5"
PROTOCOL_VERSION = 5
DEFAULT_ROUTER_PORT = 9981
ROUTER_PORT_ENV = "MULTIFROST_ROUTER_PORT"
ROUTER_BIN_ENV = "MULTIFROST_ROUTER_BIN"
ROUTER_LOG_PATH_SUFFIX = ".multifrost/router.log"
ROUTER_LOCK_PATH_SUFFIX = ".multifrost/router.lock"
ROUTER_PEER_ID = "router"

service = "service"
caller = "caller"

register = "register"
register_ack = "register_ack"
query = "query"
call = "call"
response = "response"
error = "error"
heartbeat = "heartbeat"
disconnect = "disconnect"

query_peer_exists = "peer.exists"
query_peer_get = "peer.get"

INT64_MIN = -(2**63)
INT64_MAX = 2**63 - 1


class PeerClass(str, Enum):
    """Peer class values used in router registry entries and register bodies."""

    SERVICE = service
    CALLER = caller

    def __str__(self) -> str:
        return self.value


@dataclass(slots=True, frozen=True)
class Envelope:
    """Canonical v5 routing envelope."""

    v: int
    kind: str
    msg_id: str
    from_peer: str
    to_peer: str
    ts: float

    def to_mapping(self) -> dict[str, Any]:
        return {
            "v": self.v,
            "kind": self.kind,
            "msg_id": self.msg_id,
            "from": self.from_peer,
            "to": self.to_peer,
            "ts": self.ts,
        }


@dataclass(slots=True, frozen=True)
class RegisterBody:
    peer_id: str
    peer_class: PeerClass

    def to_mapping(self) -> dict[str, Any]:
        return {"peer_id": self.peer_id, "class": self.peer_class.value}

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> RegisterBody:
        return cls(
            peer_id=str(mapping["peer_id"]),
            peer_class=_coerce_peer_class(mapping["class"]),
        )


@dataclass(slots=True, frozen=True)
class RegisterAckBody:
    accepted: bool
    reason: str | None = None

    def to_mapping(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"accepted": self.accepted}
        if self.reason is not None:
            payload["reason"] = self.reason
        return payload

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> RegisterAckBody:
        reason = mapping.get("reason")
        return cls(accepted=bool(mapping["accepted"]), reason=None if reason is None else str(reason))


@dataclass(slots=True, frozen=True)
class QueryBody:
    query: str
    peer_id: str

    def to_mapping(self) -> dict[str, Any]:
        return {"query": self.query, "peer_id": self.peer_id}

    @classmethod
    def peer_exists(cls, peer_id: str) -> QueryBody:
        return cls(query=query_peer_exists, peer_id=peer_id)

    @classmethod
    def peer_get(cls, peer_id: str) -> QueryBody:
        return cls(query=query_peer_get, peer_id=peer_id)

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> QueryBody:
        return cls(query=str(mapping["query"]), peer_id=str(mapping["peer_id"]))


@dataclass(slots=True, frozen=True)
class QueryExistsResponseBody:
    peer_id: str
    exists: bool
    peer_class: PeerClass | None
    connected: bool

    def to_mapping(self) -> dict[str, Any]:
        return {
            "peer_id": self.peer_id,
            "exists": self.exists,
            "class": None if self.peer_class is None else self.peer_class.value,
            "connected": self.connected,
        }

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> QueryExistsResponseBody:
        peer_class = mapping.get("class")
        return cls(
            peer_id=str(mapping["peer_id"]),
            exists=bool(mapping["exists"]),
            peer_class=None if peer_class is None else _coerce_peer_class(peer_class),
            connected=bool(mapping["connected"]),
        )


@dataclass(slots=True, frozen=True)
class QueryGetResponseBody:
    peer_id: str
    exists: bool
    peer_class: PeerClass | None
    connected: bool

    def to_mapping(self) -> dict[str, Any]:
        return {
            "peer_id": self.peer_id,
            "exists": self.exists,
            "class": None if self.peer_class is None else self.peer_class.value,
            "connected": self.connected,
        }

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> QueryGetResponseBody:
        peer_class = mapping.get("class")
        return cls(
            peer_id=str(mapping["peer_id"]),
            exists=bool(mapping["exists"]),
            peer_class=None if peer_class is None else _coerce_peer_class(peer_class),
            connected=bool(mapping["connected"]),
        )


@dataclass(slots=True, frozen=True)
class CallBody:
    function: str
    args: list[Any] = field(default_factory=list)
    namespace: str | None = None

    def to_mapping(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"function": self.function, "args": list(self.args)}
        if self.namespace is not None:
            payload["namespace"] = self.namespace
        return payload

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> CallBody:
        args = mapping.get("args", [])
        namespace = mapping.get("namespace")
        return cls(
            function=str(mapping["function"]),
            args=list(args),
            namespace=None if namespace is None else str(namespace),
        )


@dataclass(slots=True, frozen=True)
class ResponseBody:
    result: Any

    def to_mapping(self) -> dict[str, Any]:
        return {"result": self.result}

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> ResponseBody:
        return cls(result=mapping["result"])


@dataclass(slots=True, frozen=True)
class ErrorBody:
    code: str
    message: str
    kind: str
    stack: str | None = None
    details: Any = None

    def to_mapping(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "code": self.code,
            "message": self.message,
            "kind": self.kind,
        }
        if self.stack is not None:
            payload["stack"] = self.stack
        if self.details is not None:
            payload["details"] = self.details
        return payload

    @classmethod
    def from_mapping(cls, mapping: Mapping[str, Any]) -> ErrorBody:
        return cls(
            code=str(mapping["code"]),
            message=str(mapping["message"]),
            kind=str(mapping["kind"]),
            stack=None if mapping.get("stack") is None else str(mapping["stack"]),
            details=mapping.get("details"),
        )


def now_ts() -> float:
    return time.time()


def new_msg_id() -> str:
    return str(uuid4())


def validate_envelope_mapping(mapping: Mapping[str, Any]) -> Envelope:
    required = ("v", "kind", "msg_id", "from", "to", "ts")
    missing = [field for field in required if field not in mapping]
    if missing:
        raise ValueError(f"envelope missing required fields: {', '.join(missing)}")

    v = mapping["v"]
    kind_value = mapping["kind"]
    msg_id = mapping["msg_id"]
    from_peer = mapping["from"]
    to_peer = mapping["to"]
    ts = mapping["ts"]

    if not isinstance(v, int):
        raise TypeError("envelope field 'v' must be an int")
    if v != PROTOCOL_VERSION:
        raise ValueError(f"unsupported protocol version: {v}")
    if not isinstance(kind_value, str):
        raise TypeError("envelope field 'kind' must be a str")
    if not isinstance(msg_id, str):
        raise TypeError("envelope field 'msg_id' must be a str")
    if not isinstance(from_peer, str):
        raise TypeError("envelope field 'from' must be a str")
    if not isinstance(to_peer, str):
        raise TypeError("envelope field 'to' must be a str")
    if not isinstance(ts, (int, float)):
        raise TypeError("envelope field 'ts' must be numeric")

    return Envelope(
        v=v,
        kind=kind_value,
        msg_id=msg_id,
        from_peer=from_peer,
        to_peer=to_peer,
        ts=float(ts),
    )


def envelope_from_mapping(mapping: Mapping[str, Any]) -> Envelope:
    return validate_envelope_mapping(mapping)


def _coerce_peer_class(value: Any) -> PeerClass:
    if isinstance(value, PeerClass):
        return value
    if value == service:
        return PeerClass.SERVICE
    if value == caller:
        return PeerClass.CALLER
    raise ValueError(f"invalid peer class: {value!r}")


def build_register_envelope(peer_id: str) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=register,
        msg_id=new_msg_id(),
        from_peer=peer_id,
        to_peer=ROUTER_PEER_ID,
        ts=now_ts(),
    )


def build_query_envelope(peer_id: str, target_peer_id: str = ROUTER_PEER_ID) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=query,
        msg_id=new_msg_id(),
        from_peer=peer_id,
        to_peer=target_peer_id,
        ts=now_ts(),
    )


def build_call_envelope(from_peer: str, to_peer: str, msg_id: str | None = None) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=call,
        msg_id=msg_id or new_msg_id(),
        from_peer=from_peer,
        to_peer=to_peer,
        ts=now_ts(),
    )


def build_response_envelope(from_peer: str, to_peer: str, msg_id: str) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=response,
        msg_id=msg_id,
        from_peer=from_peer,
        to_peer=to_peer,
        ts=now_ts(),
    )


def build_error_envelope(from_peer: str, to_peer: str, msg_id: str) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=error,
        msg_id=msg_id,
        from_peer=from_peer,
        to_peer=to_peer,
        ts=now_ts(),
    )


def build_disconnect_envelope(
    from_peer: str, to_peer: str = ROUTER_PEER_ID, msg_id: str | None = None
) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=disconnect,
        msg_id=msg_id or new_msg_id(),
        from_peer=from_peer,
        to_peer=to_peer,
        ts=now_ts(),
    )


def build_heartbeat_envelope(
    from_peer: str, to_peer: str = ROUTER_PEER_ID, msg_id: str | None = None
) -> Envelope:
    return Envelope(
        v=PROTOCOL_VERSION,
        kind=heartbeat,
        msg_id=msg_id or new_msg_id(),
        from_peer=from_peer,
        to_peer=to_peer,
        ts=now_ts(),
    )


__all__ = [
    "PROTOCOL_KEY",
    "PROTOCOL_VERSION",
    "DEFAULT_ROUTER_PORT",
    "ROUTER_PORT_ENV",
    "ROUTER_BIN_ENV",
    "ROUTER_LOG_PATH_SUFFIX",
    "ROUTER_LOCK_PATH_SUFFIX",
    "ROUTER_PEER_ID",
    "service",
    "caller",
    "register",
    "register_ack",
    "query",
    "call",
    "response",
    "error",
    "heartbeat",
    "disconnect",
    "query_peer_exists",
    "query_peer_get",
    "INT64_MIN",
    "INT64_MAX",
    "PeerClass",
    "Envelope",
    "RegisterBody",
    "RegisterAckBody",
    "QueryBody",
    "QueryExistsResponseBody",
    "QueryGetResponseBody",
    "CallBody",
    "ResponseBody",
    "ErrorBody",
    "now_ts",
    "new_msg_id",
    "validate_envelope_mapping",
    "envelope_from_mapping",
    "build_register_envelope",
    "build_query_envelope",
    "build_call_envelope",
    "build_response_envelope",
    "build_error_envelope",
    "build_disconnect_envelope",
    "build_heartbeat_envelope",
]
