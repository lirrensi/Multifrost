"""
FILE: python/src/multifrost/frame.py
PURPOSE: Encode and decode the v5 binary frame layout while preserving body bytes unchanged.
OWNS: Frame framing, msgpack helpers, and strict validation for v5 envelopes and payload values.
EXPORTS: FrameParts, encode_msgpack, decode_msgpack, encode_envelope, decode_envelope, encode_body, decode_body, encode_frame, decode_frame.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from math import isfinite
from typing import Any

import msgpack  # type: ignore[import-untyped]

from .errors import MalformedFrameError
from .protocol import (
    INT64_MAX,
    INT64_MIN,
    CallBody,
    Envelope,
    ErrorBody,
    QueryBody,
    QueryExistsResponseBody,
    QueryGetResponseBody,
    RegisterAckBody,
    RegisterBody,
    ResponseBody,
    validate_envelope_mapping,
)

_MAX_BIN_LEN = 16 * 1024 * 1024
_MAX_STR_LEN = 16 * 1024 * 1024
_MAX_ARRAY_LEN = 100_000
_MAX_MAP_LEN = 100_000


@dataclass(slots=True, frozen=True)
class FrameParts:
    envelope: Envelope
    body_bytes: bytes


def encode_msgpack(value: Any) -> bytes:
    normalized = _normalize_msgpack_value(value)
    try:
        return bytes(msgpack.packb(normalized, use_bin_type=True))
    except (TypeError, ValueError, msgpack.PackException) as err:
        raise MalformedFrameError(f"failed to encode msgpack payload: {err}") from err


def decode_msgpack(data: bytes) -> Any:
    _require_bytes(data, "msgpack payload")
    try:
        return msgpack.unpackb(
            data,
            raw=False,
            strict_map_key=True,
            max_bin_len=_MAX_BIN_LEN,
            max_str_len=_MAX_STR_LEN,
            max_array_len=_MAX_ARRAY_LEN,
            max_map_len=_MAX_MAP_LEN,
        )
    except (msgpack.ExtraData, msgpack.FormatError, msgpack.StackError, ValueError, TypeError) as err:
        raise MalformedFrameError(f"failed to decode msgpack payload: {err}") from err


def encode_envelope(envelope: Envelope | Mapping[str, Any]) -> bytes:
    mapping = envelope.to_mapping() if isinstance(envelope, Envelope) else dict(envelope)
    validated = validate_envelope_mapping(mapping)
    return encode_msgpack(validated.to_mapping())


def decode_envelope(data: bytes) -> Envelope:
    mapping = decode_msgpack(data)
    if not isinstance(mapping, Mapping):
        raise MalformedFrameError(f"expected envelope map, got {type(mapping)!r}")
    return validate_envelope_mapping(mapping)


def encode_body(body: Any) -> bytes:
    return encode_msgpack(_body_to_mapping(body))


def decode_body(data: bytes) -> Any:
    return decode_msgpack(data)


def encode_frame(envelope: Envelope | Mapping[str, Any], body_bytes: bytes) -> bytes:
    _require_bytes(body_bytes, "frame body")
    envelope_bytes = encode_envelope(envelope)
    if len(envelope_bytes) > 0xFFFFFFFF:
        raise MalformedFrameError("envelope too large for v5 frame")
    return len(envelope_bytes).to_bytes(4, byteorder="big") + envelope_bytes + bytes(body_bytes)


def decode_frame(data: bytes) -> FrameParts:
    _require_bytes(data, "frame")
    if len(data) < 4:
        raise MalformedFrameError("frame too short for envelope length")

    envelope_len = int.from_bytes(data[:4], byteorder="big")
    if len(data) < 4 + envelope_len:
        raise MalformedFrameError("frame shorter than declared envelope length")

    envelope_bytes = data[4 : 4 + envelope_len]
    body_bytes = bytes(data[4 + envelope_len :])
    envelope = decode_envelope(envelope_bytes)
    return FrameParts(envelope=envelope, body_bytes=body_bytes)


def _body_to_mapping(body: Any) -> Mapping[str, Any]:
    if hasattr(body, "to_mapping"):
        mapping = body.to_mapping()
        if not isinstance(mapping, Mapping):
            raise MalformedFrameError(
                f"body to_mapping() must return a mapping, got {type(mapping)!r}"
            )
        return mapping
    if isinstance(body, Mapping):
        return body
    raise MalformedFrameError(f"unsupported msgpack body type: {type(body)!r}")


def _normalize_msgpack_value(value: Any) -> Any:
    if value is None or isinstance(value, (str, bytes, bytearray)):
        return bytes(value) if isinstance(value, bytearray) else value
    if isinstance(value, bool):
        return value
    if isinstance(value, int):
        if value < INT64_MIN or value > INT64_MAX:
            raise MalformedFrameError(
                f"integer value outside portable int64 range: {value}"
            )
        return value
    if isinstance(value, float):
        if not isfinite(value):
            raise MalformedFrameError(
                f"non-finite floats are not allowed on the generic wire path: {value!r}"
            )
        return value
    if isinstance(value, Mapping):
        normalized: dict[str, Any] = {}
        for key, item in value.items():
            if not isinstance(key, str):
                raise MalformedFrameError(
                    f"msgpack map keys must be strings, got {type(key)!r}"
                )
            normalized[key] = _normalize_msgpack_value(item)
        return normalized
    if isinstance(value, (list, tuple)):
        return [_normalize_msgpack_value(item) for item in value]
    if isinstance(value, (Envelope, RegisterBody, RegisterAckBody, QueryBody,
                          QueryExistsResponseBody, QueryGetResponseBody,
                          CallBody, ResponseBody, ErrorBody)):
        return _normalize_msgpack_value(value.to_mapping())
    if hasattr(value, "to_mapping"):
        return _normalize_msgpack_value(value.to_mapping())
    if hasattr(value, "value") and not isinstance(value, type):
        enum_value = value.value
        return _normalize_msgpack_value(enum_value)
    raise MalformedFrameError(f"unsupported msgpack value type: {type(value)!r}")


def _require_bytes(value: Any, label: str) -> None:
    if isinstance(value, (bytes, bytearray, memoryview)):
        return
    raise TypeError(f"{label} must be bytes-like, got {type(value)!r}")


__all__ = [
    "FrameParts",
    "encode_msgpack",
    "decode_msgpack",
    "encode_envelope",
    "decode_envelope",
    "encode_body",
    "decode_body",
    "encode_frame",
    "decode_frame",
]
