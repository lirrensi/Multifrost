"""
Async-first ParentWorker implementation with ROUTER/DEALER architecture.
"""

import asyncio
import os
import signal
import socket
import subprocess
import sys
import time
import uuid
import zmq
import zmq.asyncio
from typing import Optional, Dict, Any, Set, Callable

from .message import ComlinkMessage, MessageType, APP_NAME
from .sync_wrapper import SyncProxy, AsyncProxy
from .service_registry import ServiceRegistry
from .metrics import Metrics
from .logging import StructuredLogger, LogEntry, LogEvent, LogHandler


class RemoteCallError(Exception):
    """Exception for failures that occurred on the remote worker/child."""


class CircuitOpenError(Exception):
    """Raised when circuit breaker is open (too many consecutive failures)."""


# Global registry for signal handling (module-level, not ClassVar)
_signal_handlers_installed: bool = False
_active_workers: Set["ParentWorker"] = set()


def _install_signal_handlers():
    """Install signal handlers once (global)."""
    global _signal_handlers_installed
    if _signal_handlers_installed:
        return
    _signal_handlers_installed = True

    def handle_signal(signum, frame):
        """Forward signal to all active workers."""
        print(f"\nReceived signal {signum}, cleaning up...")
        for worker in list(_active_workers):
            if worker._loop and not worker._loop.is_closed():
                worker._loop.call_soon_threadsafe(
                    lambda w=worker: asyncio.create_task(w.close())
                )
        # Re-raise default handler after cleanup
        signal.default_int_handler(signum, frame)

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)


class ParentWorker:
    """
    Async-first parent worker for managing child processes.

    Supports two modes:
    - Spawn mode: Parent spawns child process (owns the child)
    - Connect mode: Parent connects to existing service via ServiceRegistry

    Features:
    - Circuit breaker: Trips after max_restart_attempts consecutive failures
    - Auto-reconnect: In connect mode, attempts to rediscover service on failure
    - Default timeout: Configurable at worker level
    - Metrics: Request latency, error rates, queue depth
    - Structured logging: JSON logs with correlation IDs
    """

    def __init__(
        self,
        script_path: Optional[str] = None,
        executable: Optional[str] = None,
        port: Optional[int] = None,
        service_id: Optional[str] = None,
        auto_restart: bool = False,
        max_restart_attempts: int = 5,
        default_timeout: Optional[float] = None,
        enable_metrics: bool = True,
        log_handler: Optional[LogHandler] = None,
        heartbeat_interval: float = 5.0,
        heartbeat_timeout: float = 3.0,
        heartbeat_max_misses: int = 3,
    ):
        # Internal config - use factory methods instead
        self._script_path = script_path
        self._executable = executable or sys.executable
        self._service_id = service_id
        self._port = port or self._find_free_port()
        self._is_spawn_mode = script_path is not None
        self._worker_id = str(uuid.uuid4())[:8]

        self.auto_restart = auto_restart
        self.max_restart_attempts = max_restart_attempts
        self.default_timeout = default_timeout
        self.restart_count = 0

        # Circuit breaker state
        self._consecutive_failures = 0
        self._circuit_open = False

        # Heartbeat configuration
        self.heartbeat_interval = heartbeat_interval
        self.heartbeat_timeout = heartbeat_timeout
        self.heartbeat_max_misses = heartbeat_max_misses

        # Heartbeat state
        self._pending_heartbeats: Dict[str, asyncio.Future] = {}
        self._consecutive_heartbeat_misses = 0
        self._last_heartbeat_rtt_ms: Optional[float] = None
        self._heartbeat_task: Optional[asyncio.Task] = None

        # Metrics collection
        self.enable_metrics = enable_metrics
        self._metrics = Metrics() if enable_metrics else None

        # Structured logging
        self._logger = StructuredLogger(
            handler=log_handler,
            worker_id=self._worker_id,
            service_id=service_id,
        )

        # Async state
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self.running = False
        self._closed = False

        # ZeroMQ setup - async DEALER socket
        self.context: Optional[zmq.asyncio.Context] = None
        self.socket: Optional[zmq.asyncio.Socket] = None  # DEALER socket
        self.process: Optional[subprocess.Popen] = None

        # Pending requests: request_id -> asyncio.Future
        self.pending_requests: Dict[str, asyncio.Future] = {}
        self._lock = asyncio.Lock()

        # ZMQ event loop task
        self._zmq_task: Optional[asyncio.Task] = None

        # Proxy interfaces
        self._sync_proxy: Optional[SyncProxy] = None
        self._async_proxy: Optional[AsyncProxy] = None

        # Register for signal handling
        _install_signal_handlers()

    @property
    def metrics(self) -> Optional[Metrics]:
        """Get the metrics collector."""
        return self._metrics

    @property
    def logger(self) -> StructuredLogger:
        """Get the structured logger."""
        return self._logger

    def set_log_handler(self, handler: LogHandler):
        """
        Set a custom log handler.

        Usage:
            worker.set_log_handler(lambda entry: print(entry.to_json()))
            # Or with pretty printing:
            from multifrost.core.logging import default_pretty_handler
            worker.set_log_handler(default_pretty_handler)
        """
        self._logger.set_handler(handler)

    @property
    def is_healthy(self) -> bool:
        """Check if the worker is healthy (circuit breaker not tripped)."""
        return not self._circuit_open and self.running

    @property
    def circuit_open(self) -> bool:
        """Check if circuit breaker is open."""
        return self._circuit_open

    @property
    def last_heartbeat_rtt_ms(self) -> Optional[float]:
        """Get the last heartbeat round-trip time in milliseconds."""
        return self._last_heartbeat_rtt_ms

    @staticmethod
    def spawn(
        script_path: str,
        executable: Optional[str] = None,
        auto_restart: bool = False,
        max_restart_attempts: int = 5,
        default_timeout: Optional[float] = None,
        enable_metrics: bool = True,
        log_handler: Optional[LogHandler] = None,
        heartbeat_interval: float = 5.0,
        heartbeat_timeout: float = 3.0,
        heartbeat_max_misses: int = 3,
    ) -> "ParentWorker":
        """
        Create a ParentWorker in spawn mode (owns the child process).

        Args:
            script_path: Path to the child worker script
            executable: Optional executable to run (default: sys.executable)
            auto_restart: Whether to auto-restart crashed children
            max_restart_attempts: Max restart attempts before circuit breaker trips
            default_timeout: Default timeout for calls in seconds
            enable_metrics: Whether to collect metrics
            log_handler: Optional handler for structured logs
            heartbeat_interval: Seconds between heartbeats (0 to disable)
            heartbeat_timeout: Seconds to wait for heartbeat response
            heartbeat_max_misses: Consecutive misses before triggering failure

        Returns:
            ParentWorker instance configured for spawn mode
        """
        return ParentWorker(
            script_path=script_path,
            executable=executable,
            auto_restart=auto_restart,
            max_restart_attempts=max_restart_attempts,
            default_timeout=default_timeout,
            enable_metrics=enable_metrics,
            log_handler=log_handler,
            heartbeat_interval=heartbeat_interval,
            heartbeat_timeout=heartbeat_timeout,
            heartbeat_max_misses=heartbeat_max_misses,
        )

    @staticmethod
    async def connect(service_id: str, timeout: float = 5.0) -> "ParentWorker":
        """
        Create a ParentWorker in connect mode (connects to existing service).

        Args:
            service_id: The service identifier to connect to
            timeout: Timeout in seconds for service discovery

        Returns:
            ParentWorker instance configured for connect mode
        """
        port = await ServiceRegistry.discover(service_id, timeout)
        return ParentWorker(service_id=service_id, port=port)

    @property
    def call(self) -> SyncProxy:
        """
        Synchronous proxy interface.

        Usage:
            worker.call.start()
            result = worker.call.my_function(1, 2)
            worker.call.close()
        """
        if self._sync_proxy is None:
            self._sync_proxy = SyncProxy(self)
        return self._sync_proxy

    @property
    def acall(self) -> AsyncProxy:
        """
        Asynchronous proxy interface.

        Usage:
            await worker.start()
            result = await worker.acall.my_function(1, 2)
            await worker.close()
        """
        if self._async_proxy is None:
            self._async_proxy = AsyncProxy(self)
        return self._async_proxy

    def _find_free_port(self) -> int:
        """Find a free port for ZeroMQ communication."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("", 0))
            s.listen(1)
            port = s.getsockname()[1]
        return port

    async def start(self):
        """Start the parent worker and child process (if spawn mode)."""
        self._loop = asyncio.get_running_loop()
        self.running = True

        # Register for signal handling
        _active_workers.add(self)

        # Setup ZeroMQ DEALER socket
        await self._setup_socket()

        # Start child process if in spawn mode
        if self._is_spawn_mode:
            await self._start_child_process()

        # Start ZMQ event loop
        self._zmq_task = asyncio.create_task(self._zmq_event_loop())

        # Start heartbeat task (spawn mode only)
        if self._is_spawn_mode and self.heartbeat_interval > 0:
            self._heartbeat_task = asyncio.create_task(self._heartbeat_loop())

        # Wait for connection
        await asyncio.sleep(1)

        # Log worker start
        mode = "spawn" if self._is_spawn_mode else "connect"
        self._logger.worker_start(mode=mode)

    async def _setup_socket(self):
        """Setup ZeroMQ DEALER socket with zmq.asyncio."""
        try:
            self.context = zmq.asyncio.Context()
            self.socket = self.context.socket(zmq.DEALER)

            if self._is_spawn_mode:
                # Spawn mode: bind to port, child connects
                endpoint = f"tcp://*:{self._port}"
                self.socket.bind(endpoint)
            else:
                # Connect mode: connect to service
                endpoint = f"tcp://localhost:{self._port}"
                self.socket.connect(endpoint)

            # Set timeouts
            self.socket.setsockopt(zmq.RCVTIMEO, 100)
            self.socket.setsockopt(zmq.SNDTIMEO, 100)

        except zmq.ZMQError as e:
            if e.errno == zmq.EADDRINUSE:
                raise Exception(f"Port {self._port} is already in use")
            raise Exception(f"Failed to setup ZMQ socket: {e}")

    async def _reconnect_socket(self):
        """Reconnect socket in connect mode (for service restarts)."""
        if self._is_spawn_mode:
            return  # Not applicable in spawn mode

        try:
            # Close existing socket
            if self.socket:
                self.socket.close()

            # Discover service again
            self._port = await ServiceRegistry.discover(self._service_id, timeout=5.0)

            # Create new socket
            self.socket = self.context.socket(zmq.DEALER)
            endpoint = f"tcp://localhost:{self._port}"
            self.socket.connect(endpoint)
            self.socket.setsockopt(zmq.RCVTIMEO, 100)
            self.socket.setsockopt(zmq.SNDTIMEO, 100)

            return True
        except Exception as e:
            print(f"Reconnect failed: {e}")
            return False

    async def _start_child_process(self):
        """Start the child process with environment."""
        env = os.environ.copy()
        env["COMLINK_ZMQ_PORT"] = str(self._port)
        env["COMLINK_WORKER_MODE"] = "1"

        self.process = subprocess.Popen(
            [self._executable, self._script_path, "--worker"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Check if process started successfully
        await asyncio.sleep(0.5)
        if self.process.poll() is not None:
            raise Exception(
                f"Child process failed to start (exit code: {self.process.returncode})"
            )

    def _record_failure(self):
        """Record a failure for circuit breaker."""
        self._consecutive_failures += 1
        if self._consecutive_failures >= self.max_restart_attempts:
            self._circuit_open = True
            self._logger.circuit_open(self._consecutive_failures)
            if self._metrics:
                self._metrics.record_circuit_breaker_trip()

    def _record_success(self):
        """Record a success, resetting circuit breaker."""
        if self._consecutive_failures > 0:
            self._consecutive_failures = 0
            if self._circuit_open:
                self._circuit_open = False
                self._logger.circuit_close()
                if self._metrics:
                    self._metrics.record_circuit_breaker_reset()

    async def _call_internal(
        self,
        func_name: str,
        *args,
        timeout: Optional[float] = None,
        namespace: str = "default",
        client_name: Optional[str] = None,
        correlation_id: Optional[str] = None,
        metadata: Optional[Dict[str, Any]] = None,
    ) -> Any:
        """
        Internal unified async call method.

        Args:
            func_name: Name of the function to call
            *args: Positional arguments
            timeout: Optional timeout in seconds (uses default_timeout if not set)
            namespace: Namespace for routing (default: 'default')
            client_name: Optional client identifier for service targeting
            correlation_id: Optional correlation ID for distributed tracing
            metadata: Optional metadata dict for custom context

        Returns:
            Result from the remote function

        Raises:
            CircuitOpenError: If circuit breaker is tripped
            RemoteCallError: If remote call fails
            TimeoutError: If call times out
        """
        # Check circuit breaker
        if self._circuit_open:
            raise CircuitOpenError(
                f"Circuit breaker open after {self._consecutive_failures} consecutive failures"
            )

        # Check if we can make a call
        if self._is_spawn_mode:
            if not self.running or not self.process or self.process.poll() is not None:
                raise Exception("Child process is not running")
        else:
            if not self.running:
                raise Exception("Worker is not running")

        # Use default timeout if not specified
        effective_timeout = timeout if timeout is not None else self.default_timeout

        request_id = str(uuid.uuid4())
        future = asyncio.Future()

        # Start metrics tracking
        start_time = time.perf_counter()
        if self._metrics:
            self._metrics.start_request(request_id, func_name, namespace)

        # Log request start
        self._logger.request_start(
            request_id=request_id,
            function=func_name,
            namespace=namespace,
            correlation_id=correlation_id,
            metadata=metadata,
        )

        # Store future
        async with self._lock:
            self.pending_requests[request_id] = future

        # Create and send message with metadata
        message = ComlinkMessage.create_call(
            function=func_name,
            args=args,
            namespace=namespace,
            msg_id=request_id,
            client_name=client_name,
            correlation_id=correlation_id,
            metadata=metadata,
        )

        await self._send_message(message)

        # Wait for response with timeout
        try:
            result = await asyncio.wait_for(future, timeout=effective_timeout)

            # Record success
            duration_ms = (time.perf_counter() - start_time) * 1000
            if self._metrics:
                self._metrics.end_request(start_time, request_id, success=True)

            self._record_success()
            self._logger.request_end(
                request_id=request_id,
                function=func_name,
                duration_ms=duration_ms,
                success=True,
                correlation_id=correlation_id,
            )
            return result

        except asyncio.TimeoutError:
            async with self._lock:
                self.pending_requests.pop(request_id, None)

            duration_ms = (time.perf_counter() - start_time) * 1000
            if self._metrics:
                self._metrics.end_request(
                    start_time, request_id, success=False, error="Timeout"
                )
            self._record_failure()

            self._logger.request_end(
                request_id=request_id,
                function=func_name,
                duration_ms=duration_ms,
                success=False,
                error=f"Timeout after {effective_timeout}s",
                correlation_id=correlation_id,
            )
            raise TimeoutError(
                f"Function '{func_name}' timed out after {effective_timeout} seconds"
            )

        except RemoteCallError as e:
            duration_ms = (time.perf_counter() - start_time) * 1000
            if self._metrics:
                self._metrics.end_request(
                    start_time, request_id, success=False, error=str(e)
                )
            self._record_failure()

            self._logger.request_end(
                request_id=request_id,
                function=func_name,
                duration_ms=duration_ms,
                success=False,
                error=str(e),
                correlation_id=correlation_id,
            )

            # Try reconnect in connect mode
            if not self._is_spawn_mode and self._service_id:
                await self._reconnect_socket()
            raise

    async def _send_message(self, message: ComlinkMessage, retries: int = 5):
        """Send a message to the child process via async DEALER socket."""
        for attempt in range(retries):
            try:
                # DEALER socket sends with empty delimiter frame (now async)
                await self.socket.send_multipart([b"", message.pack()], zmq.NOBLOCK)
                return  # Success
            except zmq.Again:
                # Socket not ready, wait and retry
                if attempt < retries - 1:
                    await asyncio.sleep(0.1)
                    continue
                raise Exception("Failed to send request: socket busy after retries")
            except Exception as e:
                async with self._lock:
                    # Clean up pending request if we have the message ID
                    if hasattr(message, "id"):
                        self.pending_requests.pop(message.id, None)
                raise Exception(f"Failed to send request: {e}")

    async def _zmq_event_loop(self):
        """Pure async event loop for handling ZMQ messages."""
        while self.running:
            try:
                # Try to receive message (now truly async, no executor needed)
                # DEALER socket receives: [empty_frame, message_data]
                try:
                    frames = await self.socket.recv_multipart(zmq.NOBLOCK)
                    if len(frames) >= 2:
                        _empty = frames[0]  # Should be empty
                        message_data = frames[1]
                        await self._handle_message(message_data)
                except zmq.Again:
                    # No message available
                    pass

                # Check child process health (spawn mode only)
                if (
                    self._is_spawn_mode
                    and self.process
                    and self.process.poll() is not None
                ):
                    await self._handle_child_exit()
                    break

                # Small sleep to prevent tight loop
                await asyncio.sleep(0.01)

            except Exception as e:
                if self.running:
                    print(f"Error in ZMQ event loop: {e}")
                break

    async def _handle_message(self, message_data):
        """Handle incoming message from child process."""
        try:
            message = ComlinkMessage.unpack(message_data)

            # Basic validation
            if not hasattr(message, "app") or message.app != APP_NAME:
                return
            if not hasattr(message, "id") or not message.id:
                return

            # Handle different message types
            if message.type in [MessageType.RESPONSE.value, MessageType.ERROR.value]:
                await self._handle_response(message)
            elif message.type == MessageType.HEARTBEAT.value:
                await self._handle_heartbeat_response(message)
            elif message.type == MessageType.STDOUT.value:
                output = getattr(message, "output", "")
                if output:
                    script_name = (
                        os.path.basename(self._script_path)
                        if self._script_path
                        else "worker"
                    )
                    print(f"[{script_name} STDOUT]: {output}")
            elif message.type == MessageType.STDERR.value:
                output = getattr(message, "output", "")
                if output:
                    script_name = (
                        os.path.basename(self._script_path)
                        if self._script_path
                        else "worker"
                    )
                    print(f"[{script_name} STDERR]: {output}", file=sys.stderr)

        except Exception as e:
            print(f"ERROR: Failed to process message: {e}")

    async def _handle_response(self, message: ComlinkMessage):
        """Handle response message from child process."""
        async with self._lock:
            future = self.pending_requests.pop(message.id, None)

        if not future or future.done():
            return

        # Set result or error
        if message.type == MessageType.RESPONSE.value:
            result = getattr(message, "result", None)
            future.set_result(result)
        else:  # ERROR
            error = getattr(message, "error", "Unknown error")
            future.set_exception(RemoteCallError(error))

    async def _handle_heartbeat_response(self, message: ComlinkMessage):
        """Handle heartbeat response from child process."""
        async with self._lock:
            future = self._pending_heartbeats.pop(message.id, None)

        if not future or future.done():
            return

        # Calculate RTT from original timestamp
        original_ts = None
        if hasattr(message, "metadata") and message.metadata:
            original_ts = message.metadata.get("hb_timestamp")

        if original_ts:
            rtt_ms = (time.time() - original_ts) * 1000
            self._last_heartbeat_rtt_ms = rtt_ms

            # Record to metrics
            if self._metrics:
                self._metrics.record_heartbeat_rtt(rtt_ms)

        # Reset consecutive misses on successful response
        self._consecutive_heartbeat_misses = 0

        # Complete the future
        future.set_result(True)

    async def _heartbeat_loop(self):
        """
        Periodically send heartbeats to child process.

        If too many consecutive heartbeats are missed, triggers circuit breaker.
        """
        # Wait for initial connection
        await asyncio.sleep(1.0)

        while self.running:
            try:
                # Only send heartbeats in spawn mode (we own the child)
                if not self._is_spawn_mode:
                    await asyncio.sleep(self.heartbeat_interval)
                    continue

                # Check if child is still running
                if self.process and self.process.poll() is not None:
                    # Child died, let _zmq_event_loop handle it
                    break

                # Create heartbeat message
                heartbeat_id = str(uuid.uuid4())
                heartbeat = ComlinkMessage.create_heartbeat(msg_id=heartbeat_id)

                # Create future for response
                future = asyncio.Future()
                async with self._lock:
                    self._pending_heartbeats[heartbeat_id] = future

                # Send heartbeat
                await self._send_message(heartbeat)

                # Wait for response with timeout
                try:
                    await asyncio.wait_for(future, timeout=self.heartbeat_timeout)
                    # Success - RTT already recorded in _handle_heartbeat_response
                except asyncio.TimeoutError:
                    # Heartbeat timed out
                    self._consecutive_heartbeat_misses += 1

                    # Clean up pending heartbeat
                    async with self._lock:
                        self._pending_heartbeats.pop(heartbeat_id, None)

                    # Log missed heartbeat
                    self._logger.heartbeat_missed(
                        consecutive=self._consecutive_heartbeat_misses,
                        max_allowed=self.heartbeat_max_misses,
                    )

                    # Check if too many misses
                    if self._consecutive_heartbeat_misses >= self.heartbeat_max_misses:
                        self._logger.heartbeat_timeout(
                            misses=self._consecutive_heartbeat_misses,
                        )
                        # Treat as failure - trip circuit breaker
                        self._record_failure()
                        # Could also trigger child exit handling here
                        break

                # Wait for next interval
                await asyncio.sleep(self.heartbeat_interval)

            except asyncio.CancelledError:
                break
            except Exception as e:
                if self.running:
                    print(f"Error in heartbeat loop: {e}")
                await asyncio.sleep(self.heartbeat_interval)

    async def _handle_child_exit(self):
        """Handle child process exit."""
        exit_code = self.process.returncode if self.process else -1
        self._logger.process_exit(exit_code)

        # Record failure for circuit breaker
        self._record_failure()

        # Notify all pending requests
        async with self._lock:
            pending = list(self.pending_requests.items())
            self.pending_requests.clear()

        for request_id, future in pending:
            if not future.done():
                future.set_exception(
                    RemoteCallError("Child process terminated unexpectedly")
                )

        # Handle restart
        if self.auto_restart and self.restart_count < self.max_restart_attempts:
            await self._attempt_restart()
        else:
            self.running = False

    async def _attempt_restart(self):
        """Attempt to restart the child process."""
        try:
            self.restart_count += 1
            self._logger.log(
                LogEvent.WORKER_RESTART,
                f"Restarting worker (attempt {self.restart_count}/{self.max_restart_attempts})",
                metadata={
                    "attempt": self.restart_count,
                    "max": self.max_restart_attempts,
                },
            )

            await asyncio.sleep(1)

            # Start new process
            env = os.environ.copy()
            env["COMLINK_ZMQ_PORT"] = str(self._port)

            self.process = subprocess.Popen(
                [self._executable, self._script_path],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            # Quick health check
            await asyncio.sleep(0.5)
            if self.process.poll() is not None:
                raise Exception(
                    f"Restart failed (exit code: {self.process.returncode})"
                )

            self._logger.info(LogEvent.WORKER_RESTART, "Worker restarted successfully")
            self.restart_count = 0

        except Exception as e:
            self._logger.error(
                LogEvent.WORKER_RESTART,
                f"Auto-restart failed: {e}",
                error=str(e),
            )
            if self.restart_count >= self.max_restart_attempts:
                self._logger.warn(
                    LogEvent.WORKER_STOP,
                    "Max restart attempts reached",
                )
            self.running = False

    async def close(self):
        """Close the worker and cleanup resources."""
        if self._closed:
            return
        self._closed = True
        self.running = False

        # Log worker stop
        self._logger.worker_stop(reason="shutdown")

        # Unregister from signal handling
        _active_workers.discard(self)

        # Cancel pending requests
        async with self._lock:
            pending = list(self.pending_requests.items())
            self.pending_requests.clear()

        for request_id, future in pending:
            if not future.done():
                future.set_exception(RemoteCallError("Worker shutting down"))

        # Cleanup resources
        try:
            # Cancel heartbeat task
            if self._heartbeat_task and not self._heartbeat_task.done():
                self._heartbeat_task.cancel()
                try:
                    await self._heartbeat_task
                except asyncio.CancelledError:
                    pass

            # Cancel pending heartbeats
            async with self._lock:
                for hb_id, future in self._pending_heartbeats.items():
                    if not future.done():
                        future.cancel()
                self._pending_heartbeats.clear()

            # Wait for ZMQ task
            if self._zmq_task and not self._zmq_task.done():
                self._zmq_task.cancel()
                try:
                    await self._zmq_task
                except asyncio.CancelledError:
                    pass

            # Close socket
            if self.socket:
                self.socket.close()

            # Terminate process (spawn mode only)
            if self._is_spawn_mode and self.process:
                self.process.terminate()
                try:
                    await self._loop.run_in_executor(
                        None, lambda: self.process.wait(timeout=2)
                    )
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    await self._loop.run_in_executor(
                        None, lambda: self.process.wait(timeout=1)
                    )

            # Close context
            if self.context:
                self.context.term()

        except Exception as e:
            self._logger.error(
                LogEvent.WORKER_STOP,
                f"Error during cleanup: {e}",
                error=str(e),
            )

    async def __aenter__(self):
        await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, tb):
        await self.close()

    def __del__(self):
        """Cleanup on deletion."""
        if not self._closed and self._loop and not self._loop.is_closed():
            # Schedule cleanup on the event loop
            self._loop.call_soon_threadsafe(lambda: asyncio.create_task(self.close()))
