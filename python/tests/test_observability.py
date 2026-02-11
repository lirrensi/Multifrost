"""
Tests for metrics and structured logging features.
"""

import asyncio
import time
import json
import sys
import os

# Add src to path for testing
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from multifrost.core.metrics import Metrics, MetricsSnapshot, RequestMetrics
from multifrost.core.logging import (
    StructuredLogger,
    LogEntry,
    LogEvent,
    LogLevel,
    default_json_handler,
    default_pretty_handler,
)
from multifrost.core.message import ComlinkMessage, MessageType


class TestMetrics:
    """Test metrics collection."""

    def test_metrics_creation(self):
        """Test creating a Metrics instance."""
        metrics = Metrics()
        assert metrics is not None
        snapshot = metrics.snapshot()
        assert snapshot.requests_total == 0
        assert snapshot.requests_success == 0
        assert snapshot.requests_failed == 0

    def test_record_request_success(self):
        """Test recording successful requests."""
        metrics = Metrics()

        start = metrics.start_request("req-1", "test_func", "default")
        time.sleep(0.01)  # 10ms
        latency = metrics.end_request(start, "req-1", success=True)

        assert latency > 0  # Should have some latency

        snapshot = metrics.snapshot()
        assert snapshot.requests_total == 1
        assert snapshot.requests_success == 1
        assert snapshot.requests_failed == 0
        assert snapshot.latency_avg_ms > 0

    def test_record_request_failure(self):
        """Test recording failed requests."""
        metrics = Metrics()

        start = metrics.start_request("req-1", "test_func", "default")
        metrics.end_request(start, "req-1", success=False, error="Timeout")

        snapshot = metrics.snapshot()
        assert snapshot.requests_total == 1
        assert snapshot.requests_success == 0
        assert snapshot.requests_failed == 1

    def test_latency_percentiles(self):
        """Test latency percentile calculations."""
        metrics = Metrics(max_latency_samples=100)

        # Record 10 requests with increasing latency
        for i in range(10):
            start = metrics.start_request(f"req-{i}", "test_func", "default")
            time.sleep(0.005 * (i + 1))  # 5ms, 10ms, 15ms, etc.
            metrics.end_request(start, f"req-{i}", success=True)

        snapshot = metrics.snapshot()
        assert snapshot.requests_total == 10
        assert snapshot.latency_min_ms < snapshot.latency_p50_ms
        assert snapshot.latency_p50_ms < snapshot.latency_p95_ms
        assert snapshot.latency_p95_ms <= snapshot.latency_max_ms

    def test_circuit_breaker_tracking(self):
        """Test circuit breaker state tracking."""
        metrics = Metrics()

        assert metrics.snapshot().circuit_breaker_state == "closed"

        metrics.record_circuit_breaker_trip()
        assert metrics.snapshot().circuit_breaker_state == "open"
        assert metrics.snapshot().circuit_breaker_trips == 1

        metrics.record_circuit_breaker_reset()
        assert metrics.snapshot().circuit_breaker_state == "closed"

    def test_queue_depth_tracking(self):
        """Test queue depth tracking."""
        metrics = Metrics()

        # Start 3 requests
        s1 = metrics.start_request("req-1", "f1", "default")
        s2 = metrics.start_request("req-2", "f2", "default")
        s3 = metrics.start_request("req-3", "f3", "default")

        snapshot = metrics.snapshot()
        assert snapshot.queue_depth == 3
        assert snapshot.queue_max_depth == 3

        # End 2 requests
        metrics.end_request(s1, "req-1", success=True)
        metrics.end_request(s2, "req-2", success=True)

        snapshot = metrics.snapshot()
        assert snapshot.queue_depth == 1
        assert snapshot.queue_max_depth == 3  # Max stays at 3

        # End last request
        metrics.end_request(s3, "req-3", success=True)
        assert metrics.snapshot().queue_depth == 0

    def test_metrics_to_dict(self):
        """Test metrics serialization to dict."""
        metrics = Metrics()

        start = metrics.start_request("req-1", "test_func", "default")
        metrics.end_request(start, "req-1", success=True)

        data = metrics.to_dict()

        assert "requests" in data
        assert "latency_ms" in data
        assert "queue" in data
        assert "circuit_breaker" in data

        assert data["requests"]["total"] == 1
        assert data["requests"]["success"] == 1
        assert data["requests"]["error_rate"] == 0.0

    def test_metrics_reset(self):
        """Test resetting metrics."""
        metrics = Metrics()

        start = metrics.start_request("req-1", "test_func", "default")
        metrics.end_request(start, "req-1", success=False)
        metrics.record_circuit_breaker_trip()

        metrics.reset()

        snapshot = metrics.snapshot()
        assert snapshot.requests_total == 0
        assert snapshot.circuit_breaker_trips == 0
        assert snapshot.circuit_breaker_state == "closed"


class TestStructuredLogger:
    """Test structured logging."""

    def test_logger_creation(self):
        """Test creating a StructuredLogger."""
        entries = []
        logger = StructuredLogger(handler=lambda e: entries.append(e))

        logger.info(LogEvent.WORKER_START, "Worker started")

        assert len(entries) == 1
        assert entries[0].event == "worker_start"
        assert entries[0].message == "Worker started"
        assert entries[0].level == "info"

    def test_log_levels(self):
        """Test different log levels."""
        entries = []
        logger = StructuredLogger(
            handler=lambda e: entries.append(e),
            level=LogLevel.WARN,  # Only warn and above
        )

        logger.debug(LogEvent.REQUEST_START, "Debug message")
        logger.info(LogEvent.REQUEST_START, "Info message")
        logger.warn(LogEvent.REQUEST_ERROR, "Warn message")
        logger.error(LogEvent.REQUEST_ERROR, "Error message")

        # Only warn and error should be logged
        assert len(entries) == 2
        assert entries[0].level == "warn"
        assert entries[1].level == "error"

    def test_log_entry_to_dict(self):
        """Test LogEntry serialization."""
        entry = LogEntry(
            event="request_end",
            level="info",
            message="Request completed",
            request_id="req-123",
            function="add",
            duration_ms=42.5,
            success=True,
        )

        data = entry.to_dict()

        assert data["event"] == "request_end"
        assert data["level"] == "info"
        assert data["message"] == "Request completed"
        assert data["request_id"] == "req-123"
        assert data["function"] == "add"
        assert data["duration_ms"] == 42.5
        assert data["success"] is True
        # None values should be omitted
        assert "error" not in data

    def test_log_entry_to_json(self):
        """Test LogEntry JSON serialization."""
        entry = LogEntry(
            event="request_start",
            level="debug",
            message="Calling function",
            request_id="req-456",
        )

        json_str = entry.to_json()
        data = json.loads(json_str)

        assert data["event"] == "request_start"
        assert data["request_id"] == "req-456"

    def test_convenience_methods(self):
        """Test convenience logging methods."""
        entries = []
        logger = StructuredLogger(
            handler=lambda e: entries.append(e),
            level=LogLevel.DEBUG,  # request_start logs at DEBUG
        )

        # Test request_start
        logger.request_start(
            request_id="req-1",
            function="add",
            namespace="math",
            correlation_id="corr-123",
        )

        assert entries[-1].event == "request_start"
        assert entries[-1].function == "add"
        assert entries[-1].correlation_id == "corr-123"

        # Test request_end
        logger.request_end(
            request_id="req-1",
            function="add",
            duration_ms=15.5,
            success=True,
        )

        assert entries[-1].event == "request_end"
        assert entries[-1].duration_ms == 15.5
        assert entries[-1].success is True

        # Test circuit_open
        logger.circuit_open(failures=5)

        assert entries[-1].event == "circuit_open"
        assert "5" in entries[-1].message

    def test_worker_context(self):
        """Test worker context in log entries."""
        entries = []
        logger = StructuredLogger(
            handler=lambda e: entries.append(e),
            worker_id="worker-abc",
            service_id="math-service",
        )

        logger.info(LogEvent.WORKER_START, "Started")

        assert entries[0].worker_id == "worker-abc"
        assert entries[0].service_id == "math-service"

    def test_handler_exception_handling(self):
        """Test that handler exceptions don't break logging."""
        entries = []

        def bad_handler(entry):
            raise ValueError("Handler error")

        # Capture print output
        import io
        import contextlib

        logger = StructuredLogger(handler=bad_handler)

        # Should not raise, just print error
        with contextlib.redirect_stdout(io.StringIO()):
            logger.info(LogEvent.WORKER_START, "Test")

        # No exception raised = success


class TestComlinkMessageMetadata:
    """Test message metadata support."""

    def test_create_call_with_metadata(self):
        """Test creating a call message with metadata."""
        msg = ComlinkMessage.create_call(
            function="add",
            args=(1, 2),
            namespace="math",
            correlation_id="corr-123",
            parent_request_id="parent-456",
            metadata={"custom": "value"},
        )

        assert msg.function == "add"
        assert msg.args == (1, 2)
        assert msg.namespace == "math"
        assert msg.metadata["correlation_id"] == "corr-123"
        assert msg.metadata["parent_request_id"] == "parent-456"
        assert msg.metadata["custom"] == "value"
        assert "timestamp_sent" in msg.metadata

    def test_create_response_with_metadata(self):
        """Test creating a response message with metadata."""
        msg = ComlinkMessage.create_response(
            result=42,
            msg_id="req-123",
            metadata={"cached": True},
        )

        assert msg.result == 42
        assert msg.id == "req-123"
        assert msg.metadata.get("cached") is True
        assert "timestamp_received" in msg.metadata

    def test_create_error_with_metadata(self):
        """Test creating an error message with metadata."""
        msg = ComlinkMessage.create_error(
            error="Division by zero",
            msg_id="req-123",
            metadata={"retryable": True},
        )

        assert msg.error == "Division by zero"
        assert msg.id == "req-123"
        assert msg.metadata.get("retryable") is True

    def test_message_pack_unpack_with_metadata(self):
        """Test packing and unpacking message with metadata."""
        original = ComlinkMessage.create_call(
            function="multiply",
            args=(3, 4),
            correlation_id="corr-789",
            metadata={"source": "test"},
        )

        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.function == "multiply"
        # msgpack converts tuples to lists
        assert list(unpacked.args) == [3, 4]
        assert unpacked.metadata["correlation_id"] == "corr-789"
        assert unpacked.metadata["source"] == "test"


class TestIntegration:
    """Integration tests with ParentWorker."""

    def test_parent_worker_with_logging(self):
        """Test ParentWorker with logging callback."""
        entries = []

        # Create worker with log handler
        from multifrost import ParentWorker

        worker = ParentWorker.spawn(
            "dummy.py",
            log_handler=lambda e: entries.append(e),
        )

        # Logger should be configured
        assert worker.logger is not None
        assert worker.metrics is not None

    def test_parent_worker_metrics_property(self):
        """Test accessing metrics from ParentWorker."""
        from multifrost import ParentWorker

        worker = ParentWorker.spawn("dummy.py")

        # Metrics should be available
        assert worker.metrics is not None

        snapshot = worker.metrics.snapshot()
        assert snapshot.requests_total == 0

    def test_set_log_handler(self):
        """Test setting log handler after creation."""
        entries = []

        from multifrost import ParentWorker

        worker = ParentWorker.spawn("dummy.py")
        worker.set_log_handler(lambda e: entries.append(e))

        # Log something
        worker.logger.info(LogEvent.WORKER_START, "Test")

        assert len(entries) == 1


class TestHeartbeat:
    """Test heartbeat functionality."""

    def test_create_heartbeat_message(self):
        """Test creating a heartbeat message."""
        msg = ComlinkMessage.create_heartbeat()

        assert msg.type == MessageType.HEARTBEAT.value
        assert msg.id is not None
        assert "hb_timestamp" in msg.metadata

    def test_create_heartbeat_with_id(self):
        """Test creating a heartbeat with specific ID."""
        msg = ComlinkMessage.create_heartbeat(msg_id="hb-123")

        assert msg.id == "hb-123"
        assert msg.type == MessageType.HEARTBEAT.value

    def test_create_heartbeat_response(self):
        """Test creating a heartbeat response."""
        original_ts = time.time() - 0.01  # 10ms ago
        response = ComlinkMessage.create_heartbeat_response(
            request_id="hb-123",
            original_timestamp=original_ts,
        )

        assert response.id == "hb-123"
        assert response.type == MessageType.HEARTBEAT.value
        assert response.metadata["hb_timestamp"] == original_ts
        assert response.metadata["hb_response"] is True

    def test_heartbeat_pack_unpack(self):
        """Test packing and unpacking heartbeat message."""
        original = ComlinkMessage.create_heartbeat(msg_id="hb-456")
        packed = original.pack()
        unpacked = ComlinkMessage.unpack(packed)

        assert unpacked.id == "hb-456"
        assert unpacked.type == MessageType.HEARTBEAT.value
        assert "hb_timestamp" in unpacked.metadata

    def test_metrics_heartbeat_rtt(self):
        """Test recording heartbeat RTT in metrics."""
        metrics = Metrics()

        # Record some heartbeat RTTs
        metrics.record_heartbeat_rtt(5.5)
        metrics.record_heartbeat_rtt(10.2)
        metrics.record_heartbeat_rtt(3.1)

        snapshot = metrics.snapshot()
        assert snapshot.heartbeat_rtt_last_ms == 3.1
        assert snapshot.heartbeat_rtt_avg_ms > 0

    def test_metrics_heartbeat_miss(self):
        """Test recording heartbeat misses in metrics."""
        metrics = Metrics()

        metrics.record_heartbeat_miss()
        metrics.record_heartbeat_miss()

        snapshot = metrics.snapshot()
        assert snapshot.heartbeat_misses == 2

    def test_heartbeat_log_events(self):
        """Test heartbeat logging events."""
        entries = []
        logger = StructuredLogger(
            handler=lambda e: entries.append(e),
            level=LogLevel.DEBUG,
        )

        # Test heartbeat_sent
        logger.heartbeat_sent()
        assert entries[-1].event == "heartbeat_sent"

        # Test heartbeat_received
        logger.heartbeat_received(rtt_ms=5.5)
        assert entries[-1].event == "heartbeat_received"
        assert entries[-1].duration_ms == 5.5

        # Test heartbeat_missed
        logger.heartbeat_missed(consecutive=2, max_allowed=3)
        assert entries[-1].event == "heartbeat_missed"
        assert entries[-1].metadata["consecutive_misses"] == 2

        # Test heartbeat_timeout
        logger.heartbeat_timeout(misses=3)
        assert entries[-1].event == "heartbeat_timeout"

    def test_parent_worker_heartbeat_config(self):
        """Test ParentWorker heartbeat configuration."""
        from multifrost import ParentWorker

        # Default config
        worker = ParentWorker.spawn("dummy.py")
        assert worker.heartbeat_interval == 5.0
        assert worker.heartbeat_timeout == 3.0
        assert worker.heartbeat_max_misses == 3

        # Custom config
        worker = ParentWorker.spawn(
            "dummy.py",
            heartbeat_interval=10.0,
            heartbeat_timeout=5.0,
            heartbeat_max_misses=5,
        )
        assert worker.heartbeat_interval == 10.0
        assert worker.heartbeat_timeout == 5.0
        assert worker.heartbeat_max_misses == 5

        # Disabled heartbeats
        worker = ParentWorker.spawn("dummy.py", heartbeat_interval=0)
        assert worker.heartbeat_interval == 0

    def test_parent_worker_heartbeat_rtt_property(self):
        """Test accessing heartbeat RTT from ParentWorker."""
        from multifrost import ParentWorker

        worker = ParentWorker.spawn("dummy.py")

        # Initially None (no heartbeats yet)
        assert worker.last_heartbeat_rtt_ms is None


def run_tests():
    """Run all tests."""
    print("Running TestMetrics...")
    t = TestMetrics()
    t.test_metrics_creation()
    t.test_record_request_success()
    t.test_record_request_failure()
    t.test_latency_percentiles()
    t.test_circuit_breaker_tracking()
    t.test_queue_depth_tracking()
    t.test_metrics_to_dict()
    t.test_metrics_reset()
    print("  [OK] All TestMetrics passed")

    print("Running TestStructuredLogger...")
    t = TestStructuredLogger()
    t.test_logger_creation()
    t.test_log_levels()
    t.test_log_entry_to_dict()
    t.test_log_entry_to_json()
    t.test_convenience_methods()
    t.test_worker_context()
    t.test_handler_exception_handling()
    print("  [OK] All TestStructuredLogger passed")

    print("Running TestComlinkMessageMetadata...")
    t = TestComlinkMessageMetadata()
    t.test_create_call_with_metadata()
    t.test_create_response_with_metadata()
    t.test_create_error_with_metadata()
    t.test_message_pack_unpack_with_metadata()
    print("  [OK] All TestComlinkMessageMetadata passed")

    print("Running TestIntegration...")
    t = TestIntegration()
    t.test_parent_worker_with_logging()
    t.test_parent_worker_metrics_property()
    t.test_set_log_handler()
    print("  [OK] All TestIntegration passed")

    print("Running TestHeartbeat...")
    t = TestHeartbeat()
    t.test_create_heartbeat_message()
    t.test_create_heartbeat_with_id()
    t.test_create_heartbeat_response()
    t.test_heartbeat_pack_unpack()
    t.test_metrics_heartbeat_rtt()
    t.test_metrics_heartbeat_miss()
    t.test_heartbeat_log_events()
    t.test_parent_worker_heartbeat_config()
    t.test_parent_worker_heartbeat_rtt_property()
    print("  [OK] All TestHeartbeat passed")

    print("\n[PASS] All tests passed!")


if __name__ == "__main__":
    run_tests()
