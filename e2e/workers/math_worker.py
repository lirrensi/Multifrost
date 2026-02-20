#!/usr/bin/env python3
"""
E2E Test Worker - Python implementation
This worker provides various methods for cross-language testing.
"""

import math
import os
import sys

from multifrost import ChildWorker


class MathWorker(ChildWorker):
    """Worker with math and utility methods for E2E testing."""

    def add(self, a, b):
        """Add two numbers."""
        return a + b

    def multiply(self, a, b):
        """Multiply two numbers."""
        return a * b

    def divide(self, a, b):
        """Divide two numbers."""
        if b == 0:
            raise ValueError("Cannot divide by zero")
        return a / b

    def factorial(self, n):
        """Calculate factorial."""
        if n < 0:
            raise ValueError("Factorial not defined for negative numbers")
        if n > 100:
            raise ValueError("Input too large")
        return math.factorial(n)

    def echo(self, value):
        """Echo back the value."""
        return value

    def get_info(self):
        """Return worker info."""
        return {
            "language": "python",
            "pid": os.getpid(),
            "version": sys.version.split()[0],
        }

    def throw_error(self, message):
        """Raise an error with the given message."""
        raise RuntimeError(message)

    async def async_add(self, a, b):
        """Async method - add two numbers."""
        import asyncio

        await asyncio.sleep(0.01)  # Simulate async work
        return a + b

    def large_data(self, size):
        """Return a large data structure of specified size."""
        return {"data": list(range(size)), "length": size}

    def fibonacci(self, n):
        """Calculate nth Fibonacci number (iterative, efficient)."""
        if n < 0:
            raise ValueError("Fibonacci not defined for negative numbers")
        if n == 0:
            return 0
        if n == 1:
            return 1

        a, b = 0, 1
        for _ in range(2, n + 1):
            a, b = b, a + b
        return b

    async def fibonacci_async(self, n):
        """Async Fibonacci - simulates longer computation."""
        import asyncio

        await asyncio.sleep(0.05)  # Simulate computation time
        return self.fibonacci(n)


if __name__ == "__main__":
    worker = MathWorker()
    worker.run()
