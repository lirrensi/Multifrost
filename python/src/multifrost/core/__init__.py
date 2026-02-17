"""
Core modules for async-first IPC implementation.
"""

from .message import ComlinkMessage, MessageType, APP_NAME
from .async_worker import ParentWorker, RemoteCallError, CircuitOpenError
from .child import ChildWorker
from .sync_wrapper import (
    SyncWrapper,
    SyncProxy,
    AsyncProxy,
    ParentHandle,
    ParentHandleSync,
    SyncCallProxy,
)
from .metrics import Metrics, MetricsSnapshot, RequestMetrics
from .logging import (
    StructuredLogger,
    LogEntry,
    LogEvent,
    LogLevel,
    LogHandler,
    default_json_handler,
    default_pretty_handler,
)

__all__ = [
    "ComlinkMessage",
    "MessageType",
    "APP_NAME",
    "RemoteCallError",
    "CircuitOpenError",
    "ParentWorker",
    "ChildWorker",
    "SyncWrapper",
    "SyncProxy",
    "AsyncProxy",
    "ParentHandle",
    "ParentHandleSync",
    "SyncCallProxy",
    # Metrics
    "Metrics",
    "MetricsSnapshot",
    "RequestMetrics",
    # Logging
    "StructuredLogger",
    "LogEntry",
    "LogEvent",
    "LogLevel",
    "LogHandler",
    "default_json_handler",
    "default_pretty_handler",
]
