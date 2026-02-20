#!/usr/bin/env python3
"""
Real-world demo app using Multifrost for IPC.

This demonstrates using multifrost as it would be used in practice:
- Python parent spawns a Python worker for computation
- Python parent spawns a JS worker for computation
- Results are aggregated and displayed

Run: python e2e/demo_app.py
"""

import asyncio
import sys
import os

# Add the venv Python to PATH
VENV_PYTHON = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    ".venv-e2e",
    "Scripts",
    "python.exe",
)

# Full path to tsx for JS worker
TSX_EXECUTABLE = os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..",
    "javascript",
    "node_modules",
    ".bin",
    "tsx.CMD",
)

# Worker paths
PYTHON_WORKER = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "workers", "math_worker.py"
)
JS_WORKER = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "workers", "math_worker.ts"
)


async def main():
    print("=" * 60)
    print("Multifrost Demo App - Real-world computation example")
    print("=" * 60)
    print()

    from multifrost import ParentWorker

    # Demo: Calculate fibonacci using both workers and compare
    fib_n = 30

    print(f"Task: Calculate Fibonacci({fib_n}) using multiple workers")
    print("-" * 60)

    # Spawn Python worker
    print("[1] Spawning Python worker...")
    async with ParentWorker.spawn(PYTHON_WORKER, VENV_PYTHON) as py_worker:
        await asyncio.sleep(0.5)
        print("[1] Python worker ready")

        # Call synchronous fibonacci
        py_result = await py_worker.acall.fibonacci(fib_n)
        print(f"[1] Python sync fibonacci({fib_n}) = {py_result}")

        # Call async fibonacci
        py_async_result = await py_worker.acall.fibonacci_async(fib_n)
        print(f"[1] Python async fibonacci({fib_n}) = {py_async_result}")

        # More calculations
        py_fact = await py_worker.acall.factorial(12)
        print(f"[1] Python factorial(12) = {py_fact}")

    print()

    # Spawn JS worker
    print("[2] Spawning JavaScript worker...")
    async with ParentWorker.spawn(JS_WORKER, TSX_EXECUTABLE) as js_worker:
        await asyncio.sleep(1.0)
        print("[2] JavaScript worker ready")

        # Call synchronous fibonacci
        js_result = await js_worker.acall.fibonacci(fib_n)
        print(f"[2] JS sync fibonacci({fib_n}) = {js_result}")

        # Call async fibonacci
        js_async_result = await js_worker.acall.fibonacciAsync(fib_n)
        print(f"[2] JS async fibonacci({fib_n}) = {js_async_result}")

        # More calculations
        js_fact = await js_worker.acall.factorial(12)
        print(f"[2] JS factorial(12) = {js_fact}")

    print()
    print("=" * 60)
    print("Results Summary:")
    print("=" * 60)
    print(f"Python fibonacci({fib_n}) = {py_result}")
    print(f"JavaScript fibonacci({fib_n}) = {js_result}")
    print(f"Match: {'OK' if py_result == js_result else 'FAIL'}")
    print()
    print("Demo complete!")


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)
