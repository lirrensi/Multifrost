# Multifrost Python Quick Examples

Get started with the Python implementation of Multifrost IPC library.

## Installation

```bash
# Install dependencies
pip install msgpack psutil pyzmq

# Install the library in editable mode
cd python
pip install -e .
```

## Quick Start: Parent-Child Example

Create a child worker with a callable method:

```python
# math_worker.py
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def add(self, a: int, b: int) -> int:
        """Add two numbers"""
        return a + b

    def multiply(self, a: int, b: int) -> int:
        """Multiply two numbers"""
        return a * b

if __name__ == "__main__":
    worker = MathWorker()
    worker.run()
```

Create a parent that calls the child:

```python
# parent.py
import asyncio
from multifrost import ParentWorker

async def main():
    # Spawn the child worker
    worker = ParentWorker.spawn("./math_worker.py")

    # Get a handle (async mode)
    handle = worker.handle()

    # Start the worker
    await handle.start()

    # Call methods via the handle
    result1 = await handle.call.add(5, 3)
    print(f"5 + 3 = {result1}")

    result2 = await handle.call.multiply(4, 7)
    print(f"4 * 7 = {result2}")

    # Clean up
    await handle.stop()

if __name__ == "__main__":
    asyncio.run(main())
```

Run the example:

```bash
# Start the child worker in one terminal
python math_worker.py

# In another terminal, run the parent
python parent.py
```

## Worker → Handle Pattern (v4)

Starting with v4, the API separates process definition (Worker) from runtime interface (Handle):

```
Worker = config/state (holds socket, process, registry internally)
Handle = lightweight API view (lifecycle + call interface)
```

### Async Handle (Recommended)

```python
from multifrost import ParentWorker

async def main():
    worker = ParentWorker.spawn("./worker.py")
    handle = worker.handle()  # Async handle

    await handle.start()
    result = await handle.call.add(1, 2)
    await handle.stop()
```

### Sync Handle

```python
from multifrost import ParentWorker

worker = ParentWorker.spawn("./worker.py")
handle = worker.handle_sync()  # Sync handle

handle.start()  # Blocking
result = handle.call.add(1, 2)  # No await
handle.stop()
```

### Context Manager Support

```python
# Async context manager
async def main():
    worker = ParentWorker.spawn("./worker.py")
    async with worker.handle() as h:
        result = await h.call.add(1, 2)
    # Automatic cleanup

# Sync context manager
worker = ParentWorker.spawn("./worker.py")
with worker.handle_sync() as h:
    result = h.call.add(1, 2)
# Automatic cleanup
```

## Connect Mode

Register a service and connect from a parent:

```python
# worker.py
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="math-service")

    def add(self, a, b):
        return a + b

if __name__ == "__main__":
    worker = MathWorker()
    worker.run()
```

```python
# parent.py
import asyncio
from multifrost import ParentWorker

async def main():
    # Connect to the existing service
    worker = await ParentWorker.connect("math-service", timeout=5)
    handle = worker.handle()
    await handle.start()

    result = await handle.call.add(5, 3)
    print(f"5 + 3 = {result}")

    await handle.stop()

if __name__ == "__main__":
    asyncio.run(main())
```

## Common Patterns

### Error Handling

```python
async def main():
    worker = ParentWorker.spawn("./worker.py")
    handle = worker.handle()
    await handle.start()

    try:
        result = await handle.call.add(1, 2)
        print(f"Result: {result}")
    except Exception as e:
        print(f"Error: {e}")

    await handle.stop()
```

### Method with Options

```python
# Call with custom timeout and namespace
result = await handle.call.with_options(
    timeout=5000,
    namespace="my-namespace"
).add(1, 2)
```

### List Available Methods

```python
async def main():
    worker = ParentWorker.spawn("./worker.py")
    handle = worker.handle()
    await handle.start()

    # List methods on the child
    methods = handle.call.list_functions()
    print(f"Available methods: {methods}")

    await handle.stop()
```

### Async Methods in Child

```python
# worker.py
import asyncio

class Worker(ChildWorker):
    async def fetch_data(self, url: str) -> dict:
        """Async method using dedicated event loop"""
        import aiohttp
        async with aiohttp.ClientSession() as session:
            async with session.get(url) as resp:
                return await resp.json()

if __name__ == "__main__":
    worker = Worker()
    worker.run()
```

### Metrics Collection

```python
async def main():
    worker = ParentWorker.spawn("./worker.py")
    handle = worker.handle()
    await handle.start()

    # Get metrics from worker (introspection)
    metrics = worker.metrics
    print(f"Total requests: {metrics.requests_total}")
    print(f"Success rate: {metrics.requests_success / metrics.requests_total if metrics.requests_total > 0 else 0}")

    await handle.stop()
```

### Health Checks

```python
async def main():
    worker = ParentWorker.spawn("./worker.py")
    handle = worker.handle()
    await handle.start()

    # Check if worker is healthy (introspection on worker)
    print(f"Healthy: {worker.is_healthy}")
    print(f"Circuit open: {worker.circuit_open}")
    print(f"Last RTT: {worker.last_heartbeat_rtt_ms}")

    await handle.stop()
```

## Key Concepts

### ParentWorker
- **Purpose**: Initiates calls and manages child lifecycle
- **Modes**: `spawn()` (creates new process) or `connect()` (connects to existing service)
- **Introspection**: `is_healthy`, `circuit_open`, `metrics`, `last_heartbeat_rtt_ms`

### ParentHandle / ParentHandleSync
- **Purpose**: Lightweight API view for lifecycle and calls
- **Methods**: `start()`, `stop()`, `call.*`
- **Context manager**: `async with worker.handle() as h:` or `with worker.handle_sync() as h:`

### ChildWorker
- **Purpose**: Exposes callable methods and handles requests
- **Methods**: Implement methods directly on the class
- **Modes**: Can register with `service_id` for connect mode

### Spawn Mode
- Parent finds free port and binds DEALER socket
- Parent spawns child with `COMLINK_ZMQ_PORT` environment variable
- Parent owns child process lifecycle

### Connect Mode
- Child registers with service registry (`~/.multifrost/services.json`)
- Parent discovers service and connects
- Better for long-running services

## Cross-Language Usage

Python parent calling JavaScript child:

```python
# parent.py
import asyncio
from multifrost import ParentWorker

async def main():
    # Spawn JavaScript worker
    worker = ParentWorker.spawn("./math_worker.js", "node")
    handle = worker.handle()
    await handle.start()

    # Call JavaScript method
    result = await handle.call.factorial(10)
    print(f"Factorial: {result}")

    await handle.stop()

if __name__ == "__main__":
    asyncio.run(main())
```

## Troubleshooting

**Child exits immediately:**
- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages

**Async functions don't work:**
- Verify `asyncio.iscoroutinefunction(func)` returns True
- Check if event loop is running
- Ensure timeout is set (default: 30s)

**Circuit breaker trips:**
- Check heartbeat configuration
- Verify child process is responding
- Review metrics for error patterns

For more details, see the [full architecture documentation](./arch.md).
