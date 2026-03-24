#!/usr/bin/env python3
"""
FILE: python/examples/parent_connects.py
PURPOSE: Async v5 caller example that connects to the named math service.
"""

from __future__ import annotations

import asyncio
from pathlib import Path

from multifrost import connect, spawn


async def main() -> None:
    service = spawn(Path(__file__).with_name("math_worker_service.py"))
    connection = connect("math-service")
    handle = connection.handle()

    await handle.start()
    try:
        print(await handle.call.add(5, 3))
        print(await handle.call.multiply(4, 7))
        print(await handle.call.factorial(5))
    finally:
        await handle.stop()
        service.stop()
        service.wait()


if __name__ == "__main__":
    asyncio.run(main())
