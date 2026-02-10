"""
Multifrost IPC - Inter-Process Communication Library

Version 3: Async-first architecture with unified sync/async API.

This library provides seamless IPC between Python processes and Python-Node.js processes
using ZeroMQ and msgpack for high-performance message passing.

## Quick Start

### Python Parent -> Python Child
```python
from multifrost import ParentWorker, ChildWorker

# Child worker implementation
class MathWorker(ChildWorker):
    def add(self, a, b):
        return a + b

# In child process
if __name__ == "__main__":
    worker = MathWorker()
    worker.start()

# In parent process
worker = ParentWorker("math_worker.py")
await worker.start()  # Start is async

# Synchronous call (blocking)
result = worker.call.add(1, 2)  # Returns 3

# Asynchronous call (non-blocking)
result = await worker.acall.add(1, 2)  # Returns 3

await worker.stop()
```

### Python Parent -> Node.js Child
```python
worker = ParentWorker("node_worker.js", executable="node")
await worker.start()

# Works the same way!
result = worker.call.add(1, 2)
result = await worker.acall.add(1, 2)

await worker.stop()
```

## API Changes from v2

- `worker.proxy` → `worker.call` (synchronous calls)
- `worker.aproxy` → `worker.acall` (asynchronous calls)
- `worker.astart()` → `worker.start()` (start is now async)
- ChildWorker API unchanged

## Architecture

The library uses an async-first architecture where:
1. All internal logic is async
2. Synchronous calls are wrapped in a background event loop
3. Zero code duplication between sync and async paths
4. ChildWorker implementations remain unchanged

For migration guide, see: python/REFACTOR.md

## Exports

- ParentWorker: Main class for creating IPC connections
- ChildWorker: Base class for worker implementations
- MessageType: Enum for message types
- ComlinkMessage: Message container class
"""

from .core.async_worker import ParentWorker
from .core.child import ChildWorker
from .core.message import MessageType, ComlinkMessage

__version__ = "3.0.0"
__all__ = [
    "ParentWorker",
    "ChildWorker",
    "MessageType",
    "ComlinkMessage",
]
