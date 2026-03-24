#!/usr/bin/env python3
"""
FILE: python/examples/py_calls_rust.py
PURPOSE: Async v5 caller example that targets the Rust math service example.
"""

from __future__ import annotations

import asyncio

from multifrost import connect


async def main() -> None:
    connection = connect("math-service")
    handle = connection.handle()

    await handle.start()
    try:
        print(await handle.call.add(10, 20))
        print(await handle.call.factorial(6))
    finally:
        await handle.stop()


if __name__ == "__main__":
    asyncio.run(main())
