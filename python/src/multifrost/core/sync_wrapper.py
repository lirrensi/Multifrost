"""
Sync wrapper for async ParentWorker.
Provides a synchronous API on top of the async core.
"""

import asyncio
import threading
from typing import Any, Optional, TYPE_CHECKING

if TYPE_CHECKING:
    from .async_worker import ParentWorker


class ParentHandle:
    """
    Async handle for ParentWorker - provides lifecycle + call interface.

    Separates process definition (Worker) from runtime interface (Handle).
    The handle is cheap to create and delegates all operations to the worker.

    Usage:
        worker = ParentWorker.spawn("script.py")
        handle = worker.handle()
        await handle.start()
        result = await handle.call.my_function(1, 2)
        await handle.stop()

        # Or with context manager:
        async with worker.handle() as h:
            result = await h.call.my_function(1, 2)
    """

    def __init__(self, worker: "ParentWorker"):
        """
        Initialize async handle.

        Args:
            worker: The ParentWorker instance to delegate to
        """
        self._worker = worker
        self._call: Optional["AsyncProxy"] = None

    @property
    def call(self) -> "AsyncProxy":
        """
        Get the async call proxy for remote method invocation.

        Returns:
            AsyncProxy for method calls like handle.call.my_function(args)
        """
        if self._call is None:
            self._call = AsyncProxy(self._worker)
        return self._call

    async def start(self) -> None:
        """Start the worker (spawn process, connect ZMQ)."""
        await self._worker.start()

    async def stop(self) -> None:
        """Stop the worker (cleanup resources, terminate child)."""
        await self._worker.close()

    async def __aenter__(self) -> "ParentHandle":
        """Async context manager entry."""
        await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, tb) -> None:
        """Async context manager exit."""
        await self.stop()


class ParentHandleSync:
    """
    Sync handle for ParentWorker - provides lifecycle + call interface.

    Separates process definition (Worker) from runtime interface (Handle).
    The handle is cheap to create and delegates all operations to the worker.

    Usage:
        worker = ParentWorker.spawn("script.py")
        handle = worker.handle_sync()
        handle.start()
        result = handle.call.my_function(1, 2)
        handle.stop()

        # Or with context manager:
        with worker.handle_sync() as h:
            result = h.call.my_function(1, 2)
    """

    def __init__(self, worker: "ParentWorker"):
        """
        Initialize sync handle.

        Args:
            worker: The ParentWorker instance to delegate to
        """
        self._worker = worker
        self._wrapper = SyncWrapper(worker)
        self._call: Optional["SyncCallProxy"] = None

    @property
    def call(self) -> "SyncCallProxy":
        """
        Get the sync call proxy for remote method invocation.

        Returns:
            SyncCallProxy for method calls like handle.call.my_function(args)
        """
        if self._call is None:
            self._call = SyncCallProxy(self._wrapper)
        return self._call

    def start(self) -> None:
        """Start the worker (spawn process, connect ZMQ)."""
        self._wrapper.start()

    def stop(self) -> None:
        """Stop the worker (cleanup resources, terminate child)."""
        self._wrapper.close()

    def __enter__(self) -> "ParentHandleSync":
        """Context manager entry."""
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, tb) -> None:
        """Context manager exit."""
        self.stop()


class SyncWrapper:
    """
    Synchronous wrapper for async ParentWorker.

    Provides a blocking API that internally manages an event loop.
    """

    def __init__(self, async_worker):
        """
        Initialize sync wrapper.

        Args:
            async_worker: The async ParentWorker instance to wrap
        """
        self._worker = async_worker
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self._loop_thread: Optional[threading.Thread] = None
        self._loop_started = threading.Event()
        self._loop_stopped = threading.Event()

    def start(self):
        """Start the worker synchronously."""
        self._ensure_loop()
        return self._run_async(self._worker.start())

    def call(
        self,
        func_name: str,
        *args,
        timeout: Optional[float] = None,
        namespace: str = "default",
    ) -> Any:
        """
        Call a remote function synchronously.

        Args:
            func_name: Name of the function to call
            *args: Positional arguments
            timeout: Optional timeout in seconds
            namespace: Namespace for routing (default: 'default')

        Returns:
            Result from the remote function
        """
        self._ensure_loop()
        return self._run_async(
            self._worker.call(func_name, *args, timeout=timeout, namespace=namespace)
        )

    def close(self):
        """Close the worker synchronously."""
        if self._loop and not self._loop.is_closed():
            self._run_async(self._worker.close())
            self._stop_loop()

    def _ensure_loop(self):
        """Ensure an event loop is running in a background thread."""
        if self._loop is None or self._loop.is_closed():
            self._start_loop()

    def _start_loop(self):
        """Start an event loop in a background thread."""
        self._loop_started.clear()
        self._loop_stopped.clear()

        def run_loop():
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            self._loop_started.set()
            self._loop.run_forever()
            self._loop_stopped.set()

        self._loop_thread = threading.Thread(target=run_loop, daemon=True)
        self._loop_thread.start()
        self._loop_started.wait()

    def _stop_loop(self):
        """Stop the background event loop."""
        if self._loop and not self._loop.is_closed():
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._loop_stopped.wait(timeout=2)
            if self._loop_thread is not None:
                self._loop_thread.join(timeout=2)
        self._loop = None
        self._loop_thread = None

    def _run_async(self, coro):
        """
        Run an async coroutine in the background loop.

        Args:
            coro: The coroutine to run

        Returns:
            The result of the coroutine
        """
        if not self._loop or self._loop.is_closed():
            raise RuntimeError("Event loop is not running")

        future = asyncio.run_coroutine_threadsafe(coro, self._loop)
        return future.result()

    def __enter__(self):
        """Context manager entry."""
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, tb):
        """Context manager exit."""
        self.close()

    def __del__(self):
        """Cleanup on deletion."""
        if self._loop and not self._loop.is_closed():
            self.close()


class SyncProxy:
    """
    Proxy object for synchronous method calls.

    Usage:
        worker = ParentWorker(...)
        worker.sync.start()
        result = worker.sync.call.my_function(1, 2, 3)
        worker.sync.close()
    """

    def __init__(self, async_worker):
        """
        Initialize sync proxy.

        Args:
            async_worker: The async ParentWorker instance
        """
        self._worker = async_worker
        self._wrapper = SyncWrapper(async_worker)
        self._options = {}

    def start(self):
        """Start the worker."""
        return self._wrapper.start()

    def close(self):
        """Close the worker."""
        return self._wrapper.close()

    def with_options(self, **options):
        """
        Set options for the next method call.

        Args:
            **options: Options like timeout, namespace

        Returns:
            Self for chaining
        """
        self._options = options
        return self

    def __getattr__(self, name):
        """
        Create a synchronous method call proxy.

        Args:
            name: Name of the remote method

        Returns:
            A callable that executes the remote method synchronously
        """

        def remote_method(*args):
            # Use pending options then clear them
            timeout = self._options.get("timeout")
            namespace = self._options.get("namespace", "default")
            self._options = {}

            return self._wrapper.call(name, *args, timeout=timeout, namespace=namespace)

        return remote_method

    def __enter__(self):
        """Context manager entry."""
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, tb):
        """Context manager exit."""
        self.close()


class AsyncProxy:
    """
    Proxy object for asynchronous method calls.

    Usage:
        worker = ParentWorker(...)
        await worker.start()
        result = await worker.call.my_function(1, 2, 3)
        await worker.close()
    """

    def __init__(self, async_worker):
        """
        Initialize async proxy.

        Args:
            async_worker: The async ParentWorker instance
        """
        self._worker = async_worker
        self._options = {}

    def with_options(self, **options):
        """
        Set options for the next method call.

        Args:
            **options: Options like timeout, namespace

        Returns:
            Self for chaining
        """
        self._options = options
        return self

    def __getattr__(self, name):
        """
        Create an asynchronous method call proxy.

        Args:
            name: Name of the remote method

        Returns:
            An async callable that executes the remote method
        """

        async def remote_method(*args):
            # Use pending options then clear them
            timeout = self._options.get("timeout")
            namespace = self._options.get("namespace", "default")
            self._options = {}

            return await self._worker._call_internal(
                name, *args, timeout=timeout, namespace=namespace
            )

        return remote_method


class SyncCallProxy:
    """
    Proxy object for synchronous method calls via Handle.

    Used by ParentHandleSync.call to provide fluent method access.

    Usage:
        handle = worker.handle_sync()
        handle.start()
        result = handle.call.my_function(1, 2, 3)
        handle.stop()
    """

    def __init__(self, wrapper: SyncWrapper):
        """
        Initialize sync call proxy.

        Args:
            wrapper: The SyncWrapper instance to delegate to
        """
        self._wrapper = wrapper
        self._options = {}

    def with_options(self, **options):
        """
        Set options for the next method call.

        Args:
            **options: Options like timeout, namespace

        Returns:
            Self for chaining
        """
        self._options = options
        return self

    def __getattr__(self, name):
        """
        Create a synchronous method call proxy.

        Args:
            name: Name of the remote method

        Returns:
            A callable that executes the remote method synchronously
        """

        def remote_method(*args):
            # Use pending options then clear them
            timeout = self._options.get("timeout")
            namespace = self._options.get("namespace", "default")
            self._options = {}

            return self._wrapper.call(name, *args, timeout=timeout, namespace=namespace)

        return remote_method
