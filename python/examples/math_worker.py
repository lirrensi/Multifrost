#!/usr/bin/env python3
"""
FILE: python/examples/math_worker.py
PURPOSE: Minimal v5 service example that uses the default entrypoint-based peer id.
"""

from __future__ import annotations

import math

from multifrost import ServiceContext, ServiceWorker, run_service_sync


class MathWorker(ServiceWorker):
    def add(self, a: int, b: int) -> int:
        return a + b

    def multiply(self, a: int, b: int) -> int:
        return a * b

    def factorial(self, n: int) -> int:
        if n < 0:
            raise ValueError("factorial is not defined for negative numbers")
        return math.factorial(n)

    async def fibonacci(self, n: int) -> int:
        if n <= 0:
            return 0
        if n == 1:
            return 1

        a, b = 0, 1
        for _ in range(2, n + 1):
            a, b = b, a + b
        return b


if __name__ == "__main__":
    run_service_sync(MathWorker(), ServiceContext())
