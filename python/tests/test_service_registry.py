"""
Tests for ServiceRegistry class.

Tests cover:
- Service registration
- Service discovery
- Service unregistration
- Process liveness checking
- File locking behavior
- Edge cases and error handling
"""

import asyncio
import json
import os
import sys
import time
import tempfile
import shutil
from pathlib import Path
from unittest.mock import patch, MagicMock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from multifrost.core.service_registry import ServiceRegistry


def cleanup_lock():
    """Force cleanup of lock file state."""
    # Save the current lock path before it gets changed
    lock_path = ServiceRegistry.LOCK_PATH

    # First try to release the lock properly if we have a file handle
    if ServiceRegistry._lock_file:
        try:
            if sys.platform == "win32":
                import msvcrt

                try:
                    msvcrt.locking(
                        ServiceRegistry._lock_file.fileno(), msvcrt.LK_UNLCK, 1
                    )
                except:
                    pass
            else:
                import fcntl

                try:
                    fcntl.flock(ServiceRegistry._lock_file.fileno(), fcntl.LOCK_UN)
                except:
                    pass
        except:
            pass

        try:
            ServiceRegistry._lock_file.close()
        except:
            pass
        ServiceRegistry._lock_file = None

    # Remove lock file if it exists (use the saved path)
    try:
        if lock_path.exists():
            os.unlink(lock_path)
    except:
        pass

    # Also try the current path
    try:
        if ServiceRegistry.LOCK_PATH.exists():
            os.unlink(ServiceRegistry.LOCK_PATH)
    except:
        pass


class TestServiceRegistryPaths:
    """Test ServiceRegistry path configuration."""

    def test_registry_path(self):
        """Test that registry path is in home directory."""
        expected = Path.home() / ".multifrost" / "services.json"
        assert ServiceRegistry.REGISTRY_PATH == expected

    def test_lock_path(self):
        """Test that lock path is in home directory."""
        expected = Path.home() / ".multifrost" / "registry.lock"
        assert ServiceRegistry.LOCK_PATH == expected


class TestProcessLiveness:
    """Test process liveness checking."""

    def test_current_process_is_alive(self):
        """Test that current process is detected as alive."""
        current_pid = os.getpid()
        assert ServiceRegistry._is_process_alive(current_pid) is True

    def test_nonexistent_process_is_dead(self):
        """Test that non-existent process is detected as dead."""
        # Use a very high PID that's unlikely to exist
        dead_pid = 999999
        assert ServiceRegistry._is_process_alive(dead_pid) is False


class TestRegistryIO:
    """Test registry file I/O operations."""

    def setup_method(self):
        """Setup test fixtures with temporary directory."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"

    def teardown_method(self):
        """Cleanup temporary directory."""
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path

    def test_ensure_registry_dir(self):
        """Test that registry directory is created."""
        ServiceRegistry._ensure_registry_dir()

        assert ServiceRegistry.REGISTRY_PATH.parent.exists()

    def test_read_registry_empty(self):
        """Test reading non-existent registry returns empty dict."""
        data = ServiceRegistry._read_registry()

        assert data == {}

    def test_write_and_read_registry(self):
        """Test writing and reading registry data."""
        test_data = {
            "service-1": {"port": 12345, "pid": 1234, "started": "2024-01-01T00:00:00"}
        }

        ServiceRegistry._write_registry(test_data)
        read_data = ServiceRegistry._read_registry()

        assert read_data == test_data

    def test_write_registry_atomic(self):
        """Test that registry write is atomic (temp file + rename)."""
        test_data = {"service": {"port": 8080, "pid": 1}}

        ServiceRegistry._write_registry(test_data)

        # Temp file should not exist after write
        temp_path = ServiceRegistry.REGISTRY_PATH.with_suffix(".tmp")
        assert not temp_path.exists()

        # Main file should exist
        assert ServiceRegistry.REGISTRY_PATH.exists()


class TestFindFreePort:
    """Test free port finding."""

    def test_find_free_port(self):
        """Test that _find_free_port returns a valid port."""
        port = ServiceRegistry._find_free_port()

        # Port should be in valid range
        assert 1024 <= port <= 65535

    def test_find_free_port_returns_different_ports(self):
        """Test that multiple calls return different ports."""
        ports = [ServiceRegistry._find_free_port() for _ in range(5)]

        # At least some should be different
        assert len(set(ports)) > 1


class TestServiceRegistration:
    """Test service registration."""

    def setup_method(self):
        """Setup test fixtures."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        self.original_lock = ServiceRegistry.LOCK_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"
        ServiceRegistry.LOCK_PATH = Path(self.temp_dir) / "registry.lock"
        ServiceRegistry._lock_file = None

    def teardown_method(self):
        """Cleanup test fixtures."""
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path
        ServiceRegistry.LOCK_PATH = self.original_lock

    async def test_register_service(self):
        """Test registering a service."""
        port = await ServiceRegistry.register("test-service-1")

        assert isinstance(port, int)
        assert 1024 <= port <= 65535

        # Check registry was updated
        data = ServiceRegistry._read_registry()
        assert "test-service-1" in data
        assert data["test-service-1"]["port"] == port
        assert data["test-service-1"]["pid"] == os.getpid()

    async def test_register_duplicate_service_same_pid(self):
        """Test that registering with same PID overwrites."""
        port1 = await ServiceRegistry.register("test-service-2")

        # Simulate process restart - register again with same name
        port2 = await ServiceRegistry.register("test-service-2")

        # Should succeed and potentially use different port
        data = ServiceRegistry._read_registry()
        assert data["test-service-2"]["pid"] == os.getpid()

    async def test_register_multiple_services(self):
        """Test registering multiple services."""
        port1 = await ServiceRegistry.register("service-a")
        port2 = await ServiceRegistry.register("service-b")
        port3 = await ServiceRegistry.register("service-c")

        data = ServiceRegistry._read_registry()

        assert "service-a" in data
        assert "service-b" in data
        assert "service-c" in data

        # Ports should all be valid
        for svc in ["service-a", "service-b", "service-c"]:
            assert 1024 <= data[svc]["port"] <= 65535


class TestServiceDiscovery:
    """Test service discovery."""

    def setup_method(self):
        """Setup test fixtures."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        self.original_lock = ServiceRegistry.LOCK_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"
        ServiceRegistry.LOCK_PATH = Path(self.temp_dir) / "registry.lock"
        ServiceRegistry._lock_file = None

    def teardown_method(self):
        """Cleanup test fixtures."""
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path
        ServiceRegistry.LOCK_PATH = self.original_lock

    async def test_discover_existing_service(self):
        """Test discovering an existing service."""
        # Register first
        port = await ServiceRegistry.register("discover-test")

        # Discover
        found_port = await ServiceRegistry.discover("discover-test", timeout=1.0)

        assert found_port == port

    async def test_discover_nonexistent_service_timeout(self):
        """Test that discovering non-existent service times out."""
        try:
            await ServiceRegistry.discover("nonexistent-service", timeout=0.5)
            assert False, "Should have raised TimeoutError"
        except TimeoutError as e:
            assert "nonexistent-service" in str(e)

    async def test_discover_dead_service_timeout(self):
        """Test that discovering a dead service entry times out."""
        # Manually add a dead service entry
        data = {
            "dead-service": {
                "port": 12345,
                "pid": 999999,  # Non-existent PID
                "started": "2024-01-01T00:00:00",
            }
        }
        ServiceRegistry._write_registry(data)

        try:
            await ServiceRegistry.discover("dead-service", timeout=0.5)
            assert False, "Should have raised TimeoutError"
        except TimeoutError:
            pass


class TestServiceUnregistration:
    """Test service unregistration."""

    def setup_method(self):
        """Setup test fixtures."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        self.original_lock = ServiceRegistry.LOCK_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"
        ServiceRegistry.LOCK_PATH = Path(self.temp_dir) / "registry.lock"
        ServiceRegistry._lock_file = None

    def teardown_method(self):
        """Cleanup test fixtures."""
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path
        ServiceRegistry.LOCK_PATH = self.original_lock

    async def test_unregister_service(self):
        """Test unregistering a service."""
        # Register first
        await ServiceRegistry.register("unregister-test")

        # Unregister
        await ServiceRegistry.unregister("unregister-test")

        # Should be removed
        data = ServiceRegistry._read_registry()
        assert "unregister-test" not in data

    async def test_unregister_other_pid_service(self):
        """Test that unregistering a service with different PID is a no-op."""
        # Manually add a service with different PID
        data = {
            "other-pid-service": {
                "port": 12345,
                "pid": 999999,  # Different PID
                "started": "2024-01-01T00:00:00",
            }
        }
        ServiceRegistry._write_registry(data)

        # Try to unregister
        await ServiceRegistry.unregister("other-pid-service")

        # Should still exist (different PID)
        data = ServiceRegistry._read_registry()
        assert "other-pid-service" in data

    async def test_unregister_nonexistent_service(self):
        """Test that unregistering non-existent service doesn't fail."""
        # Should not raise
        await ServiceRegistry.unregister("nonexistent-service")


class TestFileLocking:
    """Test file locking behavior."""

    def setup_method(self):
        """Setup test fixtures."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        self.original_lock = ServiceRegistry.LOCK_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"
        ServiceRegistry.LOCK_PATH = Path(self.temp_dir) / "registry.lock"
        ServiceRegistry._lock_file = None

    def teardown_method(self):
        """Cleanup test fixtures."""
        # Ensure lock is released
        if ServiceRegistry._lock_file:
            try:
                ServiceRegistry._lock_file.close()
            except:
                pass
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path
        ServiceRegistry.LOCK_PATH = self.original_lock

    async def test_acquire_and_release_lock(self):
        """Test acquiring and releasing lock."""
        await ServiceRegistry._acquire_lock()

        assert ServiceRegistry._lock_file is not None
        assert ServiceRegistry.LOCK_PATH.exists()

        await ServiceRegistry._release_lock()

        assert ServiceRegistry._lock_file is None

    async def test_lock_file_contains_pid(self):
        """Test that lock file contains the process PID."""
        await ServiceRegistry._acquire_lock()

        # Read lock file
        with open(ServiceRegistry.LOCK_PATH, "r") as f:
            content = f.read()

        assert str(os.getpid()) in content

        await ServiceRegistry._release_lock()


class TestEdgeCases:
    """Test edge cases."""

    def setup_method(self):
        """Setup test fixtures."""
        self.temp_dir = tempfile.mkdtemp()
        self.original_path = ServiceRegistry.REGISTRY_PATH
        self.original_lock = ServiceRegistry.LOCK_PATH
        ServiceRegistry.REGISTRY_PATH = Path(self.temp_dir) / "services.json"
        ServiceRegistry.LOCK_PATH = Path(self.temp_dir) / "registry.lock"
        ServiceRegistry._lock_file = None

    def teardown_method(self):
        """Cleanup test fixtures."""
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)
        ServiceRegistry.REGISTRY_PATH = self.original_path
        ServiceRegistry.LOCK_PATH = self.original_lock

    async def test_service_name_with_special_chars(self):
        """Test service names with special characters."""
        port = await ServiceRegistry.register("service/with/slashes")

        data = ServiceRegistry._read_registry()
        assert "service/with/slashes" in data

        found_port = await ServiceRegistry.discover("service/with/slashes", timeout=0.5)
        assert found_port == port

    async def test_service_name_with_unicode(self):
        """Test service names with unicode characters."""
        port = await ServiceRegistry.register("service-\u4e2d\u6587")

        data = ServiceRegistry._read_registry()
        assert "service-\u4e2d\u6587" in data

    async def test_concurrent_registrations(self):
        """Test concurrent service registrations."""

        async def register_service(name):
            return await ServiceRegistry.register(name)

        # Register multiple services concurrently
        results = await asyncio.gather(
            *[register_service(f"concurrent-{i}") for i in range(5)]
        )

        # All should succeed with valid ports
        assert all(1024 <= p <= 65535 for p in results)

        # All should be registered
        data = ServiceRegistry._read_registry()
        for i in range(5):
            assert f"concurrent-{i}" in data

    async def test_empty_registry_operations(self):
        """Test operations on empty registry."""
        # Discover on empty registry should timeout
        try:
            await ServiceRegistry.discover("any-service", timeout=0.3)
            assert False, "Should have raised TimeoutError"
        except TimeoutError:
            pass

        # Unregister on empty registry should not fail
        await ServiceRegistry.unregister("any-service")


def run_tests():
    """Run all tests."""
    print("=" * 60)
    print("Running ServiceRegistry Tests")
    print("=" * 60)

    # TestServiceRegistryPaths
    print("\nTestServiceRegistryPaths...")
    t = TestServiceRegistryPaths()
    t.test_registry_path()
    t.test_lock_path()
    print("  [OK] All TestServiceRegistryPaths passed")

    # TestProcessLiveness
    print("\nTestProcessLiveness...")
    t = TestProcessLiveness()
    t.test_current_process_is_alive()
    t.test_nonexistent_process_is_dead()
    print("  [OK] All TestProcessLiveness passed")

    # TestRegistryIO
    print("\nTestRegistryIO...")
    t = TestRegistryIO()
    try:
        t.setup_method()
        t.test_ensure_registry_dir()
        t.test_read_registry_empty()
        t.test_write_and_read_registry()
        t.test_write_registry_atomic()
        print("  [OK] All TestRegistryIO passed")
    finally:
        t.teardown_method()

    # TestFindFreePort
    print("\nTestFindFreePort...")
    t = TestFindFreePort()
    t.test_find_free_port()
    t.test_find_free_port_returns_different_ports()
    print("  [OK] All TestFindFreePort passed")

    # TestServiceRegistration
    print("\nTestServiceRegistration...")
    t = TestServiceRegistration()
    try:
        t.setup_method()

        async def run_registration_tests():
            await t.test_register_service()
            cleanup_lock()

            await t.test_register_duplicate_service_same_pid()
            cleanup_lock()

            await t.test_register_multiple_services()
            cleanup_lock()

        asyncio.run(run_registration_tests())
        cleanup_lock()
        print("  [OK] All TestServiceRegistration passed")
    finally:
        cleanup_lock()
        t.teardown_method()

    # TestServiceDiscovery
    print("\nTestServiceDiscovery...")
    t = TestServiceDiscovery()
    try:
        t.setup_method()

        async def run_discovery_tests():
            await t.test_discover_existing_service()
            cleanup_lock()

            await t.test_discover_nonexistent_service_timeout()
            cleanup_lock()

            await t.test_discover_dead_service_timeout()
            cleanup_lock()

        asyncio.run(run_discovery_tests())
        cleanup_lock()
        print("  [OK] All TestServiceDiscovery passed")
    finally:
        cleanup_lock()
        t.teardown_method()

    # TestServiceUnregistration
    print("\nTestServiceUnregistration...")
    t = TestServiceUnregistration()
    try:
        t.setup_method()

        async def run_unregistration_tests():
            await t.test_unregister_service()
            cleanup_lock()

            await t.test_unregister_other_pid_service()
            cleanup_lock()

            await t.test_unregister_nonexistent_service()
            cleanup_lock()

        asyncio.run(run_unregistration_tests())
        cleanup_lock()
        print("  [OK] All TestServiceUnregistration passed")
    finally:
        cleanup_lock()
        t.teardown_method()

    # TestFileLocking
    print("\nTestFileLocking...")
    t = TestFileLocking()
    try:
        t.setup_method()

        async def run_locking_tests():
            await t.test_acquire_and_release_lock()
            cleanup_lock()

            await t.test_lock_file_contains_pid()
            cleanup_lock()

        asyncio.run(run_locking_tests())
        cleanup_lock()
        print("  [OK] All TestFileLocking passed")
    finally:
        cleanup_lock()
        t.teardown_method()

    # TestEdgeCases
    print("\nTestEdgeCases...")
    t = TestEdgeCases()
    try:
        t.setup_method()

        async def run_edge_case_tests():
            await t.test_service_name_with_special_chars()
            cleanup_lock()

            await t.test_service_name_with_unicode()
            cleanup_lock()

            await t.test_concurrent_registrations()
            cleanup_lock()

            await t.test_empty_registry_operations()
            cleanup_lock()

        asyncio.run(run_edge_case_tests())
        cleanup_lock()
        print("  [OK] All TestEdgeCases passed")
    finally:
        cleanup_lock()
        t.teardown_method()

    print("\n" + "=" * 60)
    print("[PASS] All ServiceRegistry tests passed!")
    print("=" * 60)


if __name__ == "__main__":
    run_tests()
