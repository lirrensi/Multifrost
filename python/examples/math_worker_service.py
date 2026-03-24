#!/usr/bin/env python3
"""
FILE: python/examples/math_worker_service.py
PURPOSE: Named v5 service example that registers as `math-service`.
"""

from __future__ import annotations

import math

from multifrost import ServiceContext, ServiceWorker, run_service_sync


class MathService(ServiceWorker):
    def add(self, a: int, b: int) -> int:
        return a + b

    def multiply(self, a: int, b: int) -> int:
        return a * b

    async def factorial(self, n: int) -> int:
        if n < 0:
            raise ValueError("factorial is not defined for negative numbers")
        return math.factorial(n)


if __name__ == "__main__":
    run_service_sync(MathService(), ServiceContext(peer_id="math-service"))
