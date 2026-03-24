#!/usr/bin/env python3
"""
FILE: python/examples/concurrency_parent.py
PURPOSE: Async v5 caller example that shows concurrent requests over one handle.
"""

from __future__ import annotations

import asyncio
import time
from pathlib import Path

from multifrost import connect, spawn


async def main() -> None:
    service = spawn(Path(__file__).with_name("concurrency_worker.py"))
    connection = connect("concurrency-service")
    handle = connection.handle()

    await handle.start()
    try:
        started = time.perf_counter()
        results = await asyncio.gather(
            handle.call.async_task(1.0, "alpha"),
            handle.call.async_task(1.0, "beta"),
            handle.call.async_task(1.0, "gamma"),
        )
        elapsed = time.perf_counter() - started
        print(f"results={results}")
        print(f"elapsed={elapsed:.2f}s")
    finally:
        await handle.stop()
        service.stop()
        service.wait()


if __name__ == "__main__":
    asyncio.run(main())
