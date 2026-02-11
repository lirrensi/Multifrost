"""
Service registry for service discovery with file locking.

Provides a JSON-based registry for workers to register themselves and
for parents to discover running services. Supports concurrent access
via file locking.
"""

import asyncio
import json
import os
import socket
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Dict, Optional

try:
    import psutil
except ImportError:
    psutil = None


class ServiceRegistry:
    """File-based service registry with locking for concurrent access."""

    REGISTRY_PATH = Path.home() / ".multifrost" / "services.json"
    LOCK_PATH = Path.home() / ".multifrost" / "registry.lock"
    _lock_file = None

    @staticmethod
    def _ensure_registry_dir():
        """Create registry directory if it doesn't exist."""
        ServiceRegistry.REGISTRY_PATH.parent.mkdir(parents=True, exist_ok=True)

    @staticmethod
    def _read_registry() -> Dict:
        """Read registry file, return empty dict if not exists."""
        ServiceRegistry._ensure_registry_dir()
        if not ServiceRegistry.REGISTRY_PATH.exists():
            return {}
        with open(ServiceRegistry.REGISTRY_PATH, "r") as f:
            return json.load(f)

    @staticmethod
    def _write_registry(data: Dict):
        """Write registry atomically (temp file + rename)."""
        ServiceRegistry._ensure_registry_dir()
        temp_path = ServiceRegistry.REGISTRY_PATH.with_suffix(".tmp")
        with open(temp_path, "w") as f:
            json.dump(data, f, indent=2)
        temp_path.replace(ServiceRegistry.REGISTRY_PATH)

    @staticmethod
    def _is_process_alive(pid: int) -> bool:
        """Check if process is still alive."""
        if psutil is None:
            # Fallback: try to send signal 0
            try:
                os.kill(pid, 0)
                return True
            except (OSError, ProcessLookupError):
                return False
        try:
            return psutil.pid_exists(pid)
        except Exception:
            return False

    @staticmethod
    async def register(service_id: str) -> int:
        """
        Register service, enforce uniqueness.
        Returns: port number
        Raises: RuntimeError if service_id already running
        """
        await ServiceRegistry._acquire_lock()
        try:
            services = ServiceRegistry._read_registry()

            # Check if service_id already exists with live PID
            if service_id in services:
                existing = services[service_id]
                if ServiceRegistry._is_process_alive(existing["pid"]):
                    raise RuntimeError(
                        f"Service '{service_id}' already running (PID: {existing['pid']}, port: {existing['port']})"
                    )
                # Dead process - will be overwritten

            # Pick free port
            port = ServiceRegistry._find_free_port()

            # Register
            services[service_id] = {
                "port": port,
                "pid": os.getpid(),
                "started": datetime.now().isoformat(),
            }

            ServiceRegistry._write_registry(services)
            return port
        finally:
            await ServiceRegistry._release_lock()

    @staticmethod
    async def discover(service_id: str, timeout: float = 5.0) -> int:
        """
        Discover service by ID, with polling.
        Returns: port number
        Raises: TimeoutError if service not found
        """
        deadline = time.time() + timeout

        while time.time() < deadline:
            await ServiceRegistry._acquire_lock()
            try:
                services = ServiceRegistry._read_registry()
                reg = services.get(service_id)

                if reg and ServiceRegistry._is_process_alive(reg["pid"]):
                    return reg["port"]
            finally:
                await ServiceRegistry._release_lock()

            await asyncio.sleep(0.1)

        raise TimeoutError(f"Service '{service_id}' not found within {timeout}s")

    @staticmethod
    async def unregister(service_id: str):
        """Process cleans up its own entry on shutdown."""
        try:
            await ServiceRegistry._acquire_lock()
        except RuntimeError as e:
            # Lock acquisition failed - log warning but don't crash
            print(
                f"Warning: Could not acquire lock for unregister: {e}", file=sys.stderr
            )
            return

        try:
            services = ServiceRegistry._read_registry()
            if service_id in services:
                reg = services[service_id]
                # Only remove if it's our PID
                if reg["pid"] == os.getpid():
                    del services[service_id]
                    ServiceRegistry._write_registry(services)
        except Exception as e:
            # Log error but don't crash during shutdown
            print(f"Warning: Error during unregister: {e}", file=sys.stderr)
        finally:
            await ServiceRegistry._release_lock()

    @staticmethod
    def _find_free_port() -> int:
        """Find a free port for ZeroMQ communication."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("", 0))
            s.listen(1)
            port = s.getsockname()[1]
        return port

    @staticmethod
    async def _acquire_lock():
        """Acquire file lock with timeout using atomic file creation."""
        max_wait = 10  # seconds
        start = time.time()

        while time.time() - start < max_wait:
            try:
                ServiceRegistry._ensure_registry_dir()

                # Atomic file creation - fails if file already exists
                if sys.platform == "win32":
                    import msvcrt

                    # Use os.open with O_CREAT | O_EXCL for atomic creation
                    fd = os.open(
                        ServiceRegistry.LOCK_PATH,
                        os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                    )
                    lock_file = os.fdopen(fd, "w")
                    # Apply lock to the file descriptor
                    msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl

                    # Use os.open with O_CREAT | O_EXCL for atomic creation
                    fd = os.open(
                        ServiceRegistry.LOCK_PATH,
                        os.O_CREAT | os.O_EXCL | os.O_WRONLY,
                    )
                    lock_file = os.fdopen(fd, "w")
                    # Apply lock to the file descriptor
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)

                # Write our PID to lock file
                lock_file.write(str(os.getpid()))
                lock_file.flush()

                ServiceRegistry._lock_file = lock_file
                return
            except (IOError, OSError):
                # Lock held by another process (file already exists)
                await asyncio.sleep(0.1)

        raise RuntimeError(f"Could not acquire registry lock after {max_wait}s")

    @staticmethod
    async def _release_lock():
        """Release file lock and remove lock file."""
        if hasattr(ServiceRegistry, "_lock_file") and ServiceRegistry._lock_file:
            try:
                if sys.platform == "win32":
                    import msvcrt

                    msvcrt.locking(
                        ServiceRegistry._lock_file.fileno(), msvcrt.LK_UNLCK, 1
                    )
                else:
                    import fcntl

                    fcntl.flock(ServiceRegistry._lock_file.fileno(), fcntl.LOCK_UN)
                ServiceRegistry._lock_file.close()
                # Remove the lock file atomically
                try:
                    os.unlink(ServiceRegistry.LOCK_PATH)
                except OSError:
                    pass
            except Exception:
                pass
            finally:
                ServiceRegistry._lock_file = None
