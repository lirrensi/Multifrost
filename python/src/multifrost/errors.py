"""
FILE: python/src/multifrost/errors.py
PURPOSE: Define the v5 error hierarchy and wire-error classification helpers.
OWNS: Transport, bootstrap, registration, router, and remote call error types plus origin tracking.
EXPORTS: ErrorOrigin, MultifrostError, TransportError, MalformedFrameError, BootstrapError, RegistrationError, RouterError, RemoteCallError, error_from_body.
DOCS: docs/spec.md; agent_chat/python_v5_api_surface_2026-03-25.md
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .protocol import ErrorBody


class ErrorOrigin(str, Enum):
    ROUTER = "router"
    REMOTE = "remote"


class MultifrostError(Exception):
    """Base class for Multifrost v5 errors."""


@dataclass(slots=True)
class _ErrorContext:
    message: str
    cause: BaseException | None = None


class TransportError(MultifrostError):
    def __init__(self, message: str, *, cause: BaseException | None = None) -> None:
        super().__init__(message)
        self.message = message
        self.cause = cause


class MalformedFrameError(TransportError):
    pass


class BootstrapError(MultifrostError):
    def __init__(
        self,
        message: str,
        *,
        timed_out: bool = False,
        cause: BaseException | None = None,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.timed_out = timed_out
        self.cause = cause


class RegistrationError(MultifrostError):
    def __init__(
        self,
        message: str,
        *,
        reason: str | None = None,
        origin: ErrorOrigin = ErrorOrigin.ROUTER,
    ) -> None:
        super().__init__(message)
        self.message = message
        self.reason = reason
        self.origin = origin


class RouterError(MultifrostError):
    def __init__(
        self,
        code: str,
        message: str,
        *,
        kind: str = "router",
        origin: ErrorOrigin = ErrorOrigin.ROUTER,
        details: Any = None,
        stack: str | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.kind = kind
        self.origin = origin
        self.details = details
        self.stack = stack


class RemoteCallError(MultifrostError):
    def __init__(
        self,
        code: str,
        message: str,
        *,
        kind: str = "application",
        origin: ErrorOrigin = ErrorOrigin.REMOTE,
        details: Any = None,
        stack: str | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.kind = kind
        self.origin = origin
        self.details = details
        self.stack = stack


def error_from_body(body: ErrorBody, *, origin: ErrorOrigin) -> RouterError | RemoteCallError:
    if origin is ErrorOrigin.ROUTER:
        return RouterError(
            body.code,
            body.message,
            kind=body.kind,
            origin=origin,
            details=body.details,
            stack=body.stack,
        )
    return RemoteCallError(
        body.code,
        body.message,
        kind=body.kind,
        origin=origin,
        details=body.details,
        stack=body.stack,
    )


__all__ = [
    "ErrorOrigin",
    "MultifrostError",
    "TransportError",
    "MalformedFrameError",
    "BootstrapError",
    "RegistrationError",
    "RouterError",
    "RemoteCallError",
    "error_from_body",
]
