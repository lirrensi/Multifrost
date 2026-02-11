"""
Multifrost IPC - Inter-Process Communication Library

Version 4: ROUTER/DEALER architecture with spawn/connect modes.

This library provides seamless IPC between Python processes and Python-Node.js processes
using ZeroMQ and msgpack for high-performance message passing.

## Quick Start

### Python Parent -> Python Child (Spawn Mode)
```python
from multifrost import ParentWorker, ChildWorker

# Child worker implementation
class MathWorker(ChildWorker):
    def add(self, a, b):
        return a + b

# In child process
if __name__ == "__main__":
    worker = MathWorker()
    worker.run()

# In parent process
worker = ParentWorker.spawn("math_worker.py")
await worker.start()

# Synchronous call (blocking)
result = worker.call.add(1, 2)  # Returns 3

# Asynchronous call (non-blocking)
result = await worker.acall.add(1, 2)  # Returns 3

await worker.close()
```

### Python Parent -> Python Child (Connect Mode)
```python
# Child (runs independently as microservice)
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="math-service")

    def add(self, a, b):
        return a + b

if __name__ == "__main__":
    MathWorker().run()

# Parent (connects to running service)
from multifrost import ParentWorker

worker = await ParentWorker.connect("math-service")
await worker.start()
result = await worker.acall.add(1, 2)
await worker.close()
```

### Python Parent -> Node.js Child
```python
worker = ParentWorker.spawn("node_worker.js", executable="node")
await worker.start()

result = worker.call.add(1, 2)
result = await worker.acall.add(1, 2)

await worker.close()
```

### With Observability (Metrics & Logging)
```python
from multifrost import ParentWorker, default_json_handler

# Enable structured logging
worker = ParentWorker.spawn("child.py", log_handler=default_json_handler)
await worker.start()

# Make calls (automatically tracked)
result = await worker.acall.my_function(1, 2)

# Get metrics snapshot
metrics = worker.metrics.snapshot()
print(f"Avg latency: {metrics.latency_avg_ms}ms")
print(f"Error rate: {metrics.requests_failed / metrics.requests_total}")
```

## Architecture

The library uses ROUTER/DEALER socket pattern:
- ChildWorker uses ROUTER socket (supports multiple parents)
- ParentWorker uses DEALER socket
- ServiceRegistry provides service discovery via JSON file

For migration guide, see: python/REFACTOR.md

## Exports

- ParentWorker: Main class for creating IPC connections
- ChildWorker: Base class for worker implementations
- MessageType: Enum for message types
- ComlinkMessage: Message container class
- ServiceRegistry: Service discovery and registration
- Metrics: Metrics collection for observability
- StructuredLogger: Structured logging with correlation IDs
"""

from .core.async_worker import ParentWorker, RemoteCallError, CircuitOpenError
from .core.child import ChildWorker
from .core.message import MessageType, ComlinkMessage
from .core.service_registry import ServiceRegistry
from .core.metrics import Metrics, MetricsSnapshot, RequestMetrics
from .core.logging import (
    StructuredLogger,
    LogEntry,
    LogEvent,
    LogLevel,
    LogHandler,
    default_json_handler,
    default_pretty_handler,
)

__version__ = "3.0.0"
__all__ = [
    # Core
    "ParentWorker",
    "ChildWorker",
    "MessageType",
    "ComlinkMessage",
    "ServiceRegistry",
    # Errors
    "RemoteCallError",
    "CircuitOpenError",
    # Metrics
    "Metrics",
    "MetricsSnapshot",
    "RequestMetrics",
    # Logging
    "StructuredLogger",
    "LogEntry",
    "LogEvent",
    "LogLevel",
    "LogHandler",
    "default_json_handler",
    "default_pretty_handler",
]
