# Multifrost Python Quick Examples

## Install

```bash
pip install msgpack websockets
cd python
pip install -e .
```

## Async Caller

```python
import asyncio
from multifrost import connect


async def main() -> None:
    connection = connect("math-service")
    handle = connection.handle()

    await handle.start()
    try:
        total = await handle.call.add(10, 20)
        print(total)
    finally:
        await handle.stop()


asyncio.run(main())
```

## Sync Caller

```python
from multifrost import connect


connection = connect("math-service")
handle = connection.handle_sync()
handle.start()
try:
    total = handle.call.add(10, 20)
    print(total)
finally:
    handle.stop()
```

## Service Runner

```python
import asyncio
from multifrost import ServiceContext, ServiceWorker, run_service


class MathService(ServiceWorker):
    def add(self, a: int, b: int) -> int:
        return a + b

    async def multiply(self, a: int, b: int) -> int:
        return a * b


async def main() -> None:
    await run_service(MathService(), ServiceContext(peer_id="math-service"))


asyncio.run(main())
```

## Spawn Then Connect

```python
import asyncio
from pathlib import Path

from multifrost import connect, spawn


async def main() -> None:
    service = spawn(Path(__file__).with_name("math_worker_service.py"))
    connection = connect("math-service")
    handle = connection.handle()

    await handle.start()
    try:
        print(await handle.call.add(3, 4))
    finally:
        await handle.stop()
        service.stop()
        service.wait()


asyncio.run(main())
```

## Router Bootstrap Notes

- `connect(...)` and `run_service(...)` both bootstrap the router if it is not already reachable.
- The router port defaults to `9981`.
- Set `MULTIFROST_ROUTER_PORT` when you need a different port for a test or local demo.
