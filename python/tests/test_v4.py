#!/usr/bin/env python3
"""
Quick test for Multifrost v4 - spawn and connect modes.
Run with: uv run python tests/test_v4.py
"""

import asyncio
import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from multifrost import ParentWorker, ChildWorker, ServiceRegistry


class TestWorker(ChildWorker):
    """Simple test worker."""

    def add(self, a, b):
        return a + b

    async def async_add(self, a, b):
        await asyncio.sleep(0.1)
        return a + b


class ServiceWorker(ChildWorker):
    """Worker for connect mode testing."""

    def __init__(self):
        super().__init__(service_id="test-service-v4")

    def multiply(self, a, b):
        return a * b


async def test_spawn_mode():
    """Test spawn mode (parent spawns child)."""
    print("\n=== Test: Spawn Mode ===")

    worker = ParentWorker.spawn(__file__)
    await worker.start()

    # Wait for child to connect
    await asyncio.sleep(0.5)

    try:
        result = await worker.acall.add(5, 3)
        assert result == 8, f"Expected 8, got {result}"
        print(f"  add(5, 3) = {result} OK")

        result = await worker.acall.async_add(10, 20)
        assert result == 30, f"Expected 30, got {result}"
        print(f"  async_add(10, 20) = {result} OK")

    finally:
        await worker.close()

    print("  Spawn mode: PASSED")


async def test_connect_mode():
    """Test connect mode (parent connects to running service)."""
    print("\n=== Test: Connect Mode ===")

    # Start service worker in background task
    service_worker = ServiceWorker()
    run_task = asyncio.create_task(asyncio.to_thread(service_worker.run))

    # Give worker time to register (service registry needs time)
    await asyncio.sleep(1.0)

    try:
        # Connect as parent
        parent = await ParentWorker.connect("test-service-v4", timeout=5.0)
        await parent.start()

        # Wait for connection
        await asyncio.sleep(0.3)

        result = await parent.acall.multiply(4, 7)
        assert result == 28, f"Expected 28, got {result}"
        print(f"  multiply(4, 7) = {result} OK")

        await parent.close()
        print("  Connect mode: PASSED")

    finally:
        # Cleanup service worker
        service_worker._running = False
        run_task.cancel()
        try:
            await run_task
        except asyncio.CancelledError:
            pass

        # Unregister service
        registry = ServiceRegistry()
        registry.unregister("test-service-v4")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--worker":
        # Run as spawned worker
        TestWorker().run()
    else:
        print("Running Multifrost v4 tests...")
        asyncio.run(test_spawn_mode())
        asyncio.run(test_connect_mode())
        print("\nAll tests passed!")
