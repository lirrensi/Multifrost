"""
Tests for SyncWrapper and related classes.

Tests cover:
- SyncWrapper lifecycle (start, call, close)
- SyncProxy method calls
- AsyncProxy method calls
- ParentHandle and ParentHandleSync
- Event loop management
- Context manager usage
"""

import asyncio
import sys
import os
import time
import threading
from unittest.mock import Mock, MagicMock, AsyncMock, patch

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from multifrost.core.sync_wrapper import (
    SyncWrapper,
    SyncProxy,
    AsyncProxy,
    SyncCallProxy,
    ParentHandle,
    ParentHandleSync,
)


class MockAsyncWorker:
    """Mock async ParentWorker for testing."""

    def __init__(self):
        self.started = False
        self.closed = False
        self.call_count = 0
        self.last_call = None

    async def start(self):
        self.started = True
        await asyncio.sleep(0.01)

    async def close(self):
        self.closed = True
        await asyncio.sleep(0.01)

    async def call(self, func_name, *args, timeout=None, namespace="default"):
        self.call_count += 1
        self.last_call = (func_name, args, timeout, namespace)

        if func_name == "add":
            return args[0] + args[1]
        elif func_name == "multiply":
            return args[0] * args[1]
        elif func_name == "error":
            raise ValueError("Intentional error")
        elif func_name == "slow":
            await asyncio.sleep(0.1)
            return "slow result"

        return None

    async def _call_internal(self, func_name, *args, timeout=None, namespace="default"):
        """Internal call method (used by AsyncProxy)."""
        return await self.call(func_name, *args, timeout=timeout, namespace=namespace)


class TestSyncWrapper:
    """Test SyncWrapper class."""

    def test_init(self):
        """Test SyncWrapper initialization."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        assert wrapper._worker is worker
        assert wrapper._loop is None
        assert wrapper._loop_thread is None

    def test_start_creates_loop(self):
        """Test that start creates an event loop."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        assert wrapper._loop is not None
        assert wrapper._loop.is_running()
        assert worker.started is True

        wrapper.close()

    def test_close_stops_loop(self):
        """Test that close stops the event loop."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()
        assert wrapper._loop.is_running()

        wrapper.close()

        assert wrapper._loop is None
        assert wrapper._loop_thread is None
        assert worker.closed is True

    def test_call_method(self):
        """Test synchronous call method."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        result = wrapper.call("add", 5, 3)

        assert result == 8
        assert worker.call_count == 1
        assert worker.last_call == ("add", (5, 3), None, "default")

        wrapper.close()

    def test_call_with_timeout(self):
        """Test call with timeout parameter."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        result = wrapper.call("add", 10, 20, timeout=5.0)

        assert result == 30
        assert worker.last_call[2] == 5.0  # timeout

        wrapper.close()

    def test_call_with_namespace(self):
        """Test call with namespace parameter."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        result = wrapper.call("add", 1, 2, namespace="custom")

        assert result == 3
        assert worker.last_call[3] == "custom"  # namespace

        wrapper.close()

    def test_call_raises_error(self):
        """Test that errors are propagated."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        try:
            wrapper.call("error")
            assert False, "Should have raised ValueError"
        except ValueError as e:
            assert "Intentional error" in str(e)

        wrapper.close()

    def test_context_manager(self):
        """Test context manager usage."""
        worker = MockAsyncWorker()

        with SyncWrapper(worker) as wrapper:
            assert wrapper._loop.is_running()
            result = wrapper.call("add", 7, 8)
            assert result == 15

        assert worker.closed is True

    def test_context_manager_with_exception(self):
        """Test context manager closes on exception."""
        worker = MockAsyncWorker()

        try:
            with SyncWrapper(worker) as wrapper:
                wrapper.call("add", 1, 2)
                raise RuntimeError("Test error")
        except RuntimeError:
            pass

        assert worker.closed is True

    def test_run_async_without_loop_raises(self):
        """Test that _run_async raises without loop."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        try:
            wrapper._run_async(worker.start())
            assert False, "Should have raised RuntimeError"
        except RuntimeError as e:
            assert "Event loop is not running" in str(e)


class TestSyncProxy:
    """Test SyncProxy class."""

    def test_init(self):
        """Test SyncProxy initialization."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        assert proxy._worker is worker
        assert proxy._wrapper is not None

    def test_start(self):
        """Test SyncProxy start method."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        proxy.start()

        assert worker.started is True
        assert proxy._wrapper._loop.is_running()

        proxy.close()

    def test_close(self):
        """Test SyncProxy close method."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        proxy.start()
        proxy.close()

        assert worker.closed is True

    def test_method_call(self):
        """Test method call via attribute access."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        proxy.start()

        result = proxy.add(3, 4)

        assert result == 7

        proxy.close()

    def test_method_call_with_options(self):
        """Test method call with options."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        proxy.start()

        result = proxy.with_options(timeout=10.0, namespace="test").multiply(5, 6)

        assert result == 30
        assert worker.last_call[2] == 10.0  # timeout
        assert worker.last_call[3] == "test"  # namespace

        proxy.close()

    def test_options_are_cleared_after_call(self):
        """Test that options are cleared after a call."""
        worker = MockAsyncWorker()
        proxy = SyncProxy(worker)

        proxy.start()

        proxy.with_options(timeout=10.0).add(1, 2)
        proxy.add(3, 4)

        # Second call should have no timeout
        assert worker.last_call[2] is None

        proxy.close()

    def test_context_manager(self):
        """Test SyncProxy as context manager."""
        worker = MockAsyncWorker()

        with SyncProxy(worker) as proxy:
            result = proxy.add(10, 20)
            assert result == 30

        assert worker.closed is True


class TestAsyncProxy:
    """Test AsyncProxy class."""

    def test_init(self):
        """Test AsyncProxy initialization."""
        worker = MockAsyncWorker()
        proxy = AsyncProxy(worker)

        assert proxy._worker is worker
        assert proxy._options == {}

    async def test_async_method_call(self):
        """Test async method call via attribute access."""
        worker = MockAsyncWorker()
        proxy = AsyncProxy(worker)

        result = await proxy.add(3, 4)

        assert result == 7

    async def test_async_method_call_with_options(self):
        """Test async method call with options."""
        worker = MockAsyncWorker()
        proxy = AsyncProxy(worker)

        result = await proxy.with_options(timeout=10.0, namespace="test").multiply(5, 6)

        assert result == 30

    async def test_options_are_cleared_after_call(self):
        """Test that options are cleared after a call."""
        worker = MockAsyncWorker()
        proxy = AsyncProxy(worker)

        await proxy.with_options(timeout=10.0).add(1, 2)
        await proxy.add(3, 4)

        # Second call should have no timeout
        assert worker.last_call[2] is None


class TestParentHandle:
    """Test ParentHandle class."""

    def test_init(self):
        """Test ParentHandle initialization."""
        worker = MockAsyncWorker()
        handle = ParentHandle(worker)

        assert handle._worker is worker
        assert handle._call is None

    def test_call_property(self):
        """Test call property creates AsyncProxy."""
        worker = MockAsyncWorker()
        handle = ParentHandle(worker)

        call = handle.call

        assert isinstance(call, AsyncProxy)
        assert handle._call is call  # Cached

    def test_call_property_caches(self):
        """Test that call property caches the proxy."""
        worker = MockAsyncWorker()
        handle = ParentHandle(worker)

        call1 = handle.call
        call2 = handle.call

        assert call1 is call2


class TestParentHandleSync:
    """Test ParentHandleSync class."""

    def test_init(self):
        """Test ParentHandleSync initialization."""
        worker = MockAsyncWorker()
        handle = ParentHandleSync(worker)

        assert handle._worker is worker
        assert handle._wrapper is not None
        assert handle._call is None

    def test_call_property(self):
        """Test call property creates SyncCallProxy."""
        worker = MockAsyncWorker()
        handle = ParentHandleSync(worker)

        call = handle.call

        assert isinstance(call, SyncCallProxy)
        assert handle._call is call  # Cached

    def test_start(self):
        """Test start method."""
        worker = MockAsyncWorker()
        handle = ParentHandleSync(worker)

        handle.start()

        assert worker.started is True
        assert handle._wrapper._loop.is_running()

        handle.stop()

    def test_stop(self):
        """Test stop method."""
        worker = MockAsyncWorker()
        handle = ParentHandleSync(worker)

        handle.start()
        handle.stop()

        assert worker.closed is True

    def test_context_manager(self):
        """Test context manager usage."""
        worker = MockAsyncWorker()

        with ParentHandleSync(worker) as handle:
            assert worker.started is True
            result = handle.call.add(5, 7)
            assert result == 12

        assert worker.closed is True


class TestSyncCallProxy:
    """Test SyncCallProxy class."""

    def test_init(self):
        """Test SyncCallProxy initialization."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)
        proxy = SyncCallProxy(wrapper)

        assert proxy._wrapper is wrapper
        assert proxy._options == {}

    def test_method_call(self):
        """Test method call via attribute access."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)
        proxy = SyncCallProxy(wrapper)

        wrapper.start()

        result = proxy.add(100, 200)

        assert result == 300

        wrapper.close()

    def test_method_call_with_options(self):
        """Test method call with options."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)
        proxy = SyncCallProxy(wrapper)

        wrapper.start()

        result = proxy.with_options(timeout=5.0, namespace="ns").multiply(3, 7)

        assert result == 21
        assert worker.last_call[2] == 5.0
        assert worker.last_call[3] == "ns"

        wrapper.close()


class TestEventLoopManagement:
    """Test event loop management in SyncWrapper."""

    def test_loop_starts_in_background_thread(self):
        """Test that loop runs in a background thread."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        main_thread = threading.current_thread()

        wrapper.start()

        # The loop thread should be different from main thread
        assert wrapper._loop_thread is not main_thread
        assert wrapper._loop_thread.is_alive()

        wrapper.close()

    def test_loop_stops_on_close(self):
        """Test that loop thread stops on close."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()
        loop_thread = wrapper._loop_thread

        wrapper.close()

        # Give thread time to stop
        time.sleep(0.2)

        assert not loop_thread.is_alive()

    def test_reusable_after_close(self):
        """Test that wrapper can be reused after close."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()
        wrapper.close()

        # Start again
        wrapper.start()

        assert wrapper._loop.is_running()
        result = wrapper.call("add", 1, 2)
        assert result == 3

        wrapper.close()

    def test_multiple_calls_same_loop(self):
        """Test that multiple calls use the same loop."""
        worker = MockAsyncWorker()
        wrapper = SyncWrapper(worker)

        wrapper.start()

        loop = wrapper._loop

        wrapper.call("add", 1, 2)
        assert wrapper._loop is loop

        wrapper.call("multiply", 3, 4)
        assert wrapper._loop is loop

        wrapper.close()


def run_tests():
    """Run all tests."""
    print("=" * 60)
    print("Running SyncWrapper Tests")
    print("=" * 60)

    # TestSyncWrapper
    print("\nTestSyncWrapper...")
    t = TestSyncWrapper()
    t.test_init()
    t.test_start_creates_loop()
    t.test_close_stops_loop()
    t.test_call_method()
    t.test_call_with_timeout()
    t.test_call_with_namespace()
    t.test_call_raises_error()
    t.test_context_manager()
    t.test_context_manager_with_exception()
    t.test_run_async_without_loop_raises()
    print("  [OK] All TestSyncWrapper passed")

    # TestSyncProxy
    print("\nTestSyncProxy...")
    t = TestSyncProxy()
    t.test_init()
    t.test_start()
    t.test_close()
    t.test_method_call()
    t.test_method_call_with_options()
    t.test_options_are_cleared_after_call()
    t.test_context_manager()
    print("  [OK] All TestSyncProxy passed")

    # TestAsyncProxy
    print("\nTestAsyncProxy...")
    t = TestAsyncProxy()
    t.test_init()

    async def run_async_tests():
        await t.test_async_method_call()
        await t.test_async_method_call_with_options()
        await t.test_options_are_cleared_after_call()

    asyncio.run(run_async_tests())
    print("  [OK] All TestAsyncProxy passed")

    # TestParentHandle
    print("\nTestParentHandle...")
    t = TestParentHandle()
    t.test_init()
    t.test_call_property()
    t.test_call_property_caches()
    print("  [OK] All TestParentHandle passed")

    # TestParentHandleSync
    print("\nTestParentHandleSync...")
    t = TestParentHandleSync()
    t.test_init()
    t.test_call_property()
    t.test_start()
    t.test_stop()
    t.test_context_manager()
    print("  [OK] All TestParentHandleSync passed")

    # TestSyncCallProxy
    print("\nTestSyncCallProxy...")
    t = TestSyncCallProxy()
    t.test_init()
    t.test_method_call()
    t.test_method_call_with_options()
    print("  [OK] All TestSyncCallProxy passed")

    # TestEventLoopManagement
    print("\nTestEventLoopManagement...")
    t = TestEventLoopManagement()
    t.test_loop_starts_in_background_thread()
    t.test_loop_stops_on_close()
    t.test_reusable_after_close()
    t.test_multiple_calls_same_loop()
    print("  [OK] All TestEventLoopManagement passed")

    print("\n" + "=" * 60)
    print("[PASS] All SyncWrapper tests passed!")
    print("=" * 60)


if __name__ == "__main__":
    run_tests()
