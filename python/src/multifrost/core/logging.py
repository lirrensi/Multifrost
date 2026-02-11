"""
Structured logging with correlation IDs for observability.

Provides JSON-formatted logs with pluggable output handlers.
"""

import time
import json
from dataclasses import dataclass, field, asdict
from typing import Any, Callable, Optional, Dict
from enum import Enum


class LogLevel(Enum):
    """Log levels for structured logging."""

    DEBUG = "debug"
    INFO = "info"
    WARN = "warn"
    ERROR = "error"


class LogEvent(Enum):
    """Standard log events for IPC operations."""

    # Lifecycle
    WORKER_START = "worker_start"
    WORKER_STOP = "worker_stop"
    WORKER_RESTART = "worker_restart"

    # Requests
    REQUEST_START = "request_start"
    REQUEST_END = "request_end"
    REQUEST_TIMEOUT = "request_timeout"
    REQUEST_ERROR = "request_error"

    # Circuit breaker
    CIRCUIT_OPEN = "circuit_open"
    CIRCUIT_CLOSE = "circuit_close"
    CIRCUIT_HALF_OPEN = "circuit_half_open"

    # Connection
    SOCKET_CONNECT = "socket_connect"
    SOCKET_DISCONNECT = "socket_disconnect"
    SOCKET_RECONNECT = "socket_reconnect"

    # Process
    PROCESS_SPAWN = "process_spawn"
    PROCESS_EXIT = "process_exit"

    # Queue
    QUEUE_OVERFLOW = "queue_overflow"


@dataclass
class LogEntry:
    """
    Structured log entry with all context.

    Can be serialized to JSON or passed to custom handlers.
    """

    # Required
    event: str
    level: str
    message: str
    timestamp: float = field(default_factory=time.time)

    # Correlation
    correlation_id: Optional[str] = None
    request_id: Optional[str] = None
    parent_request_id: Optional[str] = None

    # Context
    worker_id: Optional[str] = None
    service_id: Optional[str] = None
    function: Optional[str] = None
    namespace: Optional[str] = None

    # Timing
    duration_ms: Optional[float] = None

    # Status
    success: Optional[bool] = None
    error: Optional[str] = None
    error_type: Optional[str] = None

    # Metrics snapshot (optional)
    metrics: Optional[Dict[str, Any]] = None

    # Custom metadata
    metadata: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary, omitting None values."""
        result = {}
        for key, value in asdict(self).items():
            if value is not None:
                result[key] = value
        return result

    def to_json(self) -> str:
        """Serialize to JSON string."""
        return json.dumps(self.to_dict())


# Type alias for log handler
LogHandler = Callable[[LogEntry], None]


class StructuredLogger:
    """
    Structured logger with pluggable handlers.

    Usage:
        # Simple usage with callback
        logger = StructuredLogger(
            handler=lambda entry: print(entry.to_json())
        )

        # Log events
        logger.log(
            event=LogEvent.REQUEST_START,
            message="Calling remote function",
            request_id="abc123",
            function="add",
        )

        # Convenience methods
        logger.info(LogEvent.WORKER_START, "Worker started", worker_id="worker-1")
        logger.error(LogEvent.REQUEST_ERROR, "Call failed", error="Timeout")

    Integration with ParentWorker:
        worker = ParentWorker.spawn("child.py")
        worker.set_logger(lambda entry: print(entry.to_json()))
    """

    def __init__(
        self,
        handler: Optional[LogHandler] = None,
        level: LogLevel = LogLevel.INFO,
        worker_id: Optional[str] = None,
        service_id: Optional[str] = None,
    ):
        self.handler = handler
        self.level = level
        self.worker_id = worker_id
        self.service_id = service_id
        self._level_order = {
            LogLevel.DEBUG: 0,
            LogLevel.INFO: 1,
            LogLevel.WARN: 2,
            LogLevel.ERROR: 3,
        }

    def set_handler(self, handler: LogHandler):
        """Set or update the log handler."""
        self.handler = handler

    def set_context(
        self, worker_id: Optional[str] = None, service_id: Optional[str] = None
    ):
        """Set default context for all log entries."""
        if worker_id is not None:
            self.worker_id = worker_id
        if service_id is not None:
            self.service_id = service_id

    def _should_log(self, level: LogLevel) -> bool:
        """Check if this level should be logged."""
        return self._level_order.get(level, 0) >= self._level_order.get(self.level, 0)

    def log(
        self,
        event: LogEvent,
        message: str,
        level: LogLevel = LogLevel.INFO,
        **kwargs,
    ):
        """
        Log an event with structured data.

        Args:
            event: The event type (from LogEvent enum)
            message: Human-readable message
            level: Log level (default: INFO)
            **kwargs: Additional fields for LogEntry
        """
        if not self.handler or not self._should_log(level):
            return

        entry = LogEntry(
            event=event.value,
            level=level.value,
            message=message,
            worker_id=self.worker_id,
            service_id=self.service_id,
            **kwargs,
        )

        try:
            self.handler(entry)
        except Exception as e:
            # Don't let logging errors break the application
            print(f"Log handler error: {e}")

    def debug(self, event: LogEvent, message: str, **kwargs):
        """Log at DEBUG level."""
        self.log(event, message, level=LogLevel.DEBUG, **kwargs)

    def info(self, event: LogEvent, message: str, **kwargs):
        """Log at INFO level."""
        self.log(event, message, level=LogLevel.INFO, **kwargs)

    def warn(self, event: LogEvent, message: str, **kwargs):
        """Log at WARN level."""
        self.log(event, message, level=LogLevel.WARN, **kwargs)

    def error(self, event: LogEvent, message: str, **kwargs):
        """Log at ERROR level."""
        self.log(event, message, level=LogLevel.ERROR, **kwargs)

    # Convenience methods for common events

    def request_start(
        self,
        request_id: str,
        function: str,
        namespace: str = "default",
        correlation_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ):
        """Log request start."""
        self.debug(
            LogEvent.REQUEST_START,
            f"Calling {function}",
            request_id=request_id,
            function=function,
            namespace=namespace,
            correlation_id=correlation_id,
            metadata=metadata or {},
        )

    def request_end(
        self,
        request_id: str,
        function: str,
        duration_ms: float,
        success: bool = True,
        error: Optional[str] = None,
        correlation_id: Optional[str] = None,
    ):
        """Log request completion."""
        event = LogEvent.REQUEST_END if success else LogEvent.REQUEST_ERROR
        level = LogLevel.INFO if success else LogLevel.WARN
        self.log(
            event,
            f"{'Completed' if success else 'Failed'} {function}",
            level=level,
            request_id=request_id,
            function=function,
            duration_ms=round(duration_ms, 2),
            success=success,
            error=error,
            correlation_id=correlation_id,
        )

    def circuit_open(self, failures: int):
        """Log circuit breaker opening."""
        self.warn(
            LogEvent.CIRCUIT_OPEN,
            f"Circuit breaker opened after {failures} failures",
        )

    def circuit_close(self):
        """Log circuit breaker closing (recovery)."""
        self.info(
            LogEvent.CIRCUIT_CLOSE,
            "Circuit breaker closed (recovered)",
        )

    def worker_start(self, mode: str = "spawn"):
        """Log worker start."""
        self.info(
            LogEvent.WORKER_START,
            f"Worker started in {mode} mode",
        )

    def worker_stop(self, reason: str = "shutdown"):
        """Log worker stop."""
        self.info(
            LogEvent.WORKER_STOP,
            f"Worker stopped: {reason}",
        )

    def process_exit(self, exit_code: int):
        """Log child process exit."""
        level = LogLevel.INFO if exit_code == 0 else LogLevel.WARN
        self.log(
            LogEvent.PROCESS_EXIT,
            f"Child process exited with code {exit_code}",
            level=level,
            metadata={"exit_code": exit_code},
        )


def default_json_handler(entry: LogEntry):
    """Default handler that prints JSON to stdout."""
    print(entry.to_json())


def default_pretty_handler(entry: LogEntry):
    """Default handler that prints human-readable output."""
    timestamp = time.strftime("%H:%M:%S", time.localtime(entry.timestamp))
    level = entry.level.upper().ljust(5)
    prefix = f"[{timestamp}] [{level}]"

    parts = [prefix, entry.event, entry.message]

    if entry.request_id:
        parts.append(f"req={entry.request_id[:8]}")
    if entry.function:
        parts.append(f"fn={entry.function}")
    if entry.duration_ms is not None:
        parts.append(f"{entry.duration_ms:.1f}ms")
    if entry.error:
        parts.append(f"error={entry.error}")

    print(" ".join(parts))
