"""
Example: Multiple parents connecting to a single worker.

This demonstrates the ROUTER/DEALER architecture where multiple
parent processes can connect to and call functions on a single
worker service.

Usage:
1. First, run math_worker_service.py in a separate terminal
2. Then run this script to see multiple parents connecting

python math_worker_service.py  # Terminal 1
python parent_connects.py      # Terminal 2
"""

import asyncio
import sys
import os

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "src"))

from multifrost import ParentWorker


async def parent_task(parent_id: int):
    """A single parent connecting to the worker and making calls."""
    print(f"Parent {parent_id}: Connecting to math-service...")

    try:
        worker = await ParentWorker.connect("math-service", timeout=5.0)
        handle = await worker.handle()
        print(f"Parent {parent_id}: Connected!")

        for i in range(3):
            result = await handle.call.add(parent_id * 10, i)
            print(f"Parent {parent_id}: {parent_id * 10} + {i} = {result}")
            await asyncio.sleep(0.5)

        await handle.stop()
        print(f"Parent {parent_id}: Done")
    except Exception as e:
        print(f"Parent {parent_id}: Error - {e}")


async def main():
    """Run multiple parents connecting to the same worker."""
    print("Starting multiple parents connecting to math-service...")
    print("Make sure math_worker_service.py is running!\n")

    # Run 3 parents concurrently
    await asyncio.gather(
        parent_task(1),
        parent_task(2),
        parent_task(3),
    )

    print("\nAll parents finished!")


if __name__ == "__main__":
    asyncio.run(main())
