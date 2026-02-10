"""
Child worker implementation for receiving and handling calls from parent.
"""

import asyncio
import os
import signal
import sys
import time
import traceback
import zmq
from typing import Optional

from .message import ComlinkMessage, MessageType, APP_NAME


class ChildWorker:
    """
    Base class for creating ZeroMQ-enabled worker scripts.

    Workers inherit from this class and implement methods that can be
    called remotely by the parent process.
    """

    namespace = "default"

    def __init__(self):
        """Initialize the child worker."""
        self.APP_NAME = APP_NAME
        self._running = True

        # ZeroMQ setup
        self.context: Optional[zmq.Context] = None
        self.socket: Optional[zmq.Socket] = None
        self.port: Optional[int] = None

        # IO redirection
        self.original_stdout = None
        self.original_stderr = None

    def _setup_io_redirection(self):
        """Redirect stdout/stderr to send messages over ZMQ."""

        class ZMQWriter:
            """Custom writer that sends output over ZMQ."""

            def __init__(self, worker, msg_type):
                self.worker = worker
                self.msg_type = msg_type

            def write(self, text):
                if text.strip():
                    self.worker._send_output(self.msg_type, text.rstrip())

            def flush(self):
                pass

        # Store original stdout/stderr
        self.original_stdout = sys.stdout
        self.original_stderr = sys.stderr

        # Replace with ZMQ writers
        sys.stdout = ZMQWriter(self, MessageType.STDOUT)
        sys.stderr = ZMQWriter(self, MessageType.STDERR)

    def _setup_zmq(self):
        """Setup ZeroMQ connection to parent."""
        try:
            self.context = zmq.Context()

            # Get and validate port from environment
            port_str = os.environ.get("COMLINK_ZMQ_PORT", "5555")
            try:
                self.port = int(port_str)
                if not (1024 <= self.port <= 65535):
                    raise ValueError(f"Port {self.port} out of valid range")
            except ValueError as e:
                print(f"FATAL: Invalid port '{port_str}': {e}", file=sys.stderr)
                sys.exit(1)

            # Create and configure socket
            self.socket = self.context.socket(zmq.PAIR)
            self.socket.setsockopt(zmq.LINGER, 1000)
            self.socket.setsockopt(zmq.SNDTIMEO, 100)

            # Connect to parent
            endpoint = f"tcp://localhost:{self.port}"
            self.socket.connect(endpoint)

            # Setup IO redirection AFTER ZMQ is ready
            self._setup_io_redirection()

        except zmq.ZMQError as e:
            error_msg = f"FATAL: ZMQ connection to localhost:{self.port} failed: {e}"
            if e.errno == zmq.ECONNREFUSED:
                error_msg += " (Connection refused - is parent running?)"
            print(error_msg, file=sys.stderr)
            sys.exit(1)
        except Exception as e:
            print(f"FATAL: Unexpected error connecting to parent: {e}", file=sys.stderr)
            sys.exit(1)

    def _send_output(self, msg_type: MessageType, output: str):
        """
        Send stdout/stderr output to parent.

        Args:
            msg_type: MessageType.STDOUT or MessageType.STDERR
            output: The output text to send
        """
        if self.socket:
            try:
                message = ComlinkMessage.create_output(output, msg_type)
                self.socket.send(message.pack(), zmq.NOBLOCK)
            except zmq.Again:
                # Socket busy, skip this output (acceptable for stdout/stderr)
                pass
            except Exception:
                # Ignore output send failures - don't break the worker
                pass

    def _start(self):
        """Start the worker message loop."""
        if not hasattr(self, "_running"):
            raise RuntimeError(
                f"{self.__class__.__name__}.__init__() must call super().__init__()"
            )

        self._setup_zmq()

        while self._running:
            try:
                # Try to receive message with timeout
                try:
                    message_data = self.socket.recv(zmq.NOBLOCK)
                    self._handle_message(message_data)
                except zmq.Again:
                    # No message available, continue
                    pass

                # Small sleep to prevent tight loop
                time.sleep(0.01)

            except Exception as e:
                print(f"ERROR: Error in message loop: {e}", file=sys.stderr)
                # Continue running unless it's a fatal error
                if "Context was terminated" in str(e):
                    break

    def _handle_message(self, message_data):
        """
        Handle incoming message from parent.

        Args:
            message_data: Raw message bytes from ZMQ
        """
        try:
            message = ComlinkMessage.unpack(message_data)

            # Basic validation
            if not hasattr(message, "app") or message.app != self.APP_NAME:
                return

            # Check namespace match
            if hasattr(message, "namespace") and message.namespace != self.namespace:
                return

            # Handle message types
            if message.type == MessageType.CALL.value:
                self._handle_function_call(message)
            elif message.type == MessageType.SHUTDOWN.value:
                self._running = False

        except Exception as e:
            print(f"ERROR: Failed to process message: {e}", file=sys.stderr)

    def _handle_function_call(self, message: ComlinkMessage):
        """
        Handle a function call message from parent.

        Args:
            message: The call message
        """
        response = None

        try:
            # Validate message
            if not hasattr(message, "function") or not message.function:
                raise ValueError("Message missing 'function' field")
            if not hasattr(message, "id") or not message.id:
                raise ValueError("Message missing 'id' field")

            # Extract args
            args = message.extract_call_args()

            # Validate function exists
            if not hasattr(self, message.function):
                raise AttributeError(f"Function '{message.function}' not found")

            func = getattr(self, message.function)
            if not callable(func):
                raise AttributeError(f"'{message.function}' is not callable")

            if message.function.startswith("_"):
                raise AttributeError(f"Cannot call private method '{message.function}'")

            # Call the function (sync or async)
            if asyncio.iscoroutinefunction(func):
                # Handle async functions
                try:
                    loop = asyncio.get_event_loop()
                except RuntimeError:
                    loop = asyncio.new_event_loop()
                    asyncio.set_event_loop(loop)

                result = loop.run_until_complete(func(*args))
            else:
                # Handle sync functions
                result = func(*args)

            # Create success response
            response = ComlinkMessage.create_response(result, message.id)

        except Exception as e:
            # Create error response
            error_msg = f"{type(e).__name__}: {str(e)}"
            full_error = f"{error_msg}\n{traceback.format_exc()}"
            response = ComlinkMessage.create_error(full_error, message.id)

        # Send response
        if response:
            try:
                self.socket.send(response.pack(), zmq.NOBLOCK)
            except zmq.Again:
                # Socket busy - try once more after brief pause
                try:
                    time.sleep(0.001)
                    self.socket.send(response.pack(), zmq.NOBLOCK)
                except Exception:
                    print(
                        f"CRITICAL: Failed to send response for {message.id}",
                        file=sys.stderr,
                    )
            except Exception as e:
                print(f"CRITICAL: Failed to send response: {e}", file=sys.stderr)

    def _stop(self):
        """Stop the worker and cleanup resources."""
        self._running = False

        # Restore stdout/stderr
        if self.original_stdout:
            sys.stdout = self.original_stdout
        if self.original_stderr:
            sys.stderr = self.original_stderr

        # Cleanup resources
        try:
            if self.socket:
                self.socket.close()
            if self.context:
                self.context.term()
        except Exception as e:
            print(f"Error during cleanup: {e}", file=sys.stderr)

    def list_functions(self):
        """
        List available callable public methods.

        Returns:
            List of method names that can be called remotely
        """
        excluded = set(dir(ChildWorker))
        return [
            name
            for name in dir(self)
            if callable(getattr(self, name))
            and not name.startswith("_")
            and name not in excluded
        ]

    def _handle_signals(self):
        """Setup signal handlers for graceful shutdown."""

        def signal_handler(sig, frame):
            print(f"Received signal {sig}, shutting down...")
            self._stop()

        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)

    def __enter__(self):
        """Context manager entry."""
        self._start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self._stop()

    def run(self):
        """Run the worker with signal handling."""
        self._handle_signals()
        try:
            self._start()
        finally:
            self._stop()
