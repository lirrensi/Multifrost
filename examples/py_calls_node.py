# Simple test to verify sync/async behavior
import asyncio
import time
import threading
import sys
import os
from concurrent.futures import ThreadPoolExecutor

# Test child worker (save as test_worker.py)


sys.path.append(
    os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "python", "src"
    )
)
from multifrost import ParentWorker


import os

script_dir = os.path.dirname(os.path.abspath(__file__))
ts_file = os.path.join(script_dir, "..", "examples", "math_worker.ts")
worker = ParentWorker(ts_file, "tsx.cmd")


async def main():
    await worker.start()

    fibo = worker.call.fibonacci(11)
    print("got fibo => ", fibo)

    fact = worker.call.factorial(11)
    print("got fact => ", fact)

    await worker.stop()


if __name__ == "__main__":
    asyncio.run(main())
