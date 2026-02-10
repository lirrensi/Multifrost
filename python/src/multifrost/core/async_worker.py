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
from typing import Optional, Dict, Any

from .message import ComlinkMessage, MessageType, APP_NAME
from .sync_wrapper import SyncProxy, AsyncProxy
from .service_registry import ServiceRegistry


class RemoteCallError(Exception):
    """Exception for failures that occurred on the remote worker/child."""


class ParentWorker:
    """
    Async-first parent worker for managing child processes.

    Supports two modes:
    - Spawn mode: Parent spawns child process (owns the child)
    - Connect mode: Parent connects to existing service via ServiceRegistry
    """

    def __init__(
        self,
        script_path: Optional[str] = None,
        executable: Optional[str] = None,
        port: Optional[int] = None,
        service_id: Optional[str] = None,
        auto_restart: bool = False,
        max_restart_attempts: int = 5,
    ):
        # Internal config - use factory methods instead
        self._script_path = script_path
        self._executable = executable or sys.executable
        self._service_id = service_id
        self._port = port or self._find_free_port()
        self._is_spawn_mode = script_path is not None

        self.auto_restart = auto_restart
        self.max_restart_attempts = max_restart_attempts
        self.restart_count = 0

        # Async state
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self.running = False
        self._closed = False

        # ZeroMQ setup - DEALER socket
        self.context: Optional[zmq.Context] = None
        self.socket: Optional[zmq.Socket] = None  # DEALER socket
        self.process: Optional[subprocess.Popen] = None

        # Pending requests: request_id -> asyncio.Future
        self.pending_requests: Dict[str, asyncio.Future] = {}
        self._lock = asyncio.Lock()

        # ZMQ event loop task
        self._zmq_task: Optional[asyncio.Task] = None

        # Proxy interfaces
        self._sync_proxy: Optional[SyncProxy] = None
        self._async_proxy: Optional[AsyncProxy] = None

        # Signal handlers
        signal.signal(signal.SIGINT, self._signal_handler)
        signal.signal(signal.SIGTERM, self._signal_handler)

    @staticmethod
    def spawn(script_path: str, executable: Optional[str] = None) -> "ParentWorker":
        """
        Create a ParentWorker in spawn mode (owns the child process).

        Args:
            script_path: Path to the child worker script
            executable: Optional executable to run (default: sys.executable)

        Returns:
            ParentWorker instance configured for spawn mode
        """
        return ParentWorker(script_path=script_path, executable=executable)

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

        # Setup ZeroMQ DEALER socket
        await self._setup_socket()

        # Start child process if in spawn mode
        if self._is_spawn_mode:
            await self._start_child_process()

        # Start ZMQ event loop
        self._zmq_task = asyncio.create_task(self._zmq_event_loop())

        # Wait for connection
        await asyncio.sleep(1)

    async def _setup_socket(self):
        """Setup ZeroMQ DEALER socket."""
        try:
            self.context = zmq.Context()
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

    async def _start_child_process(self):
        """Start the child process with environment."""
        env = os.environ.copy()
        env["COMLINK_ZMQ_PORT"] = str(self._port)

        self.process = subprocess.Popen(
            [self._executable, self._script_path],
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

    async def _call_internal(
        self,
        func_name: str,
        *args,
        timeout: Optional[float] = None,
        namespace: str = "default",
        client_name: Optional[str] = None,
    ) -> Any:
        """
        Internal unified async call method.

        Args:
            func_name: Name of the function to call
            *args: Positional arguments
            timeout: Optional timeout in seconds
            namespace: Namespace for routing (default: 'default')
            client_name: Optional client identifier for service targeting

        Returns:
            Result from the remote function
        """
        # Check if we can make a call
        if self._is_spawn_mode:
            if not self.running or not self.process or self.process.poll() is not None:
                raise Exception("Child process is not running")
        else:
            if not self.running:
                raise Exception("Worker is not running")

        request_id = str(uuid.uuid4())
        future = asyncio.Future()

        # Store future
        async with self._lock:
            self.pending_requests[request_id] = future

        # Create and send message
        message = ComlinkMessage.create_call(
            function=func_name,
            args=args,
            namespace=namespace,
            msg_id=request_id,
            client_name=client_name,
        )

        await self._send_message(message)

        # Wait for response with timeout
        try:
            return await asyncio.wait_for(future, timeout=timeout)
        except asyncio.TimeoutError:
            async with self._lock:
                self.pending_requests.pop(request_id, None)
            raise TimeoutError(
                f"Function '{func_name}' timed out after {timeout} seconds"
            )

    async def _send_message(self, message: ComlinkMessage):
        """Send a message to the child process via DEALER socket."""
        try:
            # DEALER socket sends with empty delimiter frame
            await self._loop.run_in_executor(
                None,
                lambda: self.socket.send_multipart([b"", message.pack()], zmq.NOBLOCK),
            )
        except zmq.Again:
            raise Exception("Failed to send request: socket busy")
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
                # Try to receive message (non-blocking via executor)
                # DEALER socket receives: [empty_frame, message_data]
                try:
                    frames = await self._loop.run_in_executor(
                        None, lambda: self.socket.recv_multipart(zmq.NOBLOCK)
                    )
                    if len(frames) >= 2:
                        empty = frames[0]  # Should be empty
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
            elif message.type == MessageType.STDOUT.value:
                output = getattr(message, "output", "")
                if output:
                    script_name = (
                        self._script_path.split("/")[-1]
                        if self._script_path
                        else "worker"
                    )
                    print(f"[{script_name} STDOUT]: {output}")
            elif message.type == MessageType.STDERR.value:
                output = getattr(message, "output", "")
                if output:
                    script_name = (
                        self._script_path.split("/")[-1]
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

    async def _handle_child_exit(self):
        """Handle child process exit."""
        print(f"Child process exited with code: {self.process.returncode}")

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
            print(
                f"Restarting worker (attempt {self.restart_count}/{self.max_restart_attempts})..."
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

            print("Worker restarted successfully")
            self.restart_count = 0

        except Exception as e:
            print(f"Auto-restart failed: {e}")
            if self.restart_count >= self.max_restart_attempts:
                print("Max restart attempts reached")
            self.running = False

    async def close(self):
        """Close the worker and cleanup resources."""
        if self._closed:
            return
        self._closed = True
        self.running = False

        # Cancel pending requests
        async with self._lock:
            pending = list(self.pending_requests.items())
            self.pending_requests.clear()

        for request_id, future in pending:
            if not future.done():
                future.set_exception(RemoteCallError("Worker shutting down"))

        # Cleanup resources
        try:
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
            print(f"Error during cleanup: {e}")

    def _signal_handler(self, signum, frame):
        """Handle signals by cleaning up."""
        print(f"\nReceived signal {signum}, cleaning up...")
        if self._loop and not self._loop.is_closed():
            self._loop.create_task(self.close())
        signal.default_int_handler(signum, frame)

    async def __aenter__(self):
        await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, tb):
        await self.close()

    def __del__(self):
        """Cleanup on deletion."""
        if not self._closed and self._loop and not self._loop.is_closed():
            # Schedule cleanup on the event loop
            self._loop.create_task(self.close())
