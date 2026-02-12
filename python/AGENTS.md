# Multifrost Python Implementation - Agent Guide

## Build & Test Commands

```bash
# Install dependencies
make install-python

# Run tests
make test-python
python -m pytest tests/ -v

# Clean build artifacts
make clean
```

## Architecture Overview

### Core Design Principles

The Python implementation is **async-first** using Python's `asyncio` event loop:

- **ParentWorker**: Fully async, uses `zmq.asyncio` for non-blocking ZeroMQ operations
- **ChildWorker**: Manages a dedicated event loop in a daemon thread for async method handlers
- **SyncWrapper**: Provides synchronous API over async core by running event loop in background thread

### Process Communication

- **ZeroMQ sockets**: DEALER (parent) / ROUTER (child)
- **Serialization**: msgpack for efficient cross-language communication
- **Modes**: Supports both spawn (start child process) and connect (service discovery)

### Key Components

- **ParentWorker**: Manages child lifecycle, handles requests via asyncio.Future, implements circuit breaker and heartbeat monitoring
- **ChildWorker**: Runs ROUTER socket, supports both sync and async methods, redirects stdout/stderr
- **ServiceRegistry**: File-based service discovery with atomic file locking for connect mode
- **Message**: Handles serialization/deserialization with msgpack, includes message validation and sanitization

## API Usage

### Async (Recommended)

```python
async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.factorial(10)
```

### Sync (Legacy)

```python
worker = ParentWorker.spawn("worker.py")
worker.sync.start()
result = worker.sync.call.factorial(10)
worker.sync.close()
```

## Code Style Guidelines

### Import Organization

1. Standard library imports first
2. Third-party imports second (msgpack, zmq, psutil)
3. Local imports last (multifrost.core.*)

### Formatting

Use `black` with `line-length = 88`.

### Type Safety

- Prefer `typing` module for type hints
- Use `mypy` for static type checking
- Type hint all public APIs

### Naming Conventions

- **Functions/variables**: `snake_case`
- **Classes**: `PascalCase`
- **Constants**: `SCREAMING_SNAKE_CASE`

### Error Handling

- Use exceptions: `raise ValueError` for invalid inputs, `raise RuntimeError` for unexpected errors
- Prefer `raise` over `assert` for runtime validation
- Include descriptive error messages

### Documentation

Use Google-style docstrings for public APIs:

```python
def async_function(self, param: str) -> int:
    """Execute async operation.

    Args:
        param: Parameter description

    Returns:
        Result description
    """
    ...
```

## Common Patterns

### Async Method Support in Child Workers

```python
class MyWorker(ChildWorker):
    def add(self, a, b):
        return a + b

    async def fetch_data(self, url):
        async with aiohttp.ClientSession() as session:
            async with session.get(url) as resp:
                return await resp.json()
```

### Context Manager Usage

Both ParentWorker and ChildWorker support `async with` for automatic cleanup.

### Circuit Breaker

- Tracks consecutive failures via `_consecutive_failures`
- Opens after `max_restart_attempts` (default: 5)
- Resets on successful call
- Cannot make calls when open

### Heartbeat Monitoring

- Sends heartbeat every 5 seconds (default)
- Waits up to 3 seconds for response
- Calculates RTT from metadata
- Trips circuit breaker after 3 consecutive misses
