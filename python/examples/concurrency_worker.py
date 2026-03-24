#!/usr/bin/env python3
"""
FILE: python/examples/concurrency_worker.py
PURPOSE: Service used by the concurrent async caller example.
"""

from __future__ import annotations

import asyncio

from multifrost import ServiceContext, ServiceWorker, run_service_sync


class ConcurrencyService(ServiceWorker):
    def slow_task(self, duration: float, task_id: str) -> str:
        return f"{task_id} ran for {duration:.1f}s"

    async def async_task(self, duration: float, task_id: str) -> str:
        await asyncio.sleep(duration)
        return f"{task_id} ran for {duration:.1f}s"


if __name__ == "__main__":
    run_service_sync(
        ConcurrencyService(),
        ServiceContext(peer_id="concurrency-service"),
    )
