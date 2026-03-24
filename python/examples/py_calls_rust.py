#!/usr/bin/env python3
"""
FILE: python/examples/py_calls_rust.py
PURPOSE: Async v5 caller example that targets the Rust math service example.
"""

from __future__ import annotations

import asyncio
import argparse
import os

from multifrost import connect


def _resolve_target() -> str:
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--target", default=os.environ.get("MULTIFROST_TARGET_PEER_ID", "math-service"))
    args, _ = parser.parse_known_args()
    target = args.target.strip()
    return target or "math-service"


async def main() -> None:
    target = _resolve_target()
    connection = connect(target)
    handle = connection.handle()

    await handle.start()
    try:
        print(f"add(10, 20) = {await handle.call.add(10, 20)}")
        print(f"multiply(7, 8) = {await handle.call.multiply(7, 8)}")
        print(f"factorial(5) = {await handle.call.factorial(5)}")
    finally:
        await handle.stop()


if __name__ == "__main__":
    asyncio.run(main())
