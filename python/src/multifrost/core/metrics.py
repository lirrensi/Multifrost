"""
Metrics collection for observability.

Tracks request latency, error rates, queue depth, and circuit breaker events.
"""

import time
from dataclasses import dataclass, field
from typing import List, Dict, Any, Optional
from collections import deque
import threading


@dataclass
class RequestMetrics:
    """Metrics for a single completed request."""

    request_id: str
    function: str
    namespace: str
    success: bool
    latency_ms: float
    error: Optional[str] = None
    timestamp: float = field(default_factory=time.time)


@dataclass
class MetricsSnapshot:
    """Point-in-time snapshot of all metrics."""

    # Counters
    requests_total: int = 0
    requests_success: int = 0
    requests_failed: int = 0

    # Latency (milliseconds)
    latency_avg_ms: float = 0.0
    latency_p50_ms: float = 0.0
    latency_p95_ms: float = 0.0
    latency_p99_ms: float = 0.0
    latency_min_ms: float = 0.0
    latency_max_ms: float = 0.0

    # Queue
    queue_depth: int = 0
    queue_max_depth: int = 0

    # Circuit breaker
    circuit_breaker_trips: int = 0
    circuit_breaker_state: str = "closed"  # closed, open, half-open

    # Time window
    window_seconds: float = 0.0

    # Timestamp
    timestamp: float = field(default_factory=time.time)


class Metrics:
    """
    Thread-safe metrics collector for ParentWorker.

    Usage:
        metrics = Metrics()

        # Record a request
        with metrics.track_request("my_func", "default") as tracker:
            result = await call_remote()
            tracker.success()

        # Or manual tracking
        start = metrics.start_request("my_func", "default")
        # ... do work ...
        metrics.end_request(start, success=True)

        # Get snapshot
        snapshot = metrics.snapshot()
        print(f"Avg latency: {snapshot.latency_avg_ms}ms")
    """

    def __init__(
        self,
        max_latency_samples: int = 1000,
        window_seconds: float = 60.0,
    ):
        self.max_latency_samples = max_latency_samples
        self.window_seconds = window_seconds

        # Counters (thread-safe via locks)
        self._lock = threading.Lock()
        self._requests_total = 0
        self._requests_success = 0
        self._requests_failed = 0
        self._circuit_breaker_trips = 0
        self._circuit_breaker_state = "closed"

        # Queue tracking
        self._queue_depth = 0
        self._queue_max_depth = 0

        # Latency samples (circular buffer)
        self._latencies: deque = deque(maxlen=max_latency_samples)

        # Request tracking (for in-flight requests)
        self._inflight: Dict[str, RequestMetrics] = {}

    def start_request(
        self,
        request_id: str,
        function: str,
        namespace: str = "default",
    ) -> float:
        """
        Start tracking a request.

        Returns start timestamp for later end_request() call.
        """
        with self._lock:
            self._requests_total += 1
            self._queue_depth += 1
            self._queue_max_depth = max(self._queue_max_depth, self._queue_depth)

        return time.perf_counter()

    def end_request(
        self,
        start_time: float,
        request_id: str,
        success: bool = True,
        error: Optional[str] = None,
    ) -> float:
        """
        End tracking a request.

        Returns latency in milliseconds.
        """
        latency_ms = (time.perf_counter() - start_time) * 1000

        with self._lock:
            self._queue_depth -= 1

            if success:
                self._requests_success += 1
            else:
                self._requests_failed += 1

            # Store latency sample
            self._latencies.append(latency_ms)

        return latency_ms

    def record_circuit_breaker_trip(self):
        """Record a circuit breaker trip event."""
        with self._lock:
            self._circuit_breaker_trips += 1
            self._circuit_breaker_state = "open"

    def record_circuit_breaker_reset(self):
        """Record circuit breaker reset (closed)."""
        with self._lock:
            self._circuit_breaker_state = "closed"

    def record_circuit_breaker_half_open(self):
        """Record circuit breaker entering half-open state."""
        with self._lock:
            self._circuit_breaker_state = "half-open"

    def snapshot(self) -> MetricsSnapshot:
        """Get a point-in-time snapshot of all metrics."""
        with self._lock:
            latencies = list(self._latencies)

            # Calculate percentiles
            if latencies:
                sorted_latencies = sorted(latencies)
                n = len(sorted_latencies)
                p50_idx = int(n * 0.50)
                p95_idx = int(n * 0.95)
                p99_idx = int(n * 0.99)

                latency_avg = sum(latencies) / n
                latency_p50 = sorted_latencies[min(p50_idx, n - 1)]
                latency_p95 = sorted_latencies[min(p95_idx, n - 1)]
                latency_p99 = sorted_latencies[min(p99_idx, n - 1)]
                latency_min = sorted_latencies[0]
                latency_max = sorted_latencies[-1]
            else:
                latency_avg = latency_p50 = latency_p95 = latency_p99 = 0.0
                latency_min = latency_max = 0.0

            return MetricsSnapshot(
                requests_total=self._requests_total,
                requests_success=self._requests_success,
                requests_failed=self._requests_failed,
                latency_avg_ms=latency_avg,
                latency_p50_ms=latency_p50,
                latency_p95_ms=latency_p95,
                latency_p99_ms=latency_p99,
                latency_min_ms=latency_min,
                latency_max_ms=latency_max,
                queue_depth=self._queue_depth,
                queue_max_depth=self._queue_max_depth,
                circuit_breaker_trips=self._circuit_breaker_trips,
                circuit_breaker_state=self._circuit_breaker_state,
                window_seconds=self.window_seconds,
            )

    def reset(self):
        """Reset all metrics."""
        with self._lock:
            self._requests_total = 0
            self._requests_success = 0
            self._requests_failed = 0
            self._circuit_breaker_trips = 0
            self._circuit_breaker_state = "closed"
            self._queue_depth = 0
            self._queue_max_depth = 0
            self._latencies.clear()
            self._inflight.clear()

    def to_dict(self) -> Dict[str, Any]:
        """Get metrics as a dictionary (for logging/serialization)."""
        snapshot = self.snapshot()
        return {
            "requests": {
                "total": snapshot.requests_total,
                "success": snapshot.requests_success,
                "failed": snapshot.requests_failed,
                "error_rate": (
                    snapshot.requests_failed / snapshot.requests_total
                    if snapshot.requests_total > 0
                    else 0.0
                ),
            },
            "latency_ms": {
                "avg": round(snapshot.latency_avg_ms, 2),
                "p50": round(snapshot.latency_p50_ms, 2),
                "p95": round(snapshot.latency_p95_ms, 2),
                "p99": round(snapshot.latency_p99_ms, 2),
                "min": round(snapshot.latency_min_ms, 2),
                "max": round(snapshot.latency_max_ms, 2),
            },
            "queue": {
                "depth": snapshot.queue_depth,
                "max_depth": snapshot.queue_max_depth,
            },
            "circuit_breaker": {
                "trips": snapshot.circuit_breaker_trips,
                "state": snapshot.circuit_breaker_state,
            },
        }
