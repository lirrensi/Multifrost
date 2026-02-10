#!/usr/bin/env python3

import asyncio
import sys
import os

# Add parent directory to path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from comlink_ipc_v2 import ParentWorker, ChildWorker


class TestWorker(ChildWorker):
    """Simple test worker for verification."""

    def add(self, a, b):
        """Add two numbers."""
        print(f"Computing {a} + {b}")
        return a + b

    def multiply(self, a, b):
        """Multiply two numbers."""
        print(f"Computing {a} * {b}")
        return a * b

    async def slow_add(self, a, b):
        """Slow add with async."""
        print(f"Slowly computing {a} + {b}")
        await asyncio.sleep(0.5)
        return a + b


async def test_async_api():
    """Test the async API."""
    print("\n=== Testing Async API ===\n")

    # Create worker
    worker = ParentWorker(__file__, auto_restart=False)

    # Start worker
    await worker.start()

    try:
        # Test basic sync method
        result = await worker.acall.add(5, 3)
        print(f"✓ add(5, 3) = {result}")
        assert result == 8, f"Expected 8, got {result}"

        # Test another sync method
        result = await worker.acall.multiply(4, 7)
        print(f"✓ multiply(4, 7) = {result}")
        assert result == 28, f"Expected 28, got {result}"

        # Test async method
        result = await worker.acall.slow_add(10, 20)
        print(f"✓ slow_add(10, 20) = {result}")
        assert result == 30, f"Expected 30, got {result}"

        # Test with options
        result = await worker.acall.with_options(timeout=5).add(100, 200)
        print(f"✓ add(100, 200) with timeout = {result}")
        assert result == 300, f"Expected 300, got {result}"

        print("\n✅ All async tests passed!\n")

    finally:
        await worker.close()


def test_sync_api():
    """Test the sync API."""
    print("\n=== Testing Sync API ===\n")

    # Create worker
    worker = ParentWorker(__file__, auto_restart=False)

    # Start worker synchronously
    worker.call.start()

    try:
        # Test basic sync method
        result = worker.call.add(5, 3)
        print(f"✓ add(5, 3) = {result}")
        assert result == 8, f"Expected 8, got {result}"

        # Test another sync method
        result = worker.call.multiply(4, 7)
        print(f"✓ multiply(4, 7) = {result}")
        assert result == 28, f"Expected 28, got {result}"

        # Test async method (sync wrapper handles it)
        result = worker.call.slow_add(10, 20)
        print(f"✓ slow_add(10, 20) = {result}")
        assert result == 30, f"Expected 30, got {result}"

        # Test with options
        result = worker.call.with_options(timeout=5).add(100, 200)
        print(f"✓ add(100, 200) with timeout = {result}")
        assert result == 300, f"Expected 300, got {result}"

        print("\n✅ All sync tests passed!\n")

    finally:
        worker.call.close()


async def test_context_managers():
    """Test context manager usage."""
    print("\n=== Testing Context Managers ===\n")

    # Test async context manager
    async with ParentWorker(__file__) as worker:
        await worker.start()
        result = await worker.acall.add(1, 2)
        print(f"✓ Async context: add(1, 2) = {result}")
        assert result == 3

    # Test sync context manager
    with ParentWorker(__file__).sync as worker:
        worker.start()
        result = worker.call.add(3, 4)
        print(f"✓ Sync context: add(3, 4) = {result}")
        assert result == 7

    print("\n✅ Context manager tests passed!\n")


if __name__ == "__main__":
    # Check if we're running as worker or parent
    if len(sys.argv) > 1 and sys.argv[1] == "--worker":
        # Run as worker
        TestWorker().run()
    else:
        # Run as parent and run tests
        print("🚀 Starting refactored comlink tests...\n")

        # Run async tests
        asyncio.run(test_async_api())

        # Run sync tests
        test_sync_api()

        # Run context manager tests
        asyncio.run(test_context_managers())

        print("\n🎉 All tests completed successfully!")
